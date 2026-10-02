use crate::ai::AiProvider;
use crate::constants::{
    APPEARANCE_REVISION, DEFAULT_CONNECTION_TIMEOUT_SECS, DEFAULT_FONT_SIZE, DEFAULT_HISTORY_LIMIT,
    DEFAULT_INACTIVE_TAB_RELEASE_IDLE_SECS, DEFAULT_MODAL_BACKDROP_DIM,
    DEFAULT_QUERY_INLINE_SUGGESTION_DELAY_MS, DEFAULT_QUERY_TIMEOUT_SECS, DEFAULT_TAB_SIZE,
    DEFAULT_TABLE_CACHE_LIMIT_ENTRIES, DEFAULT_TABLE_CACHE_LIMIT_MB, DEFAULT_TABLE_QUERY_LIMIT,
};
use crate::i18n::Language;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ThemeVariant {
    #[default]
    Original,
    Modern,
}

impl ThemeVariant {
    pub const ALL: &'static [Self] = &[Self::Original, Self::Modern];
}

impl std::fmt::Display for ThemeVariant {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&crate::i18n::tr(match self {
            Self::Original => "Original Theme",
            Self::Modern => "Modern Theme",
        }))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum ThemeChoice {
    CarbonFrost,
    CarbonFrostNight,
    CarbonFrostAsh,
    CatppuccinLatte,
    CatppuccinFrappe,
    CatppuccinMacchiato,
    CatppuccinMocha,
    TokyoNight,
    TokyoNightStorm,
    TokyoNightLight,
    AyuLight,
    AyuMirage,
    AyuDark,
    SolarizedDark,
    Base16Mocha,
    Base16Ocean,
    Base16Eighties,
    Base24GruvboxDark,
    Base24GruvboxLight,
    InspiredGitHub,
}

impl ThemeChoice {
    pub const ALL: &'static [Self] = &[
        Self::CarbonFrost,
        Self::CarbonFrostNight,
        Self::CarbonFrostAsh,
        Self::CatppuccinLatte,
        Self::CatppuccinFrappe,
        Self::CatppuccinMacchiato,
        Self::CatppuccinMocha,
        Self::TokyoNight,
        Self::TokyoNightStorm,
        Self::TokyoNightLight,
        Self::AyuLight,
        Self::AyuMirage,
        Self::AyuDark,
        Self::SolarizedDark,
        Self::Base16Mocha,
        Self::Base16Ocean,
        Self::Base16Eighties,
        Self::Base24GruvboxDark,
        Self::Base24GruvboxLight,
        Self::InspiredGitHub,
    ];
}

impl std::fmt::Display for ThemeChoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::SolarizedDark => "Solarized Dark",
            Self::CarbonFrost => "Carbon Frost",
            Self::CarbonFrostNight => "Carbon Frost Night",
            Self::CarbonFrostAsh => "Carbon Frost Ash",
            Self::CatppuccinLatte => "Catppuccin Latte",
            Self::CatppuccinFrappe => "Catppuccin Frappe",
            Self::CatppuccinMacchiato => "Catppuccin Macchiato",
            Self::CatppuccinMocha => "Catppuccin Mocha",
            Self::TokyoNight => "Tokyo Night",
            Self::TokyoNightStorm => "Tokyo Night Storm",
            Self::TokyoNightLight => "Tokyo Night Light",
            Self::AyuLight => "Ayu Light",
            Self::AyuMirage => "Ayu Mirage",
            Self::AyuDark => "Ayu Dark",
            Self::Base16Mocha => "Mocha",
            Self::Base16Ocean => "Ocean",
            Self::Base16Eighties => "Eighties",
            Self::Base24GruvboxDark => "Gruvbox Dark",
            Self::Base24GruvboxLight => "Gruvbox Light",
            Self::InspiredGitHub => "Inspired GitHub",
        })
    }
}

