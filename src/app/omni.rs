use crate::ai::sql_fix::should_offer_ai_query_fix;
use crate::app::core::App;
use crate::app::features::settings::types::FontPickerTarget;
use crate::app::features::updater;
use crate::app::features::workspace::tabs::TabEntry;
use crate::app::message::Message;
use crate::app::types::{
    OmniBarMode, OmniBarState, OmniCommandHandler, OmniCommandScope, OmniResultAction,
    OmniSettingAction,
};
use crate::constants::{MODE_COMMAND, MODE_TABLE_SEARCH, OMNIBAR_RESULT_ROW_BASE_HEIGHT};
use crate::model::settings::UiDensity;
use crate::model::table::TableModalState;
use crate::ui::ids::{omni_bar_input_id, omni_bar_results_scroll_id};
use crate::utils::helpers::format_sql_for_palette;
use iced::widget::{scrollable, text_editor};
use iced::{Task, keyboard};

impl App {
    pub(crate) fn is_omni_bar_open(&self) -> bool {
        self.shell.omni_bar.is_some() && !self.shell.omni_bar_closing
    }

    pub(crate) fn is_omni_bar_visible(&self) -> bool {
        self.shell.omni_bar.is_some() && self.shell.omni_bar_anim_progress > 0.0
    }

    pub(crate) fn tick_omni_bar_animation(&mut self) {
        let Some(_) = self.shell.omni_bar else {
            self.shell.omni_bar_anim_progress = 0.0;
            self.shell.omni_bar_closing = false;
            return;
        };
        self.shell.omni_bar_anim_progress = 1.0;
        self.shell.omni_bar_closing = false;
    }

    pub(crate) fn omni_bar_shortcut_mode(
        &self,
        key: &keyboard::Key,
        modifiers: keyboard::Modifiers,
    ) -> Option<OmniBarMode> {
        if self
            .settings
            .values
            .omni_command_shortcut
            .matches_key(key, modifiers)
        {
            return Some(MODE_COMMAND);
        }

        if self
            .settings
            .values
            .omni_command_alt_shortcut
            .matches_key(key, modifiers)
        {
            return Some(MODE_COMMAND);
        }

        if self
            .settings
            .values
            .omni_table_shortcut
            .matches_key(key, modifiers)
        {
            return Some(MODE_TABLE_SEARCH);
        }

        None
    }

    pub(crate) fn handle_omni_bar_keypress(
        &mut self,
        key: &keyboard::Key,
        modifiers: keyboard::Modifiers,
        physical_key: keyboard::key::Physical,
    ) -> Option<Task<Message>> {
        if !self.is_omni_bar_visible() {
            return None;
        }
        if !self.is_omni_bar_open() {
            return Some(Task::none());
        }

        if let Some(mode) = self.omni_bar_shortcut_mode(key, modifiers) {
            return Some(
                self.update(Message::Shell(crate::app::shell::Message::OpenOmniBar(
                    mode,
                ))),
            );
        }

        let is_enter = matches!(key, keyboard::Key::Named(keyboard::key::Named::Enter))
            || matches!(
                physical_key,
                keyboard::key::Physical::Code(keyboard::key::Code::NumpadEnter)
            )
            || physical_key == keyboard::key::Code::NumpadEnter;

        if modifiers.control() || modifiers.alt() || modifiers.logo() {
            return Some(Task::none());
        }

        match key {
            keyboard::Key::Named(keyboard::key::Named::Escape) => {
                Some(self.update(Message::Shell(crate::app::shell::Message::CloseOmniBar)))
            }
            keyboard::Key::Named(keyboard::key::Named::ArrowUp) => Some(self.update(
                Message::Shell(crate::app::shell::Message::OmniBarMoveSelection(-1)),
            )),
            keyboard::Key::Named(keyboard::key::Named::ArrowDown) => Some(self.update(
                Message::Shell(crate::app::shell::Message::OmniBarMoveSelection(1)),
            )),
            keyboard::Key::Named(keyboard::key::Named::Tab) => Some(self.update(Message::Shell(
                crate::app::shell::Message::OmniBarAutocomplete,
            ))),
            _ if is_enter => Some(Task::none()),
            _ => Some(Task::none()),
        }
    }

    pub(crate) fn open_omni_bar(&mut self, mode: OmniBarMode) -> Task<Message> {
        if mode == MODE_TABLE_SEARCH
            && (!self.connections.connected || self.current_database().is_none())
        {
            return Task::none();
        }
        self.shell.omni_bar = Some(OmniBarState {
            mode,
            command_scope: OmniCommandScope::Default,
            query: String::new(),
            selected_index: 0,
            scroll_offset_y: 0.0,
            results: Vec::new(),
        });
        self.refresh_omni_bar_results();
        self.shell.omni_bar_closing = false;
        self.shell.omni_bar_anim_progress = 1.0;

        Task::batch(vec![
            iced::widget::operation::focus(omni_bar_input_id()),
            self.scroll_omni_bar_to_selection(),
        ])
    }

    pub(crate) fn omni_bar_result_row_height(&self) -> f32 {
        self.scale_f32(OMNIBAR_RESULT_ROW_BASE_HEIGHT) + self.omni_bar_result_row_gap()
    }

