use crate::ai::AiRequestConfig;
use crate::ai::client::generate_ai_completion;
use crate::app::core::App;
use crate::app::message::Message;
use crate::constants::{
    ADHOC_QUERY_MAX_ROWS, QUERY_INLINE_AI_CONTEXT_MAX_BYTES, QUERY_INLINE_AI_CONTEXT_MAX_COLUMNS,
    QUERY_INLINE_AI_CONTEXT_MAX_TABLE_NAMES, QUERY_INLINE_AI_CONTEXT_MAX_TABLES,
    QUERY_INLINE_AI_MIN_CHARS, QUERY_INLINE_AI_MIN_INTERVAL_MS, QUERY_SUGGESTION_LIMIT,
    QUERY_UNDO_LIMIT,
};
use crate::db::query::run_table_query_with_control;
use crate::model::connection::DatabaseDriver;
use crate::model::settings::{Settings, ThemeChoice};
use crate::model::table::{QueryPrefix, TriggerInfo};
use crate::ui::widgets::code_snippet;
use crate::utils::helpers::{
    column_to_byte_offset, escape_mysql_identifier, format_definer, is_query_identifier_char,
};
use crate::utils::sql_parse::parse_single_table_select;
use iced::Task;
use iced_code_editor::{CodeEditor, ContextMenuItem, IndentStyle, Message as CodeEditorMessage};
use std::collections::HashSet;
use std::fmt::Write;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

impl App {
    pub(crate) fn new_query_editor(
        content: &str,
        settings: &Settings,
        theme_choice: ThemeChoice,
    ) -> CodeEditor {
        let mut editor = CodeEditor::new(content, "sql");
        editor.set_theme(code_snippet::style(&theme_choice.ui_theme()));
        editor.set_font(crate::ui::presentation::font_for_choice(
            &settings.editor_font,
        ));
        editor.set_font_size(settings.font_size as f32, false);
        editor.set_line_height(settings.font_size as f32 * 1.3);
        editor.set_line_numbers_enabled(settings.query_editor_line_numbers_enabled);
        editor.set_wrap_enabled(settings.query_editor_word_wrap_enabled);
        editor.set_folding_enabled(false);
        editor.set_command_palette_enabled(true);
        editor.set_custom_command_palette_entries(vec![
            ContextMenuItem::new("cryodb.format", "Format"),
            ContextMenuItem::new("cryodb.command_palette", "Open CryoDB Command Palette")
                .with_shortcut("F1"),
        ]);
        editor.set_auto_close_brackets(false);
        editor.set_auto_indent_enabled(false);
        editor.set_indent_style(if settings.insert_spaces {
            IndentStyle::Spaces(settings.tab_size as u8)
        } else {
            IndentStyle::Tab
        });
        editor
    }

    pub(crate) fn query_editor_focused(&self) -> bool {
        self.workspace.query.editor.is_focused() && !self.workspace.query.editor_blurred
    }

    pub(crate) fn focus_query_editor(&mut self) {
        self.workspace.query.editor.request_focus();
        self.workspace.query.editor_blurred = false;
    }

    pub(crate) fn sync_code_snippets(&mut self) {
        let theme = self.theme();
        let font = self.editor_font();
        let size = self.input_text_size() as f32;
        let editors = self
            .ai
            .chat_sessions
            .values_mut()
            .flatten()
            .flat_map(|session| session.messages.iter_mut())
            .flat_map(|message| message.blocks.iter_mut())
            .filter_map(|block| block.code.as_mut())
            .chain(
                self.workspace
                    .explorer
                    .table_ddl
                    .as_mut()
                    .and_then(|state| state.editor.as_mut()),
            );
        for editor in editors {
            code_snippet::restyle(editor, &theme, font, size);
        }
    }

    pub(crate) fn sync_query_editor_settings(&mut self) {
        self.workspace
            .query
            .editor
            .set_theme(code_snippet::style(&self.theme()));
        self.workspace.query.editor.set_font(self.editor_font());
        self.workspace
            .query
            .editor
            .set_font_size(self.input_text_size() as f32, false);
        self.workspace
            .query
            .editor
            .set_line_height(self.input_text_size() as f32 * 1.3);
        self.workspace
            .query
            .editor
            .set_line_numbers_enabled(self.settings.values.query_editor_line_numbers_enabled);
        self.workspace
            .query
            .editor
            .set_wrap_enabled(self.settings.values.query_editor_word_wrap_enabled);
        self.workspace
            .query
            .editor
            .set_indent_style(if self.settings.values.insert_spaces {
                IndentStyle::Spaces(self.settings.values.tab_size as u8)
            } else {
                IndentStyle::Tab
            });
        self.sync_code_snippets();
    }

    pub(crate) fn open_sql_in_new_query_tab(&mut self, sql: &str) {
        let mut state = self.blank_query_state();
        state.query =
            Self::new_query_editor(sql, &self.settings.values, self.settings.theme_choice);
        self.open_new_query_tab(state);
    }

    pub(crate) fn ai_autocomplete_available(&self) -> bool {
        let config = AiRequestConfig::for_autocomplete(&self.settings.values);
        self.settings.values.ai_autocomplete_enabled
            && (config.supports_autocomplete()
                || self.settings.values.ai_autocomplete_allow_local_cli)
            && config.is_usable()
    }

    pub(crate) fn schema_autocomplete_available(&self) -> bool {
        self.settings.values.schema_autocomplete_enabled
            && self.connections.connected
            && self.connections.pool.is_some()
    }

