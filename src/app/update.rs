use crate::app::core::App;
use crate::app::features::ai::types::AiModalTarget;
use crate::app::features::onboarding;
use crate::app::features::settings;
use crate::app::features::transfer;
use crate::app::features::updater;
use crate::app::features::workspace::explorer;
use crate::app::features::workspace::tabs::TabEntry;
use crate::app::message::Message;
use crate::app::types::{
    ErrorModalState, LayoutMode, OmniBarMode, OmniCommandHandler, OmniCommandScope,
    ResultsExportFormat, ToastLevel,
};
use crate::constants::{
    AI_PULSE_INTERVAL_MS, AI_PULSE_PERIOD_SECS, CHAT_SIDEBAR_MIN_WIDTH, DEFAULT_EDITOR_HEIGHT,
    LOADING_SPINNER_PERIOD_SECS, MODE_COMMAND, MODE_TABLE_SEARCH, RESPONSIVE_BREAKPOINT,
    SIDEBAR_MIN_WIDTH, TOOLTIP_TIMEOUT_MS, TRIGGER_EDITOR_EXTRA_HEIGHT,
};
use crate::model::connection::DatabaseDriver;
use crate::model::table::{PostgresRoleModalTab, ResultSet, TableCommand, TableModalState};
use crate::ui::ids::{omni_bar_input_id, query_editor_id, table_search_input_id, text_field_id};
use iced::widget::text_editor;
use iced::{Point, Task, keyboard};
use std::path::PathBuf;
use std::sync::Arc;

const FAVORITE_TAG_MAX_CHARS: usize = 12;

fn focused_text_field() -> Task<iced::widget::Id> {
    iced::advanced::widget::operate(iced::advanced::widget::operation::focusable::find_focused())
}

pub(crate) fn normalize_favorite_tags(input: &str) -> Vec<String> {
    input
        .split(',')
        .map(|tag| tag.trim().to_lowercase())
        .filter(|tag| !tag.is_empty() && tag.chars().count() <= FAVORITE_TAG_MAX_CHARS)
        .collect()
}

pub(crate) fn favorite_tags_input_within_limit(input: &str) -> bool {
    input
        .split(',')
        .all(|tag| tag.trim().chars().count() <= FAVORITE_TAG_MAX_CHARS)
}

impl App {
    fn update_tabs(
        &mut self,
        message: crate::app::features::workspace::tabs::Message,
    ) -> Task<Message> {
        use crate::app::features::workspace::tabs::Output;
        let scale = self.scale_f32(1.0);
        let (task, output) = self
            .workspace
            .tabs
            .update(message, scale, self.shell.global_cursor);
        let followup = match output {
            Some(Output::CloseEntry(entry)) => match entry {
                TabEntry::Query(index) => self.close_query_tab(index),
                TabEntry::Table(table) => self.close_table_tab(table),
                TabEntry::Diagram(index) => self.close_schema_diagram(index),
            },
            Some(Output::Tooltip(text)) => self.update(match text {
                Some(text) => Message::Shell(crate::app::shell::Message::SetHoveredTooltip(text)),
                None => Message::Shell(crate::app::shell::Message::ClearHoveredTooltip),
            }),
            Some(Output::Activate(entry)) => self.activate_tab_entry(entry),
            Some(Output::ContextOpened) => {
                self.workspace.results.row_context_menu = None;
                Task::none()
            }
            Some(Output::Close(indices)) => self.close_tabs_from_strip_indices(indices),
            Some(Output::TogglePin(entry)) => {
                let pinned = self.is_tab_entry_pinned(&entry);
                self.set_tab_entry_pinned(&entry, !pinned);
                Task::none()
            }
            Some(Output::Rename { index, name }) => {
                self.workspace.explorer.table_modal =
                    Some(TableModalState::RenameQueryTab { index, name });
                iced::widget::operation::focus(text_field_id("query-tab-rename"))
            }
            None => Task::none(),
        };
        Task::batch([
            task.map(|value| {
                Message::Workspace(crate::app::features::workspace::Message::Tabs(value))
            }),
            followup,
        ])
    }

    pub(crate) fn update_ai(
        &mut self,
        message: crate::app::features::ai::Message,
    ) -> Task<Message> {
        let scope = self.chat_scope_key();
        let database = self.current_database();
        let theme = self.theme();
        let (task, outputs) = self.ai.update(
            message,
            &crate::app::features::ai::Context {
                scope: &scope,
                settings: &self.settings.values,
                connection: &self.connections.current,
                database,
                pool: self.connections.pool.as_ref(),
                tables: &self.workspace.explorer.tables,
                selected_table: self.workspace.selected_table.as_deref(),
                query: &self.workspace.query.editor,
                query_error: self.workspace.query.error.as_deref(),
                role_modal_open: self.workspace.explorer.postgres_role_modal.is_some(),
                query_running: self.workspace.query.running,
                theme,
            },
        );
        let mut tasks = vec![task.map(Message::Ai)];
        for output in outputs {
            tasks.push(self.apply_ai_output(output));
        }
        Task::batch(tasks)
    }

    fn apply_ai_output(&mut self, output: crate::app::features::ai::Output) -> Task<Message> {
        use crate::app::features::ai::{Message as AiMessage, Output};
        match output {
            Output::ChatModeSelected(mode) => {
                return self.update(Message::Settings(
                    crate::app::features::settings::Message::ChatModeSelected(mode),
                ));
            }
            Output::Tooltip(text) => {
                return self.update_internal(match text {
                    Some(text) => {
                        Message::Shell(crate::app::shell::Message::SetHoveredTooltip(text))
                    }
                    None => Message::Shell(crate::app::shell::Message::ClearHoveredTooltip),
                });
            }
            Output::GeneratedSql { target, sql } => {
                if target == AiModalTarget::PostgresRoleSql {
                    if let Some(modal) = self.workspace.explorer.postgres_role_modal.as_mut() {
                        modal.active_tab = PostgresRoleModalTab::Sql;
                    }
                    self.workspace
                        .explorer
                        .set_postgres_role_modal_sql_text(&sql, true);
                } else {
                    self.set_query_text(&sql);
                    self.workspace.query.table_query = None;
                    self.workspace.query.editable_query = None;
                    self.workspace.query.table = None;
                    self.workspace.query.table_has_next_page = false;
                    self.workspace.query.last_query_was_table = false;
                    self.shell.query_editor_hidden = false;
                    let clamped_editor_ratio = self.clamp_editor_ratio(self.shell.editor_ratio);
                    self.shell.editor_ratio = clamped_editor_ratio;
                    self.shell
                        .panes
                        .resize(self.shell.editor_split, clamped_editor_ratio);
                }
            }
            Output::FixStarted => {
                self.workspace.results.apply_error = None;
                self.workspace.results.apply_message = None;
            }
            Output::FixError(error) => {
                self.workspace.results.apply_error = Some(error);
            }
            Output::FixedSql { sql, summary } => {
                self.set_query_text(&sql);
                self.workspace.query.table_query = None;
                self.workspace.query.editable_query = None;
                self.workspace.query.table = None;
                self.workspace.query.table_has_next_page = false;
                self.workspace.query.last_query_was_table = false;
                self.workspace.query.error = None;
                self.workspace.results.apply_error = None;
                self.workspace.results.apply_message = Some(summary);
            }
            Output::SidebarChanged => {
                self.shell.panes.resize(
                    self.shell.chat_split,
                    if self.ai.chat_open {
                        self.clamp_chat_ratio(self.shell.chat_ratio)
                    } else {
                        1.0
                    },
                );
            }
            Output::ClearDiagramAgent => {
                for tab in &mut self.workspace.diagram.diagram_tabs {
                    tab.state.agent = None;
                }
            }
            Output::PauseDiagramFollow => {
                for tab in &mut self.workspace.diagram.diagram_tabs {
                    tab.state.pending = None;
                }
            }
            Output::Toast(level, message) => self.push_toast(level, message),
            Output::InsertSql { sql, announce } => {
                self.record_query_undo(self.workspace.query.editor.content());
                self.set_query_text(&sql);
                self.workspace.query.table_query = None;
                self.workspace.query.editable_query = None;
                if announce {
                    self.push_toast(ToastLevel::Success, "SQL inserted into the editor.");
                }
            }
            Output::OpenSqlTab(sql) => self.open_sql_in_new_query_tab(&sql),
            Output::RunQuery { sql, ensure_tab } => {
                if ensure_tab {
                    self.ensure_query_tab_for_run();
                }
                return self.run_readonly_query_text(sql);
            }
            Output::Diagram { action, prompt } => return self.diagram_agent_run(action, prompt),
            Output::Sample { table, prompt } => {
                let resolved = self.resolve_table_reference(&table);
                return self.update_ai(AiMessage::SampleResolved {
                    table,
                    prompt,
                    resolved,
                });
            }
            Output::ExplainResults => {
                self.workspace.results.row_context_menu = None;
                if let Some(summary) = self.chat_result_summary() {
                    self.open_chat_sidebar();
                    return self.send_chat_followup(format!(
                        "Explain the results of the query I just ran.\n{summary}"
                    ));
                }
            }
            Output::Inactive { scope, message } => {
                if let Some(chat) = self.inactive_chat_state_mut(&scope) {
                    chat.apply_stream(message);
                }
            }
        }
        Task::none()
    }