    pub(crate) fn omni_bar_result_row_gap(&self) -> f32 {
        match self.settings.values.ui_density {
            UiDensity::Normal => self.scale_f32(6.0),
            UiDensity::Compact => self.scale_f32(2.0),
        }
    }

    pub(crate) fn scroll_omni_bar_to_selection(&self) -> Task<Message> {
        let selected = self
            .shell
            .omni_bar
            .as_ref()
            .map(|omni_bar| omni_bar.selected_index)
            .unwrap_or(0);
        let row_height = self.omni_bar_result_row_height();
        let viewport_rows = ((self.scale_f32(320.0) / row_height).ceil() as usize).max(1);
        let anchor = selected.saturating_sub(viewport_rows / 2);
        iced::widget::operation::scroll_to(
            omni_bar_results_scroll_id(),
            scrollable::AbsoluteOffset {
                x: 0.0,
                y: row_height * anchor as f32,
            },
        )
    }

    pub(crate) fn omni_bar_move_selection(&mut self, delta: i32) {
        let len = self.omni_bar_results().len();
        let Some(omni_bar) = self.shell.omni_bar.as_mut() else {
            return;
        };
        if len == 0 {
            omni_bar.selected_index = 0;
            return;
        }
        let max = len.saturating_sub(1) as i32;
        let next = (omni_bar.selected_index as i32 + delta).clamp(0, max);
        omni_bar.selected_index = next as usize;
    }

    pub(crate) fn omni_bar_autocomplete(&mut self) {
        let autocomplete = {
            let results = self.omni_bar_results();
            let Some(omni_bar) = self.shell.omni_bar.as_ref() else {
                return;
            };
            if results.is_empty() {
                return;
            }
            let index = omni_bar.selected_index.min(results.len() - 1);
            results.get(index).map(|item| item.autocomplete.clone())
        };

        let Some(omni_bar) = self.shell.omni_bar.as_mut() else {
            return;
        };
        if let Some(autocomplete) = autocomplete {
            omni_bar.query = autocomplete;
            omni_bar.selected_index = 0;
            omni_bar.scroll_offset_y = 0.0;
            self.refresh_omni_bar_results();
        }
    }

    pub(crate) fn omni_bar_submit(&mut self) -> Task<Message> {
        let Some(omni_bar) = self.shell.omni_bar.as_ref() else {
            return Task::none();
        };
        let mode = omni_bar.mode;
        let query_snapshot = omni_bar.query.clone();
        let selected_index = omni_bar.selected_index;
        if self.omni_bar_results().is_empty() {
            if mode == MODE_COMMAND {
                let query = query_snapshot.trim();
                if let Some(target) = query.strip_prefix("connect ") {
                    let target = target.trim();
                    if let Some(database) = self
                        .connections
                        .databases
                        .iter()
                        .find(|db| db.eq_ignore_ascii_case(target))
                        .cloned()
                    {
                        return Task::batch(vec![
                            self.update(Message::Shell(crate::app::shell::Message::CloseOmniBar)),
                            self.execute_omni_action(OmniResultAction::ConnectDatabase(database)),
                        ]);
                    }
                }
            }
            return Task::none();
        }

        let index = selected_index.min(self.omni_bar_results().len() - 1);
        let action = self.omni_bar_results()[index].action.clone();
        let keep_open = matches!(
            action,
            OmniResultAction::RunCommand(OmniCommandHandler::DbSwitchDatabase)
                | OmniResultAction::RunCommand(OmniCommandHandler::SelectDriver)
                | OmniResultAction::RunCommand(OmniCommandHandler::OpenFavoriteConnection)
                | OmniResultAction::RunCommand(OmniCommandHandler::OpenRecentConnection)
                | OmniResultAction::RunCommand(OmniCommandHandler::SwitchConnection)
                | OmniResultAction::RunCommand(OmniCommandHandler::SwitchTabs)
                | OmniResultAction::RunCommand(OmniCommandHandler::FocusOn)
                | OmniResultAction::RunCommand(OmniCommandHandler::MoveTableToFolder)
                | OmniResultAction::RunCommand(OmniCommandHandler::RemoveFolder)
                | OmniResultAction::RunCommand(OmniCommandHandler::OpenSettingsCommands)
                | OmniResultAction::RunCommand(OmniCommandHandler::OpenSettingsThemes)
                | OmniResultAction::RunCommand(OmniCommandHandler::OpenSettingsFonts)
                | OmniResultAction::RunCommand(OmniCommandHandler::OpenSettingsFontSizes)
                | OmniResultAction::RunCommand(OmniCommandHandler::OpenSettingsUiDensity)
                | OmniResultAction::RunCommand(OmniCommandHandler::OpenSettingsAiProviders)
                | OmniResultAction::RunCommand(OmniCommandHandler::OpenSettingsTableShortcuts)
                | OmniResultAction::RunCommand(OmniCommandHandler::OpenSettingsCommandShortcuts)
                | OmniResultAction::RunCommand(OmniCommandHandler::FocusOmniBar)
                | OmniResultAction::OpenCommandScope(_)
        );

        let action_task = self.execute_omni_action(action);
        if keep_open {
            action_task
        } else {
            Task::batch(vec![
                self.update(Message::Shell(crate::app::shell::Message::CloseOmniBar)),
                action_task,
            ])
        }
    }

