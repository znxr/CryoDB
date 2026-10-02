use super::Message;
use super::State;
use crate::constants::AI_TEST_OUTPUT_MAX_CHARS;
use crate::model::settings::{
    AppearanceColor, FontChoice, Settings, SettingsStore, SystemThemeMode,
};
use crate::storage::{resolve_font_choice, save_settings_store, store_ai_api_key_secret};
use crate::ui::theme::ThemeChoice;
use crate::utils::helpers::build_font_choices;
use iced::Task;
use iced::Theme;
use std::sync::Mutex;

type AiApiKeySecretStore = fn(&str) -> Result<(), String>;

pub(crate) fn settings_store_for_persistence(
    settings: &Settings,
    theme_choice: ThemeChoice,
    store_secret: AiApiKeySecretStore,
) -> (SettingsStore, Option<String>) {
    let mut store = SettingsStore::from_settings(settings, theme_choice);
    let keyring_error = store_secret(&settings.ai_api_key).err();

    store.ai_api_key = if settings.ai_api_key.trim().is_empty() {
        None
    } else {
        Some(settings.ai_api_key.clone())
    };

    (store, keyring_error)
}

impl State {
    pub(crate) fn persist_settings_store(&mut self) {
        let (store, keyring_error) = settings_store_for_persistence(
            &self.values,
            self.theme_choice,
            store_ai_api_key_secret,
        );
        match save_settings_store(&store) {
            Ok(()) => {
                if let Some(error) = keyring_error {
                    self.settings_store_error = Some(format!(
                        "Could not save AI API key to keyring: {error}. API key kept in local config fallback."
                    ));
                } else {
                    self.settings_store_error = None;
                }
            }
            Err(error) => self.settings_store_error = Some(error),
        }
    }
    pub(crate) fn apply_font_choices(&mut self, mut choices: Vec<FontChoice>) {
        if let FontChoice::System(name) = &self.values.font
            && let Some(resolved) = resolve_font_choice(name, &choices)
        {
            self.values.font = resolved;
        }
        if let FontChoice::System(name) = &self.values.editor_font
            && let Some(resolved) = resolve_font_choice(name, &choices)
        {
            self.values.editor_font = resolved;
        }
        if !choices.contains(&self.values.font) {
            choices.push(self.values.font.clone());
        }
        if !choices.contains(&self.values.editor_font) {
            choices.push(self.values.editor_font.clone());
        }

        self.font_choices = choices;
    }
    pub(crate) fn theme(&self) -> Theme {
        static CACHE: Mutex<Option<(ThemeChoice, AppearanceColor, AppearanceColor, Theme)>> =
            Mutex::new(None);

        crate::ui::theme::set_theme_variant(self.values.theme_variant);
        let resolved = self.resolve_theme_choice();
        let accent_color = self.values.accent_color;
        let default_accent_color = self.values.default_accent_color;
        let mut cache = CACHE.lock().unwrap_or_else(|error| error.into_inner());
        if let Some((cached_choice, cached_accent, cached_default, cached_theme)) = cache.as_ref()
            && *cached_choice == resolved
            && *cached_accent == accent_color
            && *cached_default == default_accent_color
        {
            return cached_theme.clone();
        }

        let theme = resolved.ui_theme();
        let accent = crate::ui::theme::appearance_color(
            accent_color,
            crate::ui::theme::appearance_color(default_accent_color, theme.palette().primary),
        );
        let theme = if accent == theme.palette().primary {
            theme
        } else {
            let base_extended = *theme.extended_palette();
            let mut palette = theme.palette();
            palette.primary = accent;
            Theme::custom_with_fn(resolved.to_string(), palette, move |palette| {
                let mut extended = base_extended;
                extended.primary = iced::theme::palette::Primary::generate(
                    palette.primary,
                    palette.background,
                    palette.text,
                );
                extended
            })
        };

        *cache = Some((resolved, accent_color, default_accent_color, theme.clone()));
        theme
    }
    pub(crate) fn resolve_theme_choice(&self) -> ThemeChoice {
        match self.values.system_theme_mode {
            SystemThemeMode::System => {
                let is_dark = crate::ui::theme::detect_os_dark_mode();
                if is_dark {
                    self.values.dark_theme
                } else {
                    self.values.light_theme
                }
            }
            SystemThemeMode::Manual => self.theme_choice,
        }
    }
    pub(crate) fn shorten_ai_test_error(error: &str) -> String {
        let cleaned = error
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .collect::<Vec<_>>()
            .join(" ");

        if cleaned.chars().count() > AI_TEST_OUTPUT_MAX_CHARS {
            format!(
                "{}...",
                cleaned
                    .chars()
                    .take(AI_TEST_OUTPUT_MAX_CHARS)
                    .collect::<String>()
                    .trim_end()
            )
        } else {
            cleaned
        }
    }
    pub(crate) fn ensure_font_choices_loaded(&mut self) -> Task<Message> {
        if self.font_choices_loaded {
            return Task::none();
        }

        self.font_choices_loaded = true;
        load_font_choices_task()
    }
}

pub(crate) fn load_font_choices_task() -> Task<Message> {
    Task::perform(
        async {
            tokio::task::spawn_blocking(build_font_choices)
                .await
                .unwrap_or_else(|_| vec![FontChoice::Sans, FontChoice::Monospace])
        },
        Message::FontChoicesLoaded,
    )
}
