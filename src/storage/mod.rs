use keyring::Entry;
use keyring::Error as KeyringError;
use std::path::{Path, PathBuf};

use crate::model::settings::ThemeChoice;
use crate::{
    APPEARANCE_REVISION, ConnectionStore, DIAGRAMS_STORE_FILE, DatabaseDriver, DiagramStore,
    FAVORITES_STORE_FILE, FOLDERS_STORE_FILE, FolderStore, FontChoice,
    MAX_QUERY_INLINE_SUGGESTION_DELAY_MS, MAX_RECENT_CONNECTIONS, MIN_FONT_SIZE,
    MIN_QUERY_INLINE_SUGGESTION_DELAY_MS, SETTINGS_STORE_FILE, Settings, SettingsStore,
    ShortcutBinding, StoredConnection, normalize_omni_prefix,
};

type ConnectionSecretStore = fn(&StoredConnection, &str) -> Result<(), String>;
type AiApiKeySecretStore = fn(&str) -> Result<(), String>;

const CONFIG_DIR_NAME: &str = "cryodb";
const LEGACY_CONFIG_DIR_NAME: &str = "cryosql";
const CONNECTION_SECRET_SERVICE: &str = "dev.znxr.cryodb.connection";
const SETTINGS_SECRET_SERVICE: &str = "dev.znxr.cryodb.settings";
const SETTINGS_AI_API_KEY_ACCOUNT: &str = "ai_api_key";

fn config_root_dir() -> PathBuf {
    if cfg!(test) {
        return std::env::temp_dir().join("cryodb-tests");
    }

    if let Some(config_dir) = std::env::var_os("CRYODB_CONFIG") {
        return PathBuf::from(config_dir);
    }

    if let Some(config_dir) = std::env::var_os("CRYODB_CONFIG_DIR") {
        return PathBuf::from(config_dir);
    }

    if let Some(xdg) = std::env::var_os("XDG_CONFIG_HOME") {
        return migrated_config_dir(PathBuf::from(xdg));
    }

    if let Some(home) = std::env::var_os("HOME") {
        return migrated_config_dir(PathBuf::from(home).join(".config"));
    }

    migrated_config_dir(std::env::temp_dir())
}

fn migrated_config_dir(parent: PathBuf) -> PathBuf {
    let config_dir = parent.join(CONFIG_DIR_NAME);
    let legacy_dir = parent.join(LEGACY_CONFIG_DIR_NAME);
    if !config_dir.exists() && legacy_dir.is_dir() {
        let _ = std::fs::rename(&legacy_dir, &config_dir);
    }
    config_dir
}

fn legacy_secret_service(service: &str) -> String {
    service.replace(CONFIG_DIR_NAME, LEGACY_CONFIG_DIR_NAME)
}

fn config_file_path(file_name: &str) -> PathBuf {
    config_root_dir().join(file_name)
}

pub(crate) fn config_artifact_path(file_name: &str) -> PathBuf {
    config_file_path(file_name)
}

pub(crate) fn sanitize_connection_store_secrets(store: &ConnectionStore) -> Option<String> {
    sanitize_connection_store_secrets_with(store, store_connection_secret)
}

fn sanitize_connection_store_secrets_with(
    store: &ConnectionStore,
    store_secret: ConnectionSecretStore,
) -> Option<String> {
    let mut first_error = None;

    for entry in store
        .profiles
        .iter()
        .chain(store.favorites.iter())
        .chain(store.recents.iter())
    {
        let password = entry.password.trim().to_string();
        if password.is_empty() {
            continue;
        }

        if let Err(error) = store_secret(entry, &password)
            && first_error.is_none()
        {
            first_error = Some(error);
        }
    }

    first_error
}

fn keyring_set_secret(service: &str, account: &str, secret: &str) -> Result<(), String> {
    let entry = Entry::new(service, account).map_err(|error| error.to_string())?;
    if secret.trim().is_empty() {
        if let Ok(legacy) = Entry::new(&legacy_secret_service(service), account) {
            let _ = legacy.delete_credential();
        }
        match entry.delete_credential() {
            Ok(()) | Err(KeyringError::NoEntry) => Ok(()),
            Err(error) => Err(error.to_string()),
        }
    } else {
        entry
            .set_password(secret)
            .map_err(|error| error.to_string())
    }
}

fn keyring_get_secret(service: &str, account: &str) -> Option<String> {
    let entry = Entry::new(service, account).ok()?;
    match entry.get_password() {
        Ok(secret) => Some(secret),
        Err(KeyringError::NoEntry) => {
            let secret = Entry::new(&legacy_secret_service(service), account)
                .ok()?
                .get_password()
                .ok()?;
            let _ = entry.set_password(&secret);
            Some(secret)
        }
        Err(_) => None,
    }
}

fn legacy_sqlite_secret_lookup_key(entry: &StoredConnection) -> Option<String> {
    if entry.driver != DatabaseDriver::Sqlite {
        return None;
    }

    let path = entry.sqlite_path.trim();
    if path.is_empty() {
        return None;
    }

    let legacy = format!("sqlite|{}", path.to_ascii_lowercase());
    if legacy == entry.secret_lookup_key() {
        None
    } else {
        Some(legacy)
    }
}