    pub(crate) fn execute_omni_action(&mut self, action: OmniResultAction) -> Task<Message> {
        match action {
            OmniResultAction::OpenCommandScope(scope) => self.open_omni_command_scope(scope),
            OmniResultAction::OpenTable(table) => self.select_table(table, 0, true),
            OmniResultAction::OpenPostgresObject(object) => {
                self.open_postgres_sidebar_object(object)
            }
            OmniResultAction::ActivateTab(entry) => self.activate_tab_entry(entry),
            OmniResultAction::ConnectDatabase(database) => {
                if self.connections.connected {
                    self.update(Message::Connections(
                        crate::app::features::connections::Message::DatabaseSelected(database),
                    ))
                } else if !database.trim().is_empty() {
                    self.connections.current.database = database;
                    if self.can_connect() {
                        self.update(Message::Connections(
                            crate::app::features::connections::Message::Connect,
                        ))
                    } else {
                        Task::none()
                    }
                } else {
                    Task::none()
                }
            }
            OmniResultAction::SelectDriver(driver) => self.update(Message::Connections(
                crate::app::features::connections::Message::DriverSelected(driver),
            )),
            OmniResultAction::OpenMoveTableToFolder(table) => {
                if !self.connections.connected
                    || !self
                        .workspace
                        .explorer
                        .tables
                        .iter()
                        .any(|entry| entry == &table)
                {
                    return Task::none();
                }
                let folder = self.folder_for_table(&table).unwrap_or_default();
                self.workspace.explorer.table_modal =
                    Some(TableModalState::MoveToFolder { table, folder });
                self.workspace.explorer.table_context_menu = None;
                self.workspace.explorer.folder_context_menu = None;
                self.workspace.explorer.sidebar_tools_open = false;
                Task::none()
            }
            OmniResultAction::DissolveFolder(folder) => {
                self.dissolve_folder(&folder);
                Task::none()
            }
            OmniResultAction::ConnectFavorite(index) => self.update(Message::Connections(
                crate::app::features::connections::Message::FavoriteSelected(index),
            )),
            OmniResultAction::ConnectRecent(index) => self.update(Message::Connections(
                crate::app::features::connections::Message::RecentSelected(index),
            )),
            OmniResultAction::ApplySetting(action) => self.apply_omni_setting_action(action),
            OmniResultAction::RunCommand(handler) => {
                self.record_recent_omni_command(handler);
                self.execute_omni_command(handler)
            }
        }
    }

