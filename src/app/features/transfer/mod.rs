mod export;
pub(crate) mod view;
use crate::app::types::ErrorModalState;
use crate::db::DatabasePool;
use crate::db::metadata::fetch_tables;
use crate::db::transfer::{
    Event, export_postgres_stream, export_sqlite_stream, export_stream, import_postgres_stream,
    import_sqlite_stream, import_stream,
};
use crate::model::connection::{ConnectionInfo, DatabaseDriver};
use crate::model::transfer::{
    ExportOptions, ExportSummary, ImportOptions, ImportSummary, InnoDbVersion, TransferError,
    TransferStage,
};
use crate::utils::format::{format_bytes, format_duration};
use iced::Task;
use rfd::AsyncFileDialog;
use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

pub(crate) struct Context<'a> {
    pub(crate) pool: Option<&'a DatabasePool>,
    pub(crate) connection: &'a ConnectionInfo,
    pub(crate) database: Option<String>,
    pub(crate) is_connected: bool,
}

#[derive(Debug, Clone)]
pub(crate) enum Message {
    TextEdited(crate::ui::widgets::history_input::Edit<Message>),
    Tooltip(String),
    ClearTooltip,
    ModalBlocked,
    OpenImportModal,
    CloseImportModal,
    OpenExportModal,
    CloseExportModal,
    ImportPathChanged(String),
    ExportPathChanged(String),
    ImportBrowsePath,
    ExportBrowsePath,
    ImportFilePicked(Option<PathBuf>),
    ExportFilePicked(Option<PathBuf>),
    ImportTargetDatabaseSelected(String),
    ExportDatabaseSelected(String),
    ImportInnoDbSelected(InnoDbVersion),
    ImportCreateDatabaseToggled(bool),
    ImportDropExistingToggled(bool),
    ImportDisableForeignKeysToggled(bool),
    ImportUseTransactionToggled(bool),
    ExportIncludeDropToggled(bool),
    ExportIncludeInsertsToggled(bool),
    ExportUseValuesToggled(bool),
    ExportIncludeCreateToggled(bool),
    ExportIncludeRoutinesToggled(bool),
    ExportTableToggled { table: String, include: bool },
    ExportSelectAllTables,
    ExportSelectNoneTables,
    ExportTableSearchChanged(String),
    RefreshExportTables,
    StartImport,
    StartExport,
    CancelImport,
    CancelExport,
    ImportProgress { progress: f32, status: String },
    ExportProgress { progress: f32, status: String },
    ImportFinished(Result<ImportSummary, TransferError>),
    ExportFinished(Result<ExportSummary, TransferError>),
    ExportTablesLoaded(Result<Vec<String>, String>),
    ToggleTransferMenu,
    CloseTransferMenu,
}

impl From<Event> for Message {
    fn from(event: Event) -> Self {
        match event {
            Event::ImportProgress { progress, status } => Self::ImportProgress { progress, status },
            Event::ExportProgress { progress, status } => Self::ExportProgress { progress, status },
            Event::ImportFinished(result) => Self::ImportFinished(result),
            Event::ExportFinished(result) => Self::ExportFinished(result),
        }
    }
}

pub(crate) enum Output {
    Opened,
    Error(Option<ErrorModalState>),
    TextEdited(crate::ui::widgets::history_input::Edit<Message>),
    Tooltip(Option<String>),
}

