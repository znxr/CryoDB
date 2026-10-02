use crate::app::core::App;
use crate::app::features::workspace::query::Message as QueryMessage;
use crate::app::features::workspace::tabs::TabEntry;
use crate::app::message::Message;
use crate::app::types::{QueryEditorResizeDrag, ToastLevel};
use crate::constants::AI_CHAT_MAX_AUTO_RETRIES;
use crate::db::metadata::{fetch_databases, fetch_tables};
use crate::db::query::run_query;
use crate::model::settings::ShortcutBinding;
use crate::model::table::{ColumnKind, QueryOutput, ResultSet};
use crate::ui::ids::{results_horizontal_scroll_id, results_vertical_scroll_id};
use crate::utils::sql_parse::schema_metadata_refresh_from_sql;
use iced::widget::text_editor;
use iced::{Task, keyboard};
use std::sync::Arc;

impl App {
    pub(crate) fn update_query(&mut self, message: QueryMessage) -> Task<Message> {
        match message {
            QueryMessage::ToggleHistoryPanel => {
                self.workspace.query.history_panel_expanded =
                    !self.workspace.query.history_panel_expanded;
                Task::none()
            }
            QueryMessage::HistoryViewModeSelected(mode) => {
                self.workspace.query.history_view_mode = mode;
                Task::none()
            }
            QueryMessage::RelationsSelected(table) => {
                if self.workspace.query.running {
                    return Task::none();
                }
                if self.workspace.selected_table.as_deref() != Some(table.as_str()) {
                    return Task::none();
                }
                let Some(pool) = self.connections.pool.clone() else {
                    self.workspace.query.error = Some(String::from("Not connected."));
                    return Task::none();
                };

                let relations_query = self.relations_query_for(&table);
                self.workspace.query.running = true;
                self.workspace.query.error = None;
                self.workspace.results.apply_error = None;
                self.workspace.results.apply_message = None;
                self.workspace.query.table_query = None;
                self.workspace.query.editable_query = None;
                self.workspace.query.table = None;
                self.workspace.query.table_has_next_page = false;
                self.workspace.query.last_query_was_table = false;
                self.clear_pending_edits_state();
                self.workspace.results.editing_cell = None;
                self.workspace.results.selected_cell = None;
                self.clear_row_selection();
                self.close_text_modal();
                self.workspace.results.current = None;
                self.workspace.results.vertical_viewport = None;
                self.workspace.results.horizontal_viewport = None;
                self.workspace.results.column_widths.clear();
                self.workspace.results.column_resize = None;
                self.workspace.explorer.selected_trigger = None;
                self.set_query_text(&relations_query);
                self.push_history(relations_query.clone());

                let database = self.current_database();
                Task::perform(run_query(pool, database, relations_query), |value| {
                    Message::Workspace(crate::app::features::workspace::Message::Query(
                        crate::app::features::workspace::query::Message::QueryFinished(value),
                    ))
                })
            }
            QueryMessage::TablePageNext => {
                if self.workspace.query.running
                    || !self.is_table_query_active()
                    || !self.workspace.query.last_query_was_table
                    || !self.workspace.query.table_has_next_page
                {
                    return Task::none();
                }
                let Some(table) = self.workspace.selected_table.clone() else {
                    return Task::none();
                };
                let page = self.workspace.table_pages.get(&table).copied().unwrap_or(0) + 1;
                self.start_table_query(table, page, false)
            }
            QueryMessage::TablePagePrev => {
                if self.workspace.query.running || !self.is_table_query_active() {
                    return Task::none();
                }
                let Some(table) = self.workspace.selected_table.clone() else {
                    return Task::none();
                };
                let page = self.workspace.table_pages.get(&table).copied().unwrap_or(0);
                if page == 0 {
                    return Task::none();
                }
                self.start_table_query(table, page - 1, false)
            }
            QueryMessage::TablePageFirst => {
                if self.workspace.query.running
                    || !self.is_table_query_active()
                    || !self.workspace.query.last_query_was_table
                {
                    return Task::none();
                }
                let Some(table) = self.workspace.selected_table.clone() else {
                    return Task::none();
                };
                if self.workspace.table_pages.get(&table).copied().unwrap_or(0) == 0 {
                    return Task::none();
                }
                self.start_table_query(table, 0, false)
            }
            QueryMessage::TablePageLast => {
                if self.workspace.query.running
                    || !self.is_table_query_active()
                    || !self.workspace.query.last_query_was_table
                    || !self.workspace.query.table_has_next_page
                {
                    return Task::none();
                }
                let Some(table) = self.workspace.selected_table.clone() else {
                    return Task::none();
                };
                let Some(pool) = self.connections.pool.clone() else {
                    return Task::none();
                };
                let database = self.current_database();
                let count_query = self.table_count_query(&table);
                Task::perform(
                    crate::db::query::run_query(pool, database, count_query),
                    move |result| {
                        Message::Workspace(crate::app::features::workspace::Message::Query(
                            crate::app::features::workspace::query::Message::TableLastPageResolved(
                                table.clone(),
                                result,
                            ),
                        ))
                    },
                )
            }
            QueryMessage::TableLastPageResolved(table, result) => {
                if self.workspace.selected_table.as_deref() != Some(table.as_str())
                    || self.workspace.query.running
                    || !self.is_table_query_active()
                {
                    return Task::none();
                }
                let total = match result {
                    Ok(QueryOutput::Rows(set)) => set
                        .rows
                        .first()
                        .and_then(|row| row.first())
                        .and_then(|value| value.trim().parse::<u64>().ok()),
                    Ok(QueryOutput::Affected(_)) => None,
                    Err(error) => {
                        self.push_toast(ToastLevel::Error, error);
                        return Task::none();
                    }
                };
                let Some(total) = total.filter(|total| *total > 0) else {
                    return Task::none();
                };
                let last_page = self.last_page_for_row_count(total);
                if last_page == self.workspace.table_pages.get(&table).copied().unwrap_or(0) {
                    return Task::none();
                }
                self.start_table_query(table, last_page, false)
            }
            QueryMessage::HistorySelected(query) => {
                self.set_query_text(&query);
                self.workspace.query.table_query = None;
                self.workspace.query.editable_query = None;
                self.workspace.query.table = None;
                self.workspace.selected_table = None;
                self.workspace.explorer.selected_postgres_object = None;
                self.workspace.explorer.table_triggers.clear();
                self.workspace.explorer.triggers_table = None;
                self.workspace.explorer.selected_trigger = None;
                self.workspace.explorer.table_relations.clear();
                self.workspace.explorer.relations_table = None;
                self.workspace.query.table_has_next_page = false;
                self.workspace.query.last_query_was_table = false;
                self.workspace.results.editing_cell = None;
                self.workspace.results.selected_cell = None;
                self.clear_row_selection();
                self.close_text_modal();
                self.workspace.results.apply_error = None;
                self.workspace.results.apply_message = None;
                Task::none()
            }
            QueryMessage::ClearQueryHistory => {
                self.workspace.query.history.clear();
                Task::none()
            }
            QueryMessage::SaveCurrentQuery => {
                self.push_saved_query(self.workspace.query.editor.content());
                self.persist_settings_store();
                self.push_toast(ToastLevel::Success, "Saved query");
                Task::none()
            }
            QueryMessage::SavedQuerySelected(query) => {
                if self.workspace.query.running || self.workspace.results.applying_changes {
                    return Task::none();
                }
                let state = self.blank_query_state();
                self.open_new_query_tab(state);
                self.set_query_text(&query);
                self.run_query_text(query)
            }
            QueryMessage::DeleteSavedQuery(index) => {
                if index < self.settings.values.saved_queries.len() {
                    self.settings.values.saved_queries.remove(index);
                    self.persist_settings_store();
                }
                Task::none()
            }
            QueryMessage::CopySavedQuery(query) => iced::clipboard::write(query),
            QueryMessage::ClearSavedQueries => {
                self.settings.values.saved_queries.clear();
                self.persist_settings_store();
                Task::none()
            }
            QueryMessage::QueryEditorResizeDragStarted => {
                self.shell.editor_ratio_pinned = true;
                let start_cursor_y = self
                    .shell
                    .global_cursor
                    .map(|position| position.y)
                    .unwrap_or(0.0);
                self.workspace.query.editor_resize_drag = Some(QueryEditorResizeDrag {
                    start_cursor_y,
                    start_ratio: self.shell.editor_ratio,
                });
                Task::none()
            }
            QueryMessage::QueryAction(action) => {
                if matches!(
                    action,
                    iced_code_editor::Message::Tab | iced_code_editor::Message::Enter
                ) || self.workspace.query.inline_suggestion.is_some()
                    && matches!(
                        action,
                        iced_code_editor::Message::ArrowKey(
                            iced_code_editor::ArrowDirection::Right,
                            _
                        )
                    )
                    || self.workspace.query.suggestions_open
                        && matches!(
                            action,
                            iced_code_editor::Message::ArrowKey(
                                iced_code_editor::ArrowDirection::Up
                                    | iced_code_editor::ArrowDirection::Down,
                                _
                            )
                        )
                {
                    self.workspace.query.editor_pending_key_action = Some(action);
                    Task::none()
                } else {
                    self.apply_query_editor_action(action)
                }
            }
            QueryMessage::RunQuery => {
                if self.workspace.query.running {
                    return Task::none();
                }
                self.ensure_query_tab_for_run();
                self.run_query_text(self.workspace.query.editor.content())
            }
            QueryMessage::CancelQuery => {
                if let Some(flag) = self.workspace.query.cancel_flag.as_ref() {
                    flag.store(true, std::sync::atomic::Ordering::Relaxed);
                }
                Task::none()
            }
            QueryMessage::ToggleStructureView => {
                if self.workspace.query.running {
                    return Task::none();
                }
                let Some(table) = self.workspace.selected_table.clone() else {
                    return Task::none();
                };
                self.workspace.explorer.selected_trigger = None;

                if self.is_structure_view_active() {
                    let page = self.workspace.table_pages.get(&table).copied().unwrap_or(0);
                    return self.start_table_query(table, page, false);
                }

                let Some(pool) = self.connections.pool.clone() else {
                    self.workspace.query.error = Some(String::from("Not connected."));
                    return Task::none();
                };

                let structure_query = self.structure_query_for(&table);
                self.workspace.query.running = true;
                self.workspace.query.error = None;
                self.workspace.results.apply_error = None;
                self.workspace.results.apply_message = None;
                self.workspace.query.table_has_next_page = false;
                self.workspace.query.last_query_was_table = false;
                self.workspace.query.editable_query = None;
                self.workspace.query.table = None;
                self.clear_pending_edits_state();
                self.workspace.results.editing_cell = None;
                self.workspace.results.selected_cell = None;
                self.clear_row_selection();
                self.workspace.results.current = None;
                self.workspace.results.column_widths.clear();
                self.workspace.results.column_resize = None;
                self.set_query_text(&structure_query);
                self.push_history(structure_query.clone());

                let database = self.current_database();
                Task::perform(run_query(pool, database, structure_query), |value| {
                    Message::Workspace(crate::app::features::workspace::Message::Query(
                        crate::app::features::workspace::query::Message::QueryFinished(value),
                    ))
                })
            }
            QueryMessage::NewQuery => {
                if self.workspace.query.running || self.workspace.results.applying_changes {
                    return Task::none();
                }
                let state = self.blank_query_state();
                self.open_new_query_tab(state);
                let scroll = match self.workspace.tabs.active_query_tab {
                    Some(index) => self.scroll_tab_entry_into_view(&TabEntry::Query(index)),
                    None => Task::none(),
                };
                self.focus_query_editor();
                scroll
            }
            QueryMessage::ClearResults => {
                self.workspace.results.current = None;
                self.clear_pending_edits_state();
                self.workspace.results.editing_cell = None;
                self.workspace.results.selected_cell = None;
                self.clear_row_selection();
                self.close_text_modal();
                self.workspace.results.vertical_viewport = None;
                self.workspace.results.horizontal_viewport = None;
                self.workspace.results.column_widths.clear();
                self.workspace.results.column_resize = None;
                self.workspace.query.error = None;
                self.workspace.results.apply_error = None;
                self.workspace.results.apply_message = None;
                self.workspace.query.table_has_next_page = false;
                self.workspace.query.last_query_was_table = false;
                self.workspace.query.editable_query = None;
                self.workspace.query.table = None;
                Task::none()
            }
            QueryMessage::QueryScriptFinished(result) => {
                let mut outputs = match result {
                    Ok(outputs) => outputs,
                    Err(error) => {
                        return self.update(Message::Workspace(
                            crate::app::features::workspace::Message::Query(
                                crate::app::features::workspace::query::Message::QueryFinished(
                                    Err(error),
                                ),
                            ),
                        ));
                    }
                };
                let result_sets = outputs
                    .iter()
                    .filter_map(|output| match output {
                        QueryOutput::Rows(results) => Some(Arc::new(results.clone())),
                        QueryOutput::Affected(_) => None,
                    })
                    .collect::<Vec<_>>();
                if result_sets.len() > 1 {
                    self.workspace.results.sets = result_sets;
                    self.workspace.results.active_set = 0;
                    self.workspace.results.sets_pending = true;
                    return self.update(Message::Workspace(
                        crate::app::features::workspace::Message::Query(
                            crate::app::features::workspace::query::Message::QueryFinished(Ok(
                                QueryOutput::Rows((*self.workspace.results.sets[0]).clone()),
                            )),
                        ),
                    ));
                }
                self.workspace.results.sets.clear();
                self.workspace.results.active_set = 0;
                self.workspace.results.sets_pending = false;
                if let Some(results) = result_sets.into_iter().next() {
                    return self.update(Message::Workspace(
                        crate::app::features::workspace::Message::Query(
                            crate::app::features::workspace::query::Message::QueryFinished(Ok(
                                QueryOutput::Rows((*results).clone()),
                            )),
                        ),
                    ));
                }
                self.update(Message::Workspace(
                    crate::app::features::workspace::Message::Query(
                        crate::app::features::workspace::query::Message::QueryFinished(Ok(outputs
                            .pop()
                            .expect("query output"))),
                    ),
                ))
            }
            QueryMessage::QueryFinished(result) => {
                if !self.connections.connected {
                    self.workspace.query.running = false;
                    self.workspace.query.cancel_flag = None;
                    self.ai.chat_auto_run_pending = false;
                    return Task::none();
                }
                self.workspace.query.running = false;
                self.workspace.query.cancel_flag = None;
                self.workspace.results.apply_error = None;
                self.workspace.results.apply_message = None;
                let preserve_view = self.workspace.results.preserve_viewport;
                let keep_query_result_sets =
                    std::mem::take(&mut self.workspace.results.sets_pending);
                self.workspace.results.preserve_viewport = false;
                if !preserve_view {
                    self.workspace.results.vertical_viewport = None;
                    self.workspace.results.horizontal_viewport = None;
                    self.clear_row_selection();
                }
                let mut post_refresh_tasks = Vec::new();
                let is_ok = result.is_ok();
                let chat_run_succeeded =
                    is_ok && std::mem::take(&mut self.ai.chat_auto_run_pending);
                if is_ok {
                    self.ai.chat_auto_retries = 0;
                }
                match result {
                    Ok(QueryOutput::Rows(mut result_set)) => {
                        if self.workspace.query.last_query_was_table
                            && result_set.rows.len() > self.settings.values.table_query_limit
                        {
                            self.workspace.query.table_has_next_page = true;
                            result_set
                                .rows
                                .truncate(self.settings.values.table_query_limit);
                        } else {
                            self.workspace.query.table_has_next_page = false;
                        }
                        let shared = Arc::new(result_set);
                        if self.workspace.query.last_query_was_table
                            && let Some(table) = self.workspace.selected_table.clone()
                        {
                            let page = self.workspace.table_pages.get(&table).copied().unwrap_or(0);
                            self.insert_table_cache_entry(
                                table,
                                page,
                                Arc::clone(&shared),
                                self.workspace.query.table_has_next_page,
                            );
                        }
                        self.workspace.results.column_widths =
                            self.initial_column_widths(&shared.columns);
                        self.workspace.results.current = Some(Arc::clone(&shared));
                        if keep_query_result_sets && self.workspace.results.sets.len() > 1 {
                            self.workspace.results.active_set = self
                                .workspace
                                .results
                                .active_set
                                .min(self.workspace.results.sets.len() - 1);
                            self.workspace.results.sets[self.workspace.results.active_set] = shared;
                        } else {
                            self.workspace.results.sets.clear();
                            self.workspace.results.active_set = 0;
                        }
                        self.clear_pending_edits_state();
                        if !preserve_view {
                            self.workspace.results.selected_cell = None;
                        }
                        self.workspace.query.error = None;
                        self.workspace.results.column_resize = None;
                        if preserve_view {
                            if let Some(vv) = self.workspace.results.vertical_viewport {
                                post_refresh_tasks.push(iced::widget::operation::scroll_to(
                                    results_vertical_scroll_id(),
                                    vv.absolute_offset(),
                                ));
                            }
                            if let Some(hv) = self.workspace.results.horizontal_viewport {
                                post_refresh_tasks.push(iced::widget::operation::scroll_to(
                                    results_horizontal_scroll_id(),
                                    hv.absolute_offset(),
                                ));
                            }
                        }
                    }
                    Ok(QueryOutput::Affected(affected)) => {
                        self.workspace.results.sets.clear();
                        self.workspace.results.active_set = 0;
                        self.workspace.query.table_has_next_page = false;
                        self.workspace.query.last_query_was_table = false;
                        self.workspace.query.editable_query = None;
                        self.workspace.query.table = None;
                        self.workspace.results.column_widths =
                            vec![self.workspace.results.column_width];
                        self.workspace.results.current = Some(Arc::new(ResultSet {
                            columns: vec![String::from("Rows affected")],
                            column_kinds: vec![ColumnKind::Integer],
                            column_nullable: vec![false],
                            rows: vec![vec![affected.to_string()]],
                        }));
                        self.workspace.results.selected_cell = None;
                        self.clear_pending_edits_state();
                        self.workspace.query.error = None;
                        self.workspace.results.column_resize = None;
                    }
                    Err(error) => {
                        self.workspace.results.current = None;
                        self.workspace.results.sets.clear();
                        self.workspace.results.active_set = 0;
                        self.clear_pending_edits_state();
                        self.workspace.results.selected_cell = None;
                        self.workspace.results.column_widths.clear();
                        self.workspace.results.column_resize = None;
                        self.workspace.query.error = Some(error.clone());
                        self.workspace.query.table_has_next_page = false;
                        self.workspace.query.last_query_was_table = false;
                        self.workspace.query.editable_query = None;
                        self.workspace.query.table = None;

                        if std::mem::take(&mut self.ai.chat_auto_run_pending)
                            && self.ai.chat_auto_retries < AI_CHAT_MAX_AUTO_RETRIES
                            && !self.ai.chat_sending
                        {
                            self.ai.chat_auto_retries += 1;
                            self.ai.chat_input = text_editor::Content::with_text(&format!(
                                "That query failed with: {error}\nFix it and return the corrected query."
                            ));
                            post_refresh_tasks.push(
                                self.update(Message::Ai(
                                    crate::app::features::ai::Message::ChatSend,
                                )),
                            );
                        }
                    }
                }
                if is_ok {
                    let query_text = self.workspace.query.editor.content();
                    let (refresh_tables, refresh_databases) =
                        schema_metadata_refresh_from_sql(&query_text);
                    if refresh_databases
                        && !self.connections.loading_databases
                        && let Some(pool) = self.connections.pool.clone()
                    {
                        self.connections.loading_databases = true;
                        self.connections.database_error = None;
                        post_refresh_tasks.push(Task::perform(fetch_databases(pool), |value| {
                            Message::Connections(
                                crate::app::features::connections::Message::DatabasesLoaded(value),
                            )
                        }));
                    }
                    if refresh_tables
                        && !self.connections.loading_tables
                        && let (Some(pool), Some(database)) =
                            (self.connections.pool.clone(), self.current_database())
                    {
                        self.connections.loading_tables = true;
                        self.connections.table_error = None;
                        post_refresh_tasks.push(Task::perform(
                            fetch_tables(pool, database),
                            |value| {
                                Message::Connections(
                                    crate::app::features::connections::Message::TablesLoaded(value),
                                )
                            },
                        ));
                    }
                }
                if let Some(prompt) = self.ai.chat_pending_sample.take() {
                    self.ai.chat_auto_run_pending = false;
                    let summary = self
                        .chat_result_summary()
                        .unwrap_or_else(|| String::from("The command returned nothing."));
                    post_refresh_tasks.push(self.send_chat_note(
                        Some(String::from("Lookup results shared with the AI")),
                        format!(
                            "Result of the command you asked for:\n{summary}\n\nNow answer: {prompt}"
                        ),
                    ));
                    return Task::batch(post_refresh_tasks);
                }

                if chat_run_succeeded && let Some(summary) = self.chat_result_summary() {
                    let rows = self
                        .workspace
                        .results
                        .current
                        .as_ref()
                        .map(|results| results.rows.len())
                        .unwrap_or(0);
                    post_refresh_tasks.push(self.send_chat_note(
                        Some(crate::i18n::tr_with("Query ran, {rows} rows shared with the AI", &[("{rows}", &rows.to_string())])),
                        format!(
                            "The query ran. Here is what it returned:\n{summary}\n\nAnswer my previous question using these results. Do not repeat the query."
                        ),
                    ));
                }

                if post_refresh_tasks.is_empty() {
                    Task::none()
                } else {
                    Task::batch(post_refresh_tasks)
                }
            }
            QueryMessage::QuerySuggestionColumnsLoaded { table, result } => {
                if let Some(cache_key) = self.query_suggestion_cache_key(&table) {
                    self.workspace
                        .query
                        .suggestion_columns_loading
                        .remove(&cache_key);

                    if let Ok(columns) = result {
                        self.workspace
                            .query
                            .suggestion_columns_cache
                            .insert(cache_key, columns);
                        if self.workspace.query.suggestions_open {
                            self.refresh_query_suggestions();
                        }
                    }
                }
                Task::none()
            }
            QueryMessage::InlineQuerySuggestionReady {
                request_id,
                anchor,
                anchor_offset,
                result,
            } => {
                if request_id != self.workspace.query.inline_suggestion_request_id {
                    return Task::none();
                }

                self.workspace.query.inline_suggestion_loading = false;

                let Some(typed) = self.inline_suggestion_typed_since(&anchor, anchor_offset) else {
                    self.workspace.query.inline_suggestion = None;
                    self.workspace.query.inline_suggestion_pending = true;
                    return Task::none();
                };

                self.workspace.query.inline_suggestion = match result {
                    Ok(suggestion) => {
                        self.ai.query_inline_suggestion_error = None;
                        let normalized = App::normalize_inline_ai_suggestion(&anchor, &suggestion);
                        self.workspace.query.inline_suggestion_cache = normalized
                            .clone()
                            .map(|suggestion| (anchor.clone(), anchor_offset, suggestion));
                        normalized
                            .and_then(|suggestion| {
                                suggestion.strip_prefix(&typed).map(str::to_string)
                            })
                            .filter(|suggestion| !suggestion.is_empty())
                    }
                    Err(error) => {
                        self.ai.query_inline_suggestion_error = Some(error);
                        None
                    }
                };
                self.workspace.query.inline_suggestion_pending =
                    self.workspace.query.inline_suggestion.is_none() && !typed.is_empty();

                Task::none()
            }
            QueryMessage::CopyQueryError => {
                if let Some(error) = self.workspace.query.error.clone() {
                    iced::clipboard::write(error)
                } else {
                    Task::none()
                }
            }
            QueryMessage::QueryEditorCapturedKeyPressed(event) => {
                if !self.query_editor_focused() || self.has_active_modal() {
                    return Task::none();
                }

                let keyboard::Event::KeyPressed { key, modifiers, .. } = event else {
                    return Task::none();
                };
                let pending_action = self.workspace.query.editor_pending_key_action.take();
                if self.workspace.query.suggestions_open {
                    return self.navigate_query_suggestions(&key);
                }
                if ShortcutBinding::primary_modifier_only(modifiers)
                    && matches!(&key, keyboard::Key::Character(ch) if ch.eq_ignore_ascii_case("z"))
                {
                    if self.workspace.query.editor_last_action_edited {
                        self.workspace.query.editor_last_action_edited = false;
                        return Task::none();
                    }
                    if !self.workspace.query.undo_stack.is_empty() {
                        return self.update(Message::Workspace(
                            crate::app::features::workspace::Message::Query(
                                crate::app::features::workspace::query::Message::UndoQuery,
                            ),
                        ));
                    }
                }
                if self.workspace.query.inline_suggestion.is_some()
                    && matches!(key, keyboard::Key::Named(keyboard::key::Named::ArrowRight))
                    && ShortcutBinding::primary_modifier_only(modifiers)
                {
                    return self.update(Message::Workspace(crate::app::features::workspace::Message::Query(crate::app::features::workspace::query::Message::AcceptInlineQuerySuggestionWord)));
                }
                let message = if self
                    .settings
                    .values
                    .run_query_shortcut
                    .matches_key(&key, modifiers)
                {
                    Some(Message::Workspace(
                        crate::app::features::workspace::Message::Query(
                            crate::app::features::workspace::query::Message::RunQuery,
                        ),
                    ))
                } else if self
                    .settings
                    .values
                    .run_selection_shortcut
                    .matches_key(&key, modifiers)
                {
                    Some(Message::Workspace(
                        crate::app::features::workspace::Message::Query(
                            crate::app::features::workspace::query::Message::RunSelection,
                        ),
                    ))
                } else if self.workspace.query.inline_suggestion.is_some()
                    && (matches!(key, keyboard::Key::Named(keyboard::key::Named::Tab))
                        || self
                            .settings
                            .values
                            .autocomplete_tables_shortcut
                            .matches_key(&key, modifiers))
                {
                    Some(Message::Workspace(crate::app::features::workspace::Message::Query(crate::app::features::workspace::query::Message::AcceptInlineQuerySuggestion)))
                } else if self.query_autocomplete_available()
                    && self
                        .settings
                        .values
                        .autocomplete_tables_shortcut
                        .matches_key(&key, modifiers)
                {
                    Some(Message::Workspace(
                        crate::app::features::workspace::Message::Query(
                            crate::app::features::workspace::query::Message::OpenQuerySuggestions,
                        ),
                    ))
                } else if self.workspace.query.inline_suggestion.is_some()
                    && matches!(key, keyboard::Key::Named(keyboard::key::Named::Escape))
                {
                    Some(Message::Workspace(crate::app::features::workspace::Message::Query(crate::app::features::workspace::query::Message::DismissInlineQuerySuggestion)))
                } else {
                    None
                };
                if let Some(message) = message {
                    return self.update(message);
                }
                pending_action
                    .map(|action| self.apply_query_editor_action(action))
                    .unwrap_or_else(Task::none)
            }
            QueryMessage::RunSelection => {
                if self.workspace.query.running {
                    return Task::none();
                }
                let full = self.workspace.query.editor.content();
                let fallback = self
                    .query_cursor_byte_offset()
                    .and_then(|offset| crate::utils::sql_parse::statement_at_offset(&full, offset))
                    .map(str::to_string);
                iced::clipboard::read().map(move |original| Message::Workspace(crate::app::features::workspace::Message::Query(crate::app::features::workspace::query::Message::RunSelectionClipboardLoaded{
                    original,
                    fallback: fallback.clone(),
                })))
            }
            QueryMessage::RunSelectionClipboardLoaded { original, fallback } => {
                const SENTINEL: &str = "__CRYODB_QUERY_SELECTION_SENTINEL__";
                let copy = self
                    .workspace
                    .query
                    .editor
                    .update(&iced_code_editor::Message::Copy)
                    .map(|value| {
                        Message::Workspace(crate::app::features::workspace::Message::Query(
                            crate::app::features::workspace::query::Message::QueryAction(value),
                        ))
                    });
                iced::clipboard::write(SENTINEL.to_string())
                    .chain(copy)
                    .chain(iced::clipboard::read().map(move |selection| {
                        Message::Workspace(crate::app::features::workspace::Message::Query(
                            crate::app::features::workspace::query::Message::RunSelectionCopied {
                                original: original.clone(),
                                selection,
                                fallback: fallback.clone(),
                            },
                        ))
                    }))
            }
            QueryMessage::RunSelectionCopied {
                original,
                selection,
                fallback,
            } => {
                const SENTINEL: &str = "__CRYODB_QUERY_SELECTION_SENTINEL__";
                let restore = iced::clipboard::write(original.unwrap_or_default());
                let query = selection
                    .filter(|value| value != SENTINEL)
                    .filter(|value| !value.trim().is_empty())
                    .or(fallback);
                let Some(query) = query else { return restore };
                self.ensure_query_tab_for_run();
                Task::batch([restore, self.run_query_text(query.trim().to_string())])
            }
            QueryMessage::UndoQuery => {
                if let Some(previous) = self.workspace.query.undo_stack.pop() {
                    self.workspace
                        .query
                        .redo_stack
                        .push(self.workspace.query.editor.content());
                    self.replace_query_buffer(&previous);
                    self.clear_query_inline_suggestion();
                    self.close_query_suggestions();
                }
                Task::none()
            }
            QueryMessage::RedoQuery => {
                if let Some(next) = self.workspace.query.redo_stack.pop() {
                    self.workspace
                        .query
                        .undo_stack
                        .push(self.workspace.query.editor.content());
                    self.replace_query_buffer(&next);
                    self.clear_query_inline_suggestion();
                    self.close_query_suggestions();
                }
                Task::none()
            }
            QueryMessage::OpenQuerySuggestions => {
                if !self.query_autocomplete_available() {
                    self.clear_query_inline_suggestion();
                    self.close_query_suggestions();
                    return Task::none();
                }
                self.clear_query_inline_suggestion();
                self.workspace.query.suggestions_open = true;
                self.refresh_query_suggestions();
                self.prefetch_query_suggestion_columns()
            }
            QueryMessage::ApplyQuerySuggestion(index) => {
                let task = self.apply_query_suggestion(index);
                self.focus_query_editor();
                task
            }
            QueryMessage::AcceptInlineQuerySuggestion => {
                self.apply_inline_query_suggestion();
                self.focus_query_editor();
                Task::none()
            }
            QueryMessage::AcceptInlineQuerySuggestionWord => {
                self.apply_inline_query_suggestion_word();
                self.focus_query_editor();
                Task::none()
            }
            QueryMessage::DismissInlineQuerySuggestion => {
                self.clear_query_inline_suggestion();
                Task::none()
            }
            QueryMessage::FocusQueryEditorNow => {
                if self.connections.connected {
                    self.focus_query_editor();
                }
                Task::none()
            }
        }
    }
}