    pub(crate) fn query_autocomplete_available(&self) -> bool {
        self.schema_autocomplete_available()
            || self.settings.values.sql_keyword_autocomplete_enabled
    }

    pub(crate) fn clear_query_inline_suggestion(&mut self) {
        self.workspace.query.inline_suggestion = None;
        self.workspace.query.inline_suggestion_loading = false;
        self.workspace.query.inline_suggestion_last_edit_at = None;
        self.workspace.query.inline_suggestion_pending = false;
        self.workspace.query.inline_suggestion_request_id = self
            .workspace
            .query
            .inline_suggestion_request_id
            .wrapping_add(1);
    }

    pub(crate) fn query_cursor_byte_offset(&self) -> Option<usize> {
        let (line, column) = self.workspace.query.editor.cursor_position();
        let text = self.workspace.query.editor.content();
        let line_text = text.split('\n').nth(line)?.trim_end_matches('\r');
        Some(self.query_line_start_offset(line) + column_to_byte_offset(line_text, column))
    }

    pub(crate) fn clamp_char_boundary_left(text: &str, index: usize) -> usize {
        let mut bounded = index.min(text.len());
        while bounded > 0 && !text.is_char_boundary(bounded) {
            bounded -= 1;
        }
        bounded
    }

    pub(crate) fn clamp_char_boundary_right(text: &str, index: usize) -> usize {
        let mut bounded = index.min(text.len());
        while bounded < text.len() && !text.is_char_boundary(bounded) {
            bounded += 1;
        }
        bounded
    }

    pub(crate) fn inline_ai_context_tables(&self, query: &str) -> Vec<String> {
        if !self.settings.values.ai_send_schema_context || self.workspace.explorer.tables.is_empty()
        {
            return Vec::new();
        }

        crate::ai::sql_fix::resolve_ai_sql_fix_tables(
            query,
            &self.workspace.explorer.tables,
            self.workspace.selected_table.as_deref(),
        )
        .into_iter()
        .take(QUERY_INLINE_AI_CONTEXT_MAX_TABLES)
        .collect()
    }

    pub(crate) fn inline_ai_schema_context(&self, query: &str) -> Option<String> {
        let tables = self.inline_ai_context_tables(query);
        let mut parts = tables
            .iter()
            .map(|table| {
                let columns = self.cached_table_columns(table);
                if columns.is_empty() {
                    table.clone()
                } else {
                    format!(
                        "{table}({})",
                        columns
                            .iter()
                            .take(QUERY_INLINE_AI_CONTEXT_MAX_COLUMNS)
                            .map(String::as_str)
                            .collect::<Vec<_>>()
                            .join(",")
                    )
                }
            })
            .collect::<Vec<_>>();

        if parts.is_empty() && self.settings.values.ai_send_schema_context {
            parts = self
                .workspace
                .explorer
                .tables
                .iter()
                .take(QUERY_INLINE_AI_CONTEXT_MAX_TABLE_NAMES)
                .cloned()
                .collect();
        }

        if parts.is_empty() {
            None
        } else {
            Some(parts.join(" "))
        }
    }

    pub(crate) fn query_inline_ai_prompt(&self) -> Option<(String, usize, String)> {
        let anchor = self.workspace.query.editor.content();
        if anchor.trim().len() < QUERY_INLINE_AI_MIN_CHARS {
            return None;
        }

        let cursor_offset = self.query_cursor_byte_offset()?;
        let mut marked = anchor.clone();
        if cursor_offset > marked.len() {
            return None;
        }
        marked.insert_str(cursor_offset, "<CURSOR>");

        if marked.len() > QUERY_INLINE_AI_CONTEXT_MAX_BYTES {
            let half = QUERY_INLINE_AI_CONTEXT_MAX_BYTES / 2;
            let start = Self::clamp_char_boundary_left(&marked, cursor_offset.saturating_sub(half));
            let end =
                Self::clamp_char_boundary_right(&marked, (cursor_offset + half).min(marked.len()));
            marked = marked[start..end].to_string();
        }

        let schema = self
            .inline_ai_schema_context(&anchor)
            .map(|context| format!("Schema: {context}\n"))
            .unwrap_or_default();

        let prompt = format!(
            "Complete this SQL at <CURSOR>. Return ONLY the exact continuation text to insert at the cursor. No markdown, no explanations, no code fences. If no safe completion exists, return an empty string.\n\n{}SQL:\n{}",
            schema, marked
        );

        Some((anchor, cursor_offset, prompt))
    }

