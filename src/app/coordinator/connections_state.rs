use crate::app::core::App;
use crate::app::message::Message;
use crate::constants::{
    LOGIN_SIDEBAR_WIDTH, MAX_RECENT_CONNECTIONS, MYSQL_DEFAULT_PORT, POSTGRES_DEFAULT_PORT,
    SIDEBAR_SCROLLBAR_GUTTER, SIDEBAR_SCROLLBAR_WIDTH,
};
use crate::model::connection::{ConnectionInfo, DatabaseDriver, StoredConnection, TlsMode};
use crate::storage::{
    load_connection_secret, load_connection_store, save_connection_store, store_connection_secret,
};
use crate::ui::ids::database_switcher_scroll_id;
use crate::utils::text::max_chars_for_width;
use iced::widget::scrollable;
use iced::{Font, Task};

impl App {
    pub(crate) fn postgres_terminal_ssl_mode(tls_mode: TlsMode) -> &'static str {
        match tls_mode {
            TlsMode::Disabled => "disable",
            TlsMode::Prefer => "prefer",
            TlsMode::Require => "require",
            TlsMode::VerifyCa => "verify-ca",
            TlsMode::VerifyFull => "verify-full",
        }
    }

    pub(crate) fn create_postgres_pgpass_file(
        &self,
    ) -> Result<Option<tempfile::NamedTempFile>, String> {
        let password = self.connections.current.password.trim();
        if password.is_empty() {
            return Ok(None);
        }

        let username = self.connections.current.username.trim();
        if username.is_empty() {
            return Ok(None);
        }

        let host = if self.connections.current.host.trim().is_empty() {
            "localhost"
        } else {
            self.connections.current.host.trim()
        };
        let port = if self.connections.current.port.trim().is_empty() {
            POSTGRES_DEFAULT_PORT
        } else {
            self.connections.current.port.trim()
        };
        let escaped_password = password.replace('\\', r"\\").replace(':', r"\:");
        let entry = format!("{host}:{port}:*:{username}:{escaped_password}\n");

        let file = tempfile::NamedTempFile::new().map_err(|error| error.to_string())?;
        std::fs::write(file.path(), entry).map_err(|error| error.to_string())?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let permissions = std::fs::Permissions::from_mode(0o600);
            std::fs::set_permissions(file.path(), permissions)
                .map_err(|error| error.to_string())?;
        }

        Ok(Some(file))
    }

    pub(crate) fn apply_saved_connection(&mut self, entry: &StoredConnection) {
        self.connections.current.driver = entry.driver;
        self.connections.current.sqlite_path = entry.sqlite_path.clone();
        self.connections.current.host = entry.host.clone();
        self.connections.current.port = if entry.port.trim().is_empty() {
            entry.driver.default_port().to_string()
        } else {
            entry.port.clone()
        };
        self.connections.current.database = entry.database.clone();
        self.connections.current.username = entry.username.clone();
        self.connections.current.password = load_connection_secret(entry).unwrap_or_default();
        self.connections.current.tls_mode = entry.tls_mode;
        self.connections.current.tls_ca_cert_path = entry.tls_ca_cert_path.clone();
        self.connections.current.tls_client_cert_path = entry.tls_client_cert_path.clone();
        self.connections.current.tls_client_key_path = entry.tls_client_key_path.clone();
        self.connections.connect_error = None;
    }

    pub(crate) fn add_favorite_from_current(&mut self) {
        let entry = StoredConnection::from_info(&self.connections.current);
        if !entry.is_valid() {
            return;
        }
        let keyring_error = if self.connections.current.password.trim().is_empty() {
            None
        } else {
            store_connection_secret(&entry, &self.connections.current.password).err()
        };
        if let Some(error) = keyring_error {
            self.connections.store_error = Some(format!(
                "Could not save password to keyring: {error}. Password kept in local fallback."
            ));
        }
        if let Some(index) = self
            .connections
            .favorites
            .iter()
            .position(|item| item.matches_identity(&entry))
        {
            let old = self.connections.favorites.remove(index);
            let mut merged = entry;
            if !old.name.trim().is_empty() && merged.name.trim().is_empty() {
                merged.name = old.name;
            }
            if !old.tags.is_empty() && merged.tags.is_empty() {
                merged.tags = old.tags;
            }
            self.connections.favorites.insert(0, merged);
        } else {
            self.connections.favorites.insert(0, entry);
        }
        self.persist_connection_store();
        self.refresh_connection_tab_labels();
    }

    pub(crate) fn record_recent_connection(&mut self) {
        let entry = StoredConnection::from_info(&self.connections.current);
        if !entry.is_valid() {
            return;
        }
        let keyring_error = if self.connections.current.password.trim().is_empty() {
            None
        } else {
            store_connection_secret(&entry, &self.connections.current.password).err()
        };
        if let Some(error) = keyring_error {
            self.connections.store_error = Some(format!(
                "Could not save password to keyring: {error}. Password kept in local fallback."
            ));
        }
        if let Some(index) = self
            .connections
            .recents
            .iter()
            .position(|item| item.matches_identity(&entry))
        {
            self.connections.recents.remove(index);
        }
        self.connections.recents.insert(0, entry);
        if self.connections.recents.len() > MAX_RECENT_CONNECTIONS {
            self.connections.recents.truncate(MAX_RECENT_CONNECTIONS);
        }
        self.persist_connection_store();
        self.refresh_connection_tab_labels();
    }

    pub(crate) fn persist_connection_store(&mut self) {
        let mut store = load_connection_store().0;
        store.favorites = self.connections.favorites.clone();
        store.recents = self.connections.recents.clone();

        let keyring_error = crate::storage::sanitize_connection_store_secrets(&store);

        match save_connection_store(&store) {
            Ok(()) => {
                self.connections.favorites = store.favorites;
                self.connections.recents = store.recents;
                if let Some(error) = keyring_error {
                    self.connections.store_error = Some(format!(
                        "Could not save password to keyring: {error}. Password kept in local fallback."
                    ));
                } else {
                    self.connections.store_error = None;
                }
            }
            Err(error) => self.connections.store_error = Some(error),
        }
    }

    pub(crate) fn default_connection_info() -> ConnectionInfo {
        ConnectionInfo {
            driver: DatabaseDriver::MySql,
            host: String::from("127.0.0.1"),
            port: MYSQL_DEFAULT_PORT.to_string(),
            database: String::new(),
            username: String::new(),
            password: String::new(),
            sqlite_path: String::new(),
            tls_mode: TlsMode::Prefer,
            tls_ca_cert_path: String::new(),
            tls_client_cert_path: String::new(),
            tls_client_key_path: String::new(),
        }
    }

    pub(crate) fn connection_tab_label_from_entry(entry: &StoredConnection) -> String {
        let mut label = entry.profile_label();
        if !entry.tags.is_empty() {
            label.push_str(" · ");
            label.push_str(&entry.tags.join(" · "));
        }
        label
    }

    pub(crate) fn current_connection_tab_label(&self) -> String {
        let entry = StoredConnection::from_info(&self.connections.current);
        self.saved_connection_for_tab_label(&entry)
            .map(Self::connection_tab_label_from_entry)
            .unwrap_or_else(|| entry.profile_label())
    }

    pub(crate) fn refresh_connection_tab_labels(&mut self) {
        for index in 0..self.connections.tabs.len() {
            let entry = StoredConnection::from_info(&self.connections.tabs[index].connection);
            let label = self
                .saved_connection_for_tab_label(&entry)
                .map(Self::connection_tab_label_from_entry)
                .unwrap_or_else(|| entry.profile_label());
            self.connections.tabs[index].label = label;
        }
    }

    pub(crate) fn next_connection_tab_id(&mut self) -> u64 {
        let id = self.connections.next_tab_id;
        self.connections.next_tab_id += 1;
        id
    }

    pub(crate) fn ensure_active_connection_tab(&mut self) {
        if !self.settings.values.multiple_connections_layout || !self.connections.connected {
            return;
        }

        let label = self.current_connection_tab_label();
        let connection = self.connections.current.clone();

        if let Some(id) = self.connections.active_tab_id
            && let Some(tab) = self.connections.tabs.iter_mut().find(|tab| tab.id == id)
        {
            tab.label = label;
            tab.connection = connection;
            tab.snapshot = None;
            return;
        }

        let id = self.next_connection_tab_id();
        self.connections.active_tab_id = Some(id);
        self.connections
            .tabs
            .push(crate::app::session::ConnectionTab {
                id,
                label,
                connection,
                snapshot: None,
            });
    }

    pub(crate) fn take_connection_snapshot(&mut self) -> crate::app::session::ConnectionSnapshot {
        let active_session = self.take_active_database_session();
        crate::app::session::ConnectionSnapshot {
            connection: self.connections.current.clone(),
            active_session,
            database_sessions: std::mem::take(&mut self.connections.database_sessions),
            databases: std::mem::take(&mut self.connections.databases),
            tables: std::mem::take(&mut self.workspace.explorer.tables),
            postgres_sidebar_objects: std::mem::take(
                &mut self.workspace.explorer.postgres_sidebar_objects,
            ),
            postgres_schema_open: std::mem::take(&mut self.workspace.explorer.postgres_schema_open),
            postgres_schema_kind_open: std::mem::take(
                &mut self.workspace.explorer.postgres_schema_kind_open,
            ),
            sidebar_triggers: std::mem::take(&mut self.workspace.explorer.sidebar_triggers),
            table_triggers_cache: std::mem::take(&mut self.workspace.explorer.table_triggers_cache),
            sidebar_relations: std::mem::take(&mut self.workspace.explorer.sidebar_relations),
            table_relations_cache: std::mem::take(
                &mut self.workspace.explorer.table_relations_cache,
            ),
            table_folders: std::mem::take(&mut self.workspace.explorer.table_folders),
            table_folder_map: std::mem::take(&mut self.workspace.explorer.table_folder_map),
            table_search: std::mem::take(&mut self.workspace.explorer.table_search),
            database_error: self.connections.database_error.take(),
            table_error: self.connections.table_error.take(),
            chat: self.take_chat_connection_state(),
        }
    }

    pub(crate) fn restore_connection_snapshot(
        &mut self,
        snapshot: crate::app::session::ConnectionSnapshot,
    ) -> Task<Message> {
        self.connections.current = snapshot.connection;
        self.connections.database_sessions = snapshot.database_sessions;
        self.connections.databases = snapshot.databases;
        self.workspace.explorer.tables = snapshot.tables;
        self.workspace.explorer.postgres_sidebar_objects = snapshot.postgres_sidebar_objects;
        self.workspace.explorer.postgres_schema_open = snapshot.postgres_schema_open;
        self.workspace.explorer.postgres_schema_kind_open = snapshot.postgres_schema_kind_open;
        self.workspace.explorer.table_folders = snapshot.table_folders;
        self.workspace.explorer.table_folder_map = snapshot.table_folder_map;
        self.workspace.explorer.table_search = snapshot.table_search;
        self.connections.database_error = snapshot.database_error;
        self.connections.table_error = snapshot.table_error;
        self.connections.pool = None;
        self.connections.connected = true;
        self.connections.connecting = false;
        self.connections.loading_tables = false;
        self.connections.loading_databases = false;
        self.workspace.query.running = false;
        self.workspace.query.cancel_flag = None;
        self.restore_database_session(snapshot.active_session);
        self.workspace.explorer.sidebar_triggers = snapshot.sidebar_triggers;
        self.workspace.explorer.table_triggers_cache = snapshot.table_triggers_cache;
        self.workspace.explorer.sidebar_relations = snapshot.sidebar_relations;
        self.workspace.explorer.table_relations_cache = snapshot.table_relations_cache;
        self.maybe_refresh_omni_bar_results();
        self.restore_chat_connection_state(snapshot.chat);
        match self.ai.chat_deferred_reply.take() {
            Some((request_id, result)) => Task::done(Message::Ai(
                crate::app::features::ai::Message::ChatReplyReady {
                    scope: self.chat_scope_key(),
                    request_id,
                    result,
                },
            )),
            None => Task::none(),
        }
    }

    pub(crate) fn save_active_connection_tab_snapshot(&mut self) {
        if !self.settings.values.multiple_connections_layout || !self.connections.connected {
            return;
        }

        let id = if let Some(id) = self.connections.active_tab_id {
            id
        } else {
            let id = self.next_connection_tab_id();
            let label = self.current_connection_tab_label();
            let connection = self.connections.current.clone();
            self.connections.active_tab_id = Some(id);
            self.connections
                .tabs
                .push(crate::app::session::ConnectionTab {
                    id,
                    label,
                    connection,
                    snapshot: None,
                });
            id
        };

        let label = self.current_connection_tab_label();
        let connection = self.connections.current.clone();
        let snapshot = self.take_connection_snapshot();

        if let Some(tab) = self.connections.tabs.iter_mut().find(|tab| tab.id == id) {
            tab.label = label;
            tab.connection = connection;
            tab.snapshot = Some(snapshot);
        }
    }

    pub(crate) fn switch_connection_tab(&mut self, id: u64) -> Task<Message> {
        if self.connections.active_tab_id == Some(id) {
            return Task::none();
        }

        if self.connections.connecting
            || self.connections.loading_tables
            || self.connections.loading_databases
            || self.workspace.query.running
            || self.workspace.results.applying_changes
        {
            return Task::none();
        }

        let Some(target_index) = self.connections.tabs.iter().position(|tab| tab.id == id) else {
            return Task::none();
        };

        if self.connections.tabs[target_index].snapshot.is_none() {
            return Task::none();
        }

        self.save_active_connection_tab_snapshot();
        let Some(snapshot) = self.connections.tabs[target_index].snapshot.take() else {
            return Task::none();
        };
        let task = self.restore_connection_snapshot(snapshot);
        self.connections.active_tab_id = Some(id);
        task
    }

    pub(crate) fn close_connection_tab(&mut self, id: u64) -> Option<Task<Message>> {
        let index = self.connections.tabs.iter().position(|tab| tab.id == id)?;

        let was_active = self.connections.active_tab_id == Some(id);
        let removed = self.connections.tabs.remove(index);
        let prefix = Self::chat_scope_prefix_for(&removed.connection);
        self.ai
            .chat_sessions
            .retain(|scope, _| !scope.starts_with(&prefix));
        self.ai
            .chat_active_session
            .retain(|scope, _| !scope.starts_with(&prefix));

        if !was_active {
            return Some(Task::none());
        }

        self.connections.active_tab_id = None;
        if self.connections.tabs.is_empty() {
            return None;
        }

        let next_index = index.saturating_sub(1).min(self.connections.tabs.len() - 1);
        let next_id = self.connections.tabs[next_index].id;
        let snapshot = self.connections.tabs[next_index].snapshot.take()?;
        let task = self.restore_connection_snapshot(snapshot);
        self.connections.active_tab_id = Some(next_id);
        Some(task)
    }

    pub(crate) fn prepare_new_connection_tab(&mut self) {
        self.save_active_connection_tab_snapshot();
        self.connections.active_tab_id = None;
        self.connections.current = Self::default_connection_info();
        self.connections.connected = false;
        self.connections.connecting = false;
        self.connections.pool = None;
        self.connections.database_sessions.clear();
        self.connections.databases.clear();
        self.workspace.explorer.tables.clear();
        self.workspace.explorer.postgres_sidebar_objects.clear();
        self.workspace.explorer.postgres_schema_open.clear();
        self.workspace.explorer.postgres_schema_kind_open.clear();
        self.workspace.explorer.sidebar_triggers.clear();
        self.workspace.explorer.table_triggers_cache.clear();
        self.workspace.explorer.sidebar_relations.clear();
        self.workspace.explorer.table_relations_cache.clear();
        self.workspace.explorer.table_folders.clear();
        self.workspace.explorer.table_folder_map.clear();
        self.connections.database_error = None;
        self.connections.table_error = None;
        self.connections.connect_error = None;
        self.reset_database_workspace();
    }

    pub(crate) fn current_table_for_info(&self) -> Option<String> {
        self.workspace
            .query
            .table
            .clone()
            .or_else(|| self.workspace.selected_table.clone())
    }

    pub(crate) fn can_open_postgres_terminal(&self) -> bool {
        self.connections.connected
            && self.connections.current.driver == DatabaseDriver::PostgreSql
            && self.current_database().is_some()
    }

    pub(crate) fn close_postgres_terminal_session(&mut self) {
        self.connections.postgres_terminal_open = false;
        self.connections.postgres_terminal = None;
        self.connections.postgres_terminal_title.clear();
        self.connections.postgres_terminal_pgpass = None;
    }

    pub(crate) fn open_postgres_terminal(&mut self) -> Result<(), String> {
        if !self.connections.connected {
            return Err(String::from("Not connected."));
        }
        if self.connections.current.driver != DatabaseDriver::PostgreSql {
            return Err(String::from(
                "PostgreSQL terminal is only available for PostgreSQL connections.",
            ));
        }

        let database = self
            .current_database()
            .ok_or_else(|| String::from("Select a database first."))?;

        let mut env = std::collections::HashMap::new();
        env.insert(String::from("TERM"), String::from("xterm-256color"));
        env.insert(String::from("PGDATABASE"), database.clone());

        let host = self.connections.current.host.trim();
        if !host.is_empty() {
            env.insert(String::from("PGHOST"), host.to_string());
        }

        let port = self.connections.current.port.trim();
        env.insert(
            String::from("PGPORT"),
            if port.is_empty() {
                POSTGRES_DEFAULT_PORT.to_string()
            } else {
                port.to_string()
            },
        );

        let username = self.connections.current.username.trim();
        if !username.is_empty() {
            env.insert(String::from("PGUSER"), username.to_string());
        }

        env.insert(
            String::from("PGSSLMODE"),
            Self::postgres_terminal_ssl_mode(self.connections.current.tls_mode).to_string(),
        );

        let ca_cert_path = self.connections.current.tls_ca_cert_path.trim();
        if !ca_cert_path.is_empty() {
            env.insert(String::from("PGSSLROOTCERT"), ca_cert_path.to_string());
        }
        let client_cert_path = self.connections.current.tls_client_cert_path.trim();
        if !client_cert_path.is_empty() {
            env.insert(String::from("PGSSLCERT"), client_cert_path.to_string());
        }
        let client_key_path = self.connections.current.tls_client_key_path.trim();
        if !client_key_path.is_empty() {
            env.insert(String::from("PGSSLKEY"), client_key_path.to_string());
        }

        let pgpass_file = self.create_postgres_pgpass_file()?;
        if let Some(file) = pgpass_file.as_ref() {
            env.insert(
                String::from("PGPASSFILE"),
                file.path().display().to_string(),
            );
        }

        let settings = iced_term::settings::Settings {
            font: iced_term::settings::FontSettings {
                size: self.input_text_size() as f32,
                scale_factor: 1.2,
                font_type: Font::MONOSPACE,
            },
            backend: iced_term::settings::BackendSettings {
                program: String::from("psql"),
                args: Vec::new(),
                env,
                working_directory: None,
            },
            ..Default::default()
        };

        let terminal_id = self.connections.next_postgres_terminal_id;
        self.connections.next_postgres_terminal_id =
            self.connections.next_postgres_terminal_id.saturating_add(1);
        let terminal = iced_term::Terminal::new(terminal_id, settings)
            .map_err(|error| format!("Could not start `psql`: {error}"))?;

        self.connections.postgres_terminal_title = format!("psql - {database}");
        self.connections.postgres_terminal_pgpass = pgpass_file;
        self.connections.postgres_terminal = Some(terminal);
        self.connections.postgres_terminal_open = true;

        Ok(())
    }

    pub(crate) fn focus_postgres_terminal(&self) -> Task<Message> {
        if let Some(terminal) = &self.connections.postgres_terminal {
            iced_term::TerminalView::focus(terminal.widget_id().clone())
        } else {
            Task::none()
        }
    }

    pub(crate) fn current_database(&self) -> Option<String> {
        let database = self.connections.current.database.trim();
        if database.is_empty() {
            None
        } else {
            Some(database.to_string())
        }
    }

    pub(crate) fn current_database_index(&self) -> Option<usize> {
        let current = self.connections.current.database.trim();
        if current.is_empty() {
            return None;
        }
        self.connections
            .databases
            .iter()
            .position(|db| db == current)
    }

    pub(crate) fn open_database_switcher(&mut self) {
        self.settings.settings_open = false;
        self.settings.theme_picker_open = false;
        self.settings.settings_theme_picker_open = false;
        self.connections.database_picker_open = false;
        self.workspace.explorer.sidebar_tools_open = false;
        self.workspace.explorer.table_context_menu = None;
        self.settings.theme_search.clear();
        self.settings.settings_theme_search.clear();
        self.connections.database_search.clear();
        self.shell.database_switcher_open = true;
        if let Some(index) = self.current_database_index() {
            self.shell.database_switcher_index = index;
        } else {
            self.shell.database_switcher_index = 0;
        }
    }

    pub(crate) fn move_database_switcher(&mut self, delta: i32) {
        if self.connections.databases.is_empty() {
            return;
        }
        let len = self.connections.databases.len() as i32;
        let next =
            (self.shell.database_switcher_index as i32 + delta).clamp(0, len.saturating_sub(1));
        self.shell.database_switcher_index = next as usize;
    }

    pub(crate) fn scroll_database_switcher_to_index(&self, index: usize) -> Task<Message> {
        let row_height = self.button_text_size() as f32 * 1.3 + self.scale_f32(8.0);
        iced::widget::operation::scroll_to(
            database_switcher_scroll_id(),
            scrollable::AbsoluteOffset {
                x: 0.0,
                y: row_height * index as f32,
            },
        )
    }

    pub(crate) fn can_connect(&self) -> bool {
        if self.connections.connecting {
            return false;
        }
        match self.connections.current.driver {
            DatabaseDriver::Sqlite => !self.connections.current.sqlite_path.trim().is_empty(),
            _ => {
                !self.connections.current.host.trim().is_empty()
                    && !self.connections.current.username.trim().is_empty()
                    && (!matches!(
                        self.connections.current.tls_mode,
                        TlsMode::VerifyCa | TlsMode::VerifyFull
                    ) || !self.connections.current.tls_ca_cert_path.trim().is_empty())
            }
        }
    }

    pub(crate) fn login_sidebar_max_chars(&self) -> usize {
        let panel_width = self.scale_f32(LOGIN_SIDEBAR_WIDTH);
        let panel_padding = self.scale_u16(8) * 2.0;
        let inner_width = (panel_width - panel_padding).max(0.0);
        let scrollbar_width = self.scale_f32(SIDEBAR_SCROLLBAR_WIDTH);
        let scrollbar_spacing = self.scale_f32(SIDEBAR_SCROLLBAR_GUTTER);
        let button_padding = self.button_padding_tight()[1] * 2.0;
        let reserved = scrollbar_width + scrollbar_spacing + button_padding;
        let available = (inner_width - reserved).max(40.0);
        let adjusted = available / self.font_scale();
        max_chars_for_width(adjusted)
    }
}
