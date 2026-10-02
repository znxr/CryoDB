use super::types::{SettingsModalTab, ShortcutCaptureTarget};
use super::{Message, Output, State};
use crate::model::settings::{Settings, ShortcutBinding};
use crate::ui::theme::ThemeChoice;
use iced::{Task, keyboard};

impl State {
    pub(crate) fn reset_settings_section(
        &mut self,
        tab: SettingsModalTab,
        outputs: &mut Vec<Output>,
    ) -> Task<Message> {
        if tab == SettingsModalTab::About {
            return Task::none();
        }

        let defaults = Settings::default();

        match tab {
            SettingsModalTab::Appearance => {
                self.values.font = defaults.font;
                self.values.editor_font = defaults.editor_font;
                self.values.font_size = defaults.font_size;
                self.values.font_size_input = defaults.font_size_input;
                self.values.ui_density = defaults.ui_density;
                self.values.theme_variant = self.values.onboarding_theme_variant;
                self.values.accent_color = defaults.accent_color;
                self.values.large_sidebar_buttons = defaults.large_sidebar_buttons;
                self.values.compact_sidebar = defaults.compact_sidebar;
                self.values.result_grid_density = defaults.result_grid_density;
                self.values.modal_backdrop_dim = defaults.modal_backdrop_dim;
                self.values.emphasize_column_headers = defaults.emphasize_column_headers;
                self.values.auto_scroll_sidebar_to_selected_table =
                    defaults.auto_scroll_sidebar_to_selected_table;
                self.values.auto_expand_selected_table = defaults.auto_expand_selected_table;
                self.values.show_hidden_tables = defaults.show_hidden_tables;
                self.theme_choice = ThemeChoice::CarbonFrost;
                self.theme_picker_open = false;
                self.theme_search.clear();
                self.settings_theme_picker_open = false;
                self.settings_theme_search.clear();
                self.font_picker_open = false;
                self.font_search.clear();
                outputs.push(Output::ResetResultsViewport);
                outputs.push(Output::SyncEditor);
            }
            SettingsModalTab::Limits => {
                let table_limit_changed =
                    self.values.table_query_limit != defaults.table_query_limit;
                let cache_limits_changed = self.values.table_cache_limit_entries
                    != defaults.table_cache_limit_entries
                    || self.values.table_cache_limit_mb != defaults.table_cache_limit_mb;

                self.values.table_query_limit = defaults.table_query_limit;
                self.values.table_query_limit_input = defaults.table_query_limit_input;
                self.values.history_limit = defaults.history_limit;
                self.values.history_limit_input = defaults.history_limit_input;
                self.values.table_cache_limit_entries = defaults.table_cache_limit_entries;
                self.values.table_cache_limit_mb = defaults.table_cache_limit_mb;
                outputs.push(Output::TrimHistory(self.values.history_limit));

                if table_limit_changed || cache_limits_changed {
                    outputs.push(Output::ClearTableCache);
                    if table_limit_changed {
                        outputs.push(Output::ReloadSelectedTable);
                    }
                }
            }
            SettingsModalTab::Tabs => {
                let tabs_were_disabled = !self.values.tabs_enabled && defaults.tabs_enabled;

                self.values.tabs_enabled = defaults.tabs_enabled;
                self.values.release_inactive_tab_memory = defaults.release_inactive_tab_memory;
                self.values.inactive_tab_release_idle_secs =
                    defaults.inactive_tab_release_idle_secs;
                self.values.inactive_tab_release_idle_secs_input =
                    defaults.inactive_tab_release_idle_secs_input;

                if tabs_were_disabled {
                    outputs.push(Output::RestoreSelectedTableTab);
                }
                outputs.push(Output::ReleaseInactiveMemoryChanged(
                    self.values.release_inactive_tab_memory,
                ));
            }
            SettingsModalTab::Diagrams => {}
            SettingsModalTab::QueryEditor => {
                self.values.query_editor_line_numbers_enabled =
                    defaults.query_editor_line_numbers_enabled;
                self.values.query_editor_word_wrap_enabled =
                    defaults.query_editor_word_wrap_enabled;
                self.values.tab_size = defaults.tab_size;
                self.values.insert_spaces = defaults.insert_spaces;
                self.values.query_inline_suggestion_delay_ms =
                    defaults.query_inline_suggestion_delay_ms;
                self.values.query_inline_suggestion_delay_ms_input =
                    defaults.query_inline_suggestion_delay_ms_input;
                self.values.schema_autocomplete_enabled = defaults.schema_autocomplete_enabled;
                self.values.sql_keyword_autocomplete_enabled =
                    defaults.sql_keyword_autocomplete_enabled;
                outputs.push(Output::ClearInlineSuggestion);
                outputs.push(Output::CloseSuggestions);
            }
            SettingsModalTab::Shortcuts => {
                self.values.omni_table_shortcut = defaults.omni_table_shortcut;
                self.values.omni_command_shortcut = defaults.omni_command_shortcut;
                self.values.omni_command_alt_shortcut = defaults.omni_command_alt_shortcut;
                self.values.open_settings_shortcut = defaults.open_settings_shortcut;
                self.values.switch_database_shortcut = defaults.switch_database_shortcut;
                self.values.focus_table_search_shortcut = defaults.focus_table_search_shortcut;
                self.values.toggle_tab_pin_shortcut = defaults.toggle_tab_pin_shortcut;
                self.values.toggle_sidebar_shortcut = defaults.toggle_sidebar_shortcut;
                self.values.toggle_chat_sidebar_shortcut = defaults.toggle_chat_sidebar_shortcut;
                self.values.cycle_theme_shortcut = defaults.cycle_theme_shortcut;
                self.values.run_query_shortcut = defaults.run_query_shortcut;
                self.values.run_selection_shortcut = defaults.run_selection_shortcut;
                self.values.autocomplete_tables_shortcut = defaults.autocomplete_tables_shortcut;
                self.values.new_query_shortcut = defaults.new_query_shortcut;
                self.values.close_tab_shortcut = defaults.close_tab_shortcut;
                self.values.next_tab_shortcut = defaults.next_tab_shortcut;
                self.values.previous_tab_shortcut = defaults.previous_tab_shortcut;
                self.values.next_results_page_shortcut = defaults.next_results_page_shortcut;
                self.values.previous_results_page_shortcut =
                    defaults.previous_results_page_shortcut;
                self.values.first_results_page_shortcut = defaults.first_results_page_shortcut;
                self.values.last_results_page_shortcut = defaults.last_results_page_shortcut;
                self.values.open_table_info_sidebar_shortcut =
                    defaults.open_table_info_sidebar_shortcut;
                self.values.omni_prefix = defaults.omni_prefix;
                self.shortcut_capture_target = None;
            }
            SettingsModalTab::Connections => {
                self.values.connection_timeout_secs = defaults.connection_timeout_secs;
                self.values.connection_timeout_secs_input = defaults.connection_timeout_secs_input;
                self.values.query_timeout_secs = defaults.query_timeout_secs;
                self.values.query_timeout_secs_input = defaults.query_timeout_secs_input;
                self.values.auto_reconnect = defaults.auto_reconnect;
            }
            SettingsModalTab::AiProvider => {
                self.values.ai_provider = defaults.ai_provider;
                self.values.ai_endpoint = defaults.ai_endpoint;
                self.values.ai_model = defaults.ai_model;
                self.values.ai_api_key = defaults.ai_api_key;
                self.values.ai_cli_command = defaults.ai_cli_command;
                self.values.ai_send_temperature = defaults.ai_send_temperature;
            }
            SettingsModalTab::AiAutocomplete => {
                self.values.ai_autocomplete_enabled = defaults.ai_autocomplete_enabled;
                self.values.ai_autocomplete_use_main_provider =
                    defaults.ai_autocomplete_use_main_provider;
                self.values.ai_autocomplete_allow_local_cli =
                    defaults.ai_autocomplete_allow_local_cli;
                self.values.ai_autocomplete_provider = defaults.ai_autocomplete_provider;
                self.values.ai_autocomplete_endpoint = defaults.ai_autocomplete_endpoint;
                self.values.ai_autocomplete_model = defaults.ai_autocomplete_model;
                self.values.ai_autocomplete_api_key = defaults.ai_autocomplete_api_key;
                self.values.ai_send_schema_context = defaults.ai_send_schema_context;
                self.values.ai_send_query_context = defaults.ai_send_query_context;
                outputs.push(Output::ClearInlineSuggestion);
            }
            SettingsModalTab::About => {}
        }

        self.persist_settings_store();
        outputs.push(Output::Success(tab.reset_toast().into()));
        Task::none()
    }
    pub(crate) fn capture_shortcut_binding(
        &mut self,
        key: &keyboard::Key,
        modifiers: keyboard::Modifiers,
    ) -> Task<Message> {
        if !self.settings_open {
            self.shortcut_capture_target = None;
            return Task::none();
        }
        let Some(target) = self.shortcut_capture_target else {
            return Task::none();
        };

        if matches!(key, keyboard::Key::Named(keyboard::key::Named::Escape))
            && !modifiers.control()
            && !modifiers.shift()
            && !modifiers.alt()
            && !modifiers.logo()
        {
            self.shortcut_capture_target = None;
            return Task::none();
        }

        let Some(shortcut) = ShortcutBinding::from_key_press(key, modifiers) else {
            return Task::none();
        };

        self.set_shortcut_binding(target, shortcut);
        self.shortcut_capture_target = None;
        self.persist_settings_store();
        Task::none()
    }
    pub(crate) fn set_shortcut_binding(
        &mut self,
        target: ShortcutCaptureTarget,
        shortcut: ShortcutBinding,
    ) {
        match target {
            ShortcutCaptureTarget::OmniTable => {
                self.values.omni_table_shortcut = shortcut;
            }
            ShortcutCaptureTarget::OmniCommand => {
                self.values.omni_command_shortcut = shortcut;
            }
            ShortcutCaptureTarget::OpenSettings => {
                self.values.open_settings_shortcut = shortcut;
            }
            ShortcutCaptureTarget::SwitchDatabase => {
                self.values.switch_database_shortcut = shortcut;
            }
            ShortcutCaptureTarget::FocusTableSearch => {
                self.values.focus_table_search_shortcut = shortcut;
            }
            ShortcutCaptureTarget::ToggleTabPin => {
                self.values.toggle_tab_pin_shortcut = shortcut;
            }
            ShortcutCaptureTarget::ToggleSidebar => {
                self.values.toggle_sidebar_shortcut = shortcut;
            }
            ShortcutCaptureTarget::ToggleChatSidebar => {
                self.values.toggle_chat_sidebar_shortcut = shortcut;
            }
            ShortcutCaptureTarget::OmniCommandAlt => {
                self.values.omni_command_alt_shortcut = shortcut;
            }
            ShortcutCaptureTarget::CycleTheme => {
                self.values.cycle_theme_shortcut = shortcut;
            }
            ShortcutCaptureTarget::RunQuery => {
                self.values.run_query_shortcut = shortcut;
            }
            ShortcutCaptureTarget::RunSelection => {
                self.values.run_selection_shortcut = shortcut;
            }
            ShortcutCaptureTarget::AutocompleteTables => {
                self.values.autocomplete_tables_shortcut = shortcut;
            }
            ShortcutCaptureTarget::NewQuery => {
                self.values.new_query_shortcut = shortcut;
            }
            ShortcutCaptureTarget::CloseTab => {
                self.values.close_tab_shortcut = shortcut;
            }
            ShortcutCaptureTarget::NextTab => {
                self.values.next_tab_shortcut = shortcut;
            }
            ShortcutCaptureTarget::PreviousTab => {
                self.values.previous_tab_shortcut = shortcut;
            }
            ShortcutCaptureTarget::NextResultsPage => {
                self.values.next_results_page_shortcut = shortcut;
            }
            ShortcutCaptureTarget::PreviousResultsPage => {
                self.values.previous_results_page_shortcut = shortcut;
            }
            ShortcutCaptureTarget::FirstResultsPage => {
                self.values.first_results_page_shortcut = shortcut;
            }
            ShortcutCaptureTarget::LastResultsPage => {
                self.values.last_results_page_shortcut = shortcut;
            }
            ShortcutCaptureTarget::OpenTableInfoSidebar => {
                self.values.open_table_info_sidebar_shortcut = shortcut;
            }
        }
    }
}