    fn apply_settings_output(&mut self, output: settings::Output) -> Task<Message> {
        match output {
            settings::Output::Opened => {
                self.shell.omni_bar = None;
                self.shell.omni_bar_anim_progress = 0.0;
                self.shell.omni_bar_closing = false;
                self.shell.database_switcher_open = false;
                self.connections.driver_picker_open = false;
                self.ai.ai_modal_open = false;
                self.ai.ai_modal_target = AiModalTarget::QueryEditor;
                self.ai.ai_modal_use_current_sql_context = false;
                self.ai.ai_modal_sql_context.clear();
                self.connections.more_options_open = false;
                self.workspace.explorer.sidebar_tools_open = false;
                self.workspace.explorer.sidebar_filter_open = false;
                self.workspace.explorer.table_context_menu = None;
                self.workspace.explorer.postgres_object_context_menu = None;
                self.workspace.explorer.postgres_role_modal = None;
                Task::none()
            }
            settings::Output::ThemePickerOpened => {
                self.connections.database_picker_open = false;
                self.connections.more_options_open = false;
                self.workspace.explorer.sidebar_tools_open = false;
                self.workspace.explorer.table_context_menu = None;
                self.workspace.explorer.postgres_object_context_menu = None;
                self.workspace.explorer.folder_context_menu = None;
                Task::none()
            }
            settings::Output::FontPickerOpened => {
                self.connections.database_picker_open = false;
                self.connections.more_options_open = false;
                self.workspace.explorer.sidebar_tools_open = false;
                self.workspace.explorer.table_context_menu = None;
                self.workspace.explorer.postgres_object_context_menu = None;
                self.workspace.explorer.folder_context_menu = None;
                self.connections.connection_picker_open = false;
                Task::none()
            }
            settings::Output::Closed => {
                self.connections.connection_picker_open = false;
                Task::none()
            }
            settings::Output::ResetResultsViewport => {
                self.workspace.results.vertical_viewport = None;
                self.workspace.results.horizontal_viewport = None;
                Task::none()
            }
            settings::Output::ConnectionsLayoutChanged(enabled) => {
                if enabled {
                    self.ensure_active_connection_tab();
                } else {
                    self.connections.tabs.clear();
                    self.connections.active_tab_id = None;
                    self.connections.connection_picker_open = false;
                }

                Task::none()
            }
            settings::Output::TabsChanged(enabled) => {
                if !enabled {
                    let selected = self.workspace.selected_table.clone();
                    self.workspace.tabs.open_tables.clear();
                    self.clear_table_tabs();
                    if let Some(selected) = selected {
                        self.workspace
                            .table_pages
                            .retain(|table, _| table == &selected);
                        self.retain_table_cache_for_selected(&selected);
                        self.workspace
                            .table_filters
                            .retain(|table, _| table == &selected);
                    } else {
                        self.workspace.table_pages.clear();
                        self.clear_table_cache();
                        self.workspace.table_filters.clear();
                    }
                } else if let Some(table) = self.workspace.selected_table.clone() {
                    self.ensure_table_tab(&table);
                }

                Task::none()
            }
            settings::Output::ReleaseInactiveMemoryChanged(enabled) => {
                if enabled {
                    self.prune_inactive_tab_memory();
                } else {
                    self.workspace.results.released = false;
                    self.workspace.table_inactive_since.clear();
                    self.workspace.inactive_table_tabs_released.clear();
                    for tab in &mut self.workspace.tabs.query_tabs {
                        tab.inactive_since = None;
                        if let Some(state) = tab.state.as_mut() {
                            state.inactive_results_released = false;
                        }
                    }
                }

                Task::none()
            }
            settings::Output::ReloadTableLimit => {
                self.clear_table_cache();
                self.persist_settings_store();
                if let Some(table) = self.workspace.selected_table.clone() {
                    let page = self.workspace.table_pages.get(&table).copied().unwrap_or(0);
                    return self.start_table_query(table, page, false);
                }
                Task::none()
            }
            settings::Output::TrimHistory(limit) => {
                self.workspace.query.history.truncate(limit);
                Task::none()
            }
            settings::Output::PruneInactiveMemory => {
                self.prune_inactive_tab_memory();
                Task::none()
            }
            settings::Output::RefreshAutocomplete => {
                if self.query_autocomplete_available() {
                    self.refresh_query_suggestions();
                } else {
                    self.close_query_suggestions();
                }
                Task::none()
            }
            settings::Output::CheckSavedDiagrams => {
                let available = !self.saved_diagram_summaries().is_empty();
                self.update_internal(Message::Settings(
                    settings::Message::SavedDiagramsAvailable(available),
                ))
            }
            settings::Output::ClearSavedDiagrams => {
                match self.clear_saved_diagrams() {
                    Ok(count) => self.push_toast(
                        ToastLevel::Success,
                        crate::i18n::tr_with(
                            "Deleted {count} saved diagram(s).",
                            &[("{count}", &count.to_string())],
                        ),
                    ),
                    Err(error) => self.push_toast(ToastLevel::Error, error),
                }
                Task::none()
            }
            settings::Output::ClearTableCache => {
                self.clear_table_cache();
                Task::none()
            }
            settings::Output::ReloadSelectedTable => {
                if let Some(table) = self.workspace.selected_table.clone() {
                    let page = self.workspace.table_pages.get(&table).copied().unwrap_or(0);
                    self.start_table_query(table, page, false)
                } else {
                    Task::none()
                }
            }
            settings::Output::RestoreSelectedTableTab => {
                if let Some(table) = self.workspace.selected_table.clone() {
                    self.ensure_table_tab(&table);
                }
                Task::none()
            }
            settings::Output::CloseSuggestions => {
                self.close_query_suggestions();
                Task::none()
            }
            settings::Output::Success(text) => {
                self.push_toast(ToastLevel::Success, text);
                Task::none()
            }
            settings::Output::AiEnabled(enabled) => {
                if !enabled {
                    self.ai.chat_open = false;
                    self.ai.ai_modal_open = false;
                }
                Task::none()
            }
            settings::Output::SyncEditor => {
                self.sync_query_editor_settings();
                Task::none()
            }
            settings::Output::ClearInlineSuggestion => {
                self.clear_query_inline_suggestion();
                Task::none()
            }
            settings::Output::Error(error) => {
                self.push_toast(ToastLevel::Error, error);
                Task::none()
            }
            settings::Output::TextEdited(edit) => {
                let edit = edit.map(Message::Settings);
                self.update_internal(Message::Shell(
                    crate::app::shell::Message::TextFieldEdited {
                        id: edit.id,
                        factory: edit.factory,
                        previous: edit.previous,
                        value: edit.value,
                    },
                ))
            }
            settings::Output::Tooltip(text) => self.update_internal(match text {
                Some(text) => Message::Shell(crate::app::shell::Message::SetHoveredTooltip(text)),
                None => Message::Shell(crate::app::shell::Message::ClearHoveredTooltip),
            }),
            settings::Output::OpenChangelog => {
                self.update_internal(Message::Shell(crate::app::shell::Message::OpenChangelog))
            }
            settings::Output::CheckForUpdates => {
                self.update_internal(Message::Updater(updater::Message::Check { announce: true }))
            }
        }
    }

    pub(crate) fn update(&mut self, message: Message) -> Task<Message> {
        let task = self.update_internal(message);
        self.flush_apply_feedback_toasts();
        self.prune_expired_toasts();
        self.sync_editor_split();
        self.sync_columns_split();
        task
    }

    fn sync_columns_split(&mut self) {
        let ratio = if self.columns_panel_visible() {
            self.shell.columns_ratio
        } else {
            1.0
        };
        if self.shell.columns_split_applied != ratio {
            self.shell.columns_split_applied = ratio;
            self.shell.panes.resize(self.shell.columns_split, ratio);
        }
    }

