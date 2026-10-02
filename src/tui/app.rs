use crate::ai::AiRequestConfig;
use crate::ai::client::generate_ai_sql;
use crate::db::DatabasePool;
use crate::db::connect::{connect_mysql, connect_postgres, connect_sqlite_profile};
use crate::db::metadata::{fetch_databases, fetch_query_suggestion_columns, fetch_tables};
use crate::db::query::run_query_with_control;
use crate::model::connection::ConnectionInfo;
use crate::storage::load_connection_secret;
use crate::utils::helpers::sql_quote_table_reference;
use crate::{DatabaseDriver, StoredConnection, TlsMode};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::widgets::ListState;
use tui_textarea::TextArea;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Normal,
    Insert(InsertMode),
    Command,
    Prompt(PromptMode),
    Help,
}

impl Mode {
    pub fn label(self) -> &'static str {
        match self {
            Mode::Normal => "NORMAL",
            Mode::Insert(InsertMode::Query) => "INSERT",
            Mode::Insert(InsertMode::Login) => "INSERT:LOGIN",
            Mode::Command => "COMMAND",
            Mode::Prompt(PromptMode::Ai) => "PROMPT:AI",
            Mode::Prompt(PromptMode::Database) => "PROMPT:DB",
            Mode::Prompt(PromptMode::Connection) => "PROMPT:CONN",
            Mode::Prompt(PromptMode::Settings) => "PROMPT:SETTINGS",
            Mode::Prompt(PromptMode::Search) => "SEARCH",
            Mode::Help => "HELP",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InsertMode {
    Query,
    Login,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptMode {
    Ai,
    Database,
    Connection,
    Settings,
    Search,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsTab {
    General,
    Database,
    AI,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pane {
    Explorer,
    View,
    Query,
}

impl Pane {
    pub fn label(self) -> &'static str {
        match self {
            Pane::Explorer => "Explorer",
            Pane::View => "View",
            Pane::Query => "Query",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoginPane {
    Recents,
    Favorites,
    Form,
}

pub struct CryoDbTui<'a> {
    pub connection: ConnectionInfo,
    pub pool: Option<DatabasePool>,
    pub tables: Vec<String>,
    pub sidebar_state: ListState,
    pub databases: Vec<String>,
    pub db_picker_state: ListState,
    pub connections: Vec<(String, StoredConnection)>,
    pub connection_state: ListState,
    pub recent_connections: Vec<(String, StoredConnection)>,
    pub favorite_connections: Vec<(String, StoredConnection)>,
    pub recent_state: ListState,
    pub favorite_state: ListState,
    pub login_pane: LoginPane,
    pub results_scroll: usize,
    pub results_column_scroll: usize,
    pub query_limit: usize,
    pub query_editor: TextArea<'a>,
    pub ai_prompt_editor: TextArea<'a>,
    pub command_editor: TextArea<'a>,
    pub login_info: ConnectionInfo,
    pub login_field: usize,
    pub login_host_editor: TextArea<'a>,
    pub login_port_editor: TextArea<'a>,
    pub login_database_editor: TextArea<'a>,
    pub login_username_editor: TextArea<'a>,
    pub login_password_editor: TextArea<'a>,
    pub login_sqlite_path_editor: TextArea<'a>,
    pub search_editor: TextArea<'a>,
    pub settings_tab: SettingsTab,
    pub gen_limit_editor: TextArea<'a>,
    pub db_timeout_editor: TextArea<'a>,
    pub ai_endpoint_editor: TextArea<'a>,
    pub ai_model_editor: TextArea<'a>,
    pub ai_key_editor: TextArea<'a>,
    pub settings_focus_index: usize,

    pub pane: Pane,
    pub results: Option<crate::ResultSet>,
    pub mode: Mode,
    pub window_prefix: bool,
    pub should_quit: bool,
    pub status_message: String,
    pub ai_config: Option<AiRequestConfig>,
}

impl<'a> CryoDbTui<'a> {
    pub fn new(
        connection: Option<ConnectionInfo>,
        recent_connections: Vec<(String, StoredConnection)>,
        favorite_connections: Vec<(String, StoredConnection)>,
        query_limit: usize,
    ) -> Self {
        let query_editor = TextArea::default();
        let mut ai_prompt_editor = TextArea::default();
        ai_prompt_editor.set_placeholder_text("Describe the query you want AI to generate...");

        let mut connection = connection.unwrap_or_else(empty_connection);
        if connection.driver == DatabaseDriver::Sqlite && connection.database.trim().is_empty() {
            connection.database = String::from("main");
        }

        let has_connection =
            !connection.host.trim().is_empty() || !connection.sqlite_path.trim().is_empty();
        let initial_mode = if !has_connection {
            Mode::Prompt(PromptMode::Connection)
        } else if connection.database.trim().is_empty() {
            Mode::Prompt(PromptMode::Database)
        } else {
            Mode::Normal
        };

        let mut sidebar_state = ListState::default();
        sidebar_state.select(Some(0));

        let mut db_picker_state = ListState::default();
        db_picker_state.select(Some(0));

        let mut connections = Vec::new();
        connections.extend(recent_connections.clone());
        connections.extend(favorite_connections.clone());

        let mut connection_state = ListState::default();
        if !connections.is_empty() {
            connection_state.select(Some(0));
        }

        let mut recent_state = ListState::default();
        if !recent_connections.is_empty() {
            recent_state.select(Some(0));
        }

        let mut favorite_state = ListState::default();
        if !favorite_connections.is_empty() {
            favorite_state.select(Some(0));
        }

        let login_pane = if !recent_connections.is_empty() {
            LoginPane::Recents
        } else if !favorite_connections.is_empty() {
            LoginPane::Favorites
        } else {
            LoginPane::Form
        };
        let login_info = recent_connections
            .first()
            .or_else(|| favorite_connections.first())
            .map(|(_, entry)| connection_info_from_entry(entry))
            .unwrap_or_else(empty_connection);
        let login_field = default_login_field(&login_info);
        let login_host_editor = TextArea::from(vec![login_info.host.clone()]);
        let login_port_editor = TextArea::from(vec![login_info.port.clone()]);
        let login_database_editor = TextArea::from(vec![login_info.database.clone()]);
        let login_username_editor = TextArea::from(vec![login_info.username.clone()]);
        let mut login_password_editor = TextArea::from(vec![login_info.password.clone()]);
        login_password_editor.set_mask_char('\u{2022}');
        let login_sqlite_path_editor = TextArea::from(vec![login_info.sqlite_path.clone()]);

        let status_message = if has_connection {
            String::from("Ready")
        } else if connections.is_empty() {
            String::from("No saved connections found")
        } else {
            format!("Select a connection ({} saved)", connections.len())
        };

        Self {
            connection,
            pool: None,
            tables: Vec::new(),
            sidebar_state,
            databases: Vec::new(),
            db_picker_state,
            connections,
            connection_state,
            recent_connections,
            favorite_connections,
            recent_state,
            favorite_state,
            login_pane,
            results_scroll: 0,
            results_column_scroll: 0,
            query_limit: query_limit.max(1),
            query_editor,
            ai_prompt_editor,
            command_editor: TextArea::default(),
            login_info,
            login_field,
            login_host_editor,
            login_port_editor,
            login_database_editor,
            login_username_editor,
            login_password_editor,
            login_sqlite_path_editor,
            search_editor: TextArea::default(),
            settings_tab: SettingsTab::General,
            gen_limit_editor: TextArea::default(),
            db_timeout_editor: TextArea::default(),
            ai_endpoint_editor: TextArea::default(),
            ai_model_editor: TextArea::default(),
            ai_key_editor: TextArea::default(),
            settings_focus_index: 0,
            pane: Pane::Explorer,
            results: None,
            mode: initial_mode,
            window_prefix: false,
            should_quit: false,
            status_message,
            ai_config: None,
        }
    }

    pub fn setup_settings_editors(&mut self, settings: &crate::Settings) {
        self.gen_limit_editor = TextArea::from(vec![settings.table_query_limit.to_string()]);
        self.db_timeout_editor = TextArea::from(vec![settings.query_timeout_secs.to_string()]);
        if let Some(config) = &self.ai_config {
            self.ai_endpoint_editor = TextArea::from(vec![config.endpoint.clone()]);
            self.ai_model_editor = TextArea::from(vec![config.model.clone()]);
            self.ai_key_editor = TextArea::from(vec![config.api_key.clone()]);
        }
    }

    pub async fn refresh_tables(&mut self) {
        if let Some(pool) = &self.pool {
            let db = self.connection.database.clone();
            match fetch_tables(pool.clone(), db).await {
                Ok(tables) => {
                    self.tables = tables;
                    if !self.tables.is_empty() {
                        self.sidebar_state.select(Some(0));
                    } else {
                        self.sidebar_state.select(None);
                    }
                    self.status_message = format!("Loaded {} tables", self.tables.len());
                }
                Err(e) => {
                    self.status_message = format!("Error loading tables: {}", e);
                }
            }
        }
    }

    pub async fn refresh_databases(&mut self) {
        if let Some(pool) = &self.pool {
            match fetch_databases(pool.clone()).await {
                Ok(dbs) => {
                    self.databases = dbs;
                    if !self.databases.is_empty() {
                        self.db_picker_state.select(Some(0));
                    }
                    self.status_message = format!("Loaded {} databases", self.databases.len());
                }
                Err(e) => {
                    self.status_message = format!("Error loading databases: {}", e);
                }
            }
        }
    }

    pub fn next_db(&mut self) {
        if !self.databases.is_empty() {
            let i = match self.db_picker_state.selected() {
                Some(i) => (i + 1) % self.databases.len(),
                None => 0,
            };
            self.db_picker_state.select(Some(i));
        }
    }

    pub fn previous_db(&mut self) {
        if !self.databases.is_empty() {
            let i = match self.db_picker_state.selected() {
                Some(i) => {
                    if i > 0 {
                        i - 1
                    } else {
                        self.databases.len() - 1
                    }
                }
                None => 0,
            };
            self.db_picker_state.select(Some(i));
        }
    }

    pub fn next_connection(&mut self) {
        match self.login_pane {
            LoginPane::Recents => {
                let len = self.filtered_recents().len();
                move_list_next(&mut self.recent_state, len);
                self.preview_login_selection();
            }
            LoginPane::Favorites => {
                let len = self.filtered_favorites().len();
                move_list_next(&mut self.favorite_state, len);
                self.preview_login_selection();
            }
            LoginPane::Form => self.next_login_field(),
        }
    }

    pub fn previous_connection(&mut self) {
        match self.login_pane {
            LoginPane::Recents => {
                let len = self.filtered_recents().len();
                move_list_previous(&mut self.recent_state, len);
                self.preview_login_selection();
            }
            LoginPane::Favorites => {
                let len = self.filtered_favorites().len();
                move_list_previous(&mut self.favorite_state, len);
                self.preview_login_selection();
            }
            LoginPane::Form => self.previous_login_field(),
        }
    }

    pub fn focus_login_form(&mut self) {
        self.login_pane = LoginPane::Form;
    }

    pub fn focus_login_sidebar(&mut self) {
        self.login_pane = if !self.recent_connections.is_empty() {
            LoginPane::Recents
        } else if !self.favorite_connections.is_empty() {
            LoginPane::Favorites
        } else {
            LoginPane::Form
        };
    }

    pub fn fill_login_form_from_selection(&mut self) {
        if self.preview_login_selection() {
            self.status_message = String::from("Connection form filled");
        }
        self.login_pane = LoginPane::Form;
    }

    pub fn preview_login_selection(&mut self) -> bool {
        if let Some((_, entry)) = self.selected_login_item() {
            self.login_info = connection_info_from_entry(&entry);
            self.login_field = default_login_field(&self.login_info);
            self.sync_login_editors_from_info();
            true
        } else {
            false
        }
    }

    pub async fn connect_login_form(&mut self) {
        self.sync_login_info_from_editors();
        let info = self.login_info.clone();
        if info.driver == DatabaseDriver::Sqlite && info.sqlite_path.trim().is_empty() {
            self.status_message = String::from("SQLite path is required");
            return;
        }
        if info.driver != DatabaseDriver::Sqlite
            && (info.host.trim().is_empty() || info.username.trim().is_empty())
        {
            self.status_message = String::from("Host and username are required");
            return;
        }
        self.connect_connection(login_label(&info), info).await;
    }

    fn selected_login_item(&self) -> Option<(String, StoredConnection)> {
        match self.login_pane {
            LoginPane::Recents => self
                .recent_state
                .selected()
                .and_then(|i| self.filtered_recents().get(i).cloned()),
            LoginPane::Favorites => self
                .favorite_state
                .selected()
                .and_then(|i| self.filtered_favorites().get(i).cloned()),
            LoginPane::Form => self
                .recent_state
                .selected()
                .and_then(|i| self.filtered_recents().get(i).cloned())
                .or_else(|| {
                    self.favorite_state
                        .selected()
                        .and_then(|i| self.filtered_favorites().get(i).cloned())
                }),
        }
    }

    pub fn next_login_field(&mut self) {
        if self.login_pane == LoginPane::Form {
            self.login_field = (self.login_field + 1) % self.login_field_count();
        }
    }

    pub fn previous_login_field(&mut self) {
        if self.login_pane == LoginPane::Form {
            let count = self.login_field_count();
            self.login_field = if self.login_field == 0 {
                count - 1
            } else {
                self.login_field - 1
            };
        }
    }

    pub fn next_login_driver(&mut self) {
        self.set_login_driver(1);
    }

    pub fn previous_login_driver(&mut self) {
        self.set_login_driver(-1);
    }

    pub fn input_login_key(&mut self, key: KeyEvent) {
        if let Some(editor) = self.login_field_editor_mut() {
            editor.input(key);
            self.sync_login_info_from_editors();
        }
    }

    pub fn login_field_count(&self) -> usize {
        if self.login_info.driver == DatabaseDriver::Sqlite {
            2
        } else {
            6
        }
    }

    fn set_login_driver(&mut self, delta: isize) {
        let drivers = DatabaseDriver::ALL;
        let previous_driver = self.login_info.driver;
        let current = drivers
            .iter()
            .position(|driver| *driver == previous_driver)
            .unwrap_or(0) as isize;
        let next = (current + delta).rem_euclid(drivers.len() as isize) as usize;
        self.login_info.driver = drivers[next];
        if self.login_info.port.trim().is_empty()
            || self.login_info.port.trim() == previous_driver.default_port()
        {
            self.login_info.port = self.login_info.driver.default_port().to_string();
        }
        if self.login_info.driver == DatabaseDriver::Sqlite
            && self.login_info.database.trim().is_empty()
        {
            self.login_info.database = String::from("main");
        }
        self.login_field = default_login_field(&self.login_info);
        self.sync_login_editors_from_info();
    }

    fn login_field_editor_mut(&mut self) -> Option<&mut TextArea<'a>> {
        match (self.login_info.driver, self.login_field) {
            (DatabaseDriver::Sqlite, 1) => Some(&mut self.login_sqlite_path_editor),
            (DatabaseDriver::Sqlite, _) => None,
            (_, 1) => Some(&mut self.login_host_editor),
            (_, 2) => Some(&mut self.login_port_editor),
            (_, 3) => Some(&mut self.login_database_editor),
            (_, 4) => Some(&mut self.login_username_editor),
            (_, 5) => Some(&mut self.login_password_editor),
            _ => None,
        }
    }

    fn sync_login_editors_from_info(&mut self) {
        self.login_host_editor = TextArea::from(vec![self.login_info.host.clone()]);
        self.login_port_editor = TextArea::from(vec![self.login_info.port.clone()]);
        self.login_database_editor = TextArea::from(vec![self.login_info.database.clone()]);
        self.login_username_editor = TextArea::from(vec![self.login_info.username.clone()]);
        let mut pw = TextArea::from(vec![self.login_info.password.clone()]);
        pw.set_mask_char('\u{2022}');
        self.login_password_editor = pw;
        self.login_sqlite_path_editor = TextArea::from(vec![self.login_info.sqlite_path.clone()]);
    }

    fn sync_login_info_from_editors(&mut self) {
        self.login_info.host = login_editor_text(&self.login_host_editor);
        self.login_info.port = login_editor_text(&self.login_port_editor);
        self.login_info.database = login_editor_text(&self.login_database_editor);
        self.login_info.username = login_editor_text(&self.login_username_editor);
        self.login_info.password = login_editor_text(&self.login_password_editor);
        self.login_info.sqlite_path = login_editor_text(&self.login_sqlite_path_editor);
    }

    async fn connect_connection(&mut self, label: String, mut info: ConnectionInfo) -> bool {
        self.status_message = format!("Connecting to {}...", label);
        let (settings, _, _) = crate::storage::load_settings_store();
        crate::constants::ACTIVE_CONNECTION_TIMEOUT_SECS.store(
            settings.connection_timeout_secs,
            std::sync::atomic::Ordering::Relaxed,
        );
        let result = match info.driver {
            DatabaseDriver::MySql | DatabaseDriver::MariaDb => {
                connect_mysql(info.clone()).await.map(DatabasePool::MySql)
            }
            DatabaseDriver::PostgreSql => connect_postgres(info.clone())
                .await
                .map(DatabasePool::Postgres),
            DatabaseDriver::Sqlite => {
                let path = info.sqlite_path.clone();
                match connect_sqlite_profile(path).await {
                    Ok((pool, normalized)) => {
                        info.sqlite_path = normalized;
                        info.database = String::from("main");
                        Ok(DatabasePool::Sqlite(pool))
                    }
                    Err(error) => Err(error),
                }
            }
        };

        match result {
            Ok(pool) => {
                self.connection = info;
                self.pool = Some(pool);
                self.tables.clear();
                self.databases.clear();
                self.results = None;
                self.results_scroll = 0;
                self.results_column_scroll = 0;
                self.mode = if self.connection.database.trim().is_empty() {
                    Mode::Prompt(PromptMode::Database)
                } else {
                    Mode::Normal
                };
                if let Mode::Prompt(PromptMode::Database) = self.mode {
                    self.refresh_databases().await;
                } else {
                    self.refresh_tables().await;
                }
                self.status_message = format!("Connected: {}", label);
                true
            }
            Err(error) => {
                self.status_message = format!("Connection error: {}", error);
                false
            }
        }
    }

    pub async fn use_selected_db(&mut self) {
        if let Some(i) = self.db_picker_state.selected()
            && let Some(db) = self.databases.get(i).cloned()
        {
            self.connection.database = db.clone();
            self.status_message = format!("Switched to database: {}", db);
            self.mode = Mode::Normal;
            self.refresh_tables().await;
        }
    }

    pub fn next_table(&mut self) {
        let tables = self.filtered_tables();
        if !tables.is_empty() {
            let i = match self.sidebar_state.selected() {
                Some(i) => (i + 1) % tables.len(),
                None => 0,
            };
            self.sidebar_state.select(Some(i));
        }
    }

    pub fn previous_table(&mut self) {
        let tables = self.filtered_tables();
        if !tables.is_empty() {
            let i = match self.sidebar_state.selected() {
                Some(i) => {
                    if i > 0 {
                        i - 1
                    } else {
                        tables.len() - 1
                    }
                }
                None => 0,
            };
            self.sidebar_state.select(Some(i));
        }
    }

    pub async fn load_selected_table_data(&mut self) {
        if let Some(i) = self.sidebar_state.selected()
            && let Some(table) = self.filtered_tables().get(i).cloned()
        {
            self.clear_search();
            let safe_table = sql_quote_table_reference(self.connection.driver, &table);
            let sql = format!("SELECT * FROM {} LIMIT {}", safe_table, self.query_limit);
            self.query_editor = TextArea::from(vec![sql]);
            self.run_active_query().await;
            self.status_message = format!("Showing data for table: {}", table);
            self.pane = Pane::View;
        }
    }

    fn selected_table(&self) -> Option<String> {
        let i = self.sidebar_state.selected()?;
        self.filtered_tables().get(i).cloned()
    }

    pub fn scroll_results_down(&mut self) {
        if let Some(rs) = &self.results
            && self.results_scroll < rs.rows.len().saturating_sub(1)
        {
            self.results_scroll += 1;
        }
    }

    pub fn scroll_results_up(&mut self) {
        if self.results_scroll > 0 {
            self.results_scroll -= 1;
        }
    }

    pub fn scroll_results_right(&mut self) {
        if let Some(rs) = &self.results
            && self.results_column_scroll < rs.columns.len().saturating_sub(1)
        {
            self.results_column_scroll += 1;
        }
    }

    pub fn scroll_results_left(&mut self) {
        if self.results_column_scroll > 0 {
            self.results_column_scroll -= 1;
        }
    }

    pub async fn run_active_query(&mut self) {
        if let Some(pool) = &self.pool {
            let db = if self.connection.database.trim().is_empty() {
                None
            } else {
                Some(self.connection.database.clone())
            };
            let sql = self.query_editor.lines().join("\n");
            if sql.trim().is_empty() {
                self.status_message = String::from("Query is empty");
                return;
            }
            self.status_message = String::from("Running query...");
            let (settings, _, _) = crate::storage::load_settings_store();
            let timeout_secs = settings.query_timeout_secs;
            let query_future =
                run_query_with_control(pool.clone(), db, sql, None, Some(self.query_limit));
            let result = if timeout_secs > 0 {
                match tokio::time::timeout(
                    std::time::Duration::from_secs(timeout_secs),
                    query_future,
                )
                .await
                {
                    Ok(result) => result,
                    Err(_) => Err(format!("Query timed out after {} seconds.", timeout_secs)),
                }
            } else {
                query_future.await
            };
            match result {
                Ok(output) => match output {
                    crate::QueryOutput::Rows(rs) => {
                        let limited = if rs.rows.len() >= self.query_limit {
                            format!(" (limited to {})", self.query_limit)
                        } else {
                            String::new()
                        };
                        self.status_message =
                            format!("Query successful: {} rows{}", rs.rows.len(), limited);
                        self.results = Some(rs);
                        self.results_scroll = 0;
                        self.results_column_scroll = 0;
                        self.pane = Pane::View;
                    }
                    crate::QueryOutput::Affected(count) => {
                        self.status_message = format!("Query successful: {} rows affected", count);
                        self.results = None;
                    }
                },
                Err(e) => {
                    self.status_message = format!("Query error: {}", e);
                }
            }
        }
    }

    pub async fn generate_with_ai(&mut self) {
        if let Some(config) = &self.ai_config {
            let prompt = self.ai_prompt_editor.lines().join("\n");
            if prompt.trim().is_empty() {
                self.status_message = String::from("AI prompt is empty");
                return;
            }
            self.status_message = String::from("AI is generating SQL...");
            let prompt = self.ai_prompt_with_context(prompt).await;
            match generate_ai_sql(config.clone(), prompt, self.connection.driver).await {
                Ok(sql) => {
                    self.query_editor =
                        TextArea::from(sql.lines().map(|s| s.to_string()).collect::<Vec<_>>());
                    self.status_message = String::from("AI generated SQL successfully");
                    self.mode = Mode::Normal;
                    self.pane = Pane::Query;
                }
                Err(e) => {
                    self.status_message = format!("AI error: {}", e);
                }
            }
        } else {
            self.status_message = String::from("AI not configured. Use :settings to configure.");
        }
    }

    async fn ai_prompt_with_context(&self, prompt: String) -> String {
        let mut context = Vec::new();
        if !self.connection.database.trim().is_empty() {
            context.push(format!("Database: {}", self.connection.database.trim()));
        }
        if let Some(table) = self.selected_table() {
            context.push(format!("Current table: {}", table));
            let db = if self.connection.database.trim().is_empty() {
                String::from("main")
            } else {
                self.connection.database.clone()
            };
            if let Some(pool) = self.pool.clone()
                && let Ok(columns) = fetch_query_suggestion_columns(pool, db, table).await
                && !columns.is_empty()
            {
                context.push(format!("Table columns: {}", columns.join(", ")));
            }
        }
        if let Some(rs) = &self.results {
            if !rs.columns.is_empty() {
                context.push(format!("Result columns: {}", rs.columns.join(", ")));
            }
            let rows = rs
                .rows
                .iter()
                .take(5)
                .map(|row| {
                    row.iter()
                        .take(12)
                        .map(|cell| compact_ai_cell(cell))
                        .collect::<Vec<_>>()
                        .join(" | ")
                })
                .collect::<Vec<_>>();
            if !rows.is_empty() {
                context.push(format!("Sample rows:\n{}", rows.join("\n")));
            }
        }
        if context.is_empty() {
            prompt
        } else {
            format!(
                "Use this current CryoDB context. Do not assume data not shown.\n{}\n\nRequest:\n{}",
                context.join("\n"),
                prompt
            )
        }
    }

    pub fn start_search(&mut self) {
        self.search_editor = TextArea::default();
        self.mode = Mode::Prompt(PromptMode::Search);
    }

    pub fn clear_search(&mut self) {
        self.search_editor = TextArea::default();
    }

    pub fn search_query(&self) -> String {
        self.search_editor.lines().join("")
    }

    pub fn filtered_tables(&self) -> Vec<String> {
        if self.pane != Pane::Explorer {
            return self.tables.clone();
        }
        let q = self.search_query();
        if q.trim().is_empty() {
            self.tables.clone()
        } else {
            let lower = q.to_lowercase();
            self.tables
                .iter()
                .filter(|t| t.to_lowercase().contains(&lower))
                .cloned()
                .collect()
        }
    }

    pub fn filtered_recents(&self) -> Vec<(String, StoredConnection)> {
        if self.login_pane != LoginPane::Recents {
            return self.recent_connections.clone();
        }
        let q = self.search_query();
        if q.trim().is_empty() {
            self.recent_connections.clone()
        } else {
            let lower = q.to_lowercase();
            self.recent_connections
                .iter()
                .filter(|(l, _)| l.to_lowercase().contains(&lower))
                .cloned()
                .collect()
        }
    }

    pub fn filtered_favorites(&self) -> Vec<(String, StoredConnection)> {
        if self.login_pane != LoginPane::Favorites {
            return self.favorite_connections.clone();
        }
        let q = self.search_query();
        if q.trim().is_empty() {
            self.favorite_connections.clone()
        } else {
            let lower = q.to_lowercase();
            self.favorite_connections
                .iter()
                .filter(|(l, _)| l.to_lowercase().contains(&lower))
                .cloned()
                .collect()
        }
    }

    pub fn filtered_results(&self) -> Option<crate::ResultSet> {
        if self.pane != Pane::View {
            return self.results.clone();
        }
        let rs = self.results.as_ref()?;
        let q = self.search_query();
        if q.trim().is_empty() {
            return self.results.clone();
        }
        let filters = parse_result_filters(&q, &rs.columns);
        if filters.is_empty() {
            let lower = q.to_lowercase();
            let rows: Vec<Vec<String>> = rs
                .rows
                .iter()
                .filter(|row| row.iter().any(|c| c.to_lowercase().contains(&lower)))
                .cloned()
                .collect();
            Some(crate::ResultSet {
                columns: rs.columns.clone(),
                column_kinds: rs.column_kinds.clone(),
                column_nullable: rs.column_nullable.clone(),
                rows,
            })
        } else {
            let rows: Vec<Vec<String>> = rs
                .rows
                .iter()
                .filter(|row| filters.iter().all(|f| f.matches(row)))
                .cloned()
                .collect();
            Some(crate::ResultSet {
                columns: rs.columns.clone(),
                column_kinds: rs.column_kinds.clone(),
                column_nullable: rs.column_nullable.clone(),
                rows,
            })
        }
    }

    pub fn save_settings(&mut self) {
        let endpoint = self.ai_endpoint_editor.lines().join("").trim().to_string();
        let model = self.ai_model_editor.lines().join("").trim().to_string();
        let api_key = self.ai_key_editor.lines().join("").trim().to_string();
        let config = AiRequestConfig {
            provider: crate::ai::AiProvider::OpenAI,
            endpoint: endpoint.clone(),
            model: model.clone(),
            api_key: api_key.clone(),
            temperature: None,
            cli_command: String::new(),
        };
        self.ai_config = Some(config);

        let limit_str = self.gen_limit_editor.lines().join("");
        let timeout_str = self.db_timeout_editor.lines().join("");

        let mut warnings = Vec::new();

        if let Ok(limit) = limit_str.trim().parse::<usize>() {
            self.query_limit = limit.max(1);
        } else {
            warnings.push(String::from("invalid table limit format"));
        }

        let query_timeout = timeout_str.trim().parse::<u64>().unwrap_or(30).max(1);

        let (settings, _, theme) = crate::storage::load_settings_store();
        let mut store = crate::model::settings::SettingsStore::from_settings(
            &settings,
            theme.unwrap_or(crate::ThemeChoice::CarbonFrost),
        );
        store.table_query_limit = Some(self.query_limit);
        store.query_timeout_secs = Some(query_timeout);
        store.ai_endpoint = Some(endpoint);
        store.ai_model = Some(model);
        if api_key.is_empty() {
            store.ai_api_key = None;
            if let Err(error) = crate::storage::store_ai_api_key_secret("") {
                warnings.push(error);
            }
        } else {
            store.ai_api_key = Some(api_key.clone());
            if let Err(error) = crate::storage::store_ai_api_key_secret(&api_key) {
                warnings.push(error);
            }
        }
        if let Err(error) = crate::storage::save_settings_store(&store) {
            warnings.push(error);
        }

        if warnings.is_empty() {
            self.status_message = format!(
                "Settings saved. Table limit: {}, Query timeout: {}s",
                self.query_limit, query_timeout
            );
        } else {
            self.status_message = format!("Settings saved with warnings: {}", warnings.join("; "));
        }

        self.mode = Mode::Normal;
    }

    pub fn focus_pane(&mut self, pane: Pane) {
        self.pane = pane;
        self.window_prefix = false;
        self.status_message = format!("Pane: {}", pane.label());
    }

    pub fn move_pane_left(&mut self) {
        if matches!(self.pane, Pane::View | Pane::Query) {
            self.focus_pane(Pane::Explorer);
        }
    }

    pub fn move_pane_right(&mut self) {
        if self.pane == Pane::Explorer {
            self.focus_pane(Pane::View);
        }
    }

    pub fn move_pane_down(&mut self) {
        match self.pane {
            Pane::Explorer | Pane::View => self.focus_pane(Pane::Query),
            Pane::Query => {}
        }
    }

    pub fn move_pane_up(&mut self) {
        match self.pane {
            Pane::Explorer | Pane::Query => self.focus_pane(Pane::View),
            Pane::View => {}
        }
    }

    pub fn move_query_cursor(&mut self, code: KeyCode) {
        self.query_editor
            .input(KeyEvent::new(code, KeyModifiers::NONE));
    }

    pub fn start_command(&mut self) {
        self.command_editor = TextArea::default();
        self.mode = Mode::Command;
        self.window_prefix = false;
    }

    pub fn command_text(&self) -> String {
        self.command_editor.lines().join("").trim().to_string()
    }

    pub fn quit(&mut self) {
        self.should_quit = true;
    }
}

fn move_list_next(state: &mut ListState, len: usize) {
    if len == 0 {
        return;
    }
    let i = state.selected().unwrap_or(0);
    if i + 1 < len {
        state.select(Some(i + 1));
    }
}

fn move_list_previous(state: &mut ListState, len: usize) {
    if len == 0 {
        return;
    }
    let i = state.selected().unwrap_or(0);
    if i > 0 {
        state.select(Some(i - 1));
    }
}

fn login_label(info: &ConnectionInfo) -> String {
    if info.driver == DatabaseDriver::Sqlite {
        if info.sqlite_path.trim().is_empty() {
            String::from("New SQLite connection")
        } else {
            info.sqlite_path.clone()
        }
    } else if info.host.trim().is_empty() {
        format!("New {} connection", info.driver)
    } else if info.database.trim().is_empty() {
        format!("{}@{}", info.username, info.host)
    } else {
        format!("{}@{}", info.database, info.host)
    }
}

fn default_login_field(info: &ConnectionInfo) -> usize {
    if info.driver == DatabaseDriver::Sqlite {
        if info.sqlite_path.trim().is_empty() {
            1
        } else {
            0
        }
    } else if info.host.trim().is_empty() {
        1
    } else if info.username.trim().is_empty() {
        4
    } else if info.password.trim().is_empty() {
        5
    } else {
        0
    }
}

fn empty_connection() -> ConnectionInfo {
    ConnectionInfo {
        driver: DatabaseDriver::MySql,
        host: String::new(),
        port: String::from("3306"),
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

fn connection_info_from_entry(entry: &StoredConnection) -> ConnectionInfo {
    ConnectionInfo {
        driver: entry.driver,
        host: entry.host.clone(),
        port: entry.port.clone(),
        database: entry.database.clone(),
        username: entry.username.clone(),
        password: load_connection_secret(entry)
            .or_else(|| std::env::var("CRYODB_PASSWORD").ok())
            .unwrap_or_default(),
        sqlite_path: entry.sqlite_path.clone(),
        tls_mode: entry.tls_mode,
        tls_ca_cert_path: entry.tls_ca_cert_path.clone(),
        tls_client_cert_path: entry.tls_client_cert_path.clone(),
        tls_client_key_path: entry.tls_client_key_path.clone(),
    }
}

fn login_editor_text(editor: &TextArea<'_>) -> String {
    editor.lines().join("")
}

fn compact_ai_cell(value: &str) -> String {
    let value = value.replace(['\n', '\r'], " ");
    if value.chars().count() > 80 {
        format!("{}...", value.chars().take(80).collect::<String>())
    } else {
        value
    }
}

struct ColumnFilter {
    col_index: usize,
    values: Vec<String>,
    is_in: bool,
}

impl ColumnFilter {
    fn matches(&self, row: &[String]) -> bool {
        if self.col_index >= row.len() {
            return false;
        }
        if self.is_in {
            self.values.iter().any(|v| row[self.col_index] == *v)
        } else {
            self.values
                .first()
                .is_some_and(|v| row[self.col_index] == *v)
        }
    }
}

fn parse_result_filters(query: &str, columns: &[String]) -> Vec<ColumnFilter> {
    if !query.contains(':') {
        return vec![];
    }
    let mut filters = Vec::new();
    for part in query.split('|') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        if let Some((col_name, raw_val)) = part.split_once(':') {
            let col_name = col_name.trim().to_lowercase();
            let col_index = columns
                .iter()
                .position(|c| c.to_lowercase() == col_name)
                .or_else(|| {
                    columns
                        .iter()
                        .position(|c| c.to_lowercase().starts_with(&col_name))
                });
            if let Some(idx) = col_index {
                let raw_val = raw_val.trim();
                if raw_val.starts_with('[') && raw_val.ends_with(']') {
                    let inner = &raw_val[1..raw_val.len() - 1];
                    let values: Vec<String> = inner
                        .split(',')
                        .map(|v| v.trim().trim_matches('"').trim_matches('\'').to_string())
                        .collect();
                    filters.push(ColumnFilter {
                        col_index: idx,
                        values,
                        is_in: true,
                    });
                } else {
                    let v = raw_val.trim_matches('"').trim_matches('\'').to_string();
                    filters.push(ColumnFilter {
                        col_index: idx,
                        values: vec![v],
                        is_in: false,
                    });
                }
            }
        }
    }
    filters
}