    pub(crate) fn request_inline_ai_suggestion(&mut self) -> Task<Message> {
        if !self.workspace.query.inline_suggestion_pending {
            return Task::none();
        }

        if !self.ai_autocomplete_available()
            || self.workspace.query.suggestions_open
            || self.workspace.query.running
            || self.ai.is_generating_ai
            || self.ai.is_fixing_query_with_ai
            || self.workspace.query.inline_suggestion_loading
        {
            return Task::none();
        }

        if self.query_cursor_byte_offset().is_some_and(|offset| {
            crate::utils::sql_parse::sql_offset_is_inert(
                &self.workspace.query.editor.content(),
                offset,
            )
        }) {
            self.workspace.query.inline_suggestion_pending = false;
            return Task::none();
        }

        if let Some(remainder) = self.cached_inline_suggestion() {
            self.workspace.query.inline_suggestion = Some(remainder);
            self.workspace.query.inline_suggestion_pending = false;
            return Task::none();
        }

        if let Some(last_edit) = self.workspace.query.inline_suggestion_last_edit_at
            && !self.query_inline_suggestion_idle_elapsed(last_edit)
        {
            return Task::none();
        }

        if let Some(last_request) = self.workspace.query.inline_suggestion_last_request_at
            && last_request.elapsed().as_millis() < QUERY_INLINE_AI_MIN_INTERVAL_MS
        {
            return Task::none();
        }

        let Some((anchor, anchor_offset, prompt)) = self.query_inline_ai_prompt() else {
            return Task::none();
        };

        self.workspace.query.inline_suggestion_loading = true;
        self.workspace.query.inline_suggestion_pending = false;
        self.workspace.query.inline_suggestion_last_request_at = Some(std::time::Instant::now());
        self.workspace.query.inline_suggestion_request_id = self
            .workspace
            .query
            .inline_suggestion_request_id
            .wrapping_add(1);
        let request_id = self.workspace.query.inline_suggestion_request_id;
        let config = AiRequestConfig::for_autocomplete(&self.settings.values);
        let driver = self.connections.current.driver;
        let prefetch = match self.inline_ai_context_tables(&anchor).into_iter().next() {
            Some(table) => self.prefetch_table_columns(&table),
            None => Task::none(),
        };

        Task::batch([
            prefetch,
            Task::perform(
                generate_ai_completion(config, prompt, driver),
                move |result| {
                    Message::Workspace(crate::app::features::workspace::Message::Query(crate::app::features::workspace::query::Message::InlineQuerySuggestionReady{
                    request_id,
                    anchor,
                    anchor_offset,
                    result,
                }))
                },
            ),
        ])
    }

    pub(crate) fn query_inline_suggestion_idle_elapsed(
        &self,
        last_edit: std::time::Instant,
    ) -> bool {
        last_edit.elapsed().as_millis()
            >= u128::from(self.settings.values.query_inline_suggestion_delay_ms)
    }

    pub(crate) fn inline_suggestion_typed_since(
        &self,
        anchor: &str,
        anchor_offset: usize,
    ) -> Option<String> {
        let current = self.workspace.query.editor.content();
        let offset = self.query_cursor_byte_offset()?;
        if offset < anchor_offset || current.len() != anchor.len() + (offset - anchor_offset) {
            return None;
        }
        if current.get(..anchor_offset) != anchor.get(..anchor_offset)
            || current.get(offset..) != anchor.get(anchor_offset..)
        {
            return None;
        }
        current.get(anchor_offset..offset).map(str::to_string)
    }

    pub(crate) fn cached_inline_suggestion(&self) -> Option<String> {
        let (anchor, offset, suggestion) = self.workspace.query.inline_suggestion_cache.as_ref()?;
        let typed = self.inline_suggestion_typed_since(anchor, *offset)?;
        let remainder = suggestion.strip_prefix(&typed)?;
        (!remainder.is_empty()).then(|| remainder.to_string())
    }

    pub(crate) fn normalize_inline_ai_suggestion(anchor: &str, raw: &str) -> Option<String> {
        crate::app::features::ai::State::normalize_inline_ai_suggestion(anchor, raw)
    }

    pub(crate) fn insert_at_query_cursor(&mut self, insertion: &str) -> bool {
        if insertion.is_empty() {
            return false;
        }

        self.record_query_undo(self.workspace.query.editor.content());
        let _ = self
            .workspace
            .query
            .editor
            .update(&CodeEditorMessage::Paste(insertion.to_string()));
        true
    }

    pub(crate) fn apply_inline_query_suggestion(&mut self) {
        let Some(inline) = self.workspace.query.inline_suggestion.clone() else {
            return;
        };
        self.insert_at_query_cursor(inline.trim_end());
        self.clear_query_inline_suggestion();
    }

    pub(crate) fn inline_suggestion_word_end(text: &str) -> usize {
        let mut end = 0;
        let mut chars = text.char_indices().peekable();
        while let Some(&(index, ch)) = chars.peek() {
            if !ch.is_whitespace() {
                break;
            }
            end = index + ch.len_utf8();
            chars.next();
        }

        let Some(&(_, first)) = chars.peek() else {
            return end;
        };
        if !first.is_alphanumeric() && first != '_' {
            return end + first.len_utf8();
        }
        for (index, ch) in chars {
            if !ch.is_alphanumeric() && ch != '_' {
                break;
            }
            end = index + ch.len_utf8();
        }
        end
    }

    pub(crate) fn apply_inline_query_suggestion_word(&mut self) {
        let Some(inline) = self.workspace.query.inline_suggestion.clone() else {
            return;
        };
        let split = Self::inline_suggestion_word_end(&inline);
        if split == 0 || split >= inline.trim_end().len() {
            self.apply_inline_query_suggestion();
            return;
        }

        let (word, rest) = inline.split_at(split);
        if self.insert_at_query_cursor(word) {
            self.workspace.query.inline_suggestion = Some(rest.to_string());
        } else {
            self.clear_query_inline_suggestion();
        }
    }

    pub(crate) fn query_suggestion_cache_key(&self, table: &str) -> Option<String> {
        let key = self.table_key(table);
        if key.is_empty() {
            None
        } else {
            Some(key.to_ascii_lowercase())
        }
    }