    fn sync_editor_split(&mut self) {
        let hidden = self.is_query_editor_hidden();
        if !self.shell.editor_ratio_pinned && !hidden {
            let height = if self.trigger_editor_visible() {
                DEFAULT_EDITOR_HEIGHT + TRIGGER_EDITOR_EXTRA_HEIGHT
            } else {
                DEFAULT_EDITOR_HEIGHT
            };
            self.shell.editor_ratio = height / self.split_workspace_height();
        } else if hidden == self.shell.editor_split_collapsed {
            return;
        }
        self.shell.editor_split_collapsed = hidden;
        let ratio = if hidden {
            0.0
        } else {
            self.clamp_editor_ratio(self.shell.editor_ratio)
        };
        self.shell.panes.resize(self.shell.editor_split, ratio);
    }

    pub(crate) fn navigate_query_suggestions(&mut self, key: &keyboard::Key) -> Task<Message> {
        match key {
            keyboard::Key::Named(keyboard::key::Named::ArrowDown) => {
                self.move_query_suggestion(true);
            }
            keyboard::Key::Named(keyboard::key::Named::ArrowUp) => {
                self.move_query_suggestion(false);
            }
            keyboard::Key::Named(keyboard::key::Named::Enter | keyboard::key::Named::Tab) => {
                return self.apply_query_suggestion(self.workspace.query.suggestion_index);
            }
            keyboard::Key::Named(keyboard::key::Named::Escape) => {
                self.close_query_suggestions();
            }
            _ => {}
        }
        Task::none()
    }

    pub(crate) fn apply_query_editor_action(
        &mut self,
        action: iced_code_editor::Message,
    ) -> Task<Message> {
        if matches!(action, iced_code_editor::Message::WriteRequested) {
            return self.update(Message::Workspace(
                crate::app::features::workspace::Message::Query(
                    crate::app::features::workspace::query::Message::SaveCurrentQuery,
                ),
            ));
        }
        if let iced_code_editor::Message::CommandPaletteAction(id) = &action {
            match id.as_str() {
                "cryodb.command_palette" => {
                    return self.update(Message::Shell(crate::app::shell::Message::OpenOmniBar(
                        OmniBarMode::Command,
                    )));
                }
                "cryodb.format" => {
                    return self.execute_omni_command(OmniCommandHandler::FormatQuery);
                }
                _ => {}
            }
        }
        if matches!(
            action,
            iced_code_editor::Message::MouseClick(_) | iced_code_editor::Message::CanvasFocusGained
        ) {
            self.workspace.query.editor_blurred = false;
            if self.workspace.query.suggestions_open {
                self.close_query_suggestions();
            }
        }

        let previous = self.workspace.query.editor.content();
        let task = self.workspace.query.editor.update(&action).map(|value| {
            Message::Workspace(crate::app::features::workspace::Message::Query(
                crate::app::features::workspace::query::Message::QueryAction(value),
            ))
        });
        let edited = previous != self.workspace.query.editor.content();
        self.workspace.query.editor_last_action_edited = edited;
        let invalid_scroll = edited
            && self.workspace.query.editor.viewport_scroll()
                > self.workspace.query.editor.content().split('\n').count() as f32
                    * self.workspace.query.editor.line_height();
        let recovery = if invalid_scroll {
            let content = self.workspace.query.editor.content();
            let cursor = self.workspace.query.editor.cursor_position();
            self.record_query_undo(previous);
            self.workspace.query.editor =
                Self::new_query_editor(&content, &self.settings.values, self.settings.theme_choice);
            self.focus_query_editor();
            let _ = self
                .workspace
                .query
                .editor
                .update(&iced_code_editor::Message::CanvasFocusGained);
            self.workspace
                .query
                .editor
                .set_cursor(cursor.0, cursor.1)
                .map(|value| {
                    Message::Workspace(crate::app::features::workspace::Message::Query(
                        crate::app::features::workspace::query::Message::QueryAction(value),
                    ))
                })
        } else {
            Task::none()
        };
        if self.workspace.query.inline_suggestion.is_some()
            && !matches!(
                action,
                iced_code_editor::Message::Scrolled(_)
                    | iced_code_editor::Message::HorizontalScrolled(_)
                    | iced_code_editor::Message::Tick
                    | iced_code_editor::Message::MouseHover(_)
            )
        {
            self.clear_query_inline_suggestion();
        }
        if edited {
            self.workspace.query.inline_suggestion_last_edit_at = Some(std::time::Instant::now());
            self.workspace.query.inline_suggestion_pending = true;
        }

        self.workspace.results.editing_cell = None;
        self.workspace.results.selected_cell = None;
        self.clear_row_selection();
        self.close_text_modal();

        if edited && self.workspace.query.suggestions_open {
            self.refresh_query_suggestions();
            return Task::batch([task, recovery, self.prefetch_query_suggestion_columns()]);
        }

        Task::batch([task, recovery])
    }