fn connection_secret_lookup_keys(entry: &StoredConnection) -> Vec<String> {
    let mut keys = vec![entry.secret_lookup_key()];
    if let Some(legacy) = legacy_sqlite_secret_lookup_key(entry) {
        keys.push(legacy);
    }
    keys
}

pub(crate) fn store_connection_secret(
    entry: &StoredConnection,
    password: &str,
) -> Result<(), String> {
    let account = entry.secret_lookup_key();
    if !password.trim().is_empty() {
        return keyring_set_secret(CONNECTION_SECRET_SERVICE, &account, password);
    }

    let mut first_error = None;
    for account in connection_secret_lookup_keys(entry) {
        if let Err(error) = keyring_set_secret(CONNECTION_SECRET_SERVICE, &account, "")
            && first_error.is_none()
        {
            first_error = Some(error);
        }
    }

    if let Some(error) = first_error {
        Err(error)
    } else {
        Ok(())
    }
}

pub(crate) fn load_connection_secret(entry: &StoredConnection) -> Option<String> {
    connection_secret_lookup_keys(entry)
        .into_iter()
        .find_map(|account| keyring_get_secret(CONNECTION_SECRET_SERVICE, &account))
        .or_else(|| {
            let password = entry.password.trim();
            if password.is_empty() {
                None
            } else {
                Some(password.to_string())
            }
        })
}

pub(crate) fn store_ai_api_key_secret(api_key: &str) -> Result<(), String> {
    keyring_set_secret(
        SETTINGS_SECRET_SERVICE,
        SETTINGS_AI_API_KEY_ACCOUNT,
        api_key,
    )
}

pub(crate) fn load_ai_api_key_secret() -> Option<String> {
    keyring_get_secret(SETTINGS_SECRET_SERVICE, SETTINGS_AI_API_KEY_ACCOUNT)
}

fn connection_store_path() -> PathBuf {
    config_file_path(FAVORITES_STORE_FILE)
}

fn settings_store_path() -> PathBuf {
    config_file_path(SETTINGS_STORE_FILE)
}

fn folder_store_path() -> PathBuf {
    config_file_path(FOLDERS_STORE_FILE)
}

fn diagram_store_path() -> PathBuf {
    config_file_path(DIAGRAMS_STORE_FILE)
}

pub(crate) fn load_diagram_store() -> DiagramStore {
    load_diagram_store_from(&diagram_store_path())
}

fn load_diagram_store_from(path: &Path) -> DiagramStore {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|contents| toml::from_str::<DiagramStore>(&contents).ok())
        .unwrap_or_default()
}

pub(crate) fn save_diagram_store(store: &DiagramStore) -> Result<(), String> {
    save_diagram_store_to(diagram_store_path(), store)
}

fn save_diagram_store_to(path: PathBuf, store: &DiagramStore) -> Result<(), String> {
    let payload = toml::to_string_pretty(store).map_err(|error| error.to_string())?;
    write_toml_file(path, payload)
}

pub(crate) fn load_connection_store() -> (ConnectionStore, Option<String>) {
    let path = connection_store_path();
    let contents = match std::fs::read_to_string(&path) {
        Ok(contents) => contents,
        Err(error) => {
            if error.kind() == std::io::ErrorKind::NotFound {
                return (ConnectionStore::default(), None);
            }
            return (ConnectionStore::default(), Some(error.to_string()));
        }
    };

    let (store, warning, updated_store) =
        parse_connection_store(&contents, store_connection_secret);
    if updated_store {
        let _ = save_connection_store(&store);
    }
    (store, warning)
}

fn parse_connection_store(
    contents: &str,
    store_secret: ConnectionSecretStore,
) -> (ConnectionStore, Option<String>, bool) {
    match toml::from_str::<ConnectionStore>(contents) {
        Ok(mut store) => {
            let mut warnings = Vec::new();
            let mut updated_store = false;

            let mut profiles: Vec<StoredConnection> = Vec::new();
            for entry in store
                .profiles
                .into_iter()
                .map(StoredConnection::normalized)
                .filter(|entry| entry.is_valid())
            {
                if !entry.password.trim().is_empty() {
                    let legacy = entry.password.clone();
                    if let Err(error) = store_secret(&entry, &legacy) {
                        warnings.push(format!(
                            "Could not migrate profile connection password to keyring; keeping local fallback: {}",
                            error
                        ));
                    }
                }

                let entry_name = entry.profile_name().map(|value| value.to_ascii_lowercase());
                if let Some(index) =
                    profiles
                        .iter()
                        .position(|item| match (&entry_name, item.profile_name()) {
                            (Some(left), Some(right)) => right.eq_ignore_ascii_case(left),
                            (None, None) => item.matches_identity(&entry),
                            _ => false,
                        })
                {
                    profiles.remove(index);
                }
                profiles.push(entry);
            }

            let mut favorites: Vec<StoredConnection> = Vec::new();
            for entry in store
                .favorites
                .into_iter()
                .map(StoredConnection::normalized)
                .filter(|entry| entry.is_valid())
            {
                if !entry.password.trim().is_empty() {
                    let legacy = entry.password.clone();
                    if let Err(error) = store_secret(&entry, &legacy) {
                        warnings.push(format!(
                            "Could not migrate favorite connection password to keyring; keeping local fallback: {}",
                            error
                        ));
                    }
                }

                if let Some(index) = favorites
                    .iter()
                    .position(|item| item.matches_identity(&entry))
                {
                    favorites.remove(index);
                }
                favorites.push(entry);
            }

            let mut recents: Vec<StoredConnection> = Vec::new();
            for entry in store
                .recents
                .into_iter()
                .map(StoredConnection::normalized)
                .filter(|entry| entry.is_valid())
            {
                if !entry.password.trim().is_empty() {
                    let legacy = entry.password.clone();
                    if let Err(error) = store_secret(&entry, &legacy) {
                        warnings.push(format!(
                            "Could not migrate recent connection password to keyring; keeping local fallback: {}",
                            error
                        ));
                    }
                }

                if let Some(index) = recents
                    .iter()
                    .position(|item| item.matches_identity(&entry))
                {
                    recents.remove(index);
                }
                recents.push(entry);
            }

            if recents.len() > MAX_RECENT_CONNECTIONS {
                recents.truncate(MAX_RECENT_CONNECTIONS);
                updated_store = true;
            }

            store.profiles = profiles;
            store.favorites = favorites;
            store.recents = recents;
            store.default_connection = store
                .default_connection
                .as_ref()
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty());

            let warning = if warnings.is_empty() {
                None
            } else {
                Some(warnings.join("\n"))
            };
            (store, warning, updated_store)
        }
        Err(error) => (ConnectionStore::default(), Some(error.to_string()), false),
    }
}