pub(crate) struct State {
    pub(crate) import_modal_open: bool,
    pub(crate) export_modal_open: bool,
    pub(crate) import_path: String,
    pub(crate) import_target_database: Option<String>,
    pub(crate) import_innodb_version: InnoDbVersion,
    pub(crate) import_create_database: bool,
    pub(crate) import_drop_existing: bool,
    pub(crate) import_disable_foreign_keys: bool,
    pub(crate) import_use_transaction: bool,
    pub(crate) import_stage: TransferStage,
    pub(crate) import_progress: f32,
    pub(crate) import_status: String,
    pub(crate) export_path: String,
    pub(crate) export_database: Option<String>,
    pub(crate) export_include_drop: bool,
    pub(crate) export_include_inserts: bool,
    pub(crate) export_use_values: bool,
    pub(crate) export_include_create: bool,
    pub(crate) export_include_routines: bool,
    pub(crate) export_stage: TransferStage,
    pub(crate) export_progress: f32,
    pub(crate) export_status: String,
    pub(crate) export_excluded_tables: BTreeSet<String>,
    pub(crate) export_table_search: String,
    pub(crate) export_tables: Vec<String>,
    pub(crate) export_tables_loading: bool,
    pub(crate) export_tables_error: Option<String>,
    pub(crate) import_cancel_flag: Option<Arc<AtomicBool>>,
    pub(crate) export_cancel_flag: Option<Arc<AtomicBool>>,
    pub(crate) transfer_menu_open: bool,
}

impl Default for State {
    fn default() -> Self {
        Self {
            import_modal_open: false,
            export_modal_open: false,
            import_path: String::new(),
            import_target_database: None,
            import_innodb_version: InnoDbVersion::Default,
            import_create_database: true,
            import_drop_existing: false,
            import_disable_foreign_keys: true,
            import_use_transaction: true,
            import_stage: TransferStage::Configure,
            import_progress: 0.0,
            import_status: String::new(),
            export_path: String::new(),
            export_database: None,
            export_include_drop: false,
            export_include_inserts: true,
            export_use_values: true,
            export_include_create: true,
            export_include_routines: false,
            export_stage: TransferStage::Configure,
            export_progress: 0.0,
            export_status: String::new(),
            export_excluded_tables: BTreeSet::new(),
            export_table_search: String::new(),
            export_tables: Vec::new(),
            export_tables_loading: false,
            export_tables_error: None,
            import_cancel_flag: None,
            export_cancel_flag: None,
            transfer_menu_open: false,
        }
    }
}

impl State {
    pub(crate) fn update(
        &mut self,
        message: Message,
        context: Context<'_>,
    ) -> (Task<Message>, Vec<Output>) {
        let mut outputs = Vec::new();
        let task = self.update_internal(message, context, &mut outputs);
        (task, outputs)
    }

