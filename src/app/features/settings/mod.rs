pub(crate) mod types;
use crate::ui::theme::AppearanceMode;
mod reset;
use crate::constants::MIN_FONT_SIZE;
use crate::utils::helpers::normalize_omni_prefix;
pub(crate) mod view;
use crate::model::settings::ChatMode;
use crate::model::settings::{ResultGridDensity, ShortcutBinding};
use iced::widget::scrollable;
pub(crate) mod effects;
use self::types::{FontPickerTarget, SettingsModalTab, ShortcutCaptureTarget};
use crate::ai::{AiMessage, AiProvider, AiRequestConfig};
use crate::constants::{
    AI_TEST_GREETING_MAX_CHARS, AI_TEST_PROMPT, DEFAULT_TAB_SIZE,
    MAX_QUERY_INLINE_SUGGESTION_DELAY_MS, MIN_QUERY_INLINE_SUGGESTION_DELAY_MS,
    SAVED_DIAGRAM_PAGE_ROWS,
};
use crate::i18n::Language;
use crate::model::settings::{AppearanceColor, SystemThemeMode, UiDensity};
use crate::model::settings::{FontChoice, Settings};
use crate::ui::theme::ThemeChoice;
use crate::ui::theme::ThemeVariant;
use iced::Task;

pub(crate) struct State {
    pub(crate) values: Settings,
    pub(crate) settings_store_error: Option<String>,
    pub(crate) ai_connection_testing: bool,
    pub(crate) ai_test_status: Option<Result<String, String>>,
    pub(crate) settings_diagram_clear_confirmation: bool,
    pub(crate) settings_diagram_visible_rows: usize,
    pub(crate) theme_search: String,
    pub(crate) settings_theme_search: String,
    pub(crate) font_search: String,
    pub(crate) theme_choice: ThemeChoice,
    pub(crate) font_choices: Vec<FontChoice>,
    pub(crate) font_choices_loaded: bool,
    pub(crate) settings_open: bool,
    pub(crate) settings_modal_tab: SettingsModalTab,
    pub(crate) shortcut_capture_target: Option<ShortcutCaptureTarget>,
    pub(crate) theme_picker_open: bool,
    pub(crate) settings_theme_picker_open: bool,
    pub(crate) font_picker_open: bool,
    pub(crate) font_picker_target: FontPickerTarget,
}

#[derive(Debug, Clone)]
pub(crate) enum Message {
    AppearanceModeSelected(AppearanceMode),
    FontsSelected(FontChoice),
    AiEnabled(bool),
    CompleteSetup,
    SavedDiagramsAvailable(bool),
    TextEdited(crate::ui::widgets::history_input::Edit<Message>),
    Tooltip(String),
    ClearTooltip,
    ModalBlocked,
    OpenChangelog,
    CheckForUpdates,
    Settings,
    ToggleSettingsThemePicker,
    ToggleFontPicker(FontPickerTarget),
    ResetSettingsSection(SettingsModalTab),
    ClearSavedDiagramsRequested,
    ClearSavedDiagramsConfirmed,
    CloseSettings,
    OmniTableShortcutSelected(ShortcutBinding),
    OmniCommandShortcutSelected(ShortcutBinding),
    OmniPrefixChanged(String),
    FontSizeSelected(u32),
    FontSizeApplied,
    ResultGridDensitySelected(ResultGridDensity),
    TableQueryLimitChanged(String),
    HistoryLimitChanged(String),
    InactiveTabReleaseIdleSecsChanged(String),
    TabsToggled(bool),
    ReleaseInactiveTabMemoryToggled(bool),
    SchemaAutocompleteToggled(bool),
    SqlKeywordAutocompleteToggled(bool),
    MultipleConnectionsLayoutToggled(bool),