    pub(crate) fn apply_omni_setting_action(&mut self, action: OmniSettingAction) -> Task<Message> {
        match action {
            OmniSettingAction::SetTheme(theme) => self.update(Message::Settings(crate::app::features::settings::Message::ThemeSelected(theme))),
            OmniSettingAction::SetSystemThemeMode(mode) => {
                self.update(Message::Settings(crate::app::features::settings::Message::SystemThemeModeSelected(mode)))
            }
            OmniSettingAction::SetThemeVariant(variant) => {
                self.update(Message::Settings(crate::app::features::settings::Message::ThemeVariantSelected(variant)))
            }
            OmniSettingAction::SetDarkTheme(theme) => {
                self.update(Message::Settings(crate::app::features::settings::Message::DarkThemeSelected(theme)))
            }
            OmniSettingAction::SetLightTheme(theme) => {
                self.update(Message::Settings(crate::app::features::settings::Message::LightThemeSelected(theme)))
            }
            OmniSettingAction::SetAccentColor(color) => {
                self.update(Message::Settings(crate::app::features::settings::Message::AccentColorSelected(color)))
            }
            OmniSettingAction::SetFont(font) => {
                self.update(Message::Settings(crate::app::features::settings::Message::FontSelected(FontPickerTarget::Ui, font)))
            }
            OmniSettingAction::SetFontSize(size) => self.update(Message::Settings(crate::app::features::settings::Message::FontSizeSelected(size))),
            OmniSettingAction::SetUiDensity(density) => {
                self.update(Message::Settings(crate::app::features::settings::Message::UiDensitySelected(density)))
            }
            OmniSettingAction::SetTableQueryLimit(limit) => {
                self.update(Message::Settings(crate::app::features::settings::Message::TableQueryLimitChanged(limit.to_string())))
            }
            OmniSettingAction::SetHistoryLimit(limit) => {
                self.update(Message::Settings(crate::app::features::settings::Message::HistoryLimitChanged(limit.to_string())))
            }
            OmniSettingAction::SetTabsEnabled(enabled) => {
                self.update(Message::Settings(crate::app::features::settings::Message::TabsToggled(enabled)))
            }
            OmniSettingAction::SetOmniTableShortcut(shortcut) => {
                self.update(Message::Settings(crate::app::features::settings::Message::OmniTableShortcutSelected(shortcut)))
            }
            OmniSettingAction::SetOmniCommandShortcut(shortcut) => {
                self.update(Message::Settings(crate::app::features::settings::Message::OmniCommandShortcutSelected(shortcut)))
            }
            OmniSettingAction::SetOpenSettingsShortcut(shortcut) => {
                self.settings.values.open_settings_shortcut = shortcut;
                self.persist_settings_store();
                Task::none()
            }
            OmniSettingAction::SetSwitchDatabaseShortcut(shortcut) => {
                self.settings.values.switch_database_shortcut = shortcut;
                self.persist_settings_store();
                Task::none()
            }
            OmniSettingAction::SetFocusTableSearchShortcut(shortcut) => {
                self.settings.values.focus_table_search_shortcut = shortcut;
                self.persist_settings_store();
                Task::none()
            }
            OmniSettingAction::SetToggleTabPinShortcut(shortcut) => {
                self.settings.values.toggle_tab_pin_shortcut = shortcut;
                self.persist_settings_store();
                Task::none()
            }
            OmniSettingAction::SetCycleThemeShortcut(shortcut) => {
                self.settings.values.cycle_theme_shortcut = shortcut;
                self.persist_settings_store();
                Task::none()
            }
            OmniSettingAction::SetRunQueryShortcut(shortcut) => {
                self.settings.values.run_query_shortcut = shortcut;
                self.persist_settings_store();
                Task::none()
            }
            OmniSettingAction::SetRunSelectionShortcut(shortcut) => {
                self.settings.values.run_selection_shortcut = shortcut;
                self.persist_settings_store();
                Task::none()
            }
            OmniSettingAction::SetAutocompleteTablesShortcut(shortcut) => {
                self.settings.values.autocomplete_tables_shortcut = shortcut;
                self.persist_settings_store();
                Task::none()
            }
            OmniSettingAction::SetOmniCommandAltShortcut(shortcut) => {
                self.settings.values.omni_command_alt_shortcut = shortcut;
                self.persist_settings_store();
                Task::none()
            }
            OmniSettingAction::SetAiProvider(provider) => {
                self.update(Message::Settings(crate::app::features::settings::Message::AiProviderSelected(provider)))
            }
            OmniSettingAction::SetAiEndpoint(endpoint) => {
                self.update(Message::Settings(crate::app::features::settings::Message::AiEndpointChanged(endpoint)))
            }
            OmniSettingAction::SetAiModel(model) => self.update(Message::Settings(crate::app::features::settings::Message::AiModelChanged(model))),
            OmniSettingAction::SetAiApiKey(key) => self.update(Message::Settings(crate::app::features::settings::Message::AiApiKeyChanged(key))),
            OmniSettingAction::SetOmniPrefix(prefix) => {
                self.update(Message::Settings(crate::app::features::settings::Message::OmniPrefixChanged(prefix)))
            }
            OmniSettingAction::SetEmphasizeColumnHeaders(enabled) => {
                self.update(Message::Settings(crate::app::features::settings::Message::EmphasizeColumnHeadersToggled(enabled)))
            }
            OmniSettingAction::SetAutoScrollSidebarToSelectedTable(enabled) => {
                self.update(Message::Settings(crate::app::features::settings::Message::AutoScrollSidebarToSelectedTableToggled(enabled)))
            }
        }
    }

    pub(crate) fn open_omni_command_scope(&mut self, scope: OmniCommandScope) -> Task<Message> {
        if matches!(
            scope,
            OmniCommandScope::FocusTargets
                | OmniCommandScope::FolderTableTargets
                | OmniCommandScope::FolderDissolveTargets
        ) && !self.connections.connected
        {
            return Task::none();
        }
        if let Some(omni_bar) = self.shell.omni_bar.as_mut() {
            omni_bar.mode = MODE_COMMAND;
            omni_bar.command_scope = scope;
            omni_bar.query.clear();
            omni_bar.selected_index = 0;
            omni_bar.scroll_offset_y = 0.0;
        }
        self.refresh_omni_bar_results();
        Task::batch(vec![
            iced::widget::operation::focus(omni_bar_input_id()),
            self.scroll_omni_bar_to_selection(),
        ])
    }