    pub(crate) fn query_suggestion_alias_stop_word(word: &str) -> bool {
        word.eq_ignore_ascii_case("where")
            || word.eq_ignore_ascii_case("group")
            || word.eq_ignore_ascii_case("order")
            || word.eq_ignore_ascii_case("limit")
            || word.eq_ignore_ascii_case("offset")
            || word.eq_ignore_ascii_case("having")
            || word.eq_ignore_ascii_case("join")
            || word.eq_ignore_ascii_case("inner")
            || word.eq_ignore_ascii_case("left")
            || word.eq_ignore_ascii_case("right")
            || word.eq_ignore_ascii_case("full")
            || word.eq_ignore_ascii_case("cross")
            || word.eq_ignore_ascii_case("on")
            || word.eq_ignore_ascii_case("union")
            || word.eq_ignore_ascii_case("set")
            || word.eq_ignore_ascii_case("values")
    }

    pub(crate) fn query_table_alias_lookup_for_editor(
        &self,
    ) -> std::collections::HashMap<String, String> {
        let mut lookup = self.table_alias_lookup();
        let tokens = crate::utils::sql_parse::sql_tokens(&self.workspace.query.editor.content());
        let mut index = 0usize;

        while index < tokens.len() {
            let is_table_keyword = matches!(
                tokens.get(index),
                Some(crate::utils::sql_parse::SqlToken::Word(word))
                    if word.eq_ignore_ascii_case("from")
                        || word.eq_ignore_ascii_case("join")
                        || word.eq_ignore_ascii_case("update")
                        || word.eq_ignore_ascii_case("into")
            );
            if !is_table_keyword {
                index += 1;
                continue;
            }

            index += 1;
            if matches!(
                tokens.get(index),
                Some(crate::utils::sql_parse::SqlToken::LParen)
            ) {
                continue;
            }

            let Some(crate::utils::sql_parse::SqlToken::Word(first_part)) = tokens.get(index)
            else {
                continue;
            };
            let mut table_parts = vec![first_part.clone()];
            index += 1;

            if matches!(
                tokens.get(index),
                Some(crate::utils::sql_parse::SqlToken::Dot)
            ) {
                index += 1;
                if let Some(crate::utils::sql_parse::SqlToken::Word(second_part)) =
                    tokens.get(index)
                {
                    table_parts.push(second_part.clone());
                    index += 1;
                }
            }

            let raw_table = table_parts.join(".");
            let resolved_table = self
                .resolve_table_reference(&raw_table)
                .unwrap_or_else(|| raw_table.clone());

            let alias = match tokens.get(index) {
                Some(crate::utils::sql_parse::SqlToken::Word(word))
                    if word.eq_ignore_ascii_case("as") =>
                {
                    index += 1;
                    match tokens.get(index) {
                        Some(crate::utils::sql_parse::SqlToken::Word(alias)) => {
                            index += 1;
                            Some(alias.clone())
                        }
                        _ => None,
                    }
                }
                Some(crate::utils::sql_parse::SqlToken::Word(word))
                    if !Self::query_suggestion_alias_stop_word(word) =>
                {
                    index += 1;
                    Some(word.clone())
                }
                _ => None,
            };

            if let Some(alias) = alias {
                lookup.insert(alias.to_ascii_lowercase(), resolved_table.clone());
            }
        }

        lookup
    }

    pub(crate) fn query_identifier_qualifier(&self, prefix: &QueryPrefix) -> Option<String> {
        let text = self.workspace.query.editor.content();
        let chars: Vec<char> = text.lines().nth(prefix.line)?.chars().collect();
        if prefix.start_col == 0 || prefix.start_col > chars.len() {
            return None;
        }

        let mut dot_index = prefix.start_col;
        while dot_index > 0 && chars[dot_index - 1].is_whitespace() {
            dot_index -= 1;
        }
        if dot_index == 0 || chars[dot_index - 1] != '.' {
            return None;
        }

        let mut ident_end = dot_index - 1;
        while ident_end > 0 && chars[ident_end - 1].is_whitespace() {
            ident_end -= 1;
        }

        let mut ident_start = ident_end;
        while ident_start > 0 && is_query_identifier_char(chars[ident_start - 1]) {
            ident_start -= 1;
        }

        if ident_start == ident_end {
            return None;
        }

        Some(chars[ident_start..ident_end].iter().collect())
    }

    pub(crate) fn query_column_context_table(&self, prefix: &QueryPrefix) -> Option<String> {
        if let Some(qualifier) = self.query_identifier_qualifier(prefix) {
            let lookup = self.query_table_alias_lookup_for_editor();
            if let Some(table) = lookup.get(&qualifier.to_ascii_lowercase()).cloned() {
                return Some(table);
            }

            if let Some(table) = self.resolve_table_reference(&qualifier) {
                return Some(table);
            }
        }

        let query = self.workspace.query.editor.content();
        if let Some(table) = parse_single_table_select(query.trim()) {
            return Some(self.resolve_table_reference(&table).unwrap_or(table));
        }

        if let Some(table) = self.workspace.query.table.clone() {
            return Some(table);
        }

        self.workspace.selected_table.clone()
    }

    pub(crate) fn query_suggestion_columns_for_context(
        &self,
        prefix: &QueryPrefix,
    ) -> Option<&Vec<String>> {
        if !self.schema_autocomplete_available() {
            return None;
        }

        let table = self.query_column_context_table(prefix)?;
        let cache_key = self.query_suggestion_cache_key(&table)?;
        self.workspace
            .query
            .suggestion_columns_cache
            .get(&cache_key)
    }