    SettingsThemeSearchChanged(String),
    FontSearchChanged(String),
    SavedDiagramsScrolled(scrollable::Viewport),
    ChatModeSelected(ChatMode),
    LanguageSelected(Language),
    ThemeSelected(ThemeChoice),
    ThemeVariantSelected(ThemeVariant),
    SystemThemeModeSelected(SystemThemeMode),
    DarkThemeSelected(ThemeChoice),
    LightThemeSelected(ThemeChoice),
    BeginShortcutCapture(ShortcutCaptureTarget),
    SettingsTabSelected(SettingsModalTab),
    ClearSavedDiagramsCancelled,
    FontSelected(FontPickerTarget, FontChoice),
    FontChoicesLoaded(Vec<FontChoice>),
    FontSizeChanged(String),
    UiDensitySelected(UiDensity),
    AccentColorSelected(AppearanceColor),
    LargeSidebarButtonsToggled(bool),
    ModalBackdropDimChanged(f32),
    EmphasizeColumnHeadersToggled(bool),
    AutoScrollSidebarToSelectedTableToggled(bool),
    AiProviderSelected(AiProvider),
    AiEndpointChanged(String),
    AiModelChanged(String),
    AiApiKeyChanged(String),
    AiCliCommandChanged(String),
    AiAutocompleteToggled(bool),
    AiAutocompleteUseMainProviderToggled(bool),
    AiAutocompleteAllowLocalCliToggled(bool),
    AiSendTemperatureToggled(bool),
    AiAutocompleteProviderSelected(AiProvider),
    AiAutocompleteEndpointChanged(String),
    AiAutocompleteModelChanged(String),
    AiAutocompleteApiKeyChanged(String),
    AiSendSchemaContextToggled(bool),
    AiSendQueryContextToggled(bool),
    TestAiProviderConnection,
    AiCliPresetSelected(&'static str),
    AiProviderConnectionTested(Result<String, String>),
    QueryInlineSuggestionDelayMsChanged(String),
    QueryEditorLineNumbersToggled(bool),
    QueryEditorWordWrapToggled(bool),
    CompactSidebarToggled(bool),
    AutoExpandSelectedTableToggled(bool),
    ShowHiddenTablesToggled(bool),
    TabSizeChanged(String),
    InsertSpacesToggled(bool),
    ConnectionTimeoutSecsChanged(String),
    QueryTimeoutSecsChanged(String),
    AutoReconnectToggled(bool),
}

pub(crate) enum Output {
    AiEnabled(bool),
    TextEdited(crate::ui::widgets::history_input::Edit<Message>),
    Tooltip(Option<String>),
    OpenChangelog,
    CheckForUpdates,
    Opened,
    ThemePickerOpened,
    FontPickerOpened,
    Closed,
    ResetResultsViewport,
    ConnectionsLayoutChanged(bool),
    TabsChanged(bool),
    ReleaseInactiveMemoryChanged(bool),
    ReloadTableLimit,
    TrimHistory(usize),
    PruneInactiveMemory,
    RefreshAutocomplete,
    CheckSavedDiagrams,
    ClearSavedDiagrams,
    ClearTableCache,
    ReloadSelectedTable,
    RestoreSelectedTableTab,
    CloseSuggestions,
    Success(String),
    SyncEditor,
    ClearInlineSuggestion,
    Error(String),
}

impl State {
    pub(crate) fn new(
        values: Settings,
        theme_choice: ThemeChoice,
        settings_store_error: Option<String>,
    ) -> Self {
        Self {
            values,
            settings_store_error,
            ai_connection_testing: false,
            ai_test_status: None,
            settings_diagram_clear_confirmation: false,
            settings_diagram_visible_rows: SAVED_DIAGRAM_PAGE_ROWS,
            theme_search: String::new(),
            settings_theme_search: String::new(),
            font_search: String::new(),
            theme_choice,
            font_choices: vec![FontChoice::Sans, FontChoice::Monospace],
            font_choices_loaded: true,
            settings_open: false,
            settings_modal_tab: SettingsModalTab::Appearance,
            shortcut_capture_target: None,
            theme_picker_open: false,
            settings_theme_picker_open: false,
            font_picker_open: false,
            font_picker_target: FontPickerTarget::Ui,
        }
    }

    pub(crate) fn update(&mut self, message: Message) -> (Task<Message>, Vec<Output>) {
        let mut outputs = Vec::new();
        let task = self.update_internal(message, &mut outputs);
        (task, outputs)
    }