pub(crate) fn resolve_font_choice(
    font_name: &str,
    font_choices: &[FontChoice],
) -> Option<FontChoice> {
    if font_name.eq_ignore_ascii_case("sans") || font_name.eq_ignore_ascii_case("system font") {
        return Some(FontChoice::Sans);
    }
    if font_name.eq_ignore_ascii_case("monospace") {
        return Some(FontChoice::Monospace);
    }
    font_choices
        .iter()
        .find(|choice| match choice {
            FontChoice::System(family) => family.eq_ignore_ascii_case(font_name),
            _ => false,
        })
        .cloned()
}

fn stored_font_choice(font_name: &str) -> Option<FontChoice> {
    let trimmed = font_name.trim();
    if trimmed.is_empty() {
        return None;
    }
    if trimmed.eq_ignore_ascii_case("sans") || trimmed.eq_ignore_ascii_case("system font") {
        return Some(FontChoice::Sans);
    }
    if trimmed.eq_ignore_ascii_case("monospace") {
        return Some(FontChoice::Monospace);
    }
    Some(FontChoice::System(trimmed.to_string()))
}

pub(crate) fn load_settings_store() -> (Settings, Option<String>, Option<ThemeChoice>) {
    let path = settings_store_path();
    let contents = match std::fs::read_to_string(&path) {
        Ok(contents) => contents,
        Err(error) => {
            if error.kind() == std::io::ErrorKind::NotFound {
                return (Settings::default(), None, None);
            }
            return (Settings::default(), Some(error.to_string()), None);
        }
    };

    let (settings, warning, theme, updated_store) = parse_settings_store(&contents);
    if updated_store {
        let store =
            SettingsStore::from_settings(&settings, theme.unwrap_or(ThemeChoice::CarbonFrost));
        let _ = save_settings_store(&store);
    }
    (settings, warning, theme)
}

fn reset_appearance(settings: &mut Settings) {
    let defaults = Settings::default();
    settings.language = defaults.language;
    settings.onboarding_completed = false;
    settings.onboarding_theme_variant = defaults.onboarding_theme_variant;
    settings.font = defaults.font;
    settings.editor_font = defaults.editor_font;
    settings.font_size = defaults.font_size;
    settings.font_size_input = defaults.font_size_input;
    settings.theme_variant = defaults.theme_variant;
    settings.system_theme_mode = defaults.system_theme_mode;
    settings.dark_theme = defaults.dark_theme;
    settings.light_theme = defaults.light_theme;
    settings.ui_density = defaults.ui_density;
    settings.accent_color = defaults.accent_color;
    settings.default_accent_color = defaults.default_accent_color;
    settings.large_sidebar_buttons = defaults.large_sidebar_buttons;
    settings.compact_sidebar = defaults.compact_sidebar;
    settings.result_grid_density = defaults.result_grid_density;
    settings.modal_backdrop_dim = defaults.modal_backdrop_dim;
    settings.emphasize_column_headers = defaults.emphasize_column_headers;
    settings.auto_scroll_sidebar_to_selected_table = defaults.auto_scroll_sidebar_to_selected_table;
    settings.auto_expand_selected_table = defaults.auto_expand_selected_table;
    settings.show_hidden_tables = defaults.show_hidden_tables;
    settings.multiple_connections_layout = defaults.multiple_connections_layout;
}

fn parse_settings_store(contents: &str) -> (Settings, Option<String>, Option<ThemeChoice>, bool) {
    parse_settings_store_with(contents, store_ai_api_key_secret)
}