    pub(crate) fn prefetch_query_suggestion_columns(&mut self) -> Task<Message> {
        if !self.workspace.query.suggestions_open || !self.schema_autocomplete_available() {
            return Task::none();
        }

        let Some(prefix) = self.query_prefix() else {
            return Task::none();
        };
        let Some(table) = self.query_column_context_table(&prefix) else {
            return Task::none();
        };
        let Some(cache_key) = self.query_suggestion_cache_key(&table) else {
            return Task::none();
        };

        if self
            .workspace
            .query
            .suggestion_columns_cache
            .contains_key(&cache_key)
            || self
                .workspace
                .query
                .suggestion_columns_loading
                .contains(&cache_key)
        {
            return Task::none();
        }

        let Some(pool) = self.connections.pool.clone() else {
            return Task::none();
        };
        let database = self.connections.current.database.trim().to_string();
        if database.is_empty() {
            return Task::none();
        }

        self.workspace
            .query
            .suggestion_columns_loading
            .insert(cache_key);
        Task::perform(
            crate::db::metadata::fetch_query_suggestion_columns(pool, database, table.clone()),
            move |result| {
                Message::Workspace(crate::app::features::workspace::Message::Query(
                    crate::app::features::workspace::query::Message::QuerySuggestionColumnsLoaded {
                        table,
                        result,
                    },
                ))
            },
        )
    }

    pub(crate) fn prefetch_table_columns(&mut self, table: &str) -> Task<Message> {
        let Some(cache_key) = self.query_suggestion_cache_key(table) else {
            return Task::none();
        };
        if self
            .workspace
            .query
            .suggestion_columns_cache
            .contains_key(&cache_key)
            || self
                .workspace
                .query
                .suggestion_columns_loading
                .contains(&cache_key)
        {
            return Task::none();
        }
        let Some(pool) = self.connections.pool.clone() else {
            return Task::none();
        };
        let database = self.connections.current.database.trim().to_string();
        if database.is_empty() {
            return Task::none();
        }

        self.workspace
            .query
            .suggestion_columns_loading
            .insert(cache_key);
        let table = table.to_string();
        Task::perform(
            crate::db::metadata::fetch_query_suggestion_columns(pool, database, table.clone()),
            move |result| {
                Message::Workspace(crate::app::features::workspace::Message::Query(
                    crate::app::features::workspace::query::Message::QuerySuggestionColumnsLoaded {
                        table,
                        result,
                    },
                ))
            },
        )
    }

    pub(crate) fn cached_table_columns(&self, table: &str) -> &[String] {
        self.query_suggestion_cache_key(table)
            .and_then(|key| self.workspace.query.suggestion_columns_cache.get(&key))
            .map(Vec::as_slice)
            .unwrap_or_default()
    }

    pub(crate) fn clear_query_suggestion_columns_cache(&mut self) {
        self.workspace.query.suggestion_columns_cache.clear();
        self.workspace.query.suggestion_columns_loading.clear();
    }

    pub(crate) fn retain_query_suggestion_columns_cache(&mut self, known_tables: &HashSet<String>) {
        let mut known_keys = HashSet::new();
        for table in known_tables {
            if let Some(cache_key) = self.query_suggestion_cache_key(table) {
                known_keys.insert(cache_key);
            }
        }

        self.workspace
            .query
            .suggestion_columns_cache
            .retain(|table, _| known_keys.contains(table));
        self.workspace
            .query
            .suggestion_columns_loading
            .retain(|table| known_keys.contains(table));
    }

    pub(crate) fn close_query_suggestions(&mut self) {
        self.workspace.query.suggestions_open = false;
        self.workspace.query.suggestions.clear();
    }

    pub(crate) fn record_query_undo(&mut self, previous: String) {
        if self
            .workspace
            .query
            .undo_stack
            .last()
            .is_some_and(|value| value == &previous)
        {
            return;
        }

        if self.workspace.query.undo_stack.len() >= QUERY_UNDO_LIMIT {
            self.workspace.query.undo_stack.remove(0);
        }
        self.workspace.query.undo_stack.push(previous);
        self.workspace.query.redo_stack.clear();
    }

    pub(crate) fn replace_query_buffer(&mut self, text: &str) {
        self.workspace.query.editor =
            Self::new_query_editor(text, &self.settings.values, self.settings.theme_choice);
        let _ = self
            .workspace
            .query
            .editor
            .set_cursor(text.split('\n').count().saturating_sub(1), usize::MAX);
    }

    pub(crate) fn set_query_text(&mut self, text: &str) {
        self.workspace.query.editor =
            Self::new_query_editor(text, &self.settings.values, self.settings.theme_choice);
        self.workspace.query.inline_suggestion_cache = None;
        self.workspace.query.undo_stack.clear();
        self.workspace.query.redo_stack.clear();
        self.clear_query_inline_suggestion();
        self.close_query_suggestions();
    }

    pub(crate) fn refresh_query_suggestions(&mut self) {
        if !self.workspace.query.suggestions_open {
            self.workspace.query.suggestions.clear();
            return;
        }

        let prefix = self
            .query_prefix()
            .map(|prefix| prefix.prefix)
            .unwrap_or_default();

        self.workspace.query.suggestions = self.build_query_suggestions(&prefix);
        self.workspace.query.suggestion_index = 0;
        if self.workspace.query.suggestions.is_empty() {
            self.close_query_suggestions();
        }
    }

