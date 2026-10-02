use crate::app::core::App;
use crate::app::features::connections::Message as ConnectionMessage;
use crate::app::features::workspace::explorer;
use crate::app::message::Message;
use crate::app::types::{
    AiModalTarget, FavoriteContextMenuState, FavoriteEditModalState, ToastLevel,
};
use crate::app::update::{favorite_tags_input_within_limit, normalize_favorite_tags};
use crate::db::DatabasePool;
use crate::db::connect::{
    connect_mysql, connect_postgres, connect_sqlite_profile, create_sqlite_database_file,
};
use crate::db::metadata::{
    fetch_databases, fetch_postgres_sidebar_objects, fetch_sidebar_relations,
    fetch_sidebar_triggers, fetch_tables,
};
use crate::model::connection::{DatabaseDriver, StoredConnection};
use crate::model::transfer::TransferStage;
use crate::ui::ids::connection_tabs_scroll_id;
use iced::widget::scrollable;
use iced::{Task, keyboard, mouse};
use rfd::AsyncFileDialog;
use std::collections::HashSet;

impl App {
    pub(crate) fn update_connections(&mut self, message: ConnectionMessage) -> Task<Message> {
        match message {
            ConnectionMessage::DriverSelected(driver) => {
                if self.connections.connecting {
                    return Task::none();
                }
                self.connections.driver_picker_open = false;
                if self.connections.current.driver == driver {
                    return Task::none();
                }
                let previous_driver = self.connections.current.driver;
                self.connections.current.driver = driver;
                if self.connections.current.port.trim().is_empty()
                    || self.connections.current.port.trim() == previous_driver.default_port()
                {
                    self.connections.current.port = driver.default_port().to_string();
                }
                if driver == DatabaseDriver::Sqlite
                    && self.connections.current.database.trim().is_empty()
                {
                    self.connections.current.database = String::from("main");
                }
                let current_query = self.workspace.query.editor.content();
                if current_query == previous_driver.default_query() {
                    self.set_query_text(driver.default_query());
                }
                self.connections.connect_error = None;
                Task::none()
            }
            ConnectionMessage::ToggleDriverPicker => {
                if self.connections.connecting {
                    return Task::none();
                }
                self.connections.driver_picker_open = !self.connections.driver_picker_open;
                Task::none()
            }
            ConnectionMessage::HostChanged(host) => {
                if self.connections.connecting {
                    return Task::none();
                }
                self.connections.current.host = host;
                self.connections.connect_error = None;
                Task::none()
            }
            ConnectionMessage::PortChanged(port) => {
                if self.connections.connecting {
                    return Task::none();
                }
                self.connections.current.port = port;
                self.connections.connect_error = None;
                Task::none()
            }
            ConnectionMessage::DatabaseChanged(database) => {
                if self.connections.connecting {
                    return Task::none();
                }
                self.connections.current.database = database;
                self.connections.connect_error = None;
                Task::none()
            }
            ConnectionMessage::UsernameChanged(username) => {
                if self.connections.connecting {
                    return Task::none();
                }
                self.connections.current.username = username;
                self.connections.connect_error = None;
                Task::none()
            }
            ConnectionMessage::PasswordChanged(password) => {
                if self.connections.connecting {
                    return Task::none();
                }
                self.connections.current.password = password;
                self.connections.connect_error = None;
                Task::none()
            }
            ConnectionMessage::TlsModeSelected(mode) => {
                if self.connections.connecting {
                    return Task::none();
                }
                self.connections.current.tls_mode = mode;
                self.connections.connect_error = None;
                Task::none()
            }
            ConnectionMessage::TlsCaCertPathChanged(path) => {
                if self.connections.connecting {
                    return Task::none();
                }
                self.connections.current.tls_ca_cert_path = path;
                self.connections.connect_error = None;
                Task::none()
            }
            ConnectionMessage::TlsCaCertBrowsePath => {
                let task = AsyncFileDialog::new()
                    .add_filter("Certificate", &["pem", "crt", "cer", "der"])
                    .pick_file();
                Task::perform(task, |file| {
                    Message::Connections(
                        crate::app::features::connections::Message::TlsCaCertPicked(
                            file.map(|handle| handle.path().to_path_buf()),
                        ),
                    )
                })
            }
            ConnectionMessage::TlsCaCertPicked(file) => {
                if let Some(file) = file {
                    self.connections.current.tls_ca_cert_path = file.display().to_string();
                    self.connections.connect_error = None;
                }
                Task::none()
            }
            ConnectionMessage::SqlitePathChanged(path) => {
                if self.connections.connecting {
                    return Task::none();
                }
                self.connections.current.sqlite_path = path;
                self.connections.connect_error = None;
                Task::none()
            }
            ConnectionMessage::SqliteBrowsePath => {
                let task = AsyncFileDialog::new()
                    .add_filter("SQLite", &["db", "sqlite", "sqlite3"])
                    .pick_file();
                Task::perform(task, |file| {
                    Message::Connections(
                        crate::app::features::connections::Message::SqliteFilePicked(
                            file.map(|handle| handle.path().to_path_buf()),
                        ),
                    )
                })
            }
            ConnectionMessage::SqliteCreatePath => {
                let task = AsyncFileDialog::new()
                    .add_filter("SQLite", &["db", "sqlite", "sqlite3"])
                    .save_file();
                Task::perform(task, |file| {
                    Message::Connections(
                        crate::app::features::connections::Message::SqliteCreatePicked(
                            file.map(|handle| handle.path().to_path_buf()),
                        ),
                    )
                })
            }
            ConnectionMessage::SqliteFilePicked(file) => {
                if let Some(file) = file {
                    self.connections.current.sqlite_path = file.display().to_string();
                    self.connections.connect_error = None;
                }
                Task::none()
            }
            ConnectionMessage::SqliteCreatePicked(file) => {
                if let Some(file) = file {
                    return Task::perform(create_sqlite_database_file(file), |value| {
                        Message::Connections(
                            crate::app::features::connections::Message::SqliteDatabaseCreated(
                                value,
                            ),
                        )
                    });
                }
                Task::none()
            }
            ConnectionMessage::SqliteDatabaseCreated(result) => {
                match result {
                    Ok(path) => {
                        self.connections.current.sqlite_path = path;
                        self.connections.connect_error = None;
                    }
                    Err(error) => self.connections.connect_error = Some(error),
                }
                Task::none()
            }
            ConnectionMessage::AddFavorite => {
                if self.connections.connecting {
                    return Task::none();
                }
                self.add_favorite_from_current();
                self.maybe_refresh_omni_bar_results();
                Task::none()
            }
            ConnectionMessage::FavoriteSelected(index) => {
                if self.connections.connecting {
                    return Task::none();
                }
                if let Some(entry) = self.connections.favorites.get(index).cloned() {
                    if self.settings.values.multiple_connections_layout {
                        self.connections.connection_picker_open = false;
                        if let Some(tab_id) = self
                            .connections
                            .tabs
                            .iter()
                            .find(|tab| {
                                let tab_entry = StoredConnection::from_info(&tab.connection);
                                entry.matches_identity(&tab_entry)
                            })
                            .map(|tab| tab.id)
                        {
                            return self.switch_connection_tab(tab_id);
                        }
                        self.save_active_connection_tab_snapshot();
                        self.connections.active_tab_id = None;
                    }
                    self.apply_saved_connection(&entry);
                    return Task::perform(async {}, |_| {
                        Message::Connections(crate::app::features::connections::Message::Connect)
                    });
                }
                Task::none()
            }
            ConnectionMessage::FavoriteDeleted(index) => {
                if index < self.connections.favorites.len() {
                    let removed = self.connections.favorites.remove(index);
                    let _ = crate::storage::store_connection_secret(&removed, "");
                    self.persist_connection_store();
                }
                self.maybe_refresh_omni_bar_results();
                Task::none()
            }
            ConnectionMessage::FavoriteContextMenuRequested { index } => {
                self.connections.favorite_context_menu = Some(FavoriteContextMenuState { index });
                Task::none()
            }
            ConnectionMessage::CloseFavoriteContextMenu => {
                self.connections.favorite_context_menu = None;
                Task::none()
            }
            ConnectionMessage::OpenFavoriteEditModal(index) => {
                if let Some(entry) = self.connections.favorites.get(index) {
                    self.connections.favorite_edit_modal = Some(FavoriteEditModalState {
                        index,
                        name_input: entry.name.clone(),
                        tags_input: entry.tags.join(", "),
                    });
                }
                self.connections.favorite_context_menu = None;
                Task::none()
            }
            ConnectionMessage::CloseFavoriteEditModal => {
                self.connections.favorite_edit_modal = None;
                Task::none()
            }
            ConnectionMessage::FavoriteEditNameChanged(value) => {
                if let Some(state) = &mut self.connections.favorite_edit_modal {
                    state.name_input = value;
                }
                Task::none()
            }
            ConnectionMessage::FavoriteEditTagsChanged(value) => {
                if let Some(state) = &mut self.connections.favorite_edit_modal
                    && favorite_tags_input_within_limit(&value)
                {
                    state.tags_input = value;
                }
                Task::none()
            }
            ConnectionMessage::SaveFavoriteEdit => {
                if let Some(state) = self.connections.favorite_edit_modal.take()
                    && state.index < self.connections.favorites.len()
                {
                    let tags = normalize_favorite_tags(&state.tags_input);
                    self.connections.favorites[state.index].name =
                        state.name_input.trim().to_string();
                    self.connections.favorites[state.index].tags = tags;
                    self.persist_connection_store();
                    self.refresh_connection_tab_labels();
                    self.maybe_refresh_omni_bar_results();
                }
                Task::none()
            }
            ConnectionMessage::RecentSelected(index) => {
                if self.connections.connecting {
                    return Task::none();
                }
                if let Some(entry) = self.connections.recents.get(index).cloned() {
                    if self.settings.values.multiple_connections_layout {
                        self.connections.connection_picker_open = false;
                        if let Some(tab_id) = self
                            .connections
                            .tabs
                            .iter()
                            .find(|tab| {
                                let tab_entry = StoredConnection::from_info(&tab.connection);
                                entry.matches_identity(&tab_entry)
                            })
                            .map(|tab| tab.id)
                        {
                            return self.switch_connection_tab(tab_id);
                        }
                        self.save_active_connection_tab_snapshot();
                        self.connections.active_tab_id = None;
                    }
                    self.apply_saved_connection(&entry);
                    return Task::perform(async {}, |_| {
                        Message::Connections(crate::app::features::connections::Message::Connect)
                    });
                }
                Task::none()
            }
            ConnectionMessage::RecentDeleted(index) => {
                if index < self.connections.recents.len() {
                    let removed = self.connections.recents.remove(index);
                    let _ = crate::storage::store_connection_secret(&removed, "");
                    self.persist_connection_store();
                }
                self.maybe_refresh_omni_bar_results();
                Task::none()
            }
            ConnectionMessage::ConnectionTabSelected(id) => {
                self.connections.connection_picker_open = false;
                self.switch_connection_tab(id)
            }
            ConnectionMessage::ConnectionTabClosed(id) => {
                self.connections.connection_picker_open = false;
                if self.connections.active_tab_id == Some(id) {
                    self.update_internal(Message::Connections(
                        crate::app::features::connections::Message::Disconnect,
                    ))
                } else {
                    self.close_connection_tab(id).unwrap_or_else(Task::none)
                }
            }
            ConnectionMessage::Connect => {
                if self.connections.connecting {
                    return Task::none();
                }
                if self.connections.current.driver == DatabaseDriver::Sqlite {
                    if self.connections.current.sqlite_path.trim().is_empty() {
                        self.connections.connect_error =
                            Some(String::from("SQLite file is required."));
                        return Task::none();
                    }
                } else {
                    if self.connections.current.host.trim().is_empty() {
                        self.connections.connect_error = Some(String::from("Host is required."));
                        return Task::none();
                    }
                    if self.connections.current.username.trim().is_empty() {
                        self.connections.connect_error =
                            Some(String::from("Username is required."));
                        return Task::none();
                    }
                }

                self.close_postgres_terminal_session();

                crate::constants::ACTIVE_CONNECTION_TIMEOUT_SECS.store(
                    self.settings.values.connection_timeout_secs,
                    std::sync::atomic::Ordering::Relaxed,
                );

                self.connections.connecting = true;
                self.settings.settings_open = false;
                self.settings.settings_diagram_clear_confirmation = false;
                self.shell.omni_bar = None;
                self.shell.omni_bar_anim_progress = 0.0;
                self.shell.omni_bar_closing = false;
                self.shell.database_switcher_open = false;
                self.connections.driver_picker_open = false;
                self.connections.connection_picker_open = false;
                self.ai.ai_modal_open = false;
                self.ai.ai_modal_target = AiModalTarget::QueryEditor;
                self.ai.ai_modal_use_current_sql_context = false;
                self.ai.ai_modal_sql_context.clear();
                self.ai.ai_prompt_error = None;
                self.ai.is_generating_ai = false;
                self.ai.is_fixing_query_with_ai = false;
                self.connections.database_picker_open = false;
                self.settings.theme_picker_open = false;
                self.settings.settings_theme_picker_open = false;
                self.settings.font_picker_open = false;
                self.workspace.explorer.sidebar_tools_open = false;
                self.workspace.explorer.sidebar_filter_open = false;
                self.workspace.results.applying_changes = false;
                self.workspace.explorer.table_action_running = false;
                self.workspace.explorer.postgres_role_action_running = false;
                self.workspace.explorer.folder_generation_running = false;
                self.workspace.explorer.pending_generated_folders.clear();
                self.workspace.results.editing_cell = None;
                self.workspace.results.selected_cell = None;
                self.clear_row_selection();
                self.workspace.results.row_context_menu = None;
                self.workspace.tabs.tab_context_menu = None;
                self.workspace.explorer.table_context_menu = None;
                self.workspace.explorer.postgres_object_context_menu = None;
                self.workspace.explorer.folder_context_menu = None;
                self.workspace.explorer.table_modal = None;
                self.workspace.explorer.postgres_role_modal = None;
                self.workspace.explorer.sidebar_drag_table = None;
                self.workspace.explorer.sidebar_drag_active = false;
                self.workspace.explorer.sidebar_drop_folder = None;
                self.workspace.explorer.sidebar_table_cursor = None;
                self.workspace.tabs.tabs_cursor = None;
                self.connections.connect_error = None;
                self.connections.table_error = None;
                self.connections.database_error = None;
                self.workspace.query.error = None;
                self.workspace.results.apply_error = None;
                self.workspace.results.apply_message = None;
                self.workspace.explorer.tables.clear();
                self.workspace.explorer.postgres_sidebar_objects.clear();
                self.workspace.explorer.postgres_schema_open.clear();
                self.workspace.explorer.postgres_schema_kind_open.clear();
                self.connections.databases.clear();
                self.connections.loading_databases = false;
                self.workspace.tabs.open_tables.clear();
                self.clear_table_tabs();
                self.workspace.table_pages.clear();
                self.clear_table_cache();
                self.workspace.table_filters.clear();
                self.workspace.table_sorts.clear();
                self.workspace.tabs.pinned_table_tabs.clear();
                self.workspace.selected_table = None;
                self.workspace.explorer.selected_postgres_object = None;
                self.workspace.explorer.table_triggers.clear();
                self.workspace.explorer.triggers_table = None;
                self.workspace.explorer.selected_trigger = None;
                self.workspace.explorer.table_relations.clear();
                self.workspace.explorer.relations_table = None;
                self.workspace.explorer.clear_table_metadata_cache();
                self.clear_query_suggestion_columns_cache();
                self.workspace.query.table_query = None;
                self.workspace.query.editable_query = None;
                self.workspace.query.table = None;
                self.workspace.explorer.table_search.clear();
                self.workspace.explorer.table_folders.clear();
                self.workspace.explorer.table_folder_map.clear();
                self.connections.database_search.clear();
                self.settings.theme_search.clear();
                self.settings.settings_theme_search.clear();
                self.settings.font_search.clear();
                self.workspace.query.undo_stack.clear();
                self.workspace.query.redo_stack.clear();
                self.close_query_suggestions();
                self.clear_query_inline_suggestion();
                self.clear_pending_edits_state();
                self.workspace.results.current = None;
                self.workspace.results.vertical_viewport = None;
                self.workspace.results.horizontal_viewport = None;
                self.workspace.results.column_widths.clear();
                self.workspace.results.column_resize = None;
                self.workspace.query.table_has_next_page = false;
                self.workspace.query.last_query_was_table = false;
                self.workspace.explorer.table_info_sidebar_open = false;
                self.workspace.explorer.table_info_loading = false;
                self.workspace.explorer.table_info_error = None;
                self.workspace.explorer.table_info_table = None;
                self.workspace.explorer.table_info = None;
                self.workspace.explorer.clear_table_info_cache();
                self.reset_query_tabs_to_welcome();
                let info = self.connections.current.clone();

                if info.driver == DatabaseDriver::Sqlite {
                    Task::perform(connect_sqlite_profile(info.sqlite_path), |value| {
                        Message::Connections(
                            crate::app::features::connections::Message::SqliteConnected(value),
                        )
                    })
                } else if info.driver == DatabaseDriver::PostgreSql {
                    Task::perform(connect_postgres(info), |value| {
                        Message::Connections(
                            crate::app::features::connections::Message::PostgresConnected(value),
                        )
                    })
                } else {
                    Task::perform(connect_mysql(info), |value| {
                        Message::Connections(crate::app::features::connections::Message::Connected(
                            value,
                        ))
                    })
                }
            }
            ConnectionMessage::SqliteConnected(result) => {
                self.connections.connecting = false;

                match result {
                    Ok((pool, path)) => {
                        self.connections.pool = Some(DatabasePool::Sqlite(pool.clone()));
                        self.connections.current.sqlite_path = path;
                        self.connections.current.database = String::from("main");
                        self.connections.connected = true;
                        self.connections.loading_tables = false;
                        self.connections.connect_error = None;
                        self.connections.table_error = None;
                        self.connections.database_error = None;
                        self.workspace.explorer.tables.clear();
                        self.clear_query_suggestion_columns_cache();
                        self.workspace.explorer.postgres_sidebar_objects.clear();
                        self.record_recent_connection();
                        self.ensure_active_connection_tab();
                        self.maybe_refresh_omni_bar_results();

                        self.connections.loading_databases = true;
                        self.connections.loading_tables = true;
                        Task::batch(vec![
                            Task::perform(
                                fetch_databases(DatabasePool::Sqlite(pool.clone())),
                                |value| {
                                    Message::Connections(
                                        crate::app::features::connections::Message::DatabasesLoaded(
                                            value,
                                        ),
                                    )
                                },
                            ),
                            Task::perform(
                                fetch_tables(DatabasePool::Sqlite(pool), String::from("main")),
                                |value| {
                                    Message::Connections(
                                        crate::app::features::connections::Message::TablesLoaded(
                                            value,
                                        ),
                                    )
                                },
                            ),
                        ])
                    }
                    Err(error) => {
                        self.connections.pool = None;
                        self.connections.connected = false;
                        self.connections.connect_error = Some(error);
                        self.connections.loading_databases = false;
                        self.connections.database_error = None;
                        self.connections.databases.clear();
                        Task::none()
                    }
                }
            }
            ConnectionMessage::Connected(result) => {
                self.connections.connecting = false;

                match result {
                    Ok(pool) => {
                        self.connections.pool = Some(DatabasePool::MySql(pool.clone()));
                        self.connections.connected = true;
                        self.record_recent_connection();
                        self.ensure_active_connection_tab();
                        self.maybe_refresh_omni_bar_results();
                        self.workspace.results.applying_changes = false;
                        self.workspace.explorer.table_action_running = false;
                        self.workspace.explorer.postgres_role_action_running = false;
                        self.workspace.explorer.folder_generation_running = false;
                        self.workspace.explorer.pending_generated_folders.clear();
                        self.workspace.results.editing_cell = None;
                        self.workspace.results.selected_cell = None;
                        self.clear_row_selection();
                        self.workspace.results.row_context_menu = None;
                        self.workspace.tabs.tab_context_menu = None;
                        self.workspace.explorer.table_context_menu = None;
                        self.workspace.explorer.postgres_object_context_menu = None;
                        self.workspace.explorer.folder_context_menu = None;
                        self.workspace.explorer.table_modal = None;
                        self.workspace.explorer.postgres_role_modal = None;
                        self.workspace.explorer.sidebar_drag_table = None;
                        self.workspace.explorer.sidebar_drag_active = false;
                        self.workspace.explorer.sidebar_drop_folder = None;
                        self.workspace.explorer.sidebar_table_cursor = None;
                        self.workspace.tabs.tabs_cursor = None;
                        self.connections.loading_tables = false;
                        self.connections.connect_error = None;
                        self.connections.table_error = None;
                        self.connections.database_error = None;
                        self.workspace.query.error = None;
                        self.workspace.results.apply_error = None;
                        self.workspace.results.apply_message = None;
                        self.workspace.explorer.tables.clear();
                        self.workspace.explorer.postgres_sidebar_objects.clear();
                        self.connections.databases.clear();
                        self.workspace.tabs.open_tables.clear();
                        self.clear_table_tabs();
                        self.workspace.table_pages.clear();
                        self.clear_table_cache();
                        self.workspace.table_filters.clear();
                        self.workspace.table_sorts.clear();
                        self.workspace.tabs.pinned_table_tabs.clear();
                        self.workspace.selected_table = None;
                        self.workspace.explorer.selected_postgres_object = None;
                        self.workspace.explorer.table_triggers.clear();
                        self.workspace.explorer.triggers_table = None;
                        self.workspace.explorer.selected_trigger = None;
                        self.workspace.explorer.table_relations.clear();
                        self.workspace.explorer.relations_table = None;
                        self.workspace.explorer.clear_table_metadata_cache();
                        self.clear_query_suggestion_columns_cache();
                        self.workspace.query.table_query = None;
                        self.workspace.query.editable_query = None;
                        self.workspace.query.table = None;
                        self.workspace.explorer.table_search.clear();
                        self.workspace.explorer.table_folders.clear();
                        self.workspace.explorer.table_folder_map.clear();
                        self.clear_pending_edits_state();
                        self.workspace.results.current = None;
                        self.workspace.results.column_widths.clear();
                        self.workspace.results.column_resize = None;
                        self.workspace.query.table_has_next_page = false;
                        self.workspace.query.last_query_was_table = false;
                        self.workspace.explorer.table_info_sidebar_open = false;
                        self.workspace.explorer.table_info_loading = false;
                        self.workspace.explorer.table_info_error = None;
                        self.workspace.explorer.table_info_table = None;
                        self.workspace.explorer.table_info = None;
                        self.workspace.explorer.clear_table_info_cache();

                        self.connections.loading_databases = true;
                        let database = self.connections.current.database.trim().to_string();
                        let mut tasks = vec![Task::perform(
                            fetch_databases(DatabasePool::MySql(pool.clone())),
                            |value| {
                                Message::Connections(
                                    crate::app::features::connections::Message::DatabasesLoaded(
                                        value,
                                    ),
                                )
                            },
                        )];

                        if database.is_empty() {
                            self.connections.table_error =
                                Some(String::from("Select a database to load tables."));
                        } else {
                            self.connections.loading_tables = true;
                            tasks.push(Task::perform(
                                fetch_tables(DatabasePool::MySql(pool), database),
                                |value| {
                                    Message::Connections(
                                        crate::app::features::connections::Message::TablesLoaded(
                                            value,
                                        ),
                                    )
                                },
                            ));
                        }

                        Task::batch(tasks)
                    }
                    Err(error) => {
                        self.connections.pool = None;
                        self.connections.connected = false;
                        self.connections.connect_error = Some(error);
                        self.connections.loading_databases = false;
                        self.connections.database_error = None;
                        self.connections.databases.clear();
                        Task::none()
                    }
                }
            }
            ConnectionMessage::PostgresConnected(result) => {
                self.connections.connecting = false;

                match result {
                    Ok(pool) => {
                        self.connections.pool = Some(DatabasePool::Postgres(pool.clone()));
                        self.connections.connected = true;
                        self.record_recent_connection();
                        self.ensure_active_connection_tab();
                        self.maybe_refresh_omni_bar_results();
                        self.workspace.results.applying_changes = false;
                        self.workspace.explorer.table_action_running = false;
                        self.workspace.explorer.postgres_role_action_running = false;
                        self.workspace.explorer.folder_generation_running = false;
                        self.workspace.explorer.pending_generated_folders.clear();
                        self.workspace.results.editing_cell = None;
                        self.workspace.results.selected_cell = None;
                        self.clear_row_selection();
                        self.workspace.results.row_context_menu = None;
                        self.workspace.tabs.tab_context_menu = None;
                        self.workspace.explorer.table_context_menu = None;
                        self.workspace.explorer.postgres_object_context_menu = None;
                        self.workspace.explorer.folder_context_menu = None;
                        self.workspace.explorer.table_modal = None;
                        self.workspace.explorer.postgres_role_modal = None;
                        self.workspace.explorer.sidebar_drag_table = None;
                        self.workspace.explorer.sidebar_drag_active = false;
                        self.workspace.explorer.sidebar_drop_folder = None;
                        self.workspace.explorer.sidebar_table_cursor = None;
                        self.workspace.tabs.tabs_cursor = None;
                        self.connections.loading_tables = false;
                        self.connections.connect_error = None;
                        self.connections.table_error = None;
                        self.connections.database_error = None;
                        self.workspace.query.error = None;
                        self.workspace.results.apply_error = None;
                        self.workspace.results.apply_message = None;
                        self.workspace.explorer.tables.clear();
                        self.workspace.explorer.postgres_sidebar_objects.clear();
                        self.connections.databases.clear();
                        self.workspace.tabs.open_tables.clear();
                        self.clear_table_tabs();
                        self.workspace.table_pages.clear();
                        self.clear_table_cache();
                        self.workspace.table_filters.clear();
                        self.workspace.table_sorts.clear();
                        self.workspace.tabs.pinned_table_tabs.clear();
                        self.workspace.selected_table = None;
                        self.workspace.explorer.selected_postgres_object = None;
                        self.workspace.explorer.table_triggers.clear();
                        self.workspace.explorer.triggers_table = None;
                        self.workspace.explorer.selected_trigger = None;
                        self.workspace.explorer.table_relations.clear();
                        self.workspace.explorer.relations_table = None;
                        self.workspace.explorer.clear_table_metadata_cache();
                        self.clear_query_suggestion_columns_cache();
                        self.workspace.query.table_query = None;
                        self.workspace.query.editable_query = None;
                        self.workspace.query.table = None;
                        self.workspace.explorer.table_search.clear();
                        self.workspace.explorer.table_folders.clear();
                        self.workspace.explorer.table_folder_map.clear();
                        self.clear_pending_edits_state();
                        self.workspace.results.current = None;
                        self.workspace.results.column_widths.clear();
                        self.workspace.results.column_resize = None;
                        self.workspace.query.table_has_next_page = false;
                        self.workspace.query.last_query_was_table = false;
                        self.workspace.explorer.table_info_sidebar_open = false;
                        self.workspace.explorer.table_info_loading = false;
                        self.workspace.explorer.table_info_error = None;
                        self.workspace.explorer.table_info_table = None;
                        self.workspace.explorer.table_info = None;
                        self.workspace.explorer.clear_table_info_cache();

                        self.connections.loading_databases = true;
                        let database = self.connections.current.database.trim().to_string();
                        let mut tasks = vec![Task::perform(
                            fetch_databases(DatabasePool::Postgres(pool.clone())),
                            |value| {
                                Message::Connections(
                                    crate::app::features::connections::Message::DatabasesLoaded(
                                        value,
                                    ),
                                )
                            },
                        )];

                        if database.is_empty() {
                            self.connections.table_error =
                                Some(String::from("Select a database to load tables."));
                        } else {
                            self.connections.loading_tables = true;
                            tasks.push(Task::perform(
                                fetch_tables(DatabasePool::Postgres(pool), database),
                                |value| {
                                    Message::Connections(
                                        crate::app::features::connections::Message::TablesLoaded(
                                            value,
                                        ),
                                    )
                                },
                            ));
                        }

                        Task::batch(tasks)
                    }
                    Err(error) => {
                        self.connections.pool = None;
                        self.connections.connected = false;
                        self.connections.connect_error = Some(error);
                        self.connections.loading_databases = false;
                        self.connections.database_error = None;
                        self.connections.databases.clear();
                        Task::none()
                    }
                }
            }
            ConnectionMessage::Disconnect => {
                if self.settings.values.multiple_connections_layout && self.connections.connected {
                    self.close_postgres_terminal_session();
                    if let Some(flag) = self.workspace.query.cancel_flag.as_ref() {
                        flag.store(true, std::sync::atomic::Ordering::Relaxed);
                    }
                    if let Some(flag) = self.transfer.import_cancel_flag.as_ref() {
                        flag.store(true, std::sync::atomic::Ordering::Relaxed);
                    }
                    if let Some(flag) = self.transfer.export_cancel_flag.as_ref() {
                        flag.store(true, std::sync::atomic::Ordering::Relaxed);
                    }
                    if let Some(id) = self.connections.active_tab_id
                        && self.connections.tabs.len() > 1
                        && let Some(task) = self.close_connection_tab(id)
                    {
                        return task;
                    }
                }
                self.close_postgres_terminal_session();
                if let Some(flag) = self.workspace.query.cancel_flag.as_ref() {
                    flag.store(true, std::sync::atomic::Ordering::Relaxed);
                }
                if let Some(flag) = self.transfer.import_cancel_flag.as_ref() {
                    flag.store(true, std::sync::atomic::Ordering::Relaxed);
                }
                if let Some(flag) = self.transfer.export_cancel_flag.as_ref() {
                    flag.store(true, std::sync::atomic::Ordering::Relaxed);
                }
                self.connections.connected = false;
                self.connections.connecting = false;
                self.settings.settings_open = false;
                self.settings.settings_diagram_clear_confirmation = false;
                self.shell.omni_bar = None;
                self.shell.omni_bar_anim_progress = 0.0;
                self.shell.omni_bar_closing = false;
                self.shell.database_switcher_open = false;
                self.connections.driver_picker_open = false;
                self.ai.ai_modal_open = false;
                self.ai.ai_modal_target = AiModalTarget::QueryEditor;
                self.ai.ai_modal_use_current_sql_context = false;
                self.ai.ai_modal_sql_context.clear();
                self.ai.ai_prompt_error = None;
                self.ai.is_generating_ai = false;
                self.ai.is_fixing_query_with_ai = false;
                self.ai.chat_open = false;
                self.shell.panes.resize(self.shell.chat_split, 1.0);
                self.ai.chat_sessions.clear();
                self.ai.chat_active_session.clear();
                self.ai.next_chat_session_id = 1;
                self.ai.chat_input = iced::widget::text_editor::Content::with_text("");
                self.ai.chat_sending = false;
                self.ai.chat_error = None;
                self.ai.chat_request_id = self.ai.chat_request_id.wrapping_add(1);
                self.ai.chat_streaming_reply = None;
                self.ai.chat_streaming_reasoning = None;
                self.ai.chat_activity = None;
                self.ai.chat_deferred_reply = None;
                self.ai.chat_auto_run_pending = false;
                self.ai.chat_auto_retries = 0;
                self.ai.chat_result_followup = false;
                self.ai.chat_pending_sample = None;
                self.ai.chat_extra_tables.clear();
                self.ai.chat_schema_rounds = 0;
                self.ai.chat_follows_diagram_agent = true;
                self.diagram_agent_clear();
                self.close_text_modal();
                self.connections.database_picker_open = false;
                self.settings.theme_picker_open = false;
                self.settings.settings_theme_picker_open = false;
                self.settings.font_picker_open = false;
                self.connections.connection_picker_open = false;
                self.connections.more_options_open = false;
                self.workspace.explorer.sidebar_tools_open = false;
                self.workspace.explorer.sidebar_filter_open = false;
                self.transfer.import_modal_open = false;
                self.transfer.export_modal_open = false;
                self.workspace.explorer.table_modal = None;
                self.workspace.explorer.postgres_role_modal = None;
                self.transfer.import_stage = TransferStage::Configure;
                self.transfer.export_stage = TransferStage::Configure;
                self.transfer.import_progress = 0.0;
                self.transfer.export_progress = 0.0;
                self.transfer.import_status.clear();
                self.transfer.export_status.clear();
                self.transfer.export_table_search.clear();
                self.transfer.import_cancel_flag = None;
                self.transfer.export_cancel_flag = None;
                self.transfer.export_tables.clear();
                self.transfer.export_tables_loading = false;
                self.transfer.export_tables_error = None;
                self.shell.error_modal = None;
                self.connections.loading_tables = false;
                self.connections.loading_databases = false;
                self.workspace.query.running = false;
                self.workspace.query.cancel_flag = None;
                self.workspace.results.applying_changes = false;
                self.workspace.explorer.table_action_running = false;
                self.workspace.explorer.postgres_role_action_running = false;
                self.workspace.explorer.folder_generation_running = false;
                self.workspace.explorer.pending_generated_folders.clear();
                self.workspace.results.editing_cell = None;
                self.workspace.results.selected_cell = None;
                self.clear_row_selection();
                self.workspace.results.row_context_menu = None;
                self.workspace.tabs.tab_context_menu = None;
                self.workspace.explorer.table_context_menu = None;
                self.workspace.explorer.postgres_object_context_menu = None;
                self.workspace.explorer.folder_context_menu = None;
                self.workspace.explorer.sidebar_drag_table = None;
                self.workspace.explorer.sidebar_drag_active = false;
                self.workspace.explorer.sidebar_drop_folder = None;
                self.workspace.explorer.sidebar_table_cursor = None;
                self.workspace.tabs.tabs_cursor = None;
                self.connections.pool = None;
                self.connections.tabs.clear();
                self.connections.active_tab_id = None;
                self.connections.database_sessions.clear();
                self.workspace.explorer.tables.clear();
                self.workspace.explorer.postgres_sidebar_objects.clear();
                self.connections.databases.clear();
                self.workspace.tabs.open_tables.clear();
                self.clear_table_tabs();
                self.reset_query_tabs_to_welcome();
                self.workspace.table_pages.clear();
                self.clear_table_cache();
                self.workspace.table_filters.clear();
                self.workspace.table_sorts.clear();
                self.workspace.tabs.pinned_table_tabs.clear();
                self.workspace.selected_table = None;
                self.workspace.explorer.selected_postgres_object = None;
                self.workspace.explorer.table_triggers.clear();
                self.workspace.explorer.triggers_table = None;
                self.workspace.explorer.selected_trigger = None;
                self.workspace.explorer.table_relations.clear();
                self.workspace.explorer.relations_table = None;
                self.workspace.explorer.clear_table_metadata_cache();
                self.clear_query_suggestion_columns_cache();
                self.workspace.query.table_query = None;
                self.workspace.query.editable_query = None;
                self.workspace.query.table = None;
                self.workspace.explorer.table_search.clear();
                self.workspace.explorer.table_folders.clear();
                self.workspace.explorer.table_folder_map.clear();
                self.connections.database_search.clear();
                self.settings.theme_search.clear();
                self.settings.settings_theme_search.clear();
                self.settings.font_search.clear();
                self.workspace.query.undo_stack.clear();
                self.workspace.query.redo_stack.clear();
                self.close_query_suggestions();
                self.clear_query_inline_suggestion();
                self.clear_pending_edits_state();
                self.workspace.results.current = None;
                self.workspace.results.column_widths.clear();
                self.workspace.results.column_resize = None;
                self.workspace.query.table_has_next_page = false;
                self.workspace.query.last_query_was_table = false;
                self.workspace.explorer.table_info_sidebar_open = false;
                self.workspace.explorer.table_info_loading = false;
                self.workspace.explorer.table_info_error = None;
                self.workspace.explorer.table_info_table = None;
                self.workspace.explorer.table_info = None;
                self.workspace.explorer.clear_table_info_cache();
                self.connections.connect_error = None;
                self.connections.table_error = None;
                self.connections.database_error = None;
                self.workspace.query.error = None;
                self.workspace.results.apply_error = None;
                self.workspace.results.apply_message = None;
                self.transfer.import_path.clear();
                self.transfer.export_path.clear();
                self.transfer.import_target_database = None;
                self.transfer.export_database = None;
                self.transfer.export_excluded_tables.clear();
                Task::none()
            }
            ConnectionMessage::RefreshTables => {
                if !self.connections.connected || self.connections.loading_tables {
                    return Task::none();
                }
                let Some(pool) = self.connections.pool.clone() else {
                    return Task::none();
                };
                let database = self.connections.current.database.trim().to_string();
                if database.is_empty() {
                    self.connections.table_error =
                        Some(String::from("Select a database to load tables."));
                    return Task::none();
                }
                self.connections.loading_tables = true;
                self.connections.table_error = None;
                self.workspace.explorer.clear_table_metadata_cache();
                self.clear_query_suggestion_columns_cache();
                if self.connections.current.driver == DatabaseDriver::PostgreSql {
                    self.workspace.explorer.postgres_sidebar_objects.clear();
                }
                Task::perform(fetch_tables(pool, database), |value| {
                    Message::Connections(crate::app::features::connections::Message::TablesLoaded(
                        value,
                    ))
                })
            }
            ConnectionMessage::RefreshDatabaseAndTables => {
                let databases = self.update_internal(Message::Connections(
                    crate::app::features::connections::Message::RefreshDatabases,
                ));
                let tables = self.update_internal(Message::Connections(
                    crate::app::features::connections::Message::RefreshTables,
                ));
                Task::batch(vec![databases, tables])
            }
            ConnectionMessage::RefreshDatabases => {
                if !self.connections.connected || self.connections.loading_databases {
                    return Task::none();
                }
                let Some(pool) = self.connections.pool.clone() else {
                    return Task::none();
                };
                self.connections.loading_databases = true;
                self.connections.database_error = None;
                Task::perform(fetch_databases(pool), |value| {
                    Message::Connections(
                        crate::app::features::connections::Message::DatabasesLoaded(value),
                    )
                })
            }
            ConnectionMessage::TableRowCountsLoaded(counts) => {
                self.workspace.explorer.table_row_counts = counts;
                Task::none()
            }
            ConnectionMessage::TablesLoaded(result) => {
                if !self.connections.connected {
                    self.connections.loading_tables = false;
                    return Task::none();
                }
                self.connections.loading_tables = false;
                let mut follow_up_tasks: Vec<Task<Message>> = Vec::new();
                match result {
                    Ok(tables) => {
                        self.workspace.explorer.tables = tables;
                        self.connections.table_error = None;
                        if let (Some(pool), Some(database)) =
                            (self.connections.pool.clone(), self.current_database())
                        {
                            follow_up_tasks.push(Task::perform(
                                crate::db::metadata::fetch_table_row_counts(pool, database),
                                |result| Message::Connections(crate::app::features::connections::Message::TableRowCountsLoaded(result.unwrap_or_default())),
                            ));
                        }
                        if self.connections.current.driver != DatabaseDriver::PostgreSql {
                            self.workspace.explorer.postgres_sidebar_objects.clear();
                        } else if let (Some(pool), Some(database)) =
                            (self.connections.pool.clone(), self.current_database())
                        {
                            follow_up_tasks.push(Task::perform(
                                fetch_postgres_sidebar_objects(pool, database.clone()),
                                move |result| {
                                    Message::Workspace(
                                        crate::app::features::workspace::Message::Explorer(
                                            explorer::Message::PostgresSidebarObjectsLoaded {
                                                database,
                                                result,
                                            },
                                        ),
                                    )
                                },
                            ));
                        }
                        if let (Some(pool), Some(database)) =
                            (self.connections.pool.clone(), self.current_database())
                        {
                            follow_up_tasks.push(Task::perform(
                                fetch_sidebar_triggers(pool.clone(), database.clone()),
                                |result| {
                                    Message::Workspace(
                                        crate::app::features::workspace::Message::Explorer(
                                            explorer::Message::SidebarTriggersLoaded(result),
                                        ),
                                    )
                                },
                            ));
                            follow_up_tasks.push(Task::perform(
                                fetch_sidebar_relations(pool, database),
                                |result| {
                                    Message::Workspace(
                                        crate::app::features::workspace::Message::Explorer(
                                            explorer::Message::SidebarRelationsLoaded(result),
                                        ),
                                    )
                                },
                            ));
                        }
                        self.load_folder_state_for_current_database();
                        let known = self
                            .workspace
                            .explorer
                            .tables
                            .iter()
                            .cloned()
                            .collect::<HashSet<_>>();
                        self.workspace.explorer.retain_table_metadata_cache(&known);
                        self.retain_query_suggestion_columns_cache(&known);
                        self.workspace.explorer.retain_table_info_cache(&known);
                        if let Some(selected) = self.workspace.selected_table.clone()
                            && !self
                                .workspace
                                .explorer
                                .tables
                                .iter()
                                .any(|t| t == &selected)
                        {
                            self.workspace.selected_table = None;
                            self.workspace.explorer.table_triggers.clear();
                            self.workspace.explorer.triggers_table = None;
                            self.workspace.explorer.selected_trigger = None;
                            self.workspace.explorer.table_relations.clear();
                            self.workspace.explorer.relations_table = None;
                            self.workspace
                                .explorer
                                .remove_table_metadata_cache(&selected);
                        }
                    }
                    Err(error) => {
                        self.workspace.explorer.tables.clear();
                        self.workspace.explorer.postgres_sidebar_objects.clear();
                        self.workspace.explorer.table_folders.clear();
                        self.workspace.explorer.table_folder_map.clear();
                        self.workspace.selected_table = None;
                        self.workspace.explorer.selected_postgres_object = None;
                        self.workspace.explorer.table_triggers.clear();
                        self.workspace.explorer.triggers_table = None;
                        self.workspace.explorer.selected_trigger = None;
                        self.workspace.explorer.table_relations.clear();
                        self.workspace.explorer.relations_table = None;
                        self.workspace.explorer.clear_table_metadata_cache();
                        self.clear_query_suggestion_columns_cache();
                        self.workspace.explorer.clear_table_info_cache();
                        self.connections.table_error = Some(error);
                    }
                }
                if self.workspace.query.suggestions_open {
                    self.refresh_query_suggestions();
                    follow_up_tasks.push(self.prefetch_query_suggestion_columns());
                }
                self.maybe_refresh_omni_bar_results();
                if follow_up_tasks.is_empty() {
                    Task::none()
                } else {
                    Task::batch(follow_up_tasks)
                }
            }

            ConnectionMessage::DatabasesLoaded(result) => {
                if !self.connections.connected {
                    self.connections.loading_databases = false;
                    return Task::none();
                }
                self.connections.loading_databases = false;
                match result {
                    Ok(mut databases) => {
                        databases.sort();
                        self.connections.databases = databases;
                        if let Some(selected) = self.transfer.import_target_database.as_ref()
                            && !self.connections.databases.iter().any(|db| db == selected)
                        {
                            self.transfer.import_target_database = None;
                        }
                        if let Some(selected) = self.transfer.export_database.as_ref()
                            && !self.connections.databases.iter().any(|db| db == selected)
                        {
                            self.transfer.export_database = None;
                        }
                        if self.shell.database_switcher_index >= self.connections.databases.len() {
                            self.shell.database_switcher_index =
                                self.connections.databases.len().saturating_sub(1);
                        }
                        self.prune_folder_store_databases();
                        self.connections.database_error = None;
                    }
                    Err(error) => {
                        self.connections.databases.clear();
                        self.connections.database_error = Some(error);
                    }
                }
                self.maybe_refresh_omni_bar_results();
                Task::none()
            }
            ConnectionMessage::DatabaseSelected(database) => {
                self.shell.database_switcher_open = false;
                self.connections.database_picker_open = false;
                self.workspace.explorer.sidebar_tools_open = false;
                self.workspace.explorer.sidebar_filter_open = false;
                self.connections.database_search.clear();
                self.workspace.tabs.tab_context_menu = None;
                self.workspace.explorer.table_context_menu = None;
                self.workspace.explorer.postgres_object_context_menu = None;
                self.workspace.explorer.folder_context_menu = None;
                self.workspace.explorer.table_modal = None;
                self.workspace.explorer.postgres_role_modal = None;
                self.workspace.explorer.postgres_role_action_running = false;
                self.workspace.explorer.sidebar_drag_table = None;
                self.workspace.explorer.sidebar_drag_active = false;
                self.workspace.explorer.sidebar_drop_folder = None;
                self.workspace.explorer.sidebar_table_cursor = None;
                if !self.connections.connected
                    || self.connections.loading_tables
                    || self.connections.loading_databases
                    || self.workspace.query.running
                    || self.workspace.results.applying_changes
                {
                    return Task::none();
                }
                if self.connections.current.database == database {
                    return Task::none();
                }

                let driver = self.connections.current.driver;
                self.save_current_database_session();
                self.connections.current.database = database;
                self.connections.table_error = None;
                self.workspace.query.error = None;
                self.workspace.results.apply_error = None;
                self.workspace.results.apply_message = None;

                let target_key = self.database_session_key_for(&self.connections.current.database);
                if let Some(session) = self.connections.database_sessions.remove(&target_key) {
                    self.restore_database_session(session);
                } else {
                    self.reset_database_workspace();
                    if driver == DatabaseDriver::PostgreSql {
                        return self.update(Message::Connections(
                            crate::app::features::connections::Message::Connect,
                        ));
                    }
                }

                let Some(pool) = self.connections.pool.clone() else {
                    return Task::none();
                };

                let database = self.connections.current.database.trim().to_string();
                if database.is_empty() {
                    self.workspace.explorer.tables.clear();
                    self.workspace.explorer.postgres_sidebar_objects.clear();
                    self.connections.table_error =
                        Some(String::from("Select a database to load tables."));
                    return Task::none();
                }

                self.connections.loading_tables = true;
                self.workspace.explorer.tables.clear();
                self.workspace.explorer.postgres_sidebar_objects.clear();
                Task::perform(fetch_tables(pool, database), |value| {
                    Message::Connections(crate::app::features::connections::Message::TablesLoaded(
                        value,
                    ))
                })
            }
            ConnectionMessage::DatabaseSearchChanged(search) => {
                self.connections.database_search = search;
                Task::none()
            }
            ConnectionMessage::CopyConnectError => {
                if let Some(error) = self.connections.connect_error.clone() {
                    iced::clipboard::write(error)
                } else {
                    Task::none()
                }
            }
            ConnectionMessage::CopyConnectionStoreError => {
                if let Some(error) = self.connections.store_error.clone() {
                    iced::clipboard::write(error)
                } else {
                    Task::none()
                }
            }
            ConnectionMessage::ConnectionTabsScrolled(viewport) => {
                self.connections.tabs_horizontal_viewport = Some(viewport);
                Task::none()
            }
            ConnectionMessage::ConnectionTabsWheelScrolled(delta) => {
                let Some(viewport) = self.connections.tabs_horizontal_viewport else {
                    return Task::none();
                };

                let step = self.scale_f32(56.0);
                let movement = match delta {
                    mouse::ScrollDelta::Lines { x, y } => (x * step) - (y * step),
                    mouse::ScrollDelta::Pixels { x, y } => x - y,
                };

                if movement.abs() < f32::EPSILON {
                    return Task::none();
                }

                let offset_x = viewport.absolute_offset().x;
                let viewport_width = viewport.bounds().width;
                let max_offset = (viewport.content_bounds().width - viewport_width).max(0.0);
                if max_offset <= 0.0 {
                    return Task::none();
                }

                let next_offset = (offset_x + movement).clamp(0.0, max_offset);
                if (next_offset - offset_x).abs() < f32::EPSILON {
                    return Task::none();
                }

                iced::widget::operation::scroll_to(
                    connection_tabs_scroll_id(),
                    scrollable::AbsoluteOffset {
                        x: Some(next_offset),
                        y: None,
                    },
                )
            }
            ConnectionMessage::LoginKeyPressed(event) => {
                if self.connections.connected {
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

                if self.connections.postgres_terminal_open {
                    return Task::none();
                }

                if let Some(task) = self.handle_omni_bar_keypress(&key, modifiers, physical_key) {
                    return task;
                }

                if let Some(mode) = self.omni_bar_shortcut_mode(&key, modifiers) {
                    return self.update(Message::Shell(crate::app::shell::Message::OpenOmniBar(
                        mode,
                    )));
                }

                if matches!(key, keyboard::Key::Named(keyboard::key::Named::Tab)) {
                    return if modifiers.shift() {
                        iced::widget::operation::focus_previous()
                    } else {
                        iced::widget::operation::focus_next()
                    };
                }

                if matches!(key, keyboard::Key::Named(keyboard::key::Named::Escape)) {
                    if self.shell.changelog_open {
                        self.shell.changelog_open = false;
                        return Task::none();
                    }
                    if self.settings.settings_open {
                        self.settings.settings_open = false;
                        self.settings.shortcut_capture_target = None;
                        self.settings.settings_theme_picker_open = false;
                        self.settings.settings_theme_search.clear();
                        self.settings.font_picker_open = false;
                        self.settings.font_search.clear();
                        return Task::none();
                    }
                    return Task::none();
                }

                let is_enter = matches!(key, keyboard::Key::Named(keyboard::key::Named::Enter))
                    || physical_key == keyboard::key::Code::NumpadEnter;

                if is_enter
                    && !modifiers.control()
                    && !modifiers.alt()
                    && !modifiers.logo()
                    && self.can_connect()
                {
                    return self.update(Message::Connections(
                        crate::app::features::connections::Message::Connect,
                    ));
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

                Task::none()
            }
            ConnectionMessage::OpenPostgresTerminal => {
                if self.connections.postgres_terminal_open {
                    return self.focus_postgres_terminal();
                }

                if let Err(error) = self.open_postgres_terminal() {
                    self.push_toast(ToastLevel::Error, error);
                    return Task::none();
                }

                self.settings.settings_open = false;
                self.shell.database_switcher_open = false;
                self.connections.database_picker_open = false;
                self.settings.theme_picker_open = false;
                self.settings.settings_theme_picker_open = false;
                self.settings.settings_theme_search.clear();
                self.settings.font_picker_open = false;
                self.connections.more_options_open = false;
                self.workspace.explorer.sidebar_tools_open = false;
                self.workspace.explorer.table_context_menu = None;
                self.workspace.explorer.folder_context_menu = None;
                self.workspace.tabs.tab_context_menu = None;
                self.workspace.explorer.table_info_sidebar_open = false;
                self.workspace.explorer.table_info_loading = false;
                self.close_text_modal();

                self.focus_postgres_terminal()
            }
            ConnectionMessage::ClosePostgresTerminal => {
                self.close_postgres_terminal_session();
                Task::none()
            }
            ConnectionMessage::PostgresTerminalEvent(event) => {
                let action = match event {
                    iced_term::Event::BackendCall(terminal_id, command) => {
                        let Some(terminal) = self.connections.postgres_terminal.as_mut() else {
                            return Task::none();
                        };
                        if terminal.id != terminal_id {
                            return Task::none();
                        }
                        terminal.handle(iced_term::Command::ProxyToBackend(command))
                    }
                };

                match action {
                    iced_term::actions::Action::Shutdown => {
                        self.close_postgres_terminal_session();
                    }
                    iced_term::actions::Action::ChangeTitle(title) => {
                        let title = title.trim();
                        if title.is_empty() {
                            self.connections.postgres_terminal_title.clear();
                        } else {
                            self.connections.postgres_terminal_title = title.to_string();
                        }
                    }
                    iced_term::actions::Action::Ignore => {}
                }

                Task::none()
            }
            ConnectionMessage::ToggleDatabasePicker => {
                if !self.connections.database_picker_open {
                    self.connections.connection_picker_open = false;
                    self.settings.theme_picker_open = false;
                    self.settings.settings_theme_picker_open = false;
                    self.settings.settings_theme_search.clear();
                    self.connections.more_options_open = false;
                    self.workspace.explorer.sidebar_tools_open = false;
                    self.workspace.explorer.table_context_menu = None;
                    self.workspace.explorer.postgres_object_context_menu = None;
                    self.workspace.explorer.folder_context_menu = None;
                }
                self.connections.database_picker_open = !self.connections.database_picker_open;
                if !self.connections.database_picker_open {
                    self.connections.database_search.clear();
                }
                Task::none()
            }
            ConnectionMessage::ToggleConnectionPicker => {
                if !self.connections.connection_picker_open {
                    self.connections.database_picker_open = false;
                    self.connections.database_search.clear();
                    self.settings.theme_picker_open = false;
                    self.settings.settings_theme_picker_open = false;
                    self.connections.more_options_open = false;
                    self.workspace.explorer.sidebar_tools_open = false;
                    self.workspace.explorer.table_context_menu = None;
                    self.workspace.explorer.postgres_object_context_menu = None;
                    self.workspace.explorer.folder_context_menu = None;
                }
                self.connections.connection_picker_open = !self.connections.connection_picker_open;
                Task::none()
            }
            ConnectionMessage::CloseConnectionPicker => {
                self.connections.connection_picker_open = false;
                Task::none()
            }
            ConnectionMessage::StartNewConnection => {
                self.connections.connection_picker_open = false;
                if self.settings.values.multiple_connections_layout {
                    self.prepare_new_connection_tab();
                    Task::none()
                } else {
                    self.update_internal(Message::Connections(
                        crate::app::features::connections::Message::Disconnect,
                    ))
                }
            }
            ConnectionMessage::CloseDatabaseSwitcher => {
                self.shell.database_switcher_open = false;
                Task::none()
            }
        }
    }
}