    pub(crate) fn execute_omni_command(&mut self, handler: OmniCommandHandler) -> Task<Message> {
        match handler {
            OmniCommandHandler::DbConnect => self.update(Message::Connections(
                crate::app::features::connections::Message::Connect,
            )),
            OmniCommandHandler::DbSwitchDatabase => {
                self.open_omni_command_scope(OmniCommandScope::Databases)
            }
            OmniCommandHandler::DbDisconnect => self.update(Message::Connections(
                crate::app::features::connections::Message::Disconnect,
            )),
            OmniCommandHandler::DbRefresh => {
                let tasks = vec![
                    self.update(Message::Connections(
                        crate::app::features::connections::Message::RefreshDatabases,
                    )),
                    self.update(Message::Connections(
                        crate::app::features::connections::Message::RefreshTables,
                    )),
                ];
                Task::batch(tasks)
            }
            OmniCommandHandler::DbReconnect => self.update(Message::Connections(
                crate::app::features::connections::Message::Connect,
            )),
            OmniCommandHandler::OpenPostgresTerminal => self.update(Message::Connections(
                crate::app::features::connections::Message::OpenPostgresTerminal,
            )),
            OmniCommandHandler::SelectDriver => {
                if self.connections.connected {
                    Task::none()
                } else {
                    self.open_omni_command_scope(OmniCommandScope::Drivers)
                }
            }
            OmniCommandHandler::NewQuery => self.update(Message::Workspace(
                crate::app::features::workspace::Message::Query(
                    crate::app::features::workspace::query::Message::NewQuery,
                ),
            )),
            OmniCommandHandler::DuplicateQueryTab => {
                if self.workspace.query.running || self.workspace.results.applying_changes {
                    return Task::none();
                }
                let current = self.workspace.query.editor.content();
                let mut state = self.blank_query_state();
                state.query = Self::new_query_editor(
                    &current,
                    &self.settings.values,
                    self.settings.theme_choice,
                );
                self.open_new_query_tab(state);
                Task::none()
            }
            OmniCommandHandler::CloseQueryTab => {
                if let Some(index) = self.workspace.tabs.active_query_tab {
                    self.close_query_tab(index)
                } else if let Some(table) = self.workspace.selected_table.clone() {
                    self.close_table_tab(table)
                } else {
                    Task::none()
                }
            }
            OmniCommandHandler::RunQuery => self.update(Message::Workspace(
                crate::app::features::workspace::Message::Query(
                    crate::app::features::workspace::query::Message::RunQuery,
                ),
            )),
            OmniCommandHandler::FormatQuery => {
                let current = self.workspace.query.editor.content();
                let formatted = format_sql_for_palette(&current);
                if let Some(edit) = iced_code_editor::compute_text_change(&current, &formatted)
                    && self.workspace.query.editor.apply_lsp_text_edits(&[edit])
                {
                    self.workspace.query.error = None;
                    self.workspace.results.apply_error = None;
                    self.workspace.results.apply_message = Some(String::from("Query formatted."));
                }
                Task::none()
            }
            OmniCommandHandler::ExplainQuery => {
                let query = self.workspace.query.editor.content();
                let trimmed = query.trim();
                if trimmed.is_empty() {
                    self.workspace.query.error = Some(String::from("Query is empty."));
                    return Task::none();
                }
                let query = if trimmed.to_ascii_lowercase().starts_with("explain ") {
                    format!("{};", trimmed.trim_end_matches(';'))
                } else {
                    format!("EXPLAIN {};", trimmed.trim_end_matches(';'))
                };
                self.set_query_text(&query);
                self.ensure_query_tab_for_run();
                self.run_query_text(query)
            }
            OmniCommandHandler::GenerateFolders => {
                let has_generated_folders = self
                    .workspace
                    .explorer
                    .table_folders
                    .iter()
                    .any(|folder| folder.auto_generated);
                if has_generated_folders {
                    let removed = self.clear_generated_folders();
                    self.workspace.results.apply_error = None;
                    self.workspace.results.apply_message = Some(crate::i18n::tr_with(
                        "Removed {count} folder assignment(s).",
                        &[("{count}", &removed.to_string())],
                    ));
                    Task::none()
                } else {
                    self.update(Message::Workspace(
                        crate::app::features::workspace::Message::Explorer(
                            crate::app::features::workspace::explorer::Message::GenerateFolders,
                        ),
                    ))
                }
            }
            OmniCommandHandler::UngroupFolders => {
                let removed = self.clear_generated_folders();
                self.workspace.results.apply_error = None;
                self.workspace.results.apply_message = Some(crate::i18n::tr_with(
                    "Removed {count} folder assignment(s).",
                    &[("{count}", &removed.to_string())],
                ));
                Task::none()
            }
            OmniCommandHandler::ExpandFolders => {
                let changed = self.set_generated_folders_open(true);
                self.workspace.results.apply_error = None;
                self.workspace.results.apply_message = Some(if changed == 0 {
                    String::from("Folders are already expanded.")
                } else {
                    crate::i18n::tr_with(
                        "Expanded {count} folder(s).",
                        &[("{count}", &changed.to_string())],
                    )
                });
                Task::none()
            }
            OmniCommandHandler::CollapseFolders => {
                let changed = self.set_generated_folders_open(false);
                self.workspace.results.apply_error = None;
                self.workspace.results.apply_message = Some(if changed == 0 {
                    String::from("Folders are already collapsed.")
                } else {
                    crate::i18n::tr_with(
                        "Collapsed {count} folder(s).",
                        &[("{count}", &changed.to_string())],
                    )
                });
                Task::none()
            }
            OmniCommandHandler::OpenSchemaDiagram => self.open_schema_diagram(),
            OmniCommandHandler::CreateFolder => self.update(Message::Workspace(
                crate::app::features::workspace::Message::Explorer(
                    crate::app::features::workspace::explorer::Message::OpenMakeFolderModal,
                ),
            )),
            OmniCommandHandler::MoveTableToFolder => {
                self.open_omni_command_scope(OmniCommandScope::FolderTableTargets)
            }
            OmniCommandHandler::RemoveFolder => {
                self.open_omni_command_scope(OmniCommandScope::FolderDissolveTargets)
            }
            OmniCommandHandler::SwitchTabs => {
                self.open_omni_command_scope(OmniCommandScope::TabSwitcher)
            }
            OmniCommandHandler::HideSidebar => {
                if self.shell.zen_mode {
                    self.workspace.results.apply_error = None;
                    self.workspace.results.apply_message = Some(String::from(
                        "Sidebar controls are unavailable in zen mode.",
                    ));
                    return Task::none();
                }
                self.shell.sidebar_hidden = true;
                self.shell.panes.resize(self.shell.sidebar_split, 0.0);
                self.workspace.results.apply_error = None;
                self.workspace.results.apply_message = Some(String::from("Sidebar hidden."));
                Task::none()
            }
            OmniCommandHandler::ShowSidebar => {
                if self.shell.zen_mode {
                    self.workspace.results.apply_error = None;
                    self.workspace.results.apply_message = Some(String::from(
                        "Sidebar controls are unavailable in zen mode.",
                    ));
                    return Task::none();
                }
                self.shell.sidebar_hidden = false;
                let restored_ratio = if self.shell.sidebar_ratio <= 0.0 {
                    0.24
                } else {
                    self.shell.sidebar_ratio
                };
                self.shell
                    .panes
                    .resize(self.shell.sidebar_split, restored_ratio.clamp(0.05, 0.95));
                self.workspace.results.apply_error = None;
                self.workspace.results.apply_message = Some(String::from("Sidebar shown."));
                Task::none()
            }
            OmniCommandHandler::HideTabs => {
                self.workspace.tabs.tabs_hidden = true;
                self.workspace.tabs.tab_context_menu = None;
                self.workspace.results.apply_error = None;
                self.workspace.results.apply_message = Some(String::from("Tabs hidden."));
                Task::none()
            }
            OmniCommandHandler::ShowTabs => {
                self.workspace.tabs.tabs_hidden = false;
                self.workspace.results.apply_error = None;
                self.workspace.results.apply_message = Some(String::from("Tabs shown."));
                Task::none()
            }
            OmniCommandHandler::HideQueryEditor => {
                self.shell.query_editor_hidden = true;
                self.workspace.results.apply_error = None;
                self.workspace.results.apply_message = Some(String::from("Query editor hidden."));
                Task::none()
            }
            OmniCommandHandler::ShowQueryEditor => {
                self.shell.query_editor_hidden = false;
                let clamped_editor_ratio = self.clamp_editor_ratio(self.shell.editor_ratio);
                self.shell.editor_ratio = clamped_editor_ratio;
                self.shell
                    .panes
                    .resize(self.shell.editor_split, clamped_editor_ratio);
                self.workspace.results.apply_error = None;
                self.workspace.results.apply_message = Some(String::from("Query editor shown."));
                Task::none()
            }
            OmniCommandHandler::ToggleZenMode => {
                let task = self.update(Message::Shell(crate::app::shell::Message::ToggleZenMode));
                self.workspace.results.apply_error = None;
                self.workspace.results.apply_message = Some(if self.shell.zen_mode {
                    String::from("Zen mode enabled.")
                } else {
                    String::from("Zen mode disabled.")
                });
                task
            }
            OmniCommandHandler::FocusOn => {
                if self.connections.connected {
                    self.open_omni_command_scope(OmniCommandScope::FocusTargets)
                } else {
                    Task::none()
                }
            }
            OmniCommandHandler::FocusSidebar => self.focus_sidebar_input(),
            OmniCommandHandler::FocusQueryEditor => self.focus_query_editor_input(),
            OmniCommandHandler::FocusTableResults => self.focus_table_results(),
            OmniCommandHandler::FocusOmniBar => iced::widget::operation::focus(omni_bar_input_id()),
            OmniCommandHandler::ClearQueryEditor => {
                self.set_query_text("");
                self.workspace.query.table_query = None;
                self.workspace.query.editable_query = None;
                self.workspace.query.table = None;
                self.workspace.query.table_has_next_page = false;
                self.workspace.query.last_query_was_table = false;
                self.workspace.query.error = None;
                self.workspace.results.apply_error = None;
                self.workspace.results.apply_message = Some(String::from("Query editor cleared."));
                Task::none()
            }
            OmniCommandHandler::OpenFavoriteConnection => {
                self.open_omni_command_scope(OmniCommandScope::FavoriteConnections)
            }
            OmniCommandHandler::OpenRecentConnection | OmniCommandHandler::SwitchConnection => {
                self.open_omni_command_scope(OmniCommandScope::RecentConnections)
            }
            OmniCommandHandler::OpenSettingsCommands => {
                self.open_omni_command_scope(OmniCommandScope::SettingsRoot)
            }
            OmniCommandHandler::OpenSettingsModal => self.update(Message::Settings(
                crate::app::features::settings::Message::Settings,
            )),
            OmniCommandHandler::CheckForUpdates => {
                self.update(Message::Updater(updater::Message::Check { announce: true }))
            }
            OmniCommandHandler::OpenSettingsThemes => {
                self.open_omni_command_scope(OmniCommandScope::SettingsThemes)
            }
            OmniCommandHandler::OpenSettingsFonts => {
                self.open_omni_command_scope(OmniCommandScope::SettingsFonts)
            }
            OmniCommandHandler::OpenSettingsFontSizes => {
                self.open_omni_command_scope(OmniCommandScope::SettingsFontSizes)
            }
            OmniCommandHandler::OpenSettingsUiDensity => {
                self.open_omni_command_scope(OmniCommandScope::SettingsUiDensity)
            }
            OmniCommandHandler::OpenSettingsAiProviders => {
                self.open_omni_command_scope(OmniCommandScope::SettingsAiProviders)
            }
            OmniCommandHandler::OpenSettingsTableShortcuts => {
                self.open_omni_command_scope(OmniCommandScope::SettingsTableShortcuts)
            }
            OmniCommandHandler::OpenSettingsCommandShortcuts => {
                self.open_omni_command_scope(OmniCommandScope::SettingsCommandShortcuts)
            }
            OmniCommandHandler::ReopenLastQuery => {
                if let Some(query) = self.workspace.query.history.first().cloned() {
                    self.set_query_text(&query);
                    self.workspace.query.table_query = None;
                    self.workspace.query.editable_query = None;
                    self.workspace.query.table = None;
                    self.workspace.query.error = None;
                    self.workspace.results.apply_error = None;
                    self.workspace.results.apply_message =
                        Some(String::from("Last query restored."));
                }
                Task::none()
            }
            OmniCommandHandler::ClearRecentQueries => self.update(Message::Workspace(
                crate::app::features::workspace::Message::Query(
                    crate::app::features::workspace::query::Message::ClearQueryHistory,
                ),
            )),
            OmniCommandHandler::GenerateSql => self.update(Message::Ai(
                crate::app::features::ai::Message::ToggleChatSidebar,
            )),
            OmniCommandHandler::FixQueryWithAi => self.update(Message::Ai(
                crate::app::features::ai::Message::FixQueryWithAi,
            )),
            OmniCommandHandler::OptimizeSql => {
                let query = self.workspace.query.editor.content();
                let trimmed = query.trim();
                if trimmed.is_empty() {
                    self.workspace.query.error = Some(String::from("Query is empty."));
                    return Task::none();
                }
                let prompt = format!(
                    "Optimize this MySQL query for performance and readability. \
Return only SQL.\n\n{}",
                    trimmed
                );
                self.ai.ai_prompt_content = text_editor::Content::with_text(&prompt);
                self.update(Message::Ai(
                    crate::app::features::ai::Message::GenerateAiQuery,
                ))
            }
            OmniCommandHandler::ExplainSchema => {
                let table_list = if self.workspace.explorer.tables.is_empty() {
                    String::from("(no tables loaded)")
                } else {
                    self.workspace
                        .explorer
                        .tables
                        .iter()
                        .take(80)
                        .map(|table| format!("- {table}"))
                        .collect::<Vec<_>>()
                        .join("\n")
                };
                let prompt = format!(
                    "Generate SQL statements that explain the schema and key \
relationships for these tables. Return only SQL.\n\n{}",
                    table_list
                );
                self.ai.ai_prompt_content = text_editor::Content::with_text(&prompt);
                self.update(Message::Ai(
                    crate::app::features::ai::Message::GenerateAiQuery,
                ))
            }
        }
    }