impl<'de> Deserialize<'de> for ThemeChoice {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Ok(match value.as_str() {
            "CarbonFrost" => Self::CarbonFrost,
            "CarbonFrostNight" => Self::CarbonFrostNight,
            "CarbonFrostAsh" => Self::CarbonFrostAsh,
            "CatppuccinLatte" => Self::CatppuccinLatte,
            "CatppuccinFrappe" => Self::CatppuccinFrappe,
            "CatppuccinMacchiato" => Self::CatppuccinMacchiato,
            "CatppuccinMocha" => Self::CatppuccinMocha,
            "TokyoNight" => Self::TokyoNight,
            "TokyoNightStorm" => Self::TokyoNightStorm,
            "TokyoNightLight" => Self::TokyoNightLight,
            "AyuLight" => Self::AyuLight,
            "AyuMirage" => Self::AyuMirage,
            "AyuDark" => Self::AyuDark,
            "SolarizedDark" => Self::SolarizedDark,
            "Base16Mocha" => Self::Base16Mocha,
            "Base16Ocean" => Self::Base16Ocean,
            "Base16Eighties" => Self::Base16Eighties,
            "Base24GruvboxDark" => Self::Base24GruvboxDark,
            "Base24GruvboxLight" => Self::Base24GruvboxLight,
            "InspiredGitHub" => Self::InspiredGitHub,
            _ => Self::CarbonFrost,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum UiDensity {
    Compact,
    Normal,
}

impl std::fmt::Display for UiDensity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UiDensity::Compact => f.write_str(&crate::i18n::tr("Compact")),
            UiDensity::Normal => f.write_str(&crate::i18n::tr("Normal")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum ResultGridDensity {
    Compact,
    Comfortable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum SystemThemeMode {
    Manual,
    System,
}

impl SystemThemeMode {
    pub(crate) const ALL: &'static [Self] = &[Self::Manual, Self::System];
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub(crate) enum HistoryViewMode {
    #[default]
    Recent,
    Saved,
}

impl std::fmt::Display for ResultGridDensity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ResultGridDensity::Compact => f.write_str(&crate::i18n::tr("Compact")),
            ResultGridDensity::Comfortable => f.write_str(&crate::i18n::tr("Comfortable")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum AppearanceColor {
    Default,
    Blue,
    Cyan,
    Teal,
    Green,
    Amber,
    Orange,
    Red,
    Purple,
    Pink,
}

impl AppearanceColor {
    pub(crate) const ALL: &'static [Self] = &[
        Self::Default,
        Self::Blue,
        Self::Cyan,
        Self::Teal,
        Self::Green,
        Self::Amber,
        Self::Orange,
        Self::Red,
        Self::Purple,
        Self::Pink,
    ];
}

impl std::fmt::Display for AppearanceColor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AppearanceColor::Default => f.write_str(&crate::i18n::tr("Default")),
            AppearanceColor::Blue => f.write_str(&crate::i18n::tr("Blue")),
            AppearanceColor::Cyan => f.write_str(&crate::i18n::tr("Cyan")),
            AppearanceColor::Teal => f.write_str(&crate::i18n::tr("Teal")),
            AppearanceColor::Green => f.write_str(&crate::i18n::tr("Green")),
            AppearanceColor::Amber => f.write_str(&crate::i18n::tr("Amber")),
            AppearanceColor::Orange => f.write_str(&crate::i18n::tr("Orange")),
            AppearanceColor::Red => f.write_str(&crate::i18n::tr("Red")),
            AppearanceColor::Purple => f.write_str(&crate::i18n::tr("Purple")),
            AppearanceColor::Pink => f.write_str(&crate::i18n::tr("Pink")),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum FontChoice {
    Sans,
    Monospace,
    System(String),
}

impl FontChoice {
    pub(crate) fn family(&self) -> &str {
        match self {
            FontChoice::Sans | FontChoice::Monospace => "monospace",
            FontChoice::System(name) => name,
        }
    }
}

impl std::fmt::Display for FontChoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FontChoice::Sans => f.write_str("System Font"),
            FontChoice::Monospace => f.write_str("Monospace"),
            FontChoice::System(name) => f.write_str(name),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ShortcutBinding {
    pub(crate) ctrl: bool,
    pub(crate) shift: bool,
    pub(crate) alt: bool,
    pub(crate) logo: bool,
    pub(crate) key: String,
}

impl ShortcutBinding {
    pub(crate) fn uses_command_key() -> bool {
        cfg!(target_os = "macos")
    }

    fn primary_modifier_label() -> &'static str {
        if Self::uses_command_key() {
            "Command"
        } else {
            "Ctrl"
        }
    }

    fn parse_with_primary_modifier(template: &str) -> Self {
        let value = template.replace("Primary", Self::primary_modifier_label());
        Self::parse(&value).expect("valid default")
    }

    pub(crate) fn is_modifier_key_token(token: &str) -> bool {
        matches!(
            token,
            "ctrl" | "control" | "shift" | "alt" | "logo" | "meta" | "super"
        )
    }

    pub(crate) fn new(
        ctrl: bool,
        shift: bool,
        alt: bool,
        logo: bool,
        key: impl Into<String>,
    ) -> Option<Self> {
        let key = Self::canonicalize_key_token(&key.into())?;
        Some(Self {
            ctrl,
            shift,
            alt,
            logo,
            key,
        })
    }

    pub(crate) fn default_omni_table() -> Self {
        Self::parse_with_primary_modifier("Primary + P")
    }

    pub(crate) fn default_omni_command() -> Self {
        Self::parse_with_primary_modifier("Primary + Shift + O")
    }

    pub(crate) fn is_editor_palette_binding(&self) -> bool {
        self.key == "p" && self.shift && !self.alt && (self.ctrl || self.logo)
    }

    pub(crate) fn default_open_settings() -> Self {
        Self::parse_with_primary_modifier("Primary + `")
    }

    pub(crate) fn default_omni_command_alt() -> Self {
        Self::parse("F1").expect("valid default")
    }

    pub(crate) fn default_switch_database() -> Self {
        Self::parse_with_primary_modifier("Primary + Shift + D")
    }

    pub(crate) fn default_focus_table_search() -> Self {
        Self::parse_with_primary_modifier("Primary + Shift + F")
    }

    pub(crate) fn default_toggle_tab_pin() -> Self {
        Self::parse_with_primary_modifier("Primary + K")
    }

    pub(crate) fn default_toggle_sidebar() -> Self {
        Self::parse_with_primary_modifier("Primary + B")
    }

    pub(crate) fn default_toggle_chat_sidebar() -> Self {
        Self::parse_with_primary_modifier("Primary + Alt + B")
    }

    pub(crate) fn default_cycle_theme() -> Self {
        Self::parse_with_primary_modifier("Primary + Shift + L")
    }

    pub(crate) fn default_run_query() -> Self {
        Self::parse_with_primary_modifier("Primary + Shift + Enter")
    }

    pub(crate) fn default_run_selection() -> Self {
        Self::parse_with_primary_modifier("Primary + Enter")
    }

    pub(crate) fn default_autocomplete_tables() -> Self {
        Self::parse("Ctrl + Space").expect("valid default")
    }

    pub(crate) fn is_bare_tab(&self) -> bool {
        self.key == "tab" && !self.ctrl && !self.shift && !self.alt && !self.logo
    }

    pub(crate) fn default_new_query() -> Self {
        Self::parse_with_primary_modifier("Primary + T")
    }

    pub(crate) fn default_close_tab() -> Self {
        Self::parse_with_primary_modifier("Primary + W")
    }

    pub(crate) fn default_next_tab() -> Self {
        Self::parse_with_primary_modifier("Primary + Tab")
    }

    pub(crate) fn default_previous_tab() -> Self {
        Self::parse_with_primary_modifier("Primary + Shift + Tab")
    }

    fn default_results_page(key: &str) -> Self {
        if Self::uses_command_key() {
            Self::parse_with_primary_modifier(&format!("Primary + Alt + {key}"))
        } else {
            Self::parse_with_primary_modifier(&format!("Primary + {key}"))
        }
    }

    pub(crate) fn default_next_results_page() -> Self {
        Self::default_results_page("ArrowRight")
    }

    pub(crate) fn default_previous_results_page() -> Self {
        Self::default_results_page("ArrowLeft")
    }

    pub(crate) fn default_first_results_page() -> Self {
        Self::parse_with_primary_modifier("Primary + Alt + Home")
    }

    pub(crate) fn default_last_results_page() -> Self {
        Self::parse_with_primary_modifier("Primary + Alt + End")
    }

    pub(crate) fn default_open_table_info_sidebar() -> Self {
        Self::parse_with_primary_modifier("Primary + I")
    }

    pub(crate) fn table_options() -> Vec<Self> {
        ["Primary + P", "Primary + K", "Primary + O"]
            .into_iter()
            .map(|value| value.replace("Primary", Self::primary_modifier_label()))
            .filter_map(|value| Self::parse(&value))
            .collect()
    }

    pub(crate) fn command_options() -> Vec<Self> {
        [
            "Primary + Shift + O",
            "Primary + Shift + K",
            "Primary + Shift + G",
        ]
        .into_iter()
        .map(|value| value.replace("Primary", Self::primary_modifier_label()))
        .filter_map(|value| Self::parse(&value))
        .collect()
    }

    pub(crate) fn serialize_value(&self) -> String {
        let mut parts: Vec<&str> = Vec::new();
        if self.ctrl {
            parts.push("ctrl");
        }
        if self.shift {
            parts.push("shift");
        }
        if self.alt {
            parts.push("alt");
        }
        if self.logo {
            parts.push("meta");
        }
        let mut value = parts.join("+");
        if !value.is_empty() {
            value.push('+');
        }
        value.push_str(&self.key);
        value
    }

    pub(crate) fn canonicalize_key_token(token: &str) -> Option<String> {
        let raw = token.trim();
        if raw.is_empty() {
            return None;
        }
        let lowered = raw.to_ascii_lowercase();
        let compact = lowered.replace(' ', "");
        let normalized = match compact.as_str() {
            "esc" => "escape",
            "return" => "enter",
            "del" => "delete",
            "ins" => "insert",
            "+" | "add" => "plus",
            "-" | "subtract" => "minus",
            "*" | "multiply" => "asterisk",
            "/" | "divide" => "slash",
            "\\" => "backslash",
            "." => "period",
            "," => "comma",
            ";" => "semicolon",
            "'" | "quote" => "quote",
            "[" => "leftbracket",
            "]" => "rightbracket",
            "=" => "equals",
            "`" => "backquote",
            "pgup" => "pageup",
            "pgdn" | "pagedn" => "pagedown",
            "up" => "arrowup",
            "down" => "arrowdown",
            "left" => "arrowleft",
            "right" => "arrowright",
            "spacebar" => "space",
            _ => compact.as_str(),
        };
        Some(normalized.to_string())
    }

    pub(crate) fn parse(value: &str) -> Option<Self> {
        let raw = value.trim();
        if raw.is_empty() {
            return None;
        }
        if !raw.contains('+') {
            let legacy = raw
                .chars()
                .filter(|ch| ch.is_ascii_alphanumeric())
                .map(|ch| ch.to_ascii_lowercase())
                .collect::<String>();
            match legacy.as_str() {
                "ctrlp" => return Self::new(true, false, false, false, "p"),
                "ctrlk" => return Self::new(true, false, false, false, "k"),
                "ctrlo" => return Self::new(true, false, false, false, "o"),
                "ctrlshiftp" => {
                    return Self::new(true, true, false, false, "p");
                }
                "ctrlshiftk" => {
                    return Self::new(true, true, false, false, "k");
                }
                "ctrlshifto" => {
                    return Self::new(true, true, false, false, "o");
                }
                _ => {}
            }
        }

        let parts: Vec<&str> = raw
            .split('+')
            .map(str::trim)
            .filter(|part| !part.is_empty())
            .collect();
        if parts.is_empty() {
            return None;
        }

        let mut ctrl = false;
        let mut shift = false;
        let mut alt = false;
        let mut logo = false;
        for modifier in parts.iter().take(parts.len().saturating_sub(1)) {
            let value = modifier.to_ascii_lowercase();
            match value.as_str() {
                "ctrl" | "control" => ctrl = true,
                "shift" => shift = true,
                "alt" | "option" => alt = true,
                "logo" | "meta" | "super" | "cmd" | "command" | "win" | "windows" => logo = true,
                _ => return None,
            }
        }

        let key = parts.last().copied().unwrap_or_default();
        Self::new(ctrl, shift, alt, logo, key)
    }

    pub(crate) fn key_label(&self) -> String {
        if self.key.chars().count() == 1 {
            return self.key.to_ascii_uppercase();
        }
        if self.key.starts_with('f') && self.key.chars().skip(1).all(|ch| ch.is_ascii_digit()) {
            return self.key.to_ascii_uppercase();
        }
        match self.key.as_str() {
            "escape" => String::from("Escape"),
            "enter" => String::from("Enter"),
            "tab" => String::from("Tab"),
            "space" => crate::i18n::tr("Space"),
            "backspace" => String::from("Backspace"),
            "delete" => String::from("Delete"),
            "insert" => String::from("Insert"),
            "home" => String::from("Home"),
            "end" => String::from("End"),
            "pageup" => crate::i18n::tr("Page Up"),
            "pagedown" => crate::i18n::tr("Page Down"),
            "arrowup" => crate::i18n::tr("Arrow Up"),
            "arrowdown" => crate::i18n::tr("Arrow Down"),
            "arrowleft" => crate::i18n::tr("Arrow Left"),
            "arrowright" => crate::i18n::tr("Arrow Right"),
            "plus" => String::from("+"),
            "minus" => String::from("-"),
            "asterisk" => String::from("*"),
            "slash" => String::from("/"),
            "backslash" => String::from("\\"),
            "period" => String::from("."),
            "comma" => String::from(","),
            "semicolon" => String::from(";"),
            "quote" => String::from("'"),
            "leftbracket" => String::from("["),
            "rightbracket" => String::from("]"),
            "equals" => String::from("="),
            "backquote" => String::from("`"),
            _ => {
                let mut chars = self.key.chars();
                let Some(first) = chars.next() else {
                    return String::new();
                };
                let mut label = String::new();
                label.push(first.to_ascii_uppercase());
                label.extend(chars);
                label
            }
        }
    }
}

impl std::fmt::Display for ShortcutBinding {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut parts: Vec<String> = Vec::new();
        if self.ctrl {
            parts.push(if Self::uses_command_key() {
                String::from("⌃")
            } else {
                String::from("Ctrl")
            });
        }
        if self.shift {
            parts.push(if Self::uses_command_key() {
                String::from("⇧")
            } else {
                String::from("Shift")
            });
        }
        if self.alt {
            parts.push(if Self::uses_command_key() {
                String::from("⌥")
            } else {
                String::from("Alt")
            });
        }
        if self.logo {
            parts.push(if Self::uses_command_key() {
                String::from("⌘")
            } else {
                String::from("Meta")
            });
        }
        let key_is_active_modifier = (self.ctrl && matches!(self.key.as_str(), "ctrl" | "control"))
            || (self.shift && self.key == "shift")
            || (self.alt && self.key == "alt")
            || (self.logo && matches!(self.key.as_str(), "logo" | "meta" | "super"));
        if !key_is_active_modifier {
            parts.push(self.key_label());
        }
        f.write_str(&parts.join(" + "))
    }
}

impl std::fmt::Display for SystemThemeMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SystemThemeMode::Manual => f.write_str(&crate::i18n::tr("Manual")),
            SystemThemeMode::System => f.write_str(&crate::i18n::tr("System")),
        }
    }
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct SettingsStore {
    pub(crate) appearance_revision: Option<u32>,
    pub(crate) language: Option<Language>,
    pub(crate) ai_enabled: Option<bool>,
    pub(crate) onboarding_completed: Option<bool>,
    pub(crate) onboarding_theme_variant: Option<ThemeVariant>,
    pub(crate) last_changelog_version: Option<String>,
    pub(crate) font: Option<String>,
    pub(crate) editor_font: Option<String>,
    pub(crate) font_size: Option<u32>,
    pub(crate) font_size_input: Option<String>,
    pub(crate) theme: Option<ThemeChoice>,
    pub(crate) theme_variant: Option<ThemeVariant>,
    pub(crate) system_theme_mode: Option<SystemThemeMode>,
    pub(crate) dark_theme: Option<ThemeChoice>,
    pub(crate) light_theme: Option<ThemeChoice>,
    pub(crate) ui_density: Option<UiDensity>,
    pub(crate) accent_color: Option<AppearanceColor>,
    pub(crate) default_accent_color: Option<AppearanceColor>,
    pub(crate) large_sidebar_buttons: Option<bool>,
    pub(crate) result_grid_density: Option<ResultGridDensity>,
    pub(crate) modal_backdrop_dim: Option<f32>,
    pub(crate) table_query_limit: Option<usize>,
    pub(crate) history_limit: Option<usize>,
    pub(crate) saved_queries: Option<Vec<String>>,
    pub(crate) recent_omni_commands: Option<Vec<String>>,
    pub(crate) table_cache_limit_entries: Option<usize>,
    pub(crate) table_cache_limit_mb: Option<usize>,
    pub(crate) tabs_enabled: Option<bool>,
    pub(crate) release_inactive_tab_memory: Option<bool>,
    pub(crate) inactive_tab_release_idle_secs: Option<u64>,
    pub(crate) ai_provider: Option<AiProvider>,
    pub(crate) ai_endpoint: Option<String>,
    pub(crate) ai_model: Option<String>,
    pub(crate) ai_api_key: Option<String>,
    pub(crate) ai_cli_command: Option<String>,
    pub(crate) ai_autocomplete_enabled: Option<bool>,
    pub(crate) ai_autocomplete_use_main_provider: Option<bool>,
    pub(crate) ai_autocomplete_allow_local_cli: Option<bool>,
    pub(crate) ai_send_temperature: Option<bool>,
    pub(crate) ai_autocomplete_provider: Option<AiProvider>,
    pub(crate) ai_autocomplete_endpoint: Option<String>,
    pub(crate) ai_autocomplete_model: Option<String>,
    pub(crate) ai_autocomplete_api_key: Option<String>,
    pub(crate) ai_send_schema_context: Option<bool>,
    pub(crate) ai_send_query_context: Option<bool>,
    pub(crate) chat_mode: Option<ChatMode>,
    pub(crate) query_inline_suggestion_delay_ms: Option<u64>,
    pub(crate) schema_autocomplete_enabled: Option<bool>,
    pub(crate) sql_keyword_autocomplete_enabled: Option<bool>,
    pub(crate) query_editor_line_numbers_enabled: Option<bool>,
    pub(crate) query_editor_word_wrap_enabled: Option<bool>,
    pub(crate) column_width_overrides: Option<HashMap<String, f32>>,
    pub(crate) compact_sidebar: Option<bool>,
    pub(crate) auto_expand_selected_table: Option<bool>,
    pub(crate) show_hidden_tables: Option<bool>,
    pub(crate) multiple_connections_layout: Option<bool>,
    pub(crate) tab_size: Option<u32>,
    pub(crate) insert_spaces: Option<bool>,
    pub(crate) connection_timeout_secs: Option<u64>,
    pub(crate) query_timeout_secs: Option<u64>,
    pub(crate) auto_reconnect: Option<bool>,
    pub(crate) omni_table_shortcut: Option<String>,
    pub(crate) omni_command_shortcut: Option<String>,
    pub(crate) open_settings_shortcut: Option<String>,
    pub(crate) switch_database_shortcut: Option<String>,
    pub(crate) focus_table_search_shortcut: Option<String>,
    pub(crate) toggle_tab_pin_shortcut: Option<String>,
    pub(crate) toggle_sidebar_shortcut: Option<String>,
    pub(crate) toggle_chat_sidebar_shortcut: Option<String>,
    pub(crate) cycle_theme_shortcut: Option<String>,
    pub(crate) run_query_shortcut: Option<String>,
    pub(crate) run_selection_shortcut: Option<String>,
    pub(crate) autocomplete_tables_shortcut: Option<String>,
    pub(crate) new_query_shortcut: Option<String>,
    pub(crate) close_tab_shortcut: Option<String>,
    pub(crate) next_tab_shortcut: Option<String>,
    pub(crate) previous_tab_shortcut: Option<String>,
    pub(crate) next_results_page_shortcut: Option<String>,
    pub(crate) previous_results_page_shortcut: Option<String>,
    pub(crate) first_results_page_shortcut: Option<String>,
    pub(crate) last_results_page_shortcut: Option<String>,
    pub(crate) open_table_info_sidebar_shortcut: Option<String>,
    pub(crate) omni_prefix: Option<String>,
    pub(crate) emphasize_column_headers: Option<bool>,
    pub(crate) auto_scroll_sidebar_to_selected_table: Option<bool>,
    pub(crate) omni_command_alt_shortcut: Option<String>,
    #[serde(alias = "omni_command_f1_alias")]
    pub(crate) omni_command_f1_alias: Option<bool>,
}

impl SettingsStore {
    pub(crate) fn from_settings(settings: &Settings, theme_choice: ThemeChoice) -> Self {
        Self {
            appearance_revision: Some(settings.appearance_revision),
            language: Some(settings.language),
            ai_enabled: Some(settings.ai_enabled),
            onboarding_completed: Some(settings.onboarding_completed),
            onboarding_theme_variant: Some(settings.onboarding_theme_variant),
            last_changelog_version: settings.last_changelog_version.clone(),
            font: Some(settings.font.to_string()),
            editor_font: Some(settings.editor_font.to_string()),
            font_size: Some(settings.font_size),
            font_size_input: Some(settings.font_size_input.clone()),
            theme: Some(theme_choice),
            theme_variant: Some(settings.theme_variant),
            system_theme_mode: Some(settings.system_theme_mode),
            dark_theme: Some(settings.dark_theme),
            light_theme: Some(settings.light_theme),
            ui_density: Some(settings.ui_density),
            accent_color: Some(settings.accent_color),
            default_accent_color: Some(settings.default_accent_color),
            large_sidebar_buttons: Some(settings.large_sidebar_buttons),
            result_grid_density: Some(settings.result_grid_density),
            modal_backdrop_dim: Some(settings.modal_backdrop_dim),
            table_query_limit: Some(settings.table_query_limit),
            history_limit: Some(settings.history_limit),
            saved_queries: Some(settings.saved_queries.clone()),
            recent_omni_commands: Some(
                settings
                    .recent_omni_commands
                    .iter()
                    .take(4)
                    .cloned()
                    .collect(),
            ),
            table_cache_limit_entries: Some(settings.table_cache_limit_entries),
            table_cache_limit_mb: Some(settings.table_cache_limit_mb),
            tabs_enabled: Some(settings.tabs_enabled),
            release_inactive_tab_memory: Some(settings.release_inactive_tab_memory),
            inactive_tab_release_idle_secs: Some(settings.inactive_tab_release_idle_secs),
            ai_provider: Some(settings.ai_provider),
            ai_endpoint: Some(settings.ai_endpoint.clone()),
            ai_model: Some(settings.ai_model.clone()),
            ai_api_key: if settings.ai_api_key.trim().is_empty() {
                None
            } else {
                Some(settings.ai_api_key.clone())
            },
            ai_cli_command: Some(settings.ai_cli_command.clone()),
            ai_autocomplete_enabled: Some(settings.ai_autocomplete_enabled),
            ai_autocomplete_use_main_provider: Some(settings.ai_autocomplete_use_main_provider),
            ai_autocomplete_allow_local_cli: Some(settings.ai_autocomplete_allow_local_cli),
            ai_send_temperature: Some(settings.ai_send_temperature),
            ai_autocomplete_provider: Some(settings.ai_autocomplete_provider),
            ai_autocomplete_endpoint: Some(settings.ai_autocomplete_endpoint.clone()),
            ai_autocomplete_model: Some(settings.ai_autocomplete_model.clone()),
            ai_autocomplete_api_key: if settings.ai_autocomplete_api_key.trim().is_empty() {
                None
            } else {
                Some(settings.ai_autocomplete_api_key.clone())
            },
            ai_send_schema_context: Some(settings.ai_send_schema_context),
            ai_send_query_context: Some(settings.ai_send_query_context),
            chat_mode: Some(settings.chat_mode),
            query_inline_suggestion_delay_ms: Some(settings.query_inline_suggestion_delay_ms),
            schema_autocomplete_enabled: Some(settings.schema_autocomplete_enabled),
            sql_keyword_autocomplete_enabled: Some(settings.sql_keyword_autocomplete_enabled),
            query_editor_line_numbers_enabled: Some(settings.query_editor_line_numbers_enabled),
            query_editor_word_wrap_enabled: Some(settings.query_editor_word_wrap_enabled),
            column_width_overrides: Some(settings.column_width_overrides.clone()),
            compact_sidebar: Some(settings.compact_sidebar),
            auto_expand_selected_table: Some(settings.auto_expand_selected_table),
            show_hidden_tables: Some(settings.show_hidden_tables),
            multiple_connections_layout: Some(settings.multiple_connections_layout),
            tab_size: Some(settings.tab_size),
            insert_spaces: Some(settings.insert_spaces),
            connection_timeout_secs: Some(settings.connection_timeout_secs),
            query_timeout_secs: Some(settings.query_timeout_secs),
            auto_reconnect: Some(settings.auto_reconnect),
            omni_table_shortcut: Some(settings.omni_table_shortcut.serialize_value()),
            omni_command_shortcut: Some(settings.omni_command_shortcut.serialize_value()),
            open_settings_shortcut: Some(settings.open_settings_shortcut.serialize_value()),
            switch_database_shortcut: Some(settings.switch_database_shortcut.serialize_value()),
            focus_table_search_shortcut: Some(
                settings.focus_table_search_shortcut.serialize_value(),
            ),
            toggle_tab_pin_shortcut: Some(settings.toggle_tab_pin_shortcut.serialize_value()),
            toggle_sidebar_shortcut: Some(settings.toggle_sidebar_shortcut.serialize_value()),
            toggle_chat_sidebar_shortcut: Some(
                settings.toggle_chat_sidebar_shortcut.serialize_value(),
            ),
            cycle_theme_shortcut: Some(settings.cycle_theme_shortcut.serialize_value()),
            run_query_shortcut: Some(settings.run_query_shortcut.serialize_value()),
            run_selection_shortcut: Some(settings.run_selection_shortcut.serialize_value()),
            autocomplete_tables_shortcut: Some(
                settings.autocomplete_tables_shortcut.serialize_value(),
            ),
            new_query_shortcut: Some(settings.new_query_shortcut.serialize_value()),
            close_tab_shortcut: Some(settings.close_tab_shortcut.serialize_value()),
            next_tab_shortcut: Some(settings.next_tab_shortcut.serialize_value()),
            previous_tab_shortcut: Some(settings.previous_tab_shortcut.serialize_value()),
            next_results_page_shortcut: Some(settings.next_results_page_shortcut.serialize_value()),
            previous_results_page_shortcut: Some(
                settings.previous_results_page_shortcut.serialize_value(),
            ),
            first_results_page_shortcut: Some(
                settings.first_results_page_shortcut.serialize_value(),
            ),
            last_results_page_shortcut: Some(settings.last_results_page_shortcut.serialize_value()),
            open_table_info_sidebar_shortcut: Some(
                settings.open_table_info_sidebar_shortcut.serialize_value(),
            ),
            omni_prefix: Some(settings.omni_prefix.clone()),
            emphasize_column_headers: Some(settings.emphasize_column_headers),
            auto_scroll_sidebar_to_selected_table: Some(
                settings.auto_scroll_sidebar_to_selected_table,
            ),
            omni_command_alt_shortcut: Some(settings.omni_command_alt_shortcut.serialize_value()),
            omni_command_f1_alias: None,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct PickerOption<T> {
    pub(crate) value: T,
    pub(crate) label: String,
}

impl<T> PickerOption<T> {
    pub(crate) fn new(value: T, label: impl Into<String>) -> Self {
        Self {
            value,
            label: label.into(),
        }
    }
}

impl<T: PartialEq> PartialEq for PickerOption<T> {
    fn eq(&self, other: &Self) -> bool {
        self.value == other.value
    }
}

impl<T: Eq> Eq for PickerOption<T> {}

impl<T> std::fmt::Display for PickerOption<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.label)
    }
}

#[derive(Debug, Clone)]
pub(crate) struct Settings {
    pub(crate) appearance_revision: u32,
    pub(crate) language: Language,
    pub(crate) ai_enabled: bool,
    pub(crate) onboarding_completed: bool,
    pub(crate) onboarding_theme_variant: ThemeVariant,
    pub(crate) last_changelog_version: Option<String>,
    pub(crate) font: FontChoice,
    pub(crate) editor_font: FontChoice,
    pub(crate) font_size: u32,
    pub(crate) font_size_input: String,
    pub(crate) theme_variant: ThemeVariant,
    pub(crate) system_theme_mode: SystemThemeMode,
    pub(crate) dark_theme: ThemeChoice,
    pub(crate) light_theme: ThemeChoice,
    pub(crate) ui_density: UiDensity,
    pub(crate) accent_color: AppearanceColor,
    pub(crate) default_accent_color: AppearanceColor,
    pub(crate) large_sidebar_buttons: bool,
    pub(crate) result_grid_density: ResultGridDensity,
    pub(crate) modal_backdrop_dim: f32,
    pub(crate) table_query_limit: usize,
    pub(crate) table_query_limit_input: String,
    pub(crate) history_limit: usize,
    pub(crate) history_limit_input: String,
    pub(crate) saved_queries: Vec<String>,
    pub(crate) recent_omni_commands: Vec<String>,
    pub(crate) table_cache_limit_entries: usize,
    pub(crate) table_cache_limit_mb: usize,
    pub(crate) tabs_enabled: bool,
    pub(crate) release_inactive_tab_memory: bool,
    pub(crate) inactive_tab_release_idle_secs: u64,
    pub(crate) inactive_tab_release_idle_secs_input: String,
    pub(crate) ai_provider: AiProvider,
    pub(crate) ai_endpoint: String,
    pub(crate) ai_model: String,
    pub(crate) ai_api_key: String,
    pub(crate) ai_cli_command: String,
    pub(crate) ai_autocomplete_enabled: bool,
    pub(crate) ai_autocomplete_use_main_provider: bool,
    pub(crate) ai_autocomplete_allow_local_cli: bool,
    pub(crate) ai_send_temperature: bool,
    pub(crate) ai_autocomplete_provider: AiProvider,
    pub(crate) ai_autocomplete_endpoint: String,
    pub(crate) ai_autocomplete_model: String,
    pub(crate) ai_autocomplete_api_key: String,
    pub(crate) ai_send_schema_context: bool,
    pub(crate) ai_send_query_context: bool,
    pub(crate) chat_mode: ChatMode,
    pub(crate) query_inline_suggestion_delay_ms: u64,
    pub(crate) query_inline_suggestion_delay_ms_input: String,
    pub(crate) schema_autocomplete_enabled: bool,
    pub(crate) sql_keyword_autocomplete_enabled: bool,
    pub(crate) query_editor_line_numbers_enabled: bool,
    pub(crate) query_editor_word_wrap_enabled: bool,
    pub(crate) column_width_overrides: HashMap<String, f32>,
    pub(crate) compact_sidebar: bool,
    pub(crate) auto_expand_selected_table: bool,
    pub(crate) show_hidden_tables: bool,
    pub(crate) multiple_connections_layout: bool,
    pub(crate) tab_size: u32,
    pub(crate) insert_spaces: bool,
    pub(crate) connection_timeout_secs: u64,
    pub(crate) connection_timeout_secs_input: String,
    pub(crate) query_timeout_secs: u64,
    pub(crate) query_timeout_secs_input: String,
    pub(crate) auto_reconnect: bool,
    pub(crate) omni_table_shortcut: ShortcutBinding,
    pub(crate) omni_command_shortcut: ShortcutBinding,
    pub(crate) open_settings_shortcut: ShortcutBinding,
    pub(crate) switch_database_shortcut: ShortcutBinding,
    pub(crate) focus_table_search_shortcut: ShortcutBinding,
    pub(crate) toggle_tab_pin_shortcut: ShortcutBinding,
    pub(crate) toggle_sidebar_shortcut: ShortcutBinding,
    pub(crate) toggle_chat_sidebar_shortcut: ShortcutBinding,
    pub(crate) cycle_theme_shortcut: ShortcutBinding,
    pub(crate) run_query_shortcut: ShortcutBinding,
    pub(crate) run_selection_shortcut: ShortcutBinding,
    pub(crate) autocomplete_tables_shortcut: ShortcutBinding,
    pub(crate) new_query_shortcut: ShortcutBinding,
    pub(crate) close_tab_shortcut: ShortcutBinding,
    pub(crate) next_tab_shortcut: ShortcutBinding,
    pub(crate) previous_tab_shortcut: ShortcutBinding,
    pub(crate) next_results_page_shortcut: ShortcutBinding,
    pub(crate) previous_results_page_shortcut: ShortcutBinding,
    pub(crate) first_results_page_shortcut: ShortcutBinding,
    pub(crate) last_results_page_shortcut: ShortcutBinding,
    pub(crate) open_table_info_sidebar_shortcut: ShortcutBinding,
    pub(crate) omni_prefix: String,
    pub(crate) emphasize_column_headers: bool,
    pub(crate) auto_scroll_sidebar_to_selected_table: bool,
    pub(crate) omni_command_alt_shortcut: ShortcutBinding,
}

impl Default for Settings {
    fn default() -> Self {
        let table_query_limit = DEFAULT_TABLE_QUERY_LIMIT;
        let history_limit = DEFAULT_HISTORY_LIMIT;
        let table_cache_limit_entries = DEFAULT_TABLE_CACHE_LIMIT_ENTRIES;
        let table_cache_limit_mb = DEFAULT_TABLE_CACHE_LIMIT_MB;
        let inactive_tab_release_idle_secs = DEFAULT_INACTIVE_TAB_RELEASE_IDLE_SECS;
        let connection_timeout_secs = DEFAULT_CONNECTION_TIMEOUT_SECS;
        let query_timeout_secs = DEFAULT_QUERY_TIMEOUT_SECS;
        let query_inline_suggestion_delay_ms = DEFAULT_QUERY_INLINE_SUGGESTION_DELAY_MS;
        Self {
            appearance_revision: APPEARANCE_REVISION,
            language: Language::default(),
            ai_enabled: true,
            onboarding_completed: false,
            onboarding_theme_variant: ThemeVariant::Original,
            last_changelog_version: None,
            font: if cfg!(target_os = "macos") {
                FontChoice::Sans
            } else {
                FontChoice::Monospace
            },
            editor_font: if cfg!(target_os = "macos") {
                FontChoice::Sans
            } else {
                FontChoice::Monospace
            },
            font_size: DEFAULT_FONT_SIZE,
            font_size_input: DEFAULT_FONT_SIZE.to_string(),
            theme_variant: ThemeVariant::Original,
            system_theme_mode: SystemThemeMode::Manual,
            dark_theme: ThemeChoice::CarbonFrostNight,
            light_theme: ThemeChoice::CarbonFrostAsh,
            ui_density: UiDensity::Normal,
            accent_color: AppearanceColor::Default,
            default_accent_color: AppearanceColor::Default,
            large_sidebar_buttons: false,
            result_grid_density: ResultGridDensity::Comfortable,
            modal_backdrop_dim: DEFAULT_MODAL_BACKDROP_DIM,
            table_query_limit,
            table_query_limit_input: table_query_limit.to_string(),
            history_limit,
            history_limit_input: history_limit.to_string(),
            saved_queries: Vec::new(),
            recent_omni_commands: Vec::new(),
            table_cache_limit_entries,
            table_cache_limit_mb,
            tabs_enabled: true,
            release_inactive_tab_memory: true,
            inactive_tab_release_idle_secs,
            inactive_tab_release_idle_secs_input: inactive_tab_release_idle_secs.to_string(),
            ai_provider: AiProvider::OpenAI,
            ai_endpoint: String::new(),
            ai_model: String::new(),
            ai_api_key: String::new(),
            ai_cli_command: String::from("claude -p"),
            ai_autocomplete_enabled: false,
            ai_autocomplete_use_main_provider: true,
            ai_autocomplete_allow_local_cli: false,
            ai_send_temperature: false,
            ai_autocomplete_provider: AiProvider::OpenAI,
            ai_autocomplete_endpoint: String::new(),
            ai_autocomplete_model: String::new(),
            ai_autocomplete_api_key: String::new(),
            ai_send_schema_context: true,
            ai_send_query_context: true,
            chat_mode: ChatMode::Ask,
            query_inline_suggestion_delay_ms,
            query_inline_suggestion_delay_ms_input: query_inline_suggestion_delay_ms.to_string(),
            schema_autocomplete_enabled: true,
            sql_keyword_autocomplete_enabled: true,
            query_editor_line_numbers_enabled: true,
            query_editor_word_wrap_enabled: true,
            column_width_overrides: HashMap::new(),
            compact_sidebar: false,
            auto_expand_selected_table: false,
            show_hidden_tables: false,
            multiple_connections_layout: true,
            tab_size: DEFAULT_TAB_SIZE,
            insert_spaces: true,
            connection_timeout_secs,
            connection_timeout_secs_input: connection_timeout_secs.to_string(),
            query_timeout_secs,
            query_timeout_secs_input: query_timeout_secs.to_string(),
            auto_reconnect: false,
            omni_table_shortcut: ShortcutBinding::default_omni_table(),
            omni_command_shortcut: ShortcutBinding::default_omni_command(),
            open_settings_shortcut: ShortcutBinding::default_open_settings(),
            switch_database_shortcut: ShortcutBinding::default_switch_database(),
            focus_table_search_shortcut: ShortcutBinding::default_focus_table_search(),
            toggle_tab_pin_shortcut: ShortcutBinding::default_toggle_tab_pin(),
            toggle_sidebar_shortcut: ShortcutBinding::default_toggle_sidebar(),
            toggle_chat_sidebar_shortcut: ShortcutBinding::default_toggle_chat_sidebar(),
            cycle_theme_shortcut: ShortcutBinding::default_cycle_theme(),
            run_query_shortcut: ShortcutBinding::default_run_query(),
            run_selection_shortcut: ShortcutBinding::default_run_selection(),
            autocomplete_tables_shortcut: ShortcutBinding::default_autocomplete_tables(),
            new_query_shortcut: ShortcutBinding::default_new_query(),
            close_tab_shortcut: ShortcutBinding::default_close_tab(),
            next_tab_shortcut: ShortcutBinding::default_next_tab(),
            previous_tab_shortcut: ShortcutBinding::default_previous_tab(),
            next_results_page_shortcut: ShortcutBinding::default_next_results_page(),
            previous_results_page_shortcut: ShortcutBinding::default_previous_results_page(),
            first_results_page_shortcut: ShortcutBinding::default_first_results_page(),
            last_results_page_shortcut: ShortcutBinding::default_last_results_page(),
            open_table_info_sidebar_shortcut: ShortcutBinding::default_open_table_info_sidebar(),
            omni_prefix: String::from(":"),
            emphasize_column_headers: true,
            auto_scroll_sidebar_to_selected_table: false,
            omni_command_alt_shortcut: ShortcutBinding::default_omni_command_alt(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ThemeChoice;

    #[test]
    fn settings_store_round_trips_query_inline_suggestion_delay_ms() {
        let settings = Settings {
            query_inline_suggestion_delay_ms: 750,
            query_inline_suggestion_delay_ms_input: String::from("750"),
            ..Settings::default()
        };

        let store = SettingsStore::from_settings(&settings, ThemeChoice::CarbonFrost);

        assert_eq!(store.query_inline_suggestion_delay_ms, Some(750));
    }

    #[test]
    fn settings_store_keeps_at_most_four_recent_commands() {
        let settings = Settings {
            recent_omni_commands: (0..5).map(|index| index.to_string()).collect(),
            ..Settings::default()
        };

        let store = SettingsStore::from_settings(&settings, ThemeChoice::CarbonFrost);

        assert_eq!(store.recent_omni_commands.unwrap().len(), 4);
    }

    #[test]
    fn every_results_page_shortcut_is_a_distinct_combination() {
        let settings = Settings::default();
        let bindings = [
            &settings.previous_results_page_shortcut,
            &settings.next_results_page_shortcut,
            &settings.first_results_page_shortcut,
            &settings.last_results_page_shortcut,
        ];

        for (index, binding) in bindings.iter().enumerate() {
            for other in bindings.iter().skip(index + 1) {
                assert_ne!(binding, other);
            }
        }
    }

    #[test]
    fn first_and_last_results_page_shortcuts_survive_a_store_round_trip() {
        let settings = Settings {
            first_results_page_shortcut: ShortcutBinding::parse("ctrl+shift+home")
                .expect("valid binding"),
            last_results_page_shortcut: ShortcutBinding::parse("ctrl+shift+end")
                .expect("valid binding"),
            ..Settings::default()
        };

        let store = SettingsStore::from_settings(&settings, ThemeChoice::CarbonFrost);

        assert_eq!(
            store
                .first_results_page_shortcut
                .as_deref()
                .and_then(ShortcutBinding::parse),
            Some(settings.first_results_page_shortcut),
        );
        assert_eq!(
            store
                .last_results_page_shortcut
                .as_deref()
                .and_then(ShortcutBinding::parse),
            Some(settings.last_results_page_shortcut),
        );
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub(crate) enum ChatMode {
    #[default]
    Ask,
    Draft,
    AutoRun,
}

impl ChatMode {
    pub(crate) const ALL: [Self; 3] = [Self::Ask, Self::Draft, Self::AutoRun];
}

impl std::fmt::Display for ChatMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&crate::i18n::tr(match self {
            ChatMode::Ask => "Ask",
            ChatMode::Draft => "Draft to editor",
            ChatMode::AutoRun => "Run read-only",
        }))
    }
}
