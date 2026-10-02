#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ShortcutCaptureTarget {
    OmniTable,
    OmniCommand,
    OpenSettings,
    SwitchDatabase,
    FocusTableSearch,
    ToggleTabPin,
    ToggleSidebar,
    ToggleChatSidebar,
    OmniCommandAlt,
    CycleTheme,
    RunQuery,
    RunSelection,
    AutocompleteTables,
    NewQuery,
    CloseTab,
    NextTab,
    PreviousTab,
    NextResultsPage,
    PreviousResultsPage,
    FirstResultsPage,
    LastResultsPage,
    OpenTableInfoSidebar,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SettingsModalTab {
    Appearance,
    Limits,
    Tabs,
    QueryEditor,
    Diagrams,
    Shortcuts,
    Connections,
    AiProvider,
    AiAutocomplete,
    About,
}

impl SettingsModalTab {
    pub(crate) fn reset_label(self) -> Option<&'static str> {
        match self {
            SettingsModalTab::Appearance => Some("Reset Appearance"),
            SettingsModalTab::Limits => Some("Reset Limits"),
            SettingsModalTab::Tabs => Some("Reset Tabs"),
            SettingsModalTab::QueryEditor => Some("Reset Query Editor"),
            SettingsModalTab::Diagrams => None,
            SettingsModalTab::Shortcuts => Some("Reset Shortcuts"),
            SettingsModalTab::Connections => Some("Reset Connections"),
            SettingsModalTab::AiProvider => Some("Reset Provider"),
            SettingsModalTab::AiAutocomplete => Some("Reset Autocomplete"),
            SettingsModalTab::About => None,
        }
    }

    pub(crate) fn reset_toast(self) -> &'static str {
        match self {
            SettingsModalTab::Appearance => "Appearance settings reset to defaults.",
            SettingsModalTab::Limits => "Limit settings reset to defaults.",
            SettingsModalTab::Tabs => "Tab settings reset to defaults.",
            SettingsModalTab::QueryEditor => "Query editor settings reset to defaults.",
            SettingsModalTab::Diagrams => "",
            SettingsModalTab::Shortcuts => "Shortcuts reset to defaults.",
            SettingsModalTab::Connections => "Connection settings reset to defaults.",
            SettingsModalTab::AiProvider => "AI provider settings reset to defaults.",
            SettingsModalTab::AiAutocomplete => "AI autocomplete settings reset to defaults.",
            SettingsModalTab::About => "",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FontPickerTarget {
    Ui,
    Editor,
}
use crate::model::settings::ShortcutBinding;
use iced::keyboard;

impl ShortcutBinding {
    pub(crate) fn primary_modifier_only(modifiers: keyboard::Modifiers) -> bool {
        if Self::uses_command_key() {
            modifiers.logo() && !modifiers.control() && !modifiers.alt()
        } else {
            modifiers.control() && !modifiers.logo() && !modifiers.alt()
        }
    }

    pub(crate) fn key_token_from_event(key: &keyboard::Key) -> Option<String> {
        match key {
            keyboard::Key::Character(value) => {
                if value.trim().is_empty() {
                    return Some(String::from("space"));
                }
                if value.chars().count() == 1 {
                    let ch = value.chars().next().unwrap_or_default();
                    return Self::canonicalize_key_token(&ch.to_string());
                }
                Self::canonicalize_key_token(value)
            }
            keyboard::Key::Named(named) => Self::canonicalize_key_token(&format!("{named:?}")),
            _ => None,
        }
    }

    pub(crate) fn from_key_press(
        key: &keyboard::Key,
        modifiers: keyboard::Modifiers,
    ) -> Option<Self> {
        let key = Self::key_token_from_event(key)?;
        if Self::is_modifier_key_token(&key) {
            return None;
        }
        Self::new(
            modifiers.control(),
            modifiers.shift(),
            modifiers.alt(),
            modifiers.logo(),
            key,
        )
    }

    pub(crate) fn matches_key(&self, key: &keyboard::Key, modifiers: keyboard::Modifiers) -> bool {
        let Some(event_key) = Self::key_token_from_event(key) else {
            return false;
        };
        let event_ctrl = modifiers.control() || matches!(event_key.as_str(), "ctrl" | "control");
        let event_shift = modifiers.shift() || event_key == "shift";
        let event_alt = modifiers.alt() || event_key == "alt";
        let event_logo =
            modifiers.logo() || matches!(event_key.as_str(), "logo" | "meta" | "super");
        if self.ctrl != event_ctrl
            || self.shift != event_shift
            || self.alt != event_alt
            || self.logo != event_logo
        {
            return false;
        }
        event_key == self.key
    }
}