    fn update_internal(
        &mut self,
        message: Message,
        context: Context<'_>,
        outputs: &mut Vec<Output>,
    ) -> Task<Message> {
        match message {
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
            Message::ModalBlocked => Task::none(),
            Message::OpenImportModal => {
                outputs.push(Output::Opened);
                self.transfer_menu_open = false;
                self.import_modal_open = true;
                self.export_modal_open = false;
                outputs.push(Output::Error(None));
                if self.import_target_database.is_none() {
                    self.import_target_database = context.database.clone();
                }
                self.import_stage = TransferStage::Configure;
                self.import_progress = 0.0;
                self.import_status.clear();
                self.import_cancel_flag = None;
                Task::none()
            }
            Message::CloseImportModal => {
                if self.import_stage == TransferStage::Running {
                    return Task::none();
                }
                self.import_modal_open = false;
                self.import_stage = TransferStage::Configure;
                self.import_progress = 0.0;
                self.import_status.clear();
                self.import_cancel_flag = None;
                outputs.push(Output::Error(None));
                Task::none()
            }
            Message::OpenExportModal => {
                outputs.push(Output::Opened);
                self.transfer_menu_open = false;
                self.export_modal_open = true;
                self.import_modal_open = false;
                outputs.push(Output::Error(None));
                if self.export_database.is_none() {
                    self.export_database = context.database.clone();
                }
                self.export_stage = TransferStage::Configure;
                self.export_progress = 0.0;
                self.export_status.clear();
                self.export_cancel_flag = None;
                if context.is_connected {
                    self.refresh_export_tables(context.pool)
                } else {
                    Task::none()
                }
            }
            Message::CloseExportModal => {
                if self.export_stage == TransferStage::Running {
                    return Task::none();
                }
                self.export_modal_open = false;
                self.export_stage = TransferStage::Configure;
                self.export_progress = 0.0;
                self.export_status.clear();
                self.export_table_search.clear();
                self.export_cancel_flag = None;
                self.export_tables_error = None;
                outputs.push(Output::Error(None));
                Task::none()
            }
            Message::ImportPathChanged(path) => {
                self.import_path = path;
                Task::none()
            }
            Message::ExportPathChanged(path) => {
                self.export_path = path;
                Task::none()
            }
            Message::ImportBrowsePath => {
                let task = if context.connection.driver == DatabaseDriver::Sqlite {
                    AsyncFileDialog::new()
                        .add_filter("SQLite / SQL", &["db", "sqlite", "sqlite3", "sql"])
                        .pick_file()
                } else {
                    AsyncFileDialog::new()
                        .add_filter("SQL", &["sql"])
                        .pick_file()
                };
                Task::perform(task, |file| {
                    Message::ImportFilePicked(file.map(|handle| handle.path().to_path_buf()))
                })
            }
            Message::ExportBrowsePath => {
                let task = if context.connection.driver == DatabaseDriver::Sqlite {
                    AsyncFileDialog::new()
                        .add_filter("SQLite / SQL", &["db", "sqlite", "sqlite3", "sql"])
                        .save_file()
                } else {
                    AsyncFileDialog::new()
                        .add_filter("SQL", &["sql"])
                        .save_file()
                };
                Task::perform(task, |file| {
                    Message::ExportFilePicked(file.map(|handle| handle.path().to_path_buf()))
                })
            }
            Message::ImportFilePicked(file) => {
                if let Some(file) = file {
                    self.import_path = file.display().to_string();
                }
                Task::none()
            }
            Message::ExportFilePicked(file) => {
                if let Some(file) = file {
                    self.export_path = file.display().to_string();
                }
                Task::none()
            }
            Message::ImportTargetDatabaseSelected(database) => {
                self.import_target_database = Some(database);
                Task::none()
            }
            Message::ExportDatabaseSelected(database) => {
                self.export_database = Some(database);
                self.export_excluded_tables.clear();
                self.export_table_search.clear();
                self.refresh_export_tables(context.pool)
            }
            Message::ImportInnoDbSelected(version) => {
                self.import_innodb_version = version;
                Task::none()
            }
            Message::ImportCreateDatabaseToggled(enabled) => {
                self.import_create_database = enabled;
                Task::none()
            }
            Message::ImportDropExistingToggled(enabled) => {
                self.import_drop_existing = enabled;
                Task::none()
            }
            Message::ImportDisableForeignKeysToggled(enabled) => {
                self.import_disable_foreign_keys = enabled;
                Task::none()
            }
            Message::ImportUseTransactionToggled(enabled) => {
                self.import_use_transaction = enabled;
                Task::none()
            }
            Message::ExportIncludeDropToggled(enabled) => {
                self.export_include_drop = enabled;
                Task::none()
            }
            Message::ExportIncludeInsertsToggled(enabled) => {
                self.export_include_inserts = enabled;
                Task::none()
            }
            Message::ExportUseValuesToggled(enabled) => {
                self.export_use_values = enabled;
                Task::none()
            }
            Message::ExportIncludeCreateToggled(enabled) => {
                self.export_include_create = enabled;
                Task::none()
            }
            Message::ExportIncludeRoutinesToggled(enabled) => {
                self.export_include_routines = enabled;
                Task::none()
            }
            Message::ExportTableToggled { table, include } => {
                if include {
                    self.export_excluded_tables.remove(&table);
                } else {
                    self.export_excluded_tables.insert(table);
                }
                Task::none()
            }
            Message::ExportSelectAllTables => {
                let filtered = self.export_visible_tables();
                let tables = if filtered.is_empty() && !self.export_table_search.trim().is_empty() {
                    Vec::new()
                } else if self.export_table_search.trim().is_empty() {
                    self.export_tables.clone()
                } else {
                    filtered
                };
                for table in tables {
                    self.export_excluded_tables.remove(&table);
                }
                Task::none()
            }
            Message::ExportSelectNoneTables => {
                let filtered = self.export_visible_tables();
                let tables = if filtered.is_empty() && !self.export_table_search.trim().is_empty() {
                    Vec::new()
                } else if self.export_table_search.trim().is_empty() {
                    self.export_tables.clone()
                } else {
                    filtered
                };
                for table in tables {
                    self.export_excluded_tables.insert(table);
                }
                Task::none()
            }
            Message::ExportTableSearchChanged(search) => {
                self.export_table_search = search;
                Task::none()
            }
            Message::RefreshExportTables => self.refresh_export_tables(context.pool),
            Message::StartImport => {
                outputs.push(Output::Error(None));
                if self.import_stage == TransferStage::Running {
                    return Task::none();
                }
                let Some(pool) = context.pool.cloned() else {
                    outputs.push(Output::Error(Some(ErrorModalState::new(
                        "Import error",
                        "Connect to a database before importing.",
                        "No active connection is available.",
                    ))));
                    return Task::none();
                };
                if self.import_path.trim().is_empty() {
                    let source_label = if context.connection.driver == DatabaseDriver::Sqlite {
                        "Select a SQLite database file or SQL script to import."
                    } else {
                        "Select a SQL file to import."
                    };
                    outputs.push(Output::Error(Some(ErrorModalState::new(
                        "Import error",
                        source_label,
                        "Import path is required before starting.",
                    ))));
                    return Task::none();
                }
                let import_path = PathBuf::from(self.import_path.trim());
                if !import_path.is_file() {
                    let source_label = if context.connection.driver == DatabaseDriver::Sqlite {
                        "The SQLite source file or SQL script could not be found."
                    } else {
                        "The SQL file could not be found."
                    };
                    outputs.push(Output::Error(Some(ErrorModalState::new(
                        "Import error",
                        source_label,
                        crate::i18n::tr_with(
                            "Import path `{path}` is not a file.",
                            &[("{path}", self.import_path.trim())],
                        ),
                    ))));
                    return Task::none();
                }
                let target_database = if context.connection.driver == DatabaseDriver::Sqlite {
                    String::from("main")
                } else {
                    let has_database = self
                        .import_target_database
                        .as_deref()
                        .is_some_and(|db| !db.trim().is_empty());
                    if !has_database {
                        outputs.push(Output::Error(Some(ErrorModalState::new(
                            "Import error",
                            "Select a database to import into.",
                            "Target database is required before starting.",
                        ))));
                        return Task::none();
                    }
                    self.import_target_database.clone().unwrap()
                };
                let options = ImportOptions {
                    create_database: self.import_create_database,
                    drop_existing: self.import_drop_existing,
                    disable_foreign_keys: self.import_disable_foreign_keys,
                    use_transaction: self.import_use_transaction,
                    innodb_version: self.import_innodb_version,
                };
                let cancel_flag = Arc::new(AtomicBool::new(false));
                self.import_cancel_flag = Some(cancel_flag.clone());
                self.import_stage = TransferStage::Running;
                self.import_progress = 0.0;
                self.import_status = String::from("Preparing import...");
                match pool {
                    DatabasePool::MySql(pool) => Task::run(
                        import_stream(pool, import_path, target_database, options, cancel_flag),
                        Message::from,
                    ),
                    DatabasePool::Postgres(_) => Task::run(
                        import_postgres_stream(
                            context.connection.clone(),
                            import_path,
                            target_database,
                            options,
                            cancel_flag,
                        ),
                        Message::from,
                    ),
                    DatabasePool::Sqlite(_) => {
                        let target_path = PathBuf::from(context.connection.sqlite_path.trim());
                        if target_path.as_os_str().is_empty() {
                            self.import_cancel_flag = None;
                            self.import_stage = TransferStage::Configure;
                            self.import_progress = 0.0;
                            self.import_status.clear();
                            outputs.push(Output::Error(Some(ErrorModalState::new(
                                "Import error",
                                "Open a destination SQLite database first.",
                                "The active SQLite connection path is empty.",
                            ))));
                            return Task::none();
                        }
                        Task::run(
                            import_sqlite_stream(import_path, target_path, options, cancel_flag),
                            Message::from,
                        )
                    }
                }
            }
            Message::StartExport => {
                outputs.push(Output::Error(None));
                if self.export_stage == TransferStage::Running {
                    return Task::none();
                }
                let Some(pool) = context.pool.cloned() else {
                    outputs.push(Output::Error(Some(ErrorModalState::new(
                        "Export error",
                        "Connect to a database before exporting.",
                        "No active connection is available.",
                    ))));
                    return Task::none();
                };
                if self.export_path.trim().is_empty() {
                    let destination_label = if context.connection.driver == DatabaseDriver::Sqlite {
                        "Select a destination SQLite file or SQL dump."
                    } else {
                        "Select a destination file for the export."
                    };
                    outputs.push(Output::Error(Some(ErrorModalState::new(
                        "Export error",
                        destination_label,
                        "Export path is required before starting.",
                    ))));
                    return Task::none();
                }
                let has_database = self
                    .export_database
                    .as_deref()
                    .is_some_and(|db| !db.trim().is_empty());
                if context.connection.driver != DatabaseDriver::Sqlite && !has_database {
                    outputs.push(Output::Error(Some(ErrorModalState::new(
                        "Export error",
                        "Select a database to export.",
                        "Database selection is required before starting.",
                    ))));
                    return Task::none();
                }
                let database = if context.connection.driver == DatabaseDriver::Sqlite {
                    String::from("main")
                } else {
                    self.export_database.clone().unwrap()
                };
                if context.connection.driver != DatabaseDriver::Sqlite && self.export_tables_loading
                {
                    outputs.push(Output::Error(Some(ErrorModalState::new(
                        "Export error",
                        "Tables are still loading.",
                        "Wait for the table list to finish loading.",
                    ))));
                    return Task::none();
                }
                if context.connection.driver != DatabaseDriver::Sqlite
                    && let Some(error) = &self.export_tables_error
                {
                    outputs.push(Output::Error(Some(ErrorModalState::new(
                        "Export error",
                        "Failed to load tables for export.",
                        error.clone(),
                    ))));
                    return Task::none();
                }
                if context.connection.driver != DatabaseDriver::Sqlite
                    && self.export_tables.is_empty()
                {
                    outputs.push(Output::Error(Some(ErrorModalState::new(
                        "Export error",
                        "Load the database tables before exporting.",
                        "No tables are available for export.",
                    ))));
                    return Task::none();
                }
                if context.connection.driver != DatabaseDriver::Sqlite
                    && !self.export_tables.is_empty()
                    && !self
                        .export_tables
                        .iter()
                        .any(|table| !self.export_excluded_tables.contains(table))
                {
                    outputs.push(Output::Error(Some(ErrorModalState::new(
                        "Export error",
                        "Select at least one table to export.",
                        "All tables are excluded from the export.",
                    ))));
                    return Task::none();
                }
                let export_path = PathBuf::from(self.export_path.trim());
                let tables = self.export_selected_tables();
                if context.connection.driver != DatabaseDriver::Sqlite && tables.is_empty() {
                    outputs.push(Output::Error(Some(ErrorModalState::new(
                        "Export error",
                        "Select at least one table to export.",
                        "No tables are selected for export.",
                    ))));
                    return Task::none();
                }
                let options = ExportOptions {
                    include_drop: self.export_include_drop,
                    include_create: self.export_include_create,
                    include_inserts: self.export_include_inserts,
                    use_values: self.export_use_values,
                    include_routines: self.export_include_routines,
                };
                let cancel_flag = Arc::new(AtomicBool::new(false));
                self.export_cancel_flag = Some(cancel_flag.clone());
                self.export_stage = TransferStage::Running;
                self.export_progress = 0.0;
                self.export_status = String::from("Preparing export...");
                match pool {
                    DatabasePool::MySql(pool) => Task::run(
                        export_stream(pool, export_path, database, tables, options, cancel_flag),
                        Message::from,
                    ),
                    DatabasePool::Postgres(_) => Task::run(
                        export_postgres_stream(
                            context.connection.clone(),
                            export_path,
                            database,
                            tables,
                            options,
                            cancel_flag,
                        ),
                        Message::from,
                    ),
                    DatabasePool::Sqlite(_) => {
                        let source_path = PathBuf::from(context.connection.sqlite_path.trim());
                        if source_path.as_os_str().is_empty() {
                            self.export_cancel_flag = None;
                            self.export_stage = TransferStage::Configure;
                            self.export_progress = 0.0;
                            self.export_status.clear();
                            outputs.push(Output::Error(Some(ErrorModalState::new(
                                "Export error",
                                "Open a source SQLite database first.",
                                "The active SQLite connection path is empty.",
                            ))));
                            return Task::none();
                        }
                        Task::run(
                            export_sqlite_stream(source_path, export_path, cancel_flag),
                            Message::from,
                        )
                    }
                }
            }
            Message::CancelImport => {
                if let Some(flag) = &self.import_cancel_flag {
                    flag.store(true, Ordering::Relaxed);
                    self.import_status = String::from("Canceling import...");
                }
                Task::none()
            }
            Message::CancelExport => {
                if let Some(flag) = &self.export_cancel_flag {
                    flag.store(true, Ordering::Relaxed);
                    self.export_status = String::from("Canceling export...");
                }
                Task::none()
            }
            Message::ImportProgress { progress, status } => {
                if self.import_stage == TransferStage::Running {
                    self.import_progress = progress.clamp(0.0, 1.0);
                    if !status.is_empty() {
                        self.import_status = status;
                    }
                }
                Task::none()
            }
            Message::ExportProgress { progress, status } => {
                if self.export_stage == TransferStage::Running {
                    self.export_progress = progress.clamp(0.0, 1.0);
                    if !status.is_empty() {
                        self.export_status = status;
                    }
                }
                Task::none()
            }
            Message::ImportFinished(result) => {
                self.import_cancel_flag = None;
                match result {
                    Ok(summary) => {
                        self.import_stage = TransferStage::Completed;
                        self.import_progress = 1.0;
                        self.import_status = if context.connection.driver == DatabaseDriver::Sqlite
                        {
                            crate::i18n::tr_with(
                                "Imported SQLite file ({size}) in {time}.",
                                &[
                                    ("{size}", &format_bytes(summary.statements as u64)),
                                    ("{time}", &format_duration(summary.duration)),
                                ],
                            )
                        } else if context.connection.driver == DatabaseDriver::PostgreSql
                            && summary.statements == 0
                        {
                            crate::i18n::tr_with(
                                "Import completed in {time}.",
                                &[("{time}", &format_duration(summary.duration))],
                            )
                        } else {
                            crate::i18n::tr_with(
                                "Imported {count} statements in {time}.",
                                &[
                                    ("{count}", &summary.statements.to_string()),
                                    ("{time}", &format_duration(summary.duration)),
                                ],
                            )
                        };
                    }
                    Err(TransferError::Cancelled) => {
                        self.import_stage = TransferStage::Completed;
                        if self.import_status.is_empty()
                            || self.import_status == "Canceling import..."
                        {
                            self.import_status = String::from("Import cancelled.");
                        }
                    }
                    Err(TransferError::Failed(error)) => {
                        self.import_stage = TransferStage::Configure;
                        self.import_progress = 0.0;
                        self.import_status.clear();
                        outputs.push(Output::Error(Some(ErrorModalState::new(
                            "Import error",
                            "Import failed.",
                            error,
                        ))));
                    }
                }
                Task::none()
            }
            Message::ExportFinished(result) => {
                self.export_cancel_flag = None;
                match result {
                    Ok(summary) => {
                        self.export_stage = TransferStage::Completed;
                        self.export_progress = 1.0;
                        self.export_status = if context.connection.driver == DatabaseDriver::Sqlite
                        {
                            crate::i18n::tr_with(
                                "Exported SQLite file ({size}) in {time}.",
                                &[
                                    ("{size}", &format_bytes(summary.bytes_written)),
                                    ("{time}", &format_duration(summary.duration)),
                                ],
                            )
                        } else if context.connection.driver == DatabaseDriver::PostgreSql
                            && summary.rows == 0
                        {
                            crate::i18n::tr_with(
                                "Exported {count} objects ({size}) in {time}.",
                                &[
                                    ("{count}", &summary.tables.to_string()),
                                    ("{size}", &format_bytes(summary.bytes_written)),
                                    ("{time}", &format_duration(summary.duration)),
                                ],
                            )
                        } else {
                            crate::i18n::tr_with(
                                "Exported {count} tables ({rows} rows, {size}) in {time}.",
                                &[
                                    ("{count}", &summary.tables.to_string()),
                                    ("{rows}", &summary.rows.to_string()),
                                    ("{size}", &format_bytes(summary.bytes_written)),
                                    ("{time}", &format_duration(summary.duration)),
                                ],
                            )
                        };
                    }
                    Err(TransferError::Cancelled) => {
                        self.export_stage = TransferStage::Completed;
                        if self.export_status.is_empty()
                            || self.export_status == "Canceling export..."
                        {
                            self.export_status = String::from("Export cancelled.");
                        }
                    }
                    Err(TransferError::Failed(error)) => {
                        self.export_stage = TransferStage::Configure;
                        self.export_progress = 0.0;
                        self.export_status.clear();
                        outputs.push(Output::Error(Some(ErrorModalState::new(
                            "Export error",
                            "Export failed.",
                            error,
                        ))));
                    }
                }
                Task::none()
            }
            Message::ExportTablesLoaded(result) => {
                self.export_tables_loading = false;
                match result {
                    Ok(mut tables) => {
                        tables.sort();
                        self.export_tables = tables;
                        self.export_tables_error = None;
                        self.export_excluded_tables
                            .retain(|table| self.export_tables.iter().any(|name| name == table));
                    }
                    Err(error) => {
                        self.export_tables.clear();
                        self.export_tables_error = Some(error);
                    }
                }
                Task::none()
            }
            Message::ToggleTransferMenu => {
                self.transfer_menu_open = !self.transfer_menu_open;
                Task::none()
            }
            Message::CloseTransferMenu => {
                self.transfer_menu_open = false;
                Task::none()
            }
        }
    }