    pub(crate) fn move_query_suggestion(&mut self, forward: bool) {
        let count = self.workspace.query.suggestions.len();
        if count == 0 {
            return;
        }
        let step = if forward { 1 } else { count - 1 };
        self.workspace.query.suggestion_index =
            (self.workspace.query.suggestion_index + step) % count;
    }

    pub(crate) fn build_query_suggestions(&self, prefix: &str) -> Vec<String> {
        let prefix_lower = prefix.trim().to_ascii_lowercase();
        let score_for = |candidate: &str| {
            if prefix_lower.is_empty() {
                Some(0)
            } else {
                let lower = candidate.to_ascii_lowercase();
                if lower.starts_with(&prefix_lower) {
                    Some(0)
                } else {
                    lower.find(&prefix_lower).map(|position| position + 1)
                }
            }
        };

        let mut ranked: Vec<(u8, usize, String)> = Vec::new();
        let mut seen = HashSet::<String>::new();
        let mut push_candidate = |group: u8, value: String, score: usize| {
            let key = value.to_ascii_lowercase();
            if seen.insert(key) {
                ranked.push((group, score, value));
            }
        };

        if let Some(prefix_info) = self.query_prefix()
            && let Some(columns) = self.query_suggestion_columns_for_context(&prefix_info)
        {
            for column in columns {
                if let Some(score) = score_for(column) {
                    push_candidate(0, column.clone(), score);
                }
            }
        }

        if self.schema_autocomplete_available() {
            for table in &self.workspace.explorer.tables {
                let lower = table.to_ascii_lowercase();
                let canonical = self.table_key(table);
                let base_lower = canonical
                    .rsplit_once('.')
                    .map(|(_, base)| base.to_ascii_lowercase());
                let searchable = self.table_search_terms(table).to_ascii_lowercase();

                let score = if prefix_lower.is_empty()
                    || lower.starts_with(&prefix_lower)
                    || base_lower
                        .as_ref()
                        .is_some_and(|base| base.starts_with(&prefix_lower))
                {
                    Some(0)
                } else if let Some(position) = lower.find(&prefix_lower) {
                    Some(position + 1)
                } else {
                    searchable.find(&prefix_lower).map(|position| position + 1)
                };

                if let Some(score) = score {
                    push_candidate(1, table.clone(), score);
                }
            }
        }

        if self.settings.values.sql_keyword_autocomplete_enabled {
            const SQL_HINTS: [&str; 8] = [
                "SELECT",
                "FROM",
                "WHERE",
                "JOIN",
                "ORDER BY",
                "GROUP BY",
                "LIMIT",
                "INSERT INTO",
            ];

            for hint in SQL_HINTS {
                if let Some(score) = score_for(hint) {
                    push_candidate(2, hint.to_string(), score);
                }
            }
        }

        ranked.sort_by(|(group_a, score_a, value_a), (group_b, score_b, value_b)| {
            group_a
                .cmp(group_b)
                .then_with(|| score_a.cmp(score_b))
                .then_with(|| value_a.len().cmp(&value_b.len()))
                .then_with(|| value_a.cmp(value_b))
        });

        ranked
            .into_iter()
            .take(QUERY_SUGGESTION_LIMIT)
            .map(|(_, _, value)| value)
            .collect()
    }

    pub(crate) fn query_prefix(&self) -> Option<QueryPrefix> {
        let (line_index, cursor_column) = self.workspace.query.editor.cursor_position();
        let text = self.workspace.query.editor.content();
        let line_text = text.split('\n').nth(line_index)?.trim_end_matches('\r');
        let chars: Vec<char> = line_text.chars().collect();
        let end_col = cursor_column.min(chars.len());
        let mut start_col = end_col;

        while start_col > 0 && is_query_identifier_char(chars[start_col - 1]) {
            start_col -= 1;
        }

        let prefix = chars[start_col..end_col].iter().collect();

        Some(QueryPrefix {
            line: line_index,
            start_col,
            end_col,
            prefix,
        })
    }

    pub(crate) fn query_line_start_offset(&self, line_index: usize) -> usize {
        self.workspace
            .query
            .editor
            .content()
            .split_inclusive('\n')
            .take(line_index)
            .map(str::len)
            .sum()
    }