fn parse_settings_store_with(
    contents: &str,
    store_ai_secret: AiApiKeySecretStore,
) -> (Settings, Option<String>, Option<ThemeChoice>, bool) {
    match toml::from_str::<SettingsStore>(contents) {
        Ok(store) => {
            let mut warnings = Vec::new();
            let mut updated_store = false;

            if let Some(legacy_ai_key) = store.ai_api_key.clone()
                && !legacy_ai_key.trim().is_empty()
                && let Err(error) = store_ai_secret(&legacy_ai_key)
            {
                warnings.push(format!(
                    "Could not migrate AI API key to keyring (keeping local fallback): {}",
                    error
                ));
            }

            let mut settings = Settings::default();
            if let Some(font_name) = store.font.as_deref()
                && let Some(font) = stored_font_choice(font_name)
            {
                settings.font = font.clone();
                settings.editor_font = font;
            }
            if let Some(font_name) = store.editor_font.as_deref()
                && let Some(font) = stored_font_choice(font_name)
            {
                settings.editor_font = font;
            }
            if let Some(font_size) = store.font_size
                && font_size >= MIN_FONT_SIZE
            {
                settings.font_size = font_size;
                settings.font_size_input = font_size.to_string();
            }
            if let Some(ui_density) = store.ui_density {
                settings.ui_density = ui_density;
            }
            if let Some(accent_color) = store.accent_color {
                settings.accent_color = accent_color;
            }
            if let Some(accent_color) = store.default_accent_color {
                settings.default_accent_color = accent_color;
            }
            if let Some(enabled) = store.large_sidebar_buttons {
                settings.large_sidebar_buttons = enabled;
            }
            if let Some(density) = store.result_grid_density {
                settings.result_grid_density = density;
            }
            if let Some(dim) = store.modal_backdrop_dim
                && dim.is_finite()
            {
                settings.modal_backdrop_dim = dim.clamp(0.20, 1.0);
            }
            if let Some(limit) = store.table_query_limit
                && limit > 0
            {
                settings.table_query_limit = limit;
            }
            if let Some(limit) = store.history_limit {
                settings.history_limit = limit;
            }
            if let Some(saved) = store.saved_queries {
                settings.saved_queries = saved;
            }
            if let Some(recent) = store.recent_omni_commands {
                settings.recent_omni_commands = recent.into_iter().take(4).collect();
            }
            if let Some(limit) = store.table_cache_limit_entries
                && limit > 0
            {
                settings.table_cache_limit_entries = limit;
            }
            if let Some(limit) = store.table_cache_limit_mb
                && limit > 0
            {
                settings.table_cache_limit_mb = limit;
            }
            if let Some(enabled) = store.tabs_enabled {
                settings.tabs_enabled = enabled;
            }
            if let Some(enabled) = store.release_inactive_tab_memory {
                settings.release_inactive_tab_memory = enabled;
            }
            if let Some(seconds) = store.inactive_tab_release_idle_secs
                && seconds > 0
            {
                settings.inactive_tab_release_idle_secs = seconds;
            }
            if let Some(provider) = store.ai_provider {
                settings.ai_provider = provider;
            }
            if let Some(endpoint) = store.ai_endpoint {
                settings.ai_endpoint = endpoint;
            }
            if let Some(model) = store.ai_model {
                settings.ai_model = model;
            }
            if let Some(command) = store.ai_cli_command {
                settings.ai_cli_command = command;
            }
            if let Some(enabled) = store.ai_autocomplete_enabled {
                settings.ai_autocomplete_enabled = enabled;
            }
            if let Some(enabled) = store.ai_autocomplete_use_main_provider {
                settings.ai_autocomplete_use_main_provider = enabled;
            }
            if let Some(enabled) = store.ai_autocomplete_allow_local_cli {
                settings.ai_autocomplete_allow_local_cli = enabled;
            }
            if let Some(enabled) = store.ai_send_temperature {
                settings.ai_send_temperature = enabled;
            }
            if let Some(provider) = store.ai_autocomplete_provider {
                settings.ai_autocomplete_provider = provider;
            }
            if let Some(endpoint) = store.ai_autocomplete_endpoint {
                settings.ai_autocomplete_endpoint = endpoint;
            }
            if let Some(model) = store.ai_autocomplete_model {
                settings.ai_autocomplete_model = model;
            }
            if let Some(key) = store.ai_autocomplete_api_key {
                settings.ai_autocomplete_api_key = key;
            }
            if let Some(enabled) = store.ai_send_schema_context {
                settings.ai_send_schema_context = enabled;
            }
            if let Some(enabled) = store.ai_send_query_context {
                settings.ai_send_query_context = enabled;
            }
            if let Some(mode) = store.chat_mode {
                settings.chat_mode = mode;
            }
            if let Some(delay_ms) = store.query_inline_suggestion_delay_ms
                && (MIN_QUERY_INLINE_SUGGESTION_DELAY_MS..=MAX_QUERY_INLINE_SUGGESTION_DELAY_MS)
                    .contains(&delay_ms)
            {
                settings.query_inline_suggestion_delay_ms = delay_ms;
            }
            if let Some(enabled) = store.schema_autocomplete_enabled {
                settings.schema_autocomplete_enabled = enabled;
            }
            if let Some(enabled) = store.sql_keyword_autocomplete_enabled {
                settings.sql_keyword_autocomplete_enabled = enabled;
            }
            if let Some(enabled) = store.query_editor_line_numbers_enabled {
                settings.query_editor_line_numbers_enabled = enabled;
            }
            if let Some(enabled) = store.query_editor_word_wrap_enabled {
                settings.query_editor_word_wrap_enabled = enabled;
            }
            if let Some(overrides) = store.column_width_overrides {
                settings.column_width_overrides = overrides;
            }
            if let Some(enabled) = store.compact_sidebar {
                settings.compact_sidebar = enabled;
            }
            if let Some(enabled) = store.auto_expand_selected_table {
                settings.auto_expand_selected_table = enabled;
            }
            if let Some(enabled) = store.show_hidden_tables {
                settings.show_hidden_tables = enabled;
            }
            if let Some(enabled) = store.multiple_connections_layout {
                settings.multiple_connections_layout = enabled;
            }
            if let Some(size) = store.tab_size
                && size > 0
                && size <= 16
            {
                settings.tab_size = size;
            }
            if let Some(enabled) = store.insert_spaces {
                settings.insert_spaces = enabled;
            }
            if let Some(secs) = store.connection_timeout_secs
                && secs > 0
            {
                settings.connection_timeout_secs = secs;
            }
            if let Some(secs) = store.query_timeout_secs
                && secs > 0
            {
                settings.query_timeout_secs = secs;
            }
            if let Some(enabled) = store.auto_reconnect {
                settings.auto_reconnect = enabled;
            }
            if let Some(key) = load_ai_api_key_secret() {
                settings.ai_api_key = key;
            } else if let Some(legacy_ai_key) = store.ai_api_key
                && !legacy_ai_key.trim().is_empty()
            {
                settings.ai_api_key = legacy_ai_key;
            }
            if let Some(shortcut) = store.omni_table_shortcut
                && let Some(binding) = ShortcutBinding::parse(&shortcut)
            {
                settings.omni_table_shortcut = binding;
            }
            if let Some(shortcut) = store.omni_command_shortcut
                && let Some(binding) = ShortcutBinding::parse(&shortcut)
                && !binding.is_editor_palette_binding()
            {
                settings.omni_command_shortcut = binding;
            }
            if let Some(shortcut) = store.open_settings_shortcut
                && let Some(binding) = ShortcutBinding::parse(&shortcut)
            {
                settings.open_settings_shortcut = binding;
            }
            if let Some(shortcut) = store.switch_database_shortcut
                && let Some(binding) = ShortcutBinding::parse(&shortcut)
            {
                settings.switch_database_shortcut = binding;
            }
            if let Some(shortcut) = store.focus_table_search_shortcut
                && let Some(binding) = ShortcutBinding::parse(&shortcut)
            {
                settings.focus_table_search_shortcut = binding;
            }
            if let Some(shortcut) = store.toggle_tab_pin_shortcut
                && let Some(binding) = ShortcutBinding::parse(&shortcut)
            {
                settings.toggle_tab_pin_shortcut = binding;
            }
            if let Some(shortcut) = store.toggle_sidebar_shortcut
                && let Some(binding) = ShortcutBinding::parse(&shortcut)
            {
                settings.toggle_sidebar_shortcut = binding;
            }
            if let Some(shortcut) = store.toggle_chat_sidebar_shortcut
                && let Some(binding) = ShortcutBinding::parse(&shortcut)
            {
                settings.toggle_chat_sidebar_shortcut = binding;
            }
            if let Some(shortcut) = store.cycle_theme_shortcut
                && let Some(binding) = ShortcutBinding::parse(&shortcut)
            {
                settings.cycle_theme_shortcut = binding;
            }
            if let Some(shortcut) = store.run_query_shortcut
                && let Some(binding) = ShortcutBinding::parse(&shortcut)
            {
                settings.run_query_shortcut = binding;
            }
            if let Some(shortcut) = store.run_selection_shortcut
                && let Some(binding) = ShortcutBinding::parse(&shortcut)
            {
                settings.run_selection_shortcut = binding;
            }
            if let Some(shortcut) = store.autocomplete_tables_shortcut
                && let Some(binding) = ShortcutBinding::parse(&shortcut)
                && !binding.is_bare_tab()
            {
                settings.autocomplete_tables_shortcut = binding;
            }
            if let Some(shortcut) = store.new_query_shortcut
                && let Some(binding) = ShortcutBinding::parse(&shortcut)
            {
                settings.new_query_shortcut = binding;
            }
            if let Some(shortcut) = store.close_tab_shortcut
                && let Some(binding) = ShortcutBinding::parse(&shortcut)
            {
                settings.close_tab_shortcut = binding;
            }
            if let Some(shortcut) = store.next_tab_shortcut
                && let Some(binding) = ShortcutBinding::parse(&shortcut)
            {
                settings.next_tab_shortcut = binding;
            }
            if let Some(shortcut) = store.previous_tab_shortcut
                && let Some(binding) = ShortcutBinding::parse(&shortcut)
            {
                settings.previous_tab_shortcut = binding;
            }
            if let Some(shortcut) = store.next_results_page_shortcut
                && let Some(binding) = ShortcutBinding::parse(&shortcut)
            {
                settings.next_results_page_shortcut = binding;
            }
            if let Some(shortcut) = store.previous_results_page_shortcut
                && let Some(binding) = ShortcutBinding::parse(&shortcut)
            {
                settings.previous_results_page_shortcut = binding;
            }
            if let Some(shortcut) = store.first_results_page_shortcut
                && let Some(binding) = ShortcutBinding::parse(&shortcut)
            {
                settings.first_results_page_shortcut = binding;
            }
            if let Some(shortcut) = store.last_results_page_shortcut
                && let Some(binding) = ShortcutBinding::parse(&shortcut)
            {
                settings.last_results_page_shortcut = binding;
            }
            if let Some(shortcut) = store.open_table_info_sidebar_shortcut
                && let Some(binding) = ShortcutBinding::parse(&shortcut)
            {
                settings.open_table_info_sidebar_shortcut = binding;
            }
            if let Some(prefix) = store.omni_prefix {
                settings.omni_prefix = normalize_omni_prefix(&prefix);
            }
            if let Some(enabled) = store.emphasize_column_headers {
                settings.emphasize_column_headers = enabled;
            }
            if let Some(enabled) = store.auto_scroll_sidebar_to_selected_table {
                settings.auto_scroll_sidebar_to_selected_table = enabled;
            }
            if let Some(variant) = store.theme_variant {
                settings.theme_variant = variant;
            }
            if let Some(language) = store.language {
                settings.language = language;
            }
            if let Some(enabled) = store.ai_enabled {
                settings.ai_enabled = enabled;
            }
            settings.onboarding_completed = store.onboarding_completed.unwrap_or(true);
            settings.last_changelog_version = Some(match store.last_changelog_version.clone() {
                Some(version) => version,
                None => {
                    updated_store = true;
                    crate::ui::changelog::current_version().to_string()
                }
            });
            settings.onboarding_theme_variant = store
                .onboarding_theme_variant
                .unwrap_or(settings.theme_variant);
            if let Some(mode) = store.system_theme_mode {
                settings.system_theme_mode = mode;
            }
            if let Some(theme) = store.dark_theme {
                settings.dark_theme = theme;
            }
            if let Some(theme) = store.light_theme {
                settings.light_theme = theme;
            }
            if let Some(shortcut) = store.omni_command_alt_shortcut {
                if let Some(binding) = ShortcutBinding::parse(&shortcut) {
                    settings.omni_command_alt_shortcut = binding;
                }
            } else if let Some(enabled) = store.omni_command_f1_alias {
                if enabled {
                    settings.omni_command_alt_shortcut =
                        ShortcutBinding::default_omni_command_alt();
                } else {
                    settings.omni_command_alt_shortcut = settings.omni_command_shortcut.clone();
                }
            }

            let stored_revision = store.appearance_revision.unwrap_or(0);
            let migrating = stored_revision < APPEARANCE_REVISION;
            let theme = if migrating {
                reset_appearance(&mut settings);
                None
            } else {
                store.theme
            };
            settings.appearance_revision = APPEARANCE_REVISION;

            settings.table_query_limit_input = settings.table_query_limit.to_string();
            settings.history_limit_input = settings.history_limit.to_string();
            settings.inactive_tab_release_idle_secs_input =
                settings.inactive_tab_release_idle_secs.to_string();
            settings.connection_timeout_secs_input = settings.connection_timeout_secs.to_string();
            settings.query_timeout_secs_input = settings.query_timeout_secs.to_string();
            settings.query_inline_suggestion_delay_ms_input =
                settings.query_inline_suggestion_delay_ms.to_string();

            let warning = if warnings.is_empty() {
                None
            } else {
                Some(warnings.join("\n"))
            };
            (settings, warning, theme, updated_store || migrating)
        }
        Err(error) => (Settings::default(), Some(error.to_string()), None, false),
    }
}