    pub(crate) fn export_visible_tables(&self) -> Vec<String> {
        let filter = self.export_table_search.trim().to_ascii_lowercase();
        self.export_tables
            .iter()
            .filter(|table| {
                if filter.is_empty() {
                    true
                } else {
                    table.to_ascii_lowercase().contains(&filter)
                }
            })
            .cloned()
            .collect()
    }

    pub(crate) fn export_selected_tables(&self) -> Vec<String> {
        self.export_tables
            .iter()
            .filter(|table| !self.export_excluded_tables.contains(*table))
            .cloned()
            .collect()
    }

    pub(crate) fn export_selected_table_count(&self) -> usize {
        self.export_tables
            .iter()
            .filter(|table| !self.export_excluded_tables.contains(*table))
            .count()
    }

    pub(crate) fn refresh_export_tables(&mut self, pool: Option<&DatabasePool>) -> Task<Message> {
        let Some(pool) = pool.cloned() else {
            self.export_tables_error = Some(String::from("Not connected."));
            self.export_tables_loading = false;
            return Task::none();
        };
        if matches!(pool, DatabasePool::Sqlite(_)) {
            self.export_tables_error = Some(String::from("Export is unavailable for SQLite."));
            self.export_tables_loading = false;
            return Task::none();
        }
        let Some(database) = self.export_database.clone() else {
            self.export_tables_error = Some(String::from("Select a database to export."));
            self.export_tables_loading = false;
            return Task::none();
        };
        self.export_tables_loading = true;
        self.export_tables_error = None;
        self.export_tables.clear();
        Task::perform(fetch_tables(pool, database), Message::ExportTablesLoaded)
    }
}

#[cfg(test)]
mod tests;