    pub(crate) fn apply_query_suggestion(&mut self, index: usize) -> Task<Message> {
        if !self.workspace.query.suggestions_open {
            return Task::none();
        }

        let Some(suggestion) = self.workspace.query.suggestions.get(index).cloned() else {
            return Task::none();
        };

        let Some(prefix) = self.query_prefix() else {
            return Task::none();
        };

        if suggestion.eq_ignore_ascii_case(&prefix.prefix) {
            self.close_query_suggestions();
            return Task::none();
        }

        let query = self.workspace.query.editor.content();
        let mut content = query.clone();
        let line_start = self.query_line_start_offset(prefix.line);
        let Some(line_text) = query.split('\n').nth(prefix.line) else {
            return Task::none();
        };
        let start = line_start + column_to_byte_offset(line_text, prefix.start_col);
        let end = line_start + column_to_byte_offset(line_text, prefix.end_col);
        if start > end || end > content.len() {
            return Task::none();
        }
        content.replace_range(start..end, &suggestion);
        if let Some(edit) = iced_code_editor::compute_text_change(&query, &content) {
            self.workspace.query.editor.apply_lsp_text_edits(&[edit]);
        }
        let new_column = prefix.start_col + suggestion.chars().count();
        let task = self
            .workspace
            .query
            .editor
            .set_cursor(prefix.line, new_column)
            .map(|value| {
                Message::Workspace(crate::app::features::workspace::Message::Query(
                    crate::app::features::workspace::query::Message::QueryAction(value),
                ))
            });
        self.close_query_suggestions();
        task
    }
    pub(crate) fn trigger_query(&self, table: &str, trigger: &TriggerInfo) -> String {
        if self.connections.current.driver == DatabaseDriver::Sqlite {
            let statement = trigger.statement.trim();
            if statement.is_empty() {
                return String::from("Trigger body unavailable.");
            }
            let mut output = statement.to_string();
            if !output.ends_with(';') {
                output.push(';');
            }
            return output;
        }
        if self.connections.current.driver == DatabaseDriver::PostgreSql {
            let statement = trigger.statement.trim();
            if statement.is_empty() {
                return String::from("Trigger body unavailable.");
            }
            let mut output = statement.to_string();
            if !output.ends_with(';') {
                output.push(';');
            }
            return output;
        }
        let trigger_name = escape_mysql_identifier(&trigger.name);
        let table_name = escape_mysql_identifier(table);
        let definer = format_definer(&trigger.definer);
        let mut output = format!("DROP TRIGGER IF EXISTS `{}`;\n\n", trigger_name);
        if let Some(definer) = definer {
            let _ = writeln!(
                &mut output,
                "CREATE {}\nTRIGGER `{}`\n{} {} ON `{}`",
                definer, trigger_name, trigger.timing, trigger.event, table_name
            );
        } else {
            let _ = writeln!(
                &mut output,
                "CREATE TRIGGER `{}`\n{} {} ON `{}`",
                trigger_name, trigger.timing, trigger.event, table_name
            );
        }
        output.push_str("FOR EACH ROW\n");
        let statement = trigger.statement.trim();
        output.push_str(statement);
        if !statement.ends_with(';') {
            output.push(';');
        }
        output
    }

    pub(crate) fn run_readonly_query_text(&mut self, query: String) -> Task<Message> {
        self.run_query_text_inner(query, true)
    }

    pub(crate) fn run_query_text(&mut self, query: String) -> Task<Message> {
        self.run_query_text_inner(query, false)
    }

    pub(crate) fn run_query_text_inner(&mut self, query: String, read_only: bool) -> Task<Message> {
        if self.workspace.query.running {
            return Task::none();
        }
        let Some(pool) = self.connections.pool.clone() else {
            self.workspace.query.cancel_flag = None;
            self.workspace.query.error = Some(String::from("Not connected."));
            return Task::none();
        };
        if query.trim().is_empty() {
            self.workspace.query.cancel_flag = None;
            self.workspace.query.error = Some(String::from("Query is empty."));
            return Task::none();
        }
        let trimmed = query.trim();
        let query_table = parse_single_table_select(trimmed)
            .map(|table| self.resolve_table_reference(&table).unwrap_or(table));
        self.workspace.explorer.selected_postgres_object = None;
        self.workspace.query.running = true;
        self.workspace.query.error = None;
        self.workspace.results.apply_error = None;
        self.workspace.results.apply_message = None;
        self.workspace.query.table_query = None;
        self.workspace.query.editable_query = query_table.as_ref().map(|_| trimmed.to_string());
        self.workspace.query.table = query_table;
        self.workspace.query.table_has_next_page = false;
        self.workspace.query.last_query_was_table = false;
        self.clear_pending_edits_state();
        self.workspace.results.editing_cell = None;
        self.workspace.results.selected_cell = None;
        self.clear_row_selection();
        self.close_text_modal();
        self.close_query_suggestions();
        self.workspace.results.current = None;
        self.workspace.results.column_widths.clear();
        self.workspace.results.column_resize = None;
        self.push_history(query.clone());
        let database = self.current_database();
        let cancel_flag = Arc::new(AtomicBool::new(false));
        self.workspace.query.cancel_flag = Some(cancel_flag.clone());
        crate::constants::ACTIVE_QUERY_TIMEOUT_SECS.store(
            self.settings.values.query_timeout_secs,
            std::sync::atomic::Ordering::Relaxed,
        );
        let timeout_secs = self.settings.values.query_timeout_secs;
        Task::perform(
            async move {
                let query_future = async move {
                    if read_only {
                        crate::db::query::run_readonly_query(
                            pool,
                            database,
                            query,
                            Some(cancel_flag),
                            Some(ADHOC_QUERY_MAX_ROWS),
                        )
                        .await
                        .map(|output| vec![output])
                    } else {
                        crate::db::query::run_multi_query_with_control(
                            pool,
                            database,
                            query,
                            Some(cancel_flag),
                            Some(ADHOC_QUERY_MAX_ROWS),
                        )
                        .await
                    }
                };
                if timeout_secs > 0 {
                    match tokio::time::timeout(
                        std::time::Duration::from_secs(timeout_secs),
                        query_future,
                    )
                    .await
                    {
                        Ok(result) => result,
                        Err(_) => Err(crate::i18n::tr_with(
                            "Query timed out after {seconds} seconds.",
                            &[("{seconds}", &timeout_secs.to_string())],
                        )),
                    }
                } else {
                    query_future.await
                }
            },
            |value| {
                Message::Workspace(crate::app::features::workspace::Message::Query(
                    crate::app::features::workspace::query::Message::QueryScriptFinished(value),
                ))
            },
        )
    }