    pub(crate) fn update_internal(&mut self, message: Message) -> Task<Message> {
        if let Message::Connections(message) = message {
            return self.update_connections(message);
        }
        if let Message::Workspace(crate::app::features::workspace::Message::Query(message)) =
            message
        {
            return self.update_query(message);
        }
        if let Message::Workspace(crate::app::features::workspace::Message::Results(message)) =
            message
        {
            return self.update_results(message);
        }
        match message {
            Message::Workspace(crate::app::features::workspace::Message::Explorer(message)) => {
                let context = explorer::Context {
                    workspace_busy: self.workspace.query.running
                        || self.workspace.results.applying_changes,
                    query_running: self.workspace.query.running,
                    theme: self.theme(),
                    settings: &self.settings.values,
                    folder_keys: self.folder_store_keys(),
                    driver: self.connections.current.driver,
                    pool: self.connections.pool.as_ref(),
                    database: self.current_database(),
                    is_connected: self.connections.connected,
                    selected_table: self.workspace.selected_table.as_deref(),
                    query: self.workspace.query.editor.content(),
                    menu_fallback: Point::new(self.scale_f32(8.0), self.scale_f32(8.0)),
                };
                let (task, output) = self.workspace.explorer.update(message, context);
                let output_task = match output {
                    Some(explorer::Output::SelectTable(table)) => {
                        self.workspace.table_filters.remove(&table);
                        self.remove_table_cache_for_table(&table);
                        Task::batch([
                            self.select_table(table.clone(), 0, true),
                            self.scroll_tab_entry_into_view(&TabEntry::Table(table)),
                        ])
                    }
                    Some(explorer::Output::OpenPostgresObject(object)) => {
                        self.open_postgres_sidebar_object(object)
                    }
                    Some(explorer::Output::SelectTrigger { table, name }) => {
                        self.ensure_table_tab(&table);
                        self.workspace.explorer.selected_postgres_object = None;
                        self.workspace.selected_table = Some(table);
                        self.update(Message::Workspace(
                            crate::app::features::workspace::Message::Explorer(
                                explorer::Message::TriggerSelected(name),
                            ),
                        ))
                    }
                    Some(explorer::Output::SelectRelations(table)) => {
                        self.ensure_table_tab(&table);
                        self.workspace.explorer.selected_postgres_object = None;
                        self.workspace.selected_table = Some(table.clone());
                        self.update(Message::Workspace(
                            crate::app::features::workspace::Message::Query(
                                crate::app::features::workspace::query::Message::RelationsSelected(
                                    table,
                                ),
                            ),
                        ))
                    }
                    Some(explorer::Output::ShowTrigger { table, trigger }) => {
                        let query = if matches!(
                            self.connections.current.driver,
                            DatabaseDriver::MySql | DatabaseDriver::MariaDb
                        ) {
                            trigger.statement.trim().to_string()
                        } else {
                            self.trigger_query(&table, &trigger)
                        };
                        self.set_query_text(&query);
                        self.workspace.query.table_query = None;
                        self.workspace.query.editable_query = None;
                        self.workspace.query.table = None;
                        self.workspace.query.table_has_next_page = false;
                        self.workspace.query.last_query_was_table = false;
                        self.workspace.query.error = None;
                        self.workspace.results.apply_error = None;
                        self.workspace.results.apply_message = None;
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
                        self.focus_query_editor();
                        Task::none()
                    }
                    Some(explorer::Output::TriggerUpdateFinished { table, result }) => match result
                    {
                        Ok(()) => {
                            self.workspace.results.apply_message =
                                Some(String::from("Trigger updated."));
                            self.load_triggers_for_table(table)
                        }
                        Err(error) => {
                            self.workspace.query.error = Some(error);
                            Task::none()
                        }
                    },
                    Some(explorer::Output::CloseTabContextMenu) => {
                        self.workspace.tabs.tab_context_menu = None;
                        Task::none()
                    }
                    Some(explorer::Output::Toast(level, text)) => {
                        self.push_toast(level, text);
                        Task::none()
                    }
                    Some(explorer::Output::RoleChanged(text)) => {
                        self.push_toast(ToastLevel::Success, text);
                        self.update(Message::Connections(
                            crate::app::features::connections::Message::RefreshTables,
                        ))
                    }
                    Some(explorer::Output::RoleSqlStarted(sql)) => {
                        self.set_query_text(&sql);
                        self.push_history(sql);
                        Task::none()
                    }
                    Some(explorer::Output::RoleAi { prompt, sql }) => {
                        if self.ai.is_generating_ai || self.ai.is_fixing_query_with_ai {
                            return Task::none();
                        }
                        self.ai.ai_modal_target = AiModalTarget::PostgresRoleSql;
                        self.ai.ai_modal_open = true;
                        self.ai.ai_prompt_error = None;
                        self.ai.ai_prompt_content = text_editor::Content::with_text(&prompt);
                        self.ai.ai_modal_sql_context = sql;
                        self.ai.ai_modal_use_current_sql_context =
                            !self.ai.ai_modal_sql_context.trim().is_empty();
                        Task::none()
                    }
                    Some(explorer::Output::RoleScript { notice, result }) => {
                        if let Some(notice) = notice {
                            self.push_toast(ToastLevel::Info, notice);
                        }
                        match result {
                            Ok(script) => {
                                self.open_sql_in_new_query_tab(&script);
                                self.set_query_text(&script);
                            }
                            Err(error) => {
                                self.shell.error_modal = Some(ErrorModalState::new(
                                    "Create role script",
                                    "Could not generate SQL script.",
                                    error,
                                ))
                            }
                        }
                        Task::none()
                    }
                    Some(explorer::Output::AskAboutTable(table)) => {
                        self.open_chat_sidebar();
                        self.send_chat_followup(format!("Describe the table {table}: what it stores, its columns, and how it relates to other tables."))
                    }
                    Some(explorer::Output::LoadColumns(table)) => {
                        self.prefetch_table_columns(&table)
                    }
                    Some(explorer::Output::TableChanged { command, message }) => {
                        self.workspace.results.apply_error = None;
                        self.workspace.results.apply_message = Some(message);
                        self.apply_table_command_side_effects(&command);

                        let refresh = self.update(Message::Connections(
                            crate::app::features::connections::Message::RefreshTables,
                        ));
                        let follow_up = match command {
                            TableCommand::Rename { new_table, .. }
                            | TableCommand::Duplicate { new_table, .. } => {
                                self.select_table(new_table, 0, false)
                            }
                            _ => Task::none(),
                        };
                        Task::batch(vec![refresh, follow_up])
                    }
                    Some(explorer::Output::RenameQueryTab { index, name }) => {
                        let name = name.trim();
                        if let Some(tab) = self.workspace.tabs.query_tabs.get_mut(index)
                            && !name.is_empty()
                        {
                            tab.title = name.to_string();
                            tab.renamed = true;
                        }
                        Task::none()
                    }
                    Some(explorer::Output::TextEdited(edit)) => {
                        let edit = edit.map(|value| {
                            Message::Workspace(crate::app::features::workspace::Message::Explorer(
                                value,
                            ))
                        });
                        self.update_internal(Message::Shell(
                            crate::app::shell::Message::TextFieldEdited {
                                id: edit.id,
                                factory: edit.factory,
                                previous: edit.previous,
                                value: edit.value,
                            },
                        ))
                    }
                    Some(explorer::Output::DdlCopied) => {
                        self.push_toast(ToastLevel::Success, crate::i18n::tr("DDL copied."));
                        Task::none()
                    }
                    Some(explorer::Output::ObjectsChanged) => {
                        self.maybe_refresh_omni_bar_results();
                        Task::none()
                    }
                    Some(explorer::Output::ApplyStatus { error, message }) => {
                        self.workspace.results.apply_error = error;
                        self.workspace.results.apply_message = message;
                        Task::none()
                    }
                    Some(explorer::Output::Error(error)) => {
                        self.shell.error_modal = error;
                        Task::none()
                    }
                    Some(explorer::Output::FolderFileSuccess(text)) => {
                        self.shell.error_modal = None;
                        self.push_toast(ToastLevel::Success, text);
                        Task::none()
                    }
                    Some(explorer::Output::FolderRemoved(folder)) => {
                        self.workspace.results.apply_error = None;
                        self.workspace.results.apply_message = Some(crate::i18n::tr_with(
                            "Folder `{folder}` removed.",
                            &[("{folder}", &folder)],
                        ));
                        Task::none()
                    }
                    Some(explorer::Output::Tooltip(value)) => self.update(match value {
                        Some(text) => {
                            Message::Shell(crate::app::shell::Message::SetHoveredTooltip(text))
                        }
                        None => Message::Shell(crate::app::shell::Message::ClearHoveredTooltip),
                    }),
                    None => Task::none(),
                };
                Task::batch([
                    task.map(|value| {
                        Message::Workspace(crate::app::features::workspace::Message::Explorer(
                            value,
                        ))
                    }),
                    output_task,
                ])
            }
            Message::Workspace(crate::app::features::workspace::Message::Tabs(message)) => {
                self.update_tabs(message)
            }
            Message::Workspace(crate::app::features::workspace::Message::Diagram(message)) => {
                self.update_diagram(message)
            }
            Message::Shell(crate::app::shell::Message::GlobalCursorMoved(position)) => {
                self.shell.global_cursor = Some(position);

                if let Some(resize_drag) = self.workspace.query.editor_resize_drag {
                    let delta = position.y - resize_drag.start_cursor_y;
                    let ratio = self.clamp_editor_ratio(
                        resize_drag.start_ratio + (delta / self.split_workspace_height()),
                    );
                    self.shell.editor_ratio = ratio;
                    self.shell.panes.resize(self.shell.editor_split, ratio);
                }

                Task::none()
            }
            Message::Shell(crate::app::shell::Message::GlobalPointerReleased) => {
                self.workspace.query.editor_resize_drag = None;
                if self.workspace.tabs.drag_tab.is_some()
                    || self.workspace.tabs.pending_tab_drag.is_some()
                {
                    return self.update(Message::Workspace(
                        crate::app::features::workspace::Message::Tabs(
                            crate::app::features::workspace::tabs::Message::TabDragReleased,
                        ),
                    ));
                }
                Task::none()
            }
            Message::Shell(crate::app::shell::Message::OpenOmniBar(mode)) => {
                self.open_omni_bar(mode)
            }
            Message::Shell(crate::app::shell::Message::CloseOmniBar) => {
                self.shell.omni_bar = None;
                self.shell.omni_bar_anim_progress = 0.0;
                self.shell.omni_bar_closing = false;
                Task::none()
            }
            Message::Shell(crate::app::shell::Message::OmniBarQueryChanged(mut query)) => {
                let mut switched_to_command = false;
                if let Some(omni_bar) = self.shell.omni_bar.as_mut() {
                    let prefix = self.settings.values.omni_prefix.trim();
                    if omni_bar.mode == MODE_TABLE_SEARCH
                        && !prefix.is_empty()
                        && query.starts_with(prefix)
                    {
                        omni_bar.mode = MODE_COMMAND;
                        omni_bar.command_scope = OmniCommandScope::Default;
                        query = query.strip_prefix(prefix).unwrap_or("").to_string();
                        switched_to_command = true;
                    }
                    omni_bar.query = query;
                    omni_bar.selected_index = 0;
                    omni_bar.scroll_offset_y = 0.0;
                }
                self.refresh_omni_bar_results();
                if switched_to_command {
                    Task::batch(vec![
                        iced::widget::operation::focus(omni_bar_input_id()),
                        self.scroll_omni_bar_to_selection(),
                    ])
                } else {
                    self.scroll_omni_bar_to_selection()
                }
            }
            Message::Shell(crate::app::shell::Message::OmniBarMoveSelection(delta)) => {
                self.omni_bar_move_selection(delta);
                Task::batch(vec![
                    iced::widget::operation::focus(omni_bar_input_id()),
                    self.scroll_omni_bar_to_selection(),
                ])
            }
            Message::Shell(crate::app::shell::Message::OmniBarAutocomplete) => {
                self.omni_bar_autocomplete();
                Task::batch(vec![
                    iced::widget::operation::focus(omni_bar_input_id()),
                    self.scroll_omni_bar_to_selection(),
                ])
            }
            Message::Shell(crate::app::shell::Message::OmniBarSubmit) => self.omni_bar_submit(),
            Message::Shell(crate::app::shell::Message::OmniBarResultPressed(index)) => {
                if let Some(omni_bar) = self.shell.omni_bar.as_mut() {
                    omni_bar.selected_index = index;
                }
                self.omni_bar_submit()
            }
            Message::Shell(crate::app::shell::Message::OmniBarResultsScrolled(viewport)) => {
                if let Some(omni_bar) = self.shell.omni_bar.as_mut() {
                    omni_bar.scroll_offset_y = viewport.absolute_offset().y.max(0.0);
                }
                Task::none()
            }
            Message::Workspace(crate::app::features::workspace::Message::OpenTableDdl) => {
                let Some(table) = self.current_table_for_info() else {
                    return Task::none();
                };
                let Some(pool) = self.connections.pool.clone() else {
                    return Task::none();
                };
                let database = self.current_database().unwrap_or_default();
                self.workspace
                    .explorer
                    .open_table_ddl(table, pool, database)
                    .map(|value| {
                        Message::Workspace(crate::app::features::workspace::Message::Explorer(
                            value,
                        ))
                    })
            }
            Message::Shell(crate::app::shell::Message::PaneResized(event)) => {
                if event.split == self.shell.sidebar_split && self.shell.sidebar_hidden {
                    self.shell.panes.resize(self.shell.sidebar_split, 0.0);
                    return Task::none();
                }
                if event.split == self.shell.chat_split && !self.ai.chat_open {
                    self.shell.panes.resize(self.shell.chat_split, 1.0);
                    return Task::none();
                }
                let ratio = if event.split == self.shell.editor_split {
                    self.shell.editor_ratio_pinned = true;
                    self.clamp_editor_ratio(event.ratio)
                } else if event.split == self.shell.sidebar_split {
                    self.clamp_sidebar_ratio(event.ratio)
                } else if event.split == self.shell.chat_split {
                    self.clamp_chat_ratio(event.ratio)
                } else {
                    if event.split == self.shell.columns_split && self.columns_panel_visible() {
                        self.shell.columns_ratio = event.ratio;
                        self.shell.columns_split_applied = event.ratio;
                    }
                    event.ratio
                };

                if event.split == self.shell.sidebar_split
                    && !self.shell.sidebar_hidden
                    && !self.shell.zen_mode
                    && self.shell.window_size.width >= RESPONSIVE_BREAKPOINT
                    && self.shell.window_size.width * ratio < self.scale_f32(SIDEBAR_MIN_WIDTH)
                {
                    self.shell.sidebar_hidden = true;
                    self.shell.panes.resize(self.shell.sidebar_split, 0.0);
                    return Task::none();
                }
                if event.split == self.shell.chat_split
                    && self.ai.chat_open
                    && self.shell.window_size.width * (1.0 - ratio)
                        < self.scale_f32(CHAT_SIDEBAR_MIN_WIDTH)
                {
                    self.ai.chat_open = false;
                    self.shell.panes.resize(self.shell.chat_split, 1.0);
                    return Task::none();
                }

                self.shell.panes.resize(event.split, ratio);
                if event.split == self.shell.sidebar_split {
                    if !self.shell.sidebar_hidden && !self.shell.zen_mode {
                        self.shell.sidebar_ratio = ratio;
                    }
                } else if event.split == self.shell.editor_split {
                    self.shell.editor_ratio = ratio;
                } else if event.split == self.shell.chat_split && self.ai.chat_open {
                    self.shell.chat_ratio = self.clamp_chat_ratio(ratio);
                }
                Task::none()
            }
            Message::Workspace(crate::app::features::workspace::Message::ToggleActiveTabPin) => {
                self.toggle_active_tab_pin();
                Task::none()
            }
            Message::Shell(crate::app::shell::Message::ClientKeyPressed(event)) => {
                if !self.connections.connected {
                    return Task::none();
                }

                let keyboard::Event::KeyPressed {
                    key,
                    modifiers,
                    physical_key,
                    ..
                } = event
                else {
                    return Task::none();
                };

                if self.settings.shortcut_capture_target.is_some() {
                    return self.capture_shortcut_binding(&key, modifiers);
                }

                if let Some(message) = App::text_history_shortcut(&key, modifiers) {
                    return self.update(message);
                }

                if let Some(task) = self.handle_omni_bar_keypress(&key, modifiers, physical_key) {
                    return task;
                }

                if let Some(mode) = self.omni_bar_shortcut_mode(&key, modifiers) {
                    return self.update(Message::Shell(crate::app::shell::Message::OpenOmniBar(
                        mode,
                    )));
                }

                if self.shell.changelog_open {
                    if matches!(key, keyboard::Key::Named(keyboard::key::Named::Escape)) {
                        self.shell.changelog_open = false;
                    }
                    return Task::none();
                }

                if self.shell.error_modal.is_some() {
                    if matches!(key, keyboard::Key::Named(keyboard::key::Named::Escape)) {
                        self.shell.error_modal = None;
                    }
                    return Task::none();
                }

                if self.ai.ai_modal_open {
                    if matches!(key, keyboard::Key::Named(keyboard::key::Named::Escape)) {
                        self.ai.ai_modal_open = false;
                        self.ai.ai_modal_target = AiModalTarget::QueryEditor;
                        self.ai.ai_modal_use_current_sql_context = false;
                        self.ai.ai_modal_sql_context.clear();
                    }
                    return Task::none();
                }

                if self.workspace.explorer.table_info_sidebar_open {
                    let close_with_shortcut = self
                        .settings
                        .values
                        .open_table_info_sidebar_shortcut
                        .matches_key(&key, modifiers);
                    if matches!(key, keyboard::Key::Named(keyboard::key::Named::Escape))
                        || close_with_shortcut
                    {
                        self.workspace.explorer.table_info_sidebar_open = false;
                        self.workspace.explorer.table_info_loading = false;
                    }
                    return Task::none();
                }

                if self.workspace.tabs.tab_context_menu.is_some() {
                    if matches!(key, keyboard::Key::Named(keyboard::key::Named::Escape)) {
                        self.workspace.tabs.tab_context_menu = None;
                    }
                    return Task::none();
                }

                if self.workspace.explorer.table_context_menu.is_some() {
                    if matches!(key, keyboard::Key::Named(keyboard::key::Named::Escape)) {
                        self.workspace.explorer.table_context_menu = None;
                    }
                    return Task::none();
                }

                if self
                    .workspace
                    .explorer
                    .postgres_object_context_menu
                    .is_some()
                {
                    if matches!(key, keyboard::Key::Named(keyboard::key::Named::Escape)) {
                        self.workspace.explorer.postgres_object_context_menu = None;
                    }
                    return Task::none();
                }

                if self.workspace.explorer.folder_context_menu.is_some() {
                    if matches!(key, keyboard::Key::Named(keyboard::key::Named::Escape)) {
                        self.workspace.explorer.folder_context_menu = None;
                    }
                    return Task::none();
                }

                if self.workspace.explorer.table_modal.is_some() {
                    if matches!(key, keyboard::Key::Named(keyboard::key::Named::Escape)) {
                        self.workspace.explorer.table_modal = None;
                    }
                    return Task::none();
                }

                if self.workspace.explorer.postgres_role_modal.is_some() {
                    if matches!(key, keyboard::Key::Named(keyboard::key::Named::Escape)) {
                        self.workspace.explorer.postgres_role_modal = None;
                    }
                    return Task::none();
                }

                if self.workspace.results.text_modal_open {
                    if matches!(key, keyboard::Key::Named(keyboard::key::Named::Escape)) {
                        self.close_text_modal();
                    }
                    return Task::none();
                }

                if self.transfer.import_modal_open {
                    if matches!(key, keyboard::Key::Named(keyboard::key::Named::Escape)) {
                        return self.update(Message::Transfer(
                            crate::app::features::transfer::Message::CloseImportModal,
                        ));
                    }
                    return Task::none();
                }

                if self.transfer.export_modal_open {
                    if matches!(key, keyboard::Key::Named(keyboard::key::Named::Escape)) {
                        return self.update(Message::Transfer(
                            crate::app::features::transfer::Message::CloseExportModal,
                        ));
                    }
                    return Task::none();
                }

                if self.settings.settings_open {
                    if matches!(key, keyboard::Key::Named(keyboard::key::Named::Escape)) {
                        self.settings.settings_open = false;
                        self.settings.shortcut_capture_target = None;
                        self.settings.settings_theme_picker_open = false;
                        self.settings.settings_theme_search.clear();
                        self.settings.font_picker_open = false;
                        self.settings.font_search.clear();
                    }
                    return Task::none();
                }

                if self.shell.database_switcher_open {
                    match key {
                        keyboard::Key::Named(keyboard::key::Named::Escape) => {
                            self.shell.database_switcher_open = false;
                        }
                        keyboard::Key::Named(keyboard::key::Named::ArrowUp) => {
                            self.move_database_switcher(-1);
                            return self.scroll_database_switcher_to_index(
                                self.shell.database_switcher_index,
                            );
                        }
                        keyboard::Key::Named(keyboard::key::Named::ArrowDown) => {
                            self.move_database_switcher(1);
                            return self.scroll_database_switcher_to_index(
                                self.shell.database_switcher_index,
                            );
                        }
                        keyboard::Key::Named(keyboard::key::Named::Enter) => {
                            if let Some(database) = self
                                .connections
                                .databases
                                .get(self.shell.database_switcher_index)
                                .cloned()
                            {
                                self.shell.database_switcher_open = false;
                                return self.update(Message::Connections(
                                    crate::app::features::connections::Message::DatabaseSelected(
                                        database,
                                    ),
                                ));
                            }
                        }
                        _ => {}
                    }

                    return Task::none();
                }

                if self.workspace.query.suggestions_open
                    && !modifiers.control()
                    && !modifiers.alt()
                    && !modifiers.logo()
                {
                    return self.navigate_query_suggestions(&key);
                }

                if self.query_editor_focused()
                    && !self.has_active_modal()
                    && !self.workspace.query.suggestions_open
                    && self
                        .settings
                        .values
                        .autocomplete_tables_shortcut
                        .matches_key(&key, modifiers)
                {
                    if self.workspace.query.inline_suggestion.is_some() {
                        return self.update(Message::Workspace(crate::app::features::workspace::Message::Query(crate::app::features::workspace::query::Message::AcceptInlineQuerySuggestion)));
                    }
                    if self.query_autocomplete_available() {
                        return self.update(Message::Workspace(crate::app::features::workspace::Message::Query(
                            crate::app::features::workspace::query::Message::OpenQuerySuggestions,
                        )));
                    }
                }

                if self
                    .settings
                    .values
                    .toggle_sidebar_shortcut
                    .matches_key(&key, modifiers)
                {
                    return self.update(Message::Shell(crate::app::shell::Message::ToggleSidebar));
                }
                if self
                    .settings
                    .values
                    .toggle_chat_sidebar_shortcut
                    .matches_key(&key, modifiers)
                {
                    return self.update(Message::Ai(
                        crate::app::features::ai::Message::ToggleChatSidebar,
                    ));
                }

                if self
                    .settings
                    .values
                    .switch_database_shortcut
                    .matches_key(&key, modifiers)
                {
                    self.open_database_switcher();
                    return Task::none();
                }
                if self
                    .settings
                    .values
                    .focus_table_search_shortcut
                    .matches_key(&key, modifiers)
                {
                    return iced::widget::operation::focus(table_search_input_id());
                }
                if self
                    .settings
                    .values
                    .toggle_tab_pin_shortcut
                    .matches_key(&key, modifiers)
                {
                    return self.update(Message::Workspace(
                        crate::app::features::workspace::Message::ToggleActiveTabPin,
                    ));
                }
                if self
                    .settings
                    .values
                    .cycle_theme_shortcut
                    .matches_key(&key, modifiers)
                {
                    return self.update(Message::Shell(crate::app::shell::Message::CycleTheme));
                }
                if self
                    .settings
                    .values
                    .open_settings_shortcut
                    .matches_key(&key, modifiers)
                {
                    return self.update(Message::Settings(
                        crate::app::features::settings::Message::Settings,
                    ));
                }
                if self
                    .settings
                    .values
                    .next_tab_shortcut
                    .matches_key(&key, modifiers)
                {
                    return self.cycle_visible_tab(false);
                }
                if self
                    .settings
                    .values
                    .previous_tab_shortcut
                    .matches_key(&key, modifiers)
                {
                    return self.cycle_visible_tab(true);
                }
                if self
                    .settings
                    .values
                    .close_tab_shortcut
                    .matches_key(&key, modifiers)
                {
                    return self.close_active_tab();
                }
                if self
                    .settings
                    .values
                    .new_query_shortcut
                    .matches_key(&key, modifiers)
                {
                    return self.update(Message::Workspace(
                        crate::app::features::workspace::Message::Query(
                            crate::app::features::workspace::query::Message::NewQuery,
                        ),
                    ));
                }
                if self
                    .settings
                    .values
                    .open_table_info_sidebar_shortcut
                    .matches_key(&key, modifiers)
                {
                    return self.update(Message::Workspace(
                        crate::app::features::workspace::Message::OpenTableInfoSidebar,
                    ));
                }
                if self
                    .settings
                    .values
                    .run_query_shortcut
                    .matches_key(&key, modifiers)
                {
                    return self.update(Message::Workspace(
                        crate::app::features::workspace::Message::Query(
                            crate::app::features::workspace::query::Message::RunQuery,
                        ),
                    ));
                }
                if self
                    .settings
                    .values
                    .run_selection_shortcut
                    .matches_key(&key, modifiers)
                {
                    return self.update(Message::Workspace(
                        crate::app::features::workspace::Message::Query(
                            crate::app::features::workspace::query::Message::RunSelection,
                        ),
                    ));
                }
                if self.workspace.results.editing_cell.is_none() {
                    if self
                        .settings
                        .values
                        .next_results_page_shortcut
                        .matches_key(&key, modifiers)
                    {
                        return self.update(Message::Workspace(
                            crate::app::features::workspace::Message::Query(
                                crate::app::features::workspace::query::Message::TablePageNext,
                            ),
                        ));
                    }
                    if self
                        .settings
                        .values
                        .previous_results_page_shortcut
                        .matches_key(&key, modifiers)
                    {
                        return self.update(Message::Workspace(
                            crate::app::features::workspace::Message::Query(
                                crate::app::features::workspace::query::Message::TablePagePrev,
                            ),
                        ));
                    }
                    if self
                        .settings
                        .values
                        .first_results_page_shortcut
                        .matches_key(&key, modifiers)
                    {
                        return self.update(Message::Workspace(
                            crate::app::features::workspace::Message::Query(
                                crate::app::features::workspace::query::Message::TablePageFirst,
                            ),
                        ));
                    }
                    if self
                        .settings
                        .values
                        .last_results_page_shortcut
                        .matches_key(&key, modifiers)
                    {
                        return self.update(Message::Workspace(
                            crate::app::features::workspace::Message::Query(
                                crate::app::features::workspace::query::Message::TablePageLast,
                            ),
                        ));
                    }
                }

                if modifiers.control() || modifiers.alt() || modifiers.logo() {
                    return Task::none();
                }

                match key {
                    keyboard::Key::Named(keyboard::key::Named::Escape) => {
                        if self.workspace.results.editing_cell.is_some() {
                            self.workspace.results.editing_cell = None;
                        }
                        self.workspace.query.editor.lose_focus();
                        self.workspace.query.editor_blurred = true;
                        Task::none()
                    }
                    keyboard::Key::Named(keyboard::key::Named::Delete) => {
                        if self.workspace.results.editing_cell.is_some() {
                            return Task::none();
                        }
                        self.clear_selected_cell_if_nullable();
                        if self.workspace.results.pending_edits.is_empty() {
                            return Task::none();
                        }
                        self.update(Message::Workspace(
                            crate::app::features::workspace::Message::Results(
                                crate::app::features::workspace::results::Message::ApplyChanges,
                            ),
                        ))
                    }
                    keyboard::Key::Named(keyboard::key::Named::Enter) => {
                        if self.workspace.results.editing_cell.is_some() {
                            self.workspace.results.editing_cell = None;
                            return self.update(Message::Workspace(
                                crate::app::features::workspace::Message::Results(
                                    crate::app::features::workspace::results::Message::ApplyChanges,
                                ),
                            ));
                        }
                        if let Some((row, column)) = self.workspace.results.selected_cell {
                            if self.should_open_text_modal(row, column) {
                                return self.open_text_modal(row, column);
                            }
                            let focus_task = self.begin_cell_edit(row, column);
                            let scroll_task = self.scroll_selected_cell_into_view();
                            return Task::batch(vec![focus_task, scroll_task]);
                        }
                        Task::none()
                    }
                    keyboard::Key::Named(keyboard::key::Named::ArrowUp) => {
                        if self.workspace.results.editing_cell.is_none()
                            && self.move_selected_cell(-1, 0)
                        {
                            return self.scroll_selected_cell_into_view();
                        }
                        Task::none()
                    }
                    keyboard::Key::Named(keyboard::key::Named::ArrowDown) => {
                        if self.workspace.results.editing_cell.is_none()
                            && self.move_selected_cell(1, 0)
                        {
                            return self.scroll_selected_cell_into_view();
                        }
                        Task::none()
                    }
                    keyboard::Key::Named(keyboard::key::Named::ArrowLeft) => {
                        if self.workspace.results.editing_cell.is_none()
                            && self.move_selected_cell(0, -1)
                        {
                            return self.scroll_selected_cell_into_view();
                        }
                        Task::none()
                    }
                    keyboard::Key::Named(keyboard::key::Named::ArrowRight) => {
                        if self.workspace.results.editing_cell.is_none()
                            && self.move_selected_cell(0, 1)
                        {
                            return self.scroll_selected_cell_into_view();
                        }
                        Task::none()
                    }
                    _ => Task::none(),
                }
            }
            Message::Shell(crate::app::shell::Message::WindowResized(size)) => {
                self.shell.window_size = size;
                let layout = if size.width < RESPONSIVE_BREAKPOINT {
                    LayoutMode::Compact
                } else {
                    LayoutMode::Wide
                };
                let clamped_editor_ratio = self.clamp_editor_ratio(self.shell.editor_ratio);
                if (clamped_editor_ratio - self.shell.editor_ratio).abs() > f32::EPSILON {
                    self.shell.editor_ratio = clamped_editor_ratio;
                    self.shell
                        .panes
                        .resize(self.shell.editor_split, clamped_editor_ratio);
                }
                if layout == LayoutMode::Wide && !self.shell.sidebar_hidden && !self.shell.zen_mode
                {
                    let ratio = self.clamp_sidebar_ratio(self.shell.sidebar_ratio);
                    if (ratio - self.shell.sidebar_ratio).abs() > f32::EPSILON {
                        self.shell.sidebar_ratio = ratio;
                        self.shell.panes.resize(self.shell.sidebar_split, ratio);
                    }
                }
                if layout == LayoutMode::Wide && self.ai.chat_open {
                    let ratio = self.clamp_chat_ratio(self.shell.chat_ratio);
                    if (ratio - self.shell.chat_ratio).abs() > f32::EPSILON {
                        self.shell.chat_ratio = ratio;
                        self.shell.panes.resize(self.shell.chat_split, ratio);
                    }
                }
                if layout != self.shell.layout_mode {
                    self.shell.layout_mode = layout;
                    return self.scroll_selected_cell_into_view();
                }
                self.shell.layout_mode = layout;
                Task::none()
            }
            Message::Shell(crate::app::shell::Message::AiPulseTick) => {
                if self.ai.is_generating_ai || self.ai.is_fixing_query_with_ai {
                    let step = (AI_PULSE_INTERVAL_MS as f32 / 1000.0) / AI_PULSE_PERIOD_SECS;
                    self.ai.ai_pulse_progress += step;
                    if self.ai.ai_pulse_progress >= 1.0 {
                        self.ai.ai_pulse_progress -= 1.0;
                    }
                } else {
                    self.ai.ai_pulse_progress = 0.0;
                }

                if self.has_active_loading_animation() {
                    let step = (AI_PULSE_INTERVAL_MS as f32 / 1000.0) / LOADING_SPINNER_PERIOD_SECS;
                    self.shell.loading_spinner_progress += step;
                    if self.shell.loading_spinner_progress >= 1.0 {
                        self.shell.loading_spinner_progress -= 1.0;
                    }
                    self.shell.loading_spinner_frame =
                        ((self.shell.loading_spinner_progress * 4.0) as usize) % 4;
                }

                self.diagram_agent_tick();
                self.tick_diagram_search_animation();
                self.tick_omni_bar_animation();
                self.request_inline_ai_suggestion()
            }
            Message::Workspace(crate::app::features::workspace::Message::InactiveTabPruneTick) => {
                self.prune_inactive_tab_memory();
                Task::none()
            }
            Message::Shell(crate::app::shell::Message::ToastTick) => {
                self.prune_expired_toasts();
                Task::none()
            }
            Message::Shell(crate::app::shell::Message::ToastDismissed(toast_id)) => {
                self.dismiss_toast(toast_id);
                Task::none()
            }
            Message::Settings(message) => {
                let (task, outputs) = self.settings.update(message);
                let mut tasks = vec![task.map(Message::Settings)];
                tasks.extend(
                    outputs
                        .into_iter()
                        .map(|output| self.apply_settings_output(output)),
                );
                Task::batch(tasks)
            }
            Message::Ai(message) => self.update_ai(message),
            Message::Onboarding(message) => {
                let Some(output) = self.onboarding.update(message) else {
                    return Task::none();
                };
                match output {
                    onboarding::Output::LanguageSelected(value) => {
                        self.update_internal(Message::Settings(
                            crate::app::features::settings::Message::LanguageSelected(value),
                        ))
                    }
                    onboarding::Output::ThemeVariantSelected(value) => {
                        self.update_internal(Message::Settings(
                            crate::app::features::settings::Message::ThemeVariantSelected(value),
                        ))
                    }
                    onboarding::Output::UiDensitySelected(value) => {
                        self.update_internal(Message::Settings(
                            crate::app::features::settings::Message::UiDensitySelected(value),
                        ))
                    }
                    onboarding::Output::AccentColorSelected(value) => {
                        self.update_internal(Message::Settings(
                            crate::app::features::settings::Message::AccentColorSelected(value),
                        ))
                    }
                    onboarding::Output::AppearanceModeSelected(mode) => self.update_internal(
                        Message::Settings(settings::Message::AppearanceModeSelected(mode)),
                    ),
                    onboarding::Output::FontSelected(font) => self
                        .update_internal(Message::Settings(settings::Message::FontsSelected(font))),
                    onboarding::Output::AiEnabled(enabled) => self
                        .update_internal(Message::Settings(settings::Message::AiEnabled(enabled))),
                    onboarding::Output::Finished => {
                        self.update_internal(Message::Settings(settings::Message::CompleteSetup))
                    }
                }
            }
            Message::Shell(crate::app::shell::Message::ToggleSidebar) => {
                if self.shell.zen_mode {
                    return Task::none();
                }
                self.shell.sidebar_hidden = !self.shell.sidebar_hidden;
                let ratio = if self.shell.sidebar_hidden {
                    0.0
                } else if self.shell.sidebar_ratio <= 0.0 {
                    0.24
                } else {
                    self.clamp_sidebar_ratio(self.shell.sidebar_ratio)
                };
                if !self.shell.sidebar_hidden
                    && self.shell.window_size.width * ratio < self.scale_f32(SIDEBAR_MIN_WIDTH)
                {
                    self.shell.sidebar_hidden = true;
                    self.shell.panes.resize(self.shell.sidebar_split, 0.0);
                } else {
                    self.shell.panes.resize(self.shell.sidebar_split, ratio);
                }
                Task::none()
            }
            Message::Workspace(crate::app::features::workspace::Message::OpenTableInfoSidebar) => {
                self.workspace.tabs.tab_context_menu = None;
                if let Some(table) = self.current_table_for_info() {
                    self.request_table_info(table)
                } else {
                    self.workspace.explorer.table_info_sidebar_open = true;
                    self.workspace.explorer.table_info_loading = false;
                    self.workspace.explorer.table_info_table = None;
                    self.workspace.explorer.table_info = None;
                    self.workspace.explorer.table_info_error =
                        Some(String::from("Select a table first."));
                    Task::none()
                }
            }
            Message::Shell(crate::app::shell::Message::ToggleMoreOptions) => {
                if !self.connections.more_options_open {
                    self.connections.database_picker_open = false;
                    self.connections.connection_picker_open = false;
                    self.settings.theme_picker_open = false;
                    self.workspace.explorer.sidebar_tools_open = false;
                    self.workspace.explorer.table_context_menu = None;
                    self.workspace.explorer.postgres_object_context_menu = None;
                    self.workspace.explorer.folder_context_menu = None;
                }
                self.connections.more_options_open = !self.connections.more_options_open;
                Task::none()
            }
            Message::Shell(crate::app::shell::Message::CloseMoreOptions) => {
                self.connections.more_options_open = false;
                Task::none()
            }
            Message::Transfer(message) => {
                let database = self.current_database();
                let (task, outputs) = self.transfer.update(
                    message,
                    transfer::Context {
                        pool: self.connections.pool.as_ref(),
                        connection: &self.connections.current,
                        database,
                        is_connected: self.connections.connected,
                    },
                );
                let mut tasks = vec![task.map(Message::Transfer)];
                for output in outputs {
                    match output {
                        transfer::Output::Opened => {
                            self.connections.more_options_open = false;
                            self.workspace.explorer.sidebar_tools_open = false;
                            self.connections.database_picker_open = false;
                            self.settings.theme_picker_open = false;
                            self.settings.settings_theme_picker_open = false;
                            self.settings.settings_theme_search.clear();
                            self.workspace.explorer.table_context_menu = None;
                            self.workspace.explorer.postgres_object_context_menu = None;
                            self.workspace.explorer.folder_context_menu = None;
                            self.workspace.explorer.postgres_role_modal = None;
                            self.workspace.explorer.postgres_role_action_running = false;
                        }
                        transfer::Output::Error(error) => self.shell.error_modal = error,
                        transfer::Output::TextEdited(edit) => {
                            let edit = edit.map(Message::Transfer);
                            tasks.push(self.update_internal(Message::Shell(
                                crate::app::shell::Message::TextFieldEdited {
                                    id: edit.id,
                                    factory: edit.factory,
                                    previous: edit.previous,
                                    value: edit.value,
                                },
                            )));
                        }
                        transfer::Output::Tooltip(text) => {
                            tasks.push(self.update_internal(match text {
                                Some(text) => Message::Shell(
                                    crate::app::shell::Message::SetHoveredTooltip(text),
                                ),
                                None => {
                                    Message::Shell(crate::app::shell::Message::ClearHoveredTooltip)
                                }
                            }))
                        }
                    }
                }
                Task::batch(tasks)
            }
            Message::Shell(crate::app::shell::Message::CopyErrorModal) => {
                if let Some(text) = self.error_modal_text() {
                    iced::clipboard::write(text)
                } else {
                    Task::none()
                }
            }
            Message::Shell(crate::app::shell::Message::CloseErrorModal) => {
                self.shell.error_modal = None;
                Task::none()
            }
            Message::Shell(crate::app::shell::Message::ToggleZenMode) => {
                self.shell.zen_mode = !self.shell.zen_mode;
                if self.shell.zen_mode {
                    self.settings.settings_open = false;
                    self.settings.shortcut_capture_target = None;
                    self.connections.driver_picker_open = false;
                    self.connections.database_picker_open = false;
                    self.settings.theme_picker_open = false;
                    self.settings.settings_theme_picker_open = false;
                    self.settings.settings_theme_search.clear();
                    self.settings.font_picker_open = false;
                    self.connections.more_options_open = false;
                    self.workspace.explorer.sidebar_tools_open = false;
                    self.workspace.explorer.table_info_sidebar_open = false;
                    self.workspace.explorer.table_context_menu = None;
                    self.workspace.explorer.postgres_object_context_menu = None;
                    self.workspace.explorer.folder_context_menu = None;
                    self.workspace.explorer.postgres_role_modal = None;
                    self.workspace.explorer.postgres_role_action_running = false;
                    self.workspace.tabs.tab_context_menu = None;
                    self.shell.panes.resize(self.shell.sidebar_split, 0.0);
                } else {
                    let sidebar_ratio = if self.shell.sidebar_hidden {
                        0.0
                    } else if self.shell.sidebar_ratio <= 0.0 {
                        0.24
                    } else {
                        self.clamp_sidebar_ratio(self.shell.sidebar_ratio)
                    };
                    self.shell
                        .panes
                        .resize(self.shell.sidebar_split, sidebar_ratio);
                }
                let clamped_editor_ratio = self.clamp_editor_ratio(self.shell.editor_ratio);
                self.shell.editor_ratio = clamped_editor_ratio;
                self.shell
                    .panes
                    .resize(self.shell.editor_split, clamped_editor_ratio);
                Task::none()
            }
            Message::Shell(crate::app::shell::Message::SetHoveredTooltip(text)) => {
                *self.shell.hovered_tooltip_text.borrow_mut() = Some(text);
                self.shell.tooltip_visible_since = Some(std::time::Instant::now());
                Task::none()
            }
            Message::Shell(crate::app::shell::Message::ClearHoveredTooltip) => {
                *self.shell.hovered_tooltip_text.borrow_mut() = None;
                self.shell.tooltip_visible_since = None;
                Task::none()
            }
            Message::Shell(crate::app::shell::Message::TooltipTick) => {
                if let Some(since) = self.shell.tooltip_visible_since
                    && since.elapsed().as_millis() >= TOOLTIP_TIMEOUT_MS as u128
                {
                    *self.shell.hovered_tooltip_text.borrow_mut() = None;
                    self.shell.tooltip_visible_since = None;
                }
                Task::none()
            }
            Message::Updater(message) => {
                let (task, output) = self.updater.update(message);
                if let Some(updater::Output::Toast(level, message)) = output {
                    self.push_toast(level, message);
                }
                task.map(Message::Updater)
            }
            Message::Shell(crate::app::shell::Message::AnnounceChangelog) => {
                self.announce_changelog();
                Task::none()
            }
            Message::Shell(crate::app::shell::Message::OpenChangelog) => {
                self.shell.changelog_open = true;
                self.shell.changelog_expanded = 0;
                Task::none()
            }
            Message::Shell(crate::app::shell::Message::CloseChangelog) => {
                self.shell.changelog_open = false;
                Task::none()
            }
            Message::Shell(crate::app::shell::Message::ChangelogReleaseToggled(index)) => {
                self.shell.changelog_expanded = if self.shell.changelog_expanded == index {
                    usize::MAX
                } else {
                    index
                };
                Task::none()
            }
            Message::Shell(crate::app::shell::Message::TextFieldEdited {
                id,
                factory,
                previous,
                value,
            }) => {
                self.shell
                    .text_history
                    .record(&id, &factory, previous, &value);
                self.update(factory.message(value))
            }
            Message::Shell(crate::app::shell::Message::UndoFocusedTextField) => {
                focused_text_field()
                    .map(|value| Message::Shell(crate::app::shell::Message::UndoTextField(value)))
            }
            Message::Shell(crate::app::shell::Message::RedoFocusedTextField) => {
                focused_text_field()
                    .map(|value| Message::Shell(crate::app::shell::Message::RedoTextField(value)))
            }
            Message::Shell(crate::app::shell::Message::UndoTextField(id)) => {
                if id == query_editor_id() {
                    return self.update(Message::Workspace(
                        crate::app::features::workspace::Message::Query(
                            crate::app::features::workspace::query::Message::UndoQuery,
                        ),
                    ));
                }
                match self.shell.text_history.undo(&id) {
                    Some(message) => self.update(message),
                    None => Task::none(),
                }
            }
            Message::Shell(crate::app::shell::Message::RedoTextField(id)) => {
                if id == query_editor_id() {
                    return self.update(Message::Workspace(
                        crate::app::features::workspace::Message::Query(
                            crate::app::features::workspace::query::Message::RedoQuery,
                        ),
                    ));
                }
                match self.shell.text_history.redo(&id) {
                    Some(message) => self.update(message),
                    None => Task::none(),
                }
            }
            Message::Shell(crate::app::shell::Message::CycleTheme) => {
                if self.settings.values.system_theme_mode == crate::SystemThemeMode::Manual {
                    self.settings.theme_choice = self.next_theme();
                    self.sync_query_editor_settings();
                    self.persist_settings_store();
                }
                Task::none()
            }
            Message::Shell(crate::app::shell::Message::ModalBlocked) => Task::none(),
            _ => unreachable!(),
        }
    }
}