pub(crate) fn load_folder_store() -> (FolderStore, Option<String>) {
    let path = folder_store_path();
    let contents = match std::fs::read_to_string(&path) {
        Ok(contents) => contents,
        Err(error) => {
            if error.kind() == std::io::ErrorKind::NotFound {
                return (FolderStore::default(), None);
            }
            return (FolderStore::default(), Some(error.to_string()));
        }
    };

    match toml::from_str::<FolderStore>(&contents) {
        Ok(store) => (store, None),
        Err(error) => (FolderStore::default(), Some(error.to_string())),
    }
}

fn write_toml_file(path: PathBuf, payload: String) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    std::fs::write(path, payload).map_err(|error| error.to_string())
}

pub(crate) fn save_connection_store(store: &ConnectionStore) -> Result<(), String> {
    let path = connection_store_path();
    let payload = toml::to_string_pretty(store).map_err(|error| error.to_string())?;
    write_toml_file(path, payload)
}

pub(crate) fn save_settings_store(store: &SettingsStore) -> Result<(), String> {
    let path = settings_store_path();
    let payload = toml::to_string_pretty(store).map_err(|error| error.to_string())?;
    write_toml_file(path, payload)
}

pub(crate) fn save_folder_store(store: &FolderStore) -> Result<(), String> {
    let path = folder_store_path();
    let payload = toml::to_string_pretty(store).map_err(|error| error.to_string())?;
    write_toml_file(path, payload)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::Language;
    use crate::{DiagramRelations, StoredDiagram, TlsMode};

    #[test]
    fn diagram_store_round_trips_through_a_real_file() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let path = directory.path().join(DIAGRAMS_STORE_FILE);
        let mut store = DiagramStore::default();
        store
            .connections
            .entry(String::from("connection"))
            .or_default()
            .databases
            .entry(String::from("database"))
            .or_default()
            .diagrams
            .push(StoredDiagram {
                name: String::from("Billing"),
                zoom: 0.75,
                fullscreen: true,
                relations: DiagramRelations::Selected,
                ..StoredDiagram::default()
            });

        save_diagram_store_to(path.clone(), &store).expect("save diagram store");
        let loaded = load_diagram_store_from(&path);
        let diagram = &loaded.connections["connection"].databases["database"].diagrams[0];

        assert_eq!(diagram.name, "Billing");
        assert_eq!(diagram.zoom, 0.75);
        assert!(diagram.fullscreen);
        assert_eq!(diagram.relations, DiagramRelations::Selected);
    }

    fn stored_mysql(password: &str) -> StoredConnection {
        StoredConnection {
            name: String::from("primary"),
            driver: DatabaseDriver::MySql,
            sqlite_path: String::new(),
            host: String::from("db.example.com"),
            port: String::from("3306"),
            database: String::from("app"),
            username: String::from("root"),
            password: password.to_string(),
            tls_mode: TlsMode::Prefer,
            tls_ca_cert_path: String::new(),
            tls_client_cert_path: String::new(),
            tls_client_key_path: String::new(),
            tags: Vec::new(),
        }
    }

    fn keyring_write_succeeds(_entry: &StoredConnection, _password: &str) -> Result<(), String> {
        Ok(())
    }

    fn keyring_write_fails(_entry: &StoredConnection, _password: &str) -> Result<(), String> {
        Err(String::from("keyring unavailable"))
    }

    fn ai_keyring_write_succeeds(_api_key: &str) -> Result<(), String> {
        Ok(())
    }

    fn ai_keyring_write_fails(_api_key: &str) -> Result<(), String> {
        Err(String::from("keyring unavailable"))
    }

    #[test]
    fn sanitize_connection_store_secrets_keeps_password_when_keyring_write_fails() {
        let store = ConnectionStore {
            profiles: vec![stored_mysql("secret")],
            favorites: vec![stored_mysql("favorite-secret")],
            recents: Vec::new(),
            ..ConnectionStore::default()
        };

        let warning = sanitize_connection_store_secrets_with(&store, keyring_write_fails);

        assert_eq!(warning, Some(String::from("keyring unavailable")));
        assert_eq!(store.profiles[0].password, "secret");
        assert_eq!(store.favorites[0].password, "favorite-secret");
    }

    #[test]
    fn sanitize_connection_store_secrets_keeps_password_when_keyring_write_succeeds() {
        let store = ConnectionStore {
            profiles: vec![stored_mysql("secret")],
            recents: Vec::new(),
            ..ConnectionStore::default()
        };

        let warning = sanitize_connection_store_secrets_with(&store, keyring_write_succeeds);

        assert_eq!(warning, None);
        assert_eq!(store.profiles[0].password, "secret");
    }

    #[test]
    fn load_connection_store_keeps_profile_password_when_keyring_migration_fails() {
        let store = ConnectionStore {
            profiles: vec![stored_mysql("profile-secret")],
            recents: Vec::new(),
            ..ConnectionStore::default()
        };
        let payload = toml::to_string(&store).expect("connection store payload");

        let (loaded, warning, updated_store) =
            parse_connection_store(&payload, keyring_write_fails);

        assert!(!updated_store);
        assert_eq!(loaded.profiles[0].password, "profile-secret");
        assert!(
            warning
                .expect("migration warning")
                .contains("keeping local fallback")
        );
    }

    #[test]
    fn load_connection_store_keeps_favorite_password_when_keyring_migration_fails() {
        let store = ConnectionStore {
            profiles: Vec::new(),
            favorites: vec![stored_mysql("favorite-secret")],
            recents: Vec::new(),
            ..ConnectionStore::default()
        };
        let payload = toml::to_string(&store).expect("connection store payload");

        let (loaded, warning, updated_store) =
            parse_connection_store(&payload, keyring_write_fails);

        assert!(!updated_store);
        assert_eq!(loaded.favorites[0].password, "favorite-secret");
        assert!(
            warning
                .expect("migration warning")
                .contains("keeping local fallback")
        );
    }

    #[test]
    fn load_connection_store_keeps_profile_password_when_keyring_migration_succeeds() {
        let store = ConnectionStore {
            profiles: vec![stored_mysql("profile-secret")],
            recents: Vec::new(),
            ..ConnectionStore::default()
        };
        let payload = toml::to_string(&store).expect("connection store payload");

        let (loaded, warning, updated_store) =
            parse_connection_store(&payload, keyring_write_succeeds);

        assert!(!updated_store);
        assert_eq!(warning, None);
        assert_eq!(loaded.profiles[0].password, "profile-secret");
    }

    #[test]
    fn load_connection_store_keeps_recent_password_when_keyring_migration_fails() {
        let store = ConnectionStore {
            profiles: Vec::new(),
            recents: vec![stored_mysql("recent-secret")],
            ..ConnectionStore::default()
        };
        let payload = toml::to_string(&store).expect("connection store payload");

        let (loaded, warning, updated_store) =
            parse_connection_store(&payload, keyring_write_fails);

        assert!(!updated_store);
        assert_eq!(loaded.recents[0].password, "recent-secret");
        assert!(
            warning
                .expect("migration warning")
                .contains("keeping local fallback")
        );
    }

    #[test]
    fn appearance_revision_bump_resets_looks_and_reruns_onboarding() {
        let saved = Settings {
            appearance_revision: 0,
            onboarding_completed: true,
            language: Language::Spanish,
            ui_density: crate::UiDensity::Compact,
            accent_color: crate::model::settings::AppearanceColor::Pink,
            font_size: 21,
            multiple_connections_layout: false,
            query_timeout_secs: 99,
            omni_prefix: String::from(">"),
            ai_endpoint: String::from("http://localhost:11434"),
            ai_model: String::from("llama3"),
            omni_table_shortcut: ShortcutBinding::parse("ctrl+shift+j").expect("binding"),
            ..Settings::default()
        };
        let mut store = SettingsStore::from_settings(&saved, ThemeChoice::TokyoNight);
        store.appearance_revision = Some(0);
        let payload = toml::to_string(&store).expect("settings store payload");

        let (settings, _warning, theme, updated_store) =
            parse_settings_store_with(&payload, ai_keyring_write_succeeds);

        let defaults = Settings::default();
        assert!(updated_store, "the migrated store must be written back");
        assert_eq!(settings.appearance_revision, APPEARANCE_REVISION);
        assert!(!settings.onboarding_completed);
        assert_eq!(theme, None, "theme falls back to the default palette");

        assert_eq!(settings.language, defaults.language);
        assert_eq!(settings.ui_density, defaults.ui_density);
        assert_eq!(settings.accent_color, defaults.accent_color);
        assert_eq!(settings.font_size, defaults.font_size);
        assert!(settings.multiple_connections_layout);

        assert_eq!(settings.query_timeout_secs, 99);
        assert_eq!(settings.omni_prefix, ">");
        assert_eq!(settings.ai_endpoint, "http://localhost:11434");
        assert_eq!(settings.ai_model, "llama3");
        assert_eq!(
            settings.omni_table_shortcut,
            ShortcutBinding::parse("ctrl+shift+j").expect("binding")
        );
    }

    #[test]
    fn current_appearance_revision_is_left_alone() {
        let saved = Settings {
            onboarding_completed: true,
            ui_density: crate::UiDensity::Compact,
            last_changelog_version: Some(String::from("0.1.0")),
            ..Settings::default()
        };
        let store = SettingsStore::from_settings(&saved, ThemeChoice::TokyoNight);
        let payload = toml::to_string(&store).expect("settings store payload");

        let (settings, _warning, theme, updated_store) =
            parse_settings_store_with(&payload, ai_keyring_write_succeeds);

        assert!(!updated_store);
        assert!(settings.onboarding_completed);
        assert_eq!(settings.ui_density, crate::UiDensity::Compact);
        assert_eq!(theme, Some(ThemeChoice::TokyoNight));
    }

    #[test]
    fn a_store_without_a_changelog_baseline_is_sealed_and_written_back() {
        let store = SettingsStore {
            last_changelog_version: None,
            ..SettingsStore::from_settings(&Settings::default(), ThemeChoice::CarbonFrost)
        };
        let payload = toml::to_string(&store).expect("settings store payload");

        let (settings, _warning, _theme, updated_store) =
            parse_settings_store_with(&payload, ai_keyring_write_succeeds);

        assert_eq!(
            settings.last_changelog_version.as_deref(),
            Some(crate::ui::changelog::current_version()),
            "an install predating release notes must not be shown them"
        );
        assert!(
            updated_store,
            "the sealed baseline must be persisted, or the next upgrade re-seals and the notes never appear"
        );
    }

    #[test]
    fn a_stored_changelog_baseline_is_kept() {
        let saved = Settings {
            last_changelog_version: Some(String::from("0.1.0")),
            ..Settings::default()
        };
        let store = SettingsStore::from_settings(&saved, ThemeChoice::CarbonFrost);
        let payload = toml::to_string(&store).expect("settings store payload");

        let (settings, _warning, _theme, updated_store) =
            parse_settings_store_with(&payload, ai_keyring_write_succeeds);

        assert_eq!(settings.last_changelog_version.as_deref(), Some("0.1.0"));
        assert!(!updated_store);
    }

    #[test]
    fn load_settings_store_keeps_legacy_ai_key_when_keyring_migration_succeeds() {
        let store = SettingsStore {
            ai_api_key: Some(String::from("sk-test")),
            ..SettingsStore::from_settings(&Settings::default(), ThemeChoice::CarbonFrost)
        };
        let payload = toml::to_string(&store).expect("settings store payload");

        let (settings, warning, _theme, _updated_store) =
            parse_settings_store_with(&payload, ai_keyring_write_succeeds);
        let rewritten = SettingsStore::from_settings(&settings, ThemeChoice::CarbonFrost);

        assert_eq!(warning, None);
        assert_eq!(settings.ai_api_key, "sk-test");
        assert_eq!(rewritten.ai_api_key, Some(String::from("sk-test")));
    }

    #[test]
    fn load_settings_store_keeps_legacy_ai_key_when_keyring_migration_fails() {
        let saved = Settings {
            last_changelog_version: Some(String::from("0.1.0")),
            ..Settings::default()
        };
        let store = SettingsStore {
            ai_api_key: Some(String::from("sk-test")),
            ..SettingsStore::from_settings(&saved, ThemeChoice::CarbonFrost)
        };
        let payload = toml::to_string(&store).expect("settings store payload");

        let (settings, warning, _theme, updated_store) =
            parse_settings_store_with(&payload, ai_keyring_write_fails);

        assert!(!updated_store);
        assert_eq!(settings.ai_api_key, "sk-test");
        assert!(
            warning
                .expect("migration warning")
                .contains("keeping local fallback")
        );
    }
}