    pub(crate) fn record_recent_omni_command(&mut self, handler: OmniCommandHandler) {
        let Some(id) = self
            .shell
            .omni_commands
            .iter()
            .find(|command| command.handler == handler)
            .map(|command| command.id.to_string())
        else {
            return;
        };
        self.settings
            .values
            .recent_omni_commands
            .retain(|recent| recent != &id);
        self.settings.values.recent_omni_commands.insert(0, id);
        self.settings.values.recent_omni_commands.truncate(4);
        self.persist_settings_store();
    }

    pub(crate) fn omni_command_visible(&self, handler: OmniCommandHandler) -> bool {
        let has_query = !self.workspace.query.editor.content().trim().is_empty();

        if !self.connections.connected {
            return match handler {
                OmniCommandHandler::DbConnect => true,
                OmniCommandHandler::SelectDriver => !self.connections.connecting,
                OmniCommandHandler::OpenPostgresTerminal => false,
                OmniCommandHandler::OpenFavoriteConnection => {
                    !self.connections.favorites.is_empty()
                }
                OmniCommandHandler::OpenRecentConnection | OmniCommandHandler::SwitchConnection => {
                    !self.connections.recents.is_empty()
                }
                OmniCommandHandler::OpenSettingsCommands
                | OmniCommandHandler::OpenSettingsModal
                | OmniCommandHandler::OpenSettingsThemes
                | OmniCommandHandler::OpenSettingsFonts
                | OmniCommandHandler::OpenSettingsFontSizes
                | OmniCommandHandler::OpenSettingsUiDensity
                | OmniCommandHandler::OpenSettingsAiProviders
                | OmniCommandHandler::OpenSettingsTableShortcuts
                | OmniCommandHandler::OpenSettingsCommandShortcuts
                | OmniCommandHandler::CheckForUpdates
                | OmniCommandHandler::FocusOmniBar => true,
                _ => false,
            };
        }

        match handler {
            OmniCommandHandler::DbConnect => false,
            OmniCommandHandler::DbSwitchDatabase => !self.connections.databases.is_empty(),
            OmniCommandHandler::DbDisconnect
            | OmniCommandHandler::DbRefresh
            | OmniCommandHandler::DbReconnect => true,
            OmniCommandHandler::OpenPostgresTerminal => self.can_open_postgres_terminal(),
            OmniCommandHandler::SelectDriver => false,
            OmniCommandHandler::NewQuery | OmniCommandHandler::DuplicateQueryTab => {
                !self.workspace.query.running && !self.workspace.results.applying_changes
            }
            OmniCommandHandler::CloseQueryTab => {
                !self.workspace.query.running
                    && !self.workspace.results.applying_changes
                    && self.workspace.tabs.tab_strip.len() > 1
                    && (self.workspace.tabs.active_query_tab.is_some()
                        || self.workspace.selected_table.is_some())
            }
            OmniCommandHandler::RunQuery | OmniCommandHandler::FormatQuery => {
                has_query && !self.workspace.query.running
            }
            OmniCommandHandler::ExplainQuery => has_query,
            OmniCommandHandler::GenerateSql => true,
            OmniCommandHandler::FixQueryWithAi => {
                self.workspace
                    .query
                    .error
                    .as_ref()
                    .is_some_and(|error| should_offer_ai_query_fix(error))
                    && !self.ai.is_generating_ai
                    && !self.ai.is_fixing_query_with_ai
                    && !self.workspace.query.running
            }
            OmniCommandHandler::OptimizeSql => {
                has_query && !self.ai.is_generating_ai && !self.ai.is_fixing_query_with_ai
            }
            OmniCommandHandler::ExplainSchema => !self.workspace.explorer.tables.is_empty(),
            OmniCommandHandler::CheckForUpdates => !self.updater.status.is_busy(),
            OmniCommandHandler::GenerateFolders => {
                !self.workspace.explorer.tables.is_empty()
                    && !self.workspace.explorer.folder_generation_running
            }
            OmniCommandHandler::UngroupFolders
            | OmniCommandHandler::ExpandFolders
            | OmniCommandHandler::CollapseFolders => self
                .workspace
                .explorer
                .table_folders
                .iter()
                .any(|folder| folder.auto_generated),
            OmniCommandHandler::OpenSchemaDiagram => self.connections.connected,
            OmniCommandHandler::CreateFolder => true,
            OmniCommandHandler::MoveTableToFolder => {
                !self.workspace.explorer.tables.is_empty()
                    && !self.workspace.explorer.table_folders.is_empty()
            }
            OmniCommandHandler::RemoveFolder => !self.workspace.explorer.table_folders.is_empty(),
            OmniCommandHandler::SwitchTabs => {
                let mut visible = self.workspace.tabs.tab_strip.iter().filter(|entry| {
                    matches!(entry, TabEntry::Query(_))
                        || (self.settings.values.tabs_enabled
                            && matches!(entry, TabEntry::Table(_)))
                });
                visible.next().is_some() && visible.next().is_some()
            }
            OmniCommandHandler::HideSidebar => !self.shell.zen_mode && !self.shell.sidebar_hidden,
            OmniCommandHandler::ShowSidebar => !self.shell.zen_mode && self.shell.sidebar_hidden,
            OmniCommandHandler::HideTabs => !self.workspace.tabs.tabs_hidden,
            OmniCommandHandler::ShowTabs => self.workspace.tabs.tabs_hidden,
            OmniCommandHandler::HideQueryEditor => !self.shell.query_editor_hidden,
            OmniCommandHandler::ShowQueryEditor => self.shell.query_editor_hidden,
            OmniCommandHandler::FocusOn => true,
            OmniCommandHandler::FocusSidebar => !self.is_sidebar_hidden(),
            OmniCommandHandler::FocusQueryEditor => !self.shell.query_editor_hidden,
            OmniCommandHandler::FocusTableResults => self.workspace.results.current.is_some(),
            OmniCommandHandler::FocusOmniBar => true,
            OmniCommandHandler::ClearQueryEditor => has_query,
            OmniCommandHandler::OpenFavoriteConnection => !self.connections.favorites.is_empty(),
            OmniCommandHandler::OpenRecentConnection | OmniCommandHandler::SwitchConnection => {
                !self.connections.recents.is_empty()
            }
            OmniCommandHandler::ReopenLastQuery => !self.workspace.query.history.is_empty(),
            OmniCommandHandler::ClearRecentQueries => !self.workspace.query.history.is_empty(),
            OmniCommandHandler::OpenSettingsCommands
            | OmniCommandHandler::OpenSettingsModal
            | OmniCommandHandler::OpenSettingsThemes
            | OmniCommandHandler::OpenSettingsFonts
            | OmniCommandHandler::OpenSettingsFontSizes
            | OmniCommandHandler::OpenSettingsUiDensity
            | OmniCommandHandler::OpenSettingsAiProviders
            | OmniCommandHandler::OpenSettingsTableShortcuts
            | OmniCommandHandler::OpenSettingsCommandShortcuts
            | OmniCommandHandler::ToggleZenMode => true,
        }
    }
}