fn csv_escape_cell(value: &str) -> String {
    let needs_quotes =
        value.contains(',') || value.contains('\n') || value.contains('\r') || value.contains('"');
    if !needs_quotes {
        return value.to_string();
    }
    format!("\"{}\"", value.replace('"', "\"\""))
}

pub(crate) async fn export_results_file(
    results: Arc<ResultSet>,
    format: ResultsExportFormat,
    path: PathBuf,
) -> Result<String, String> {
    let display_path = path.display().to_string();
    match format {
        ResultsExportFormat::Csv => {
            let mut output = String::new();
            output.push_str(
                &results
                    .columns
                    .iter()
                    .map(|value| csv_escape_cell(value))
                    .collect::<Vec<_>>()
                    .join(","),
            );
            output.push('\n');
            for row in &results.rows {
                output.push_str(
                    &row.iter()
                        .map(|value| csv_escape_cell(value))
                        .collect::<Vec<_>>()
                        .join(","),
                );
                output.push('\n');
            }
            tokio::fs::write(&path, output)
                .await
                .map_err(|error| error.to_string())?;
        }
        ResultsExportFormat::Json => {
            let payload = results
                .rows
                .iter()
                .map(|row| {
                    let mut object = serde_json::Map::with_capacity(results.columns.len());
                    for (column, value) in results.columns.iter().zip(row.iter()) {
                        object.insert(column.clone(), serde_json::Value::String(value.clone()));
                    }
                    serde_json::Value::Object(object)
                })
                .collect::<Vec<_>>();
            let output =
                serde_json::to_string_pretty(&payload).map_err(|error| error.to_string())?;
            tokio::fs::write(&path, output)
                .await
                .map_err(|error| error.to_string())?;
        }
        ResultsExportFormat::Xlsx => {
            let mut workbook = rust_xlsxwriter::Workbook::new();
            let worksheet = workbook.add_worksheet();

            for (column_index, column_name) in results.columns.iter().enumerate() {
                worksheet
                    .write_string(0, column_index as u16, column_name)
                    .map_err(|error| error.to_string())?;
            }

            for (row_index, row) in results.rows.iter().enumerate() {
                for (column_index, value) in row.iter().enumerate() {
                    worksheet
                        .write_string((row_index + 1) as u32, column_index as u16, value)
                        .map_err(|error| error.to_string())?;
                }
            }

            workbook.save(&path).map_err(|error| error.to_string())?;
        }
    }
    Ok(display_path)
}