    fn update_internal(&mut self, message: Message, outputs: &mut Vec<Output>) -> Task<Message> {
        match message {
            Message::AppearanceModeSelected(mode) => {
                let was_dark = crate::ui::theme::is_dark_theme(&self.theme());
                self.values.system_theme_mode = match mode {
                    AppearanceMode::System => SystemThemeMode::System,
                    _ => SystemThemeMode::Manual,
                };
                match mode {
                    AppearanceMode::Dark if !was_dark => {
                        self.values.light_theme = self.theme_choice;
                        self.theme_choice = self.values.dark_theme;
                    }
                    AppearanceMode::Light if was_dark => {
                        self.values.dark_theme = self.theme_choice;
                        self.theme_choice = self.values.light_theme;
                    }
                    AppearanceMode::System => self.refresh_system_theme(outputs),
                    _ => {}
                }
                outputs.push(Output::SyncEditor);
                self.persist_settings_store();
                Task::none()
            }
            Message::FontsSelected(font) => {
                self.values.font = font.clone();
                self.values.editor_font = font;
                outputs.push(Output::SyncEditor);
                self.persist_settings_store();
                Task::none()
            }
            Message::AiEnabled(enabled) => {
                self.values.ai_enabled = enabled;
                outputs.push(Output::AiEnabled(enabled));
                self.persist_settings_store();
                Task::none()
            }
            Message::CompleteSetup => {
                self.values.onboarding_completed = true;
                self.values.onboarding_theme_variant = self.values.theme_variant;
                self.values.last_changelog_version =
                    Some(crate::ui::changelog::current_version().to_string());
                self.persist_settings_store();
                Task::none()
            }
            Message::SavedDiagramsAvailable(available) => {
                if available {
                    self.settings_diagram_clear_confirmation = true;
                }
                Task::none()
            }
            Message::TextEdited(edit) => {
                outputs.push(Output::TextEdited(edit));
                Task::none()
            }
            Message::Tooltip(text) => {
                outputs.push(Output::Tooltip(Some(text)));
                Task::none()
            }
            Message::ClearTooltip => {
                outputs.push(Output::Tooltip(None));
                Task::none()
            }
            Message::OpenChangelog => {
                outputs.push(Output::OpenChangelog);
                Task::none()
            }
            Message::CheckForUpdates => {
                outputs.push(Output::CheckForUpdates);
                Task::none()
            }
            Message::ModalBlocked => Task::none(),
            Message::Settings => {
                self.settings_open = true;
                self.settings_modal_tab = SettingsModalTab::Appearance;
                self.shortcut_capture_target = None;
                self.theme_picker_open = false;
                self.theme_search.clear();
                self.settings_theme_picker_open = false;
                self.settings_theme_search.clear();
                self.font_picker_open = false;
                self.font_search.clear();
                outputs.push(Output::Opened);
                Task::none()
            }
            Message::ToggleSettingsThemePicker => {
                if !self.settings_theme_picker_open {
                    outputs.push(Output::ThemePickerOpened);
                    self.theme_picker_open = false;
                    self.theme_search.clear();
                    self.font_picker_open = false;
                    self.font_search.clear();
                }
                self.settings_theme_picker_open = !self.settings_theme_picker_open;
                if !self.settings_theme_picker_open {
                    outputs.push(Output::ThemePickerOpened);
                    self.settings_theme_search.clear();
                }
                Task::none()
            }
            Message::ToggleFontPicker(target) => {
                let was_open_for_target =
                    self.font_picker_open && self.font_picker_target == target;
                let mut load_fonts = Task::none();
                if !was_open_for_target {
                    outputs.push(Output::FontPickerOpened);
                    self.theme_picker_open = false;
                    self.settings_theme_picker_open = false;
                    self.settings_theme_search.clear();
                    load_fonts = self.ensure_font_choices_loaded();
                    self.font_picker_target = target;
                    self.font_picker_open = true;
                } else {
                    self.font_picker_open = false;
                }
                if !self.font_picker_open {
                    self.font_search.clear();
                }
                load_fonts
            }
            Message::CloseSettings => {
                self.settings_open = false;
                self.settings_diagram_clear_confirmation = false;
                self.shortcut_capture_target = None;
                self.settings_theme_picker_open = false;
                self.settings_theme_search.clear();
                self.font_picker_open = false;
                outputs.push(Output::Closed);
                self.font_search.clear();
                Task::none()
            }
            Message::FontSizeSelected(size) => {
                self.values.font_size = size;
                self.values.font_size_input = size.to_string();
                outputs.push(Output::ResetResultsViewport);
                outputs.push(Output::SyncEditor);
                self.persist_settings_store();
                Task::none()
            }
            Message::FontSizeApplied => {
                if let Ok(parsed) = self.values.font_size_input.trim().parse::<u32>()
                    && parsed >= MIN_FONT_SIZE
                {
                    self.values.font_size = parsed;
                    outputs.push(Output::ResetResultsViewport);
                    outputs.push(Output::SyncEditor);
                    self.persist_settings_store();
                } else {
                    self.values.font_size_input = self.values.font_size.to_string();
                }
                Task::none()
            }
            Message::ResultGridDensitySelected(density) => {
                self.values.result_grid_density = density;
                outputs.push(Output::ResetResultsViewport);
                self.persist_settings_store();
                Task::none()
            }
            Message::MultipleConnectionsLayoutToggled(enabled) => {
                self.values.multiple_connections_layout = enabled;
                outputs.push(Output::ConnectionsLayoutChanged(enabled));
                self.persist_settings_store();
                Task::none()
            }
            Message::OmniTableShortcutSelected(shortcut) => {
                self.values.omni_table_shortcut = shortcut;
                self.persist_settings_store();
                Task::none()
            }
            Message::OmniCommandShortcutSelected(shortcut) => {
                self.values.omni_command_shortcut = shortcut;
                self.persist_settings_store();
                Task::none()
            }
            Message::OmniPrefixChanged(value) => {
                self.values.omni_prefix = normalize_omni_prefix(&value);
                self.persist_settings_store();
                Task::none()
            }
            Message::ClearSavedDiagramsRequested => {
                outputs.push(Output::CheckSavedDiagrams);
                Task::none()
            }
            Message::ClearSavedDiagramsConfirmed => {
                self.settings_diagram_clear_confirmation = false;
                outputs.push(Output::ClearSavedDiagrams);
                Task::none()
            }
            Message::ResetSettingsSection(tab) => self.reset_settings_section(tab, outputs),
            Message::TableQueryLimitChanged(value) => {
                self.values.table_query_limit_input = value;
                let parsed = self.values.table_query_limit_input.trim().parse::<usize>();
                if let Ok(limit) = parsed
                    && limit > 0
                {
                    self.values.table_query_limit = limit;
                    outputs.push(Output::ReloadTableLimit);
                }
                Task::none()
            }
            Message::HistoryLimitChanged(value) => {
                self.values.history_limit_input = value;
                let parsed = self.values.history_limit_input.trim().parse::<usize>();
                if let Ok(limit) = parsed {
                    self.values.history_limit = limit;
                    outputs.push(Output::TrimHistory(limit));
                    self.persist_settings_store();
                }
                Task::none()
            }
            Message::InactiveTabReleaseIdleSecsChanged(value) => {
                self.values.inactive_tab_release_idle_secs_input = value;
                let parsed = self
                    .values
                    .inactive_tab_release_idle_secs_input
                    .trim()
                    .parse::<u64>();
                if let Ok(seconds) = parsed
                    && seconds > 0
                {
                    let clamped = seconds.min(86_400);
                    self.values.inactive_tab_release_idle_secs = clamped;
                    if self.values.release_inactive_tab_memory {
                        outputs.push(Output::PruneInactiveMemory);
                    }
                    self.persist_settings_store();
                }
                Task::none()
            }
            Message::TabsToggled(enabled) => {
                self.values.tabs_enabled = enabled;
                outputs.push(Output::TabsChanged(enabled));
                self.persist_settings_store();
                Task::none()
            }
            Message::ReleaseInactiveTabMemoryToggled(enabled) => {
                self.values.release_inactive_tab_memory = enabled;
                outputs.push(Output::ReleaseInactiveMemoryChanged(enabled));
                self.persist_settings_store();
                Task::none()
            }
            Message::SchemaAutocompleteToggled(enabled) => {
                self.values.schema_autocomplete_enabled = enabled;
                outputs.push(Output::RefreshAutocomplete);
                self.persist_settings_store();
                Task::none()
            }
            Message::SqlKeywordAutocompleteToggled(enabled) => {
                self.values.sql_keyword_autocomplete_enabled = enabled;
                outputs.push(Output::RefreshAutocomplete);
                self.persist_settings_store();
                Task::none()
            }
            Message::SettingsThemeSearchChanged(search) => {
                self.settings_theme_search = search;
                Task::none()
            }
            Message::FontSearchChanged(search) => {
                self.font_search = search;
                Task::none()
            }
            Message::SavedDiagramsScrolled(viewport) => {
                if viewport.relative_offset().y > 0.75 {
                    self.settings_diagram_visible_rows = self
                        .settings_diagram_visible_rows
                        .saturating_add(SAVED_DIAGRAM_PAGE_ROWS);
                }
                Task::none()
            }
            Message::LanguageSelected(language) => {
                self.values.language = language;
                crate::i18n::set_language(language);
                self.persist_settings_store();
                Task::none()
            }
            Message::ThemeSelected(theme) => {
                self.theme_choice = theme;
                self.theme_picker_open = false;
                self.theme_search.clear();
                self.settings_theme_picker_open = false;
                self.settings_theme_search.clear();
                outputs.push(Output::SyncEditor);
                self.persist_settings_store();
                Task::none()
            }
            Message::ThemeVariantSelected(variant) => {
                self.values.theme_variant = variant;
                crate::ui::theme::set_theme_variant(variant);
                outputs.push(Output::SyncEditor);
                self.persist_settings_store();
                Task::none()
            }
            Message::SystemThemeModeSelected(mode) => {
                self.values.system_theme_mode = mode;
                self.refresh_system_theme(outputs);
                self.persist_settings_store();
                Task::none()
            }
            Message::DarkThemeSelected(theme) => {
                self.values.dark_theme = theme;
                self.refresh_system_theme(outputs);
                self.persist_settings_store();
                Task::none()
            }
            Message::LightThemeSelected(theme) => {
                self.values.light_theme = theme;
                self.refresh_system_theme(outputs);
                self.persist_settings_store();
                Task::none()
            }
            Message::ChatModeSelected(mode) => {
                self.values.chat_mode = mode;
                self.persist_settings_store();
                Task::none()
            }
            Message::FontChoicesLoaded(choices) => {
                self.apply_font_choices(choices);
                Task::none()
            }
            Message::FontSelected(target, font) => {
                match target {
                    FontPickerTarget::Ui => self.values.font = font,
                    FontPickerTarget::Editor => self.values.editor_font = font,
                }
                self.font_picker_open = false;
                self.font_search.clear();
                outputs.push(Output::SyncEditor);
                self.persist_settings_store();
                Task::none()
            }
            Message::FontSizeChanged(value) => {
                self.values.font_size_input = value;
                Task::none()
            }
            Message::UiDensitySelected(density) => {
                self.values.ui_density = density;
                self.persist_settings_store();
                Task::none()
            }
            Message::AccentColorSelected(color) => {
                self.values.accent_color = color;
                if !self.values.onboarding_completed {
                    self.values.default_accent_color = color;
                }
                outputs.push(Output::SyncEditor);
                self.persist_settings_store();
                Task::none()
            }
            Message::LargeSidebarButtonsToggled(enabled) => {
                self.values.large_sidebar_buttons = enabled;
                self.persist_settings_store();
                Task::none()
            }
            Message::ModalBackdropDimChanged(dim) => {
                if dim.is_finite() {
                    self.values.modal_backdrop_dim = dim.clamp(0.20, 1.0);
                    self.persist_settings_store();
                }
                Task::none()
            }
            Message::EmphasizeColumnHeadersToggled(enabled) => {
                self.values.emphasize_column_headers = enabled;
                self.persist_settings_store();
                Task::none()
            }
            Message::AutoScrollSidebarToSelectedTableToggled(enabled) => {
                self.values.auto_scroll_sidebar_to_selected_table = enabled;
                self.persist_settings_store();
                Task::none()
            }
            Message::CompactSidebarToggled(enabled) => {
                self.values.compact_sidebar = enabled;
                self.persist_settings_store();
                Task::none()
            }
            Message::AutoExpandSelectedTableToggled(enabled) => {
                self.values.auto_expand_selected_table = enabled;
                self.persist_settings_store();
                Task::none()
            }
            Message::ShowHiddenTablesToggled(enabled) => {
                self.values.show_hidden_tables = enabled;
                self.persist_settings_store();
                Task::none()
            }
            Message::TabSizeChanged(value) => {
                self.values.tab_size = value.parse().unwrap_or(DEFAULT_TAB_SIZE).clamp(1, 16);
                outputs.push(Output::SyncEditor);
                self.persist_settings_store();
                Task::none()
            }
            Message::InsertSpacesToggled(enabled) => {
                self.values.insert_spaces = enabled;
                outputs.push(Output::SyncEditor);
                self.persist_settings_store();
                Task::none()
            }
            Message::ConnectionTimeoutSecsChanged(value) => {
                self.values.connection_timeout_secs_input = value.clone();
                if let Ok(parsed) = value.parse::<u64>() {
                    self.values.connection_timeout_secs = parsed.max(1);
                }
                self.persist_settings_store();
                Task::none()
            }
            Message::QueryTimeoutSecsChanged(value) => {
                self.values.query_timeout_secs_input = value.clone();
                if let Ok(parsed) = value.parse::<u64>() {
                    self.values.query_timeout_secs = parsed.max(1);
                }
                self.persist_settings_store();
                Task::none()
            }
            Message::AutoReconnectToggled(enabled) => {
                self.values.auto_reconnect = enabled;
                self.persist_settings_store();
                Task::none()
            }
            Message::BeginShortcutCapture(target) => {
                self.shortcut_capture_target = Some(target);
                Task::none()
            }
            Message::SettingsTabSelected(tab) => {
                self.settings_modal_tab = tab;
                self.settings_diagram_clear_confirmation = false;
                self.settings_diagram_visible_rows = SAVED_DIAGRAM_PAGE_ROWS;
                self.shortcut_capture_target = None;
                self.settings_theme_picker_open = false;
                self.settings_theme_search.clear();
                self.font_picker_open = false;
                self.font_search.clear();
                Task::none()
            }
            Message::ClearSavedDiagramsCancelled => {
                self.settings_diagram_clear_confirmation = false;
                Task::none()
            }
            Message::AiProviderSelected(provider) => {
                self.values.ai_provider = provider;
                self.ai_test_status = None;
                self.persist_settings_store();
                Task::none()
            }
            Message::AiEndpointChanged(value) => {
                self.values.ai_endpoint = value;
                self.ai_test_status = None;
                outputs.push(Output::ClearInlineSuggestion);
                self.persist_settings_store();
                Task::none()
            }
            Message::AiModelChanged(value) => {
                self.values.ai_model = value;
                self.ai_test_status = None;
                outputs.push(Output::ClearInlineSuggestion);
                self.persist_settings_store();
                Task::none()
            }
            Message::AiApiKeyChanged(value) => {
                self.values.ai_api_key = value;
                self.persist_settings_store();
                Task::none()
            }
            Message::AiCliCommandChanged(value) => {
                self.values.ai_cli_command = value;
                self.ai_test_status = None;
                outputs.push(Output::ClearInlineSuggestion);
                self.persist_settings_store();
                Task::none()
            }
            Message::AiAutocompleteToggled(enabled) => {
                self.values.ai_autocomplete_enabled = enabled;
                outputs.push(Output::ClearInlineSuggestion);
                self.persist_settings_store();
                Task::none()
            }
            Message::AiSendTemperatureToggled(enabled) => {
                self.values.ai_send_temperature = enabled;
                self.ai_test_status = None;
                self.persist_settings_store();
                Task::none()
            }
            Message::AiAutocompleteAllowLocalCliToggled(enabled) => {
                self.values.ai_autocomplete_allow_local_cli = enabled;
                outputs.push(Output::ClearInlineSuggestion);
                self.persist_settings_store();
                Task::none()
            }
            Message::AiAutocompleteUseMainProviderToggled(enabled) => {
                self.values.ai_autocomplete_use_main_provider = enabled;
                outputs.push(Output::ClearInlineSuggestion);
                self.persist_settings_store();
                Task::none()
            }
            Message::AiAutocompleteProviderSelected(provider) => {
                self.values.ai_autocomplete_provider = provider;
                outputs.push(Output::ClearInlineSuggestion);
                self.persist_settings_store();
                Task::none()
            }
            Message::AiAutocompleteEndpointChanged(value) => {
                self.values.ai_autocomplete_endpoint = value;
                outputs.push(Output::ClearInlineSuggestion);
                self.persist_settings_store();
                Task::none()
            }
            Message::AiAutocompleteModelChanged(value) => {
                self.values.ai_autocomplete_model = value;
                outputs.push(Output::ClearInlineSuggestion);
                self.persist_settings_store();
                Task::none()
            }
            Message::AiAutocompleteApiKeyChanged(value) => {
                self.values.ai_autocomplete_api_key = value;
                self.persist_settings_store();
                Task::none()
            }
            Message::AiSendSchemaContextToggled(enabled) => {
                self.values.ai_send_schema_context = enabled;
                self.persist_settings_store();
                Task::none()
            }
            Message::AiSendQueryContextToggled(enabled) => {
                self.values.ai_send_query_context = enabled;
                self.persist_settings_store();
                Task::none()
            }
            Message::TestAiProviderConnection => {
                if self.ai_connection_testing {
                    return Task::none();
                }
                let config = AiRequestConfig::from_settings(&self.values);
                if !config.is_usable() {
                    outputs.push(Output::Error(String::from(
                        "Configure the provider before testing the connection.",
                    )));
                    return Task::none();
                }

                self.ai_connection_testing = true;
                self.ai_test_status = None;

                Task::perform(
                    async move {
                        crate::ai::client::send_ai_messages(
                            &config,
                            vec![AiMessage::user(String::from(AI_TEST_PROMPT))],
                        )
                        .await
                    },
                    Message::AiProviderConnectionTested,
                )
            }
            Message::AiProviderConnectionTested(result) => {
                self.ai_connection_testing = false;
                self.ai_test_status = Some(match result {
                    Ok(reply) => {
                        let greeting = reply
                            .lines()
                            .map(str::trim)
                            .find(|line| !line.is_empty())
                            .unwrap_or("Hi.")
                            .chars()
                            .take(AI_TEST_GREETING_MAX_CHARS)
                            .collect::<String>();
                        Ok(greeting)
                    }
                    Err(error) => Err(Self::shorten_ai_test_error(&error)),
                });
                Task::none()
            }
            Message::AiCliPresetSelected(command) => {
                self.values.ai_cli_command = command.to_string();
                self.ai_test_status = None;
                self.persist_settings_store();
                Task::none()
            }
            Message::QueryInlineSuggestionDelayMsChanged(value) => {
                self.values.query_inline_suggestion_delay_ms_input = value.clone();
                if let Ok(parsed) = value.parse::<u64>() {
                    self.values.query_inline_suggestion_delay_ms = parsed.clamp(
                        MIN_QUERY_INLINE_SUGGESTION_DELAY_MS,
                        MAX_QUERY_INLINE_SUGGESTION_DELAY_MS,
                    );
                }
                self.persist_settings_store();
                Task::none()
            }
            Message::QueryEditorLineNumbersToggled(enabled) => {
                self.values.query_editor_line_numbers_enabled = enabled;
                outputs.push(Output::SyncEditor);
                self.persist_settings_store();
                Task::none()
            }
            Message::QueryEditorWordWrapToggled(enabled) => {
                self.values.query_editor_word_wrap_enabled = enabled;
                outputs.push(Output::SyncEditor);
                self.persist_settings_store();
                Task::none()
            }
        }
    }

    fn refresh_system_theme(&mut self, outputs: &mut Vec<Output>) {
        if self.values.system_theme_mode == SystemThemeMode::System {
            self.theme_choice = self.resolve_theme_choice();
            outputs.push(Output::SyncEditor);
        }
    }
}