    pub(crate) fn select_table(
        &mut self,
        table: String,
        page: usize,
        push_history: bool,
    ) -> Task<Message> {
        self.open_folder_for_table(&table);
        self.leave_query_tab_context();
        let scroll_task = if self.settings.values.auto_scroll_sidebar_to_selected_table {
            self.scroll_sidebar_to_table(&table)
        } else {
            Task::none()
        };
        let triggers_task = self.load_triggers_for_table(table.clone());
        let relations_task = self.load_relations_for_table(table.clone());
        let table_task = self.start_table_query(table, page, push_history);
        Task::batch(vec![table_task, triggers_task, relations_task, scroll_task])
    }

    pub(crate) fn start_table_query(
        &mut self,
        table: String,
        page: usize,
        push_history: bool,
    ) -> Task<Message> {
        self.deactivate_diagram();
        self.ensure_table_tab(&table);

        if let Some(previous) = self.workspace.selected_table.as_ref()
            && previous != &table
        {
            self.workspace
                .table_inactive_since
                .insert(previous.clone(), std::time::Instant::now());
        }
        self.workspace.explorer.selected_postgres_object = None;
        self.workspace.selected_table = Some(table.clone());
        self.workspace.table_inactive_since.remove(&table);
        self.prune_inactive_tab_memory();
        let info_task = if self.workspace.explorer.table_info_sidebar_open {
            self.request_table_info(table.clone())
        } else {
            Task::none()
        };
        self.workspace.table_pages.insert(table.clone(), page);

        let (display_query, fetch_query) = self.table_queries(&table, page);
        self.set_query_text(&display_query);
        self.workspace.query.table_query = Some(display_query.trim().to_string());
        self.workspace.query.editable_query = if self.table_is_row_editable(&table) {
            Some(display_query.trim().to_string())
        } else {
            None
        };
        self.workspace.query.table = Some(table.clone());
        self.workspace.query.error = None;
        self.workspace.results.apply_error = None;
        self.workspace.results.apply_message = None;
        self.clear_pending_edits_state();
        self.workspace.results.editing_cell = None;
        if !self.workspace.results.preserve_viewport {
            self.workspace.results.selected_cell = None;
            self.clear_row_selection();
        }
        self.workspace.results.current = None;
        if !self.workspace.results.preserve_viewport {
            self.workspace.results.vertical_viewport = None;
            self.workspace.results.horizontal_viewport = None;
        }
        self.workspace.results.column_widths.clear();
        self.workspace.results.column_resize = None;
        self.workspace.query.table_has_next_page = false;

        let cache_key = (table.clone(), page);
        if let Some(cache) = self.get_table_cache_entry(&cache_key) {
            self.workspace.results.current = Some(Arc::clone(&cache.results));
            self.workspace.results.column_widths =
                self.initial_column_widths(&cache.results.columns);
            self.workspace.query.table_has_next_page = cache.has_next_page;
            self.workspace.query.last_query_was_table = true;
            self.workspace.query.cancel_flag = None;
            if !self.workspace.results.preserve_viewport {
                self.workspace.results.selected_cell = None;
                self.clear_row_selection();
            }
            return info_task;
        }

        if push_history {
            self.push_history(display_query.clone());
        }

        if self.workspace.query.running {
            return Task::none();
        }

        let Some(pool) = self.connections.pool.clone() else {
            self.workspace.query.cancel_flag = None;
            self.workspace.query.error = Some(String::from("Not connected."));
            return Task::none();
        };

        self.workspace.query.running = true;
        self.workspace.query.last_query_was_table = true;
        let database = self.current_database();
        let cancel_flag = Arc::new(AtomicBool::new(false));
        self.workspace.query.cancel_flag = Some(cancel_flag.clone());
        Task::batch(vec![
            Task::perform(
                run_table_query_with_control(pool, database, table, fetch_query, Some(cancel_flag)),
                |value| {
                    Message::Workspace(crate::app::features::workspace::Message::Query(
                        crate::app::features::workspace::query::Message::QueryFinished(value),
                    ))
                },
            ),
            info_task,
        ])
    }

    pub(crate) fn push_history(&mut self, query: String) {
        let trimmed = query.trim().to_string();
        if trimmed.is_empty() {
            return;
        }

        if self.settings.values.history_limit == 0 {
            self.workspace.query.history.clear();
            self.workspace.query.history_times.clear();
            return;
        }

        if let Some(index) = self
            .workspace
            .query
            .history
            .iter()
            .position(|entry| entry == &trimmed)
        {
            self.workspace.query.history.remove(index);
            if index < self.workspace.query.history_times.len() {
                self.workspace.query.history_times.remove(index);
            }
        }
        self.workspace.query.history.insert(0, trimmed);
        self.workspace
            .query
            .history_times
            .insert(0, crate::utils::format::clock_label());
        if self.workspace.query.history.len() > self.settings.values.history_limit {
            self.workspace
                .query
                .history
                .truncate(self.settings.values.history_limit);
            self.workspace
                .query
                .history_times
                .truncate(self.settings.values.history_limit);
        }
    }

    pub(crate) fn push_saved_query(&mut self, query: String) {
        let trimmed = query.trim().to_string();
        if trimmed.is_empty() {
            return;
        }

        self.settings
            .values
            .saved_queries
            .retain(|entry| entry != &trimmed);
        self.settings.values.saved_queries.insert(0, trimmed);
    }
}
