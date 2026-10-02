use crate::constants::EXPORT_INSERT_BATCH_SIZE;
use crate::db::codecs::sql_literal_from_row;
use crate::db::connect::connect_sqlite_profile;
use crate::db::query::query_returns_rows;
use crate::model::connection::{ConnectionInfo, DatabaseDriver};
use crate::model::table::ColumnSpec;
use crate::model::transfer::{
    ExportOptions, ExportSummary, ImportOptions, ImportSummary, InnoDbVersion, ProgressThrottle,
    TransferError,
};
use crate::utils::helpers::{
    column_kind_from_type_name, escape_mysql_identifier, escape_sqlite_identifier,
    escape_sqlite_string_literal, sql_quote_table_reference,
};
use futures_util::{StreamExt, TryStreamExt};
use tokio::sync::mpsc;

use sqlx::mysql::MySqlRow;
use sqlx::{Acquire, AssertSqlSafe, Executor, MySqlConnection, MySqlPool, Row, SqliteConnection};
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{ChildStderr, Command};
use tokio::sync::mpsc as tokio_mpsc;
use tokio::time::{Duration as TokioDuration, sleep};

#[derive(Debug, Clone)]
pub(crate) enum Event {
    ImportProgress { progress: f32, status: String },
    ExportProgress { progress: f32, status: String },
    ImportFinished(Result<ImportSummary, TransferError>),
    ExportFinished(Result<ExportSummary, TransferError>),
}

fn event_stream<F, Fut>(capacity: usize, worker: F) -> impl futures_util::Stream<Item = Event>
where
    F: FnOnce(mpsc::Sender<Event>) -> Fut,
    Fut: std::future::Future<Output = ()>,
{
    let (sender, receiver) = mpsc::channel(capacity);
    let events = futures_util::stream::unfold(receiver, |mut receiver| async {
        receiver.recv().await.map(|event| (event, receiver))
    });
    let work = futures_util::stream::once(worker(sender)).filter_map(|()| async { None });
    futures_util::stream::select(events, work)
}

struct TempPgPassFile {
    file: tempfile::NamedTempFile,
}

impl TempPgPassFile {
    fn path(&self) -> &std::path::Path {
        self.file.path()
    }
}

async fn create_pgpass_file(
    connection: &ConnectionInfo,
) -> Result<Option<TempPgPassFile>, TransferError> {
    let password = connection.password.trim();
    if password.is_empty() {
        return Ok(None);
    }

    let host = if connection.host.trim().is_empty() {
        "localhost"
    } else {
        connection.host.trim()
    };
    let port = postgres_port_or_default(connection);
    let username = connection.username.trim();
    if username.is_empty() {
        return Ok(None);
    }

    let escaped_password = password.replace('\\', r"\\").replace(':', r"\:");
    let entry = format!("{host}:{port}:*:{username}:{escaped_password}\n");

    let file =
        tempfile::NamedTempFile::new().map_err(|error| TransferError::Failed(error.to_string()))?;
    let path = file.path().to_path_buf();

    tokio::fs::write(&path, entry)
        .await
        .map_err(|error| TransferError::Failed(error.to_string()))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let permissions = std::fs::Permissions::from_mode(0o600);
        std::fs::set_permissions(&path, permissions)
            .map_err(|error| TransferError::Failed(error.to_string()))?;
    }

    Ok(Some(TempPgPassFile { file }))
}

pub(crate) fn import_stream(
    pool: MySqlPool,
    path: PathBuf,
    database: String,
    options: ImportOptions,
    cancel: Arc<AtomicBool>,
) -> impl futures_util::Stream<Item = Event> {
    event_stream(100, move |mut output| async move {
        let result =
            import_database_worker(pool, path, database, options, cancel, &mut output).await;
        let _ = output.send(Event::ImportFinished(result)).await;
    })
}

pub(crate) fn export_stream(
    pool: MySqlPool,
    path: PathBuf,
    database: String,
    tables: Vec<String>,
    options: ExportOptions,
    cancel: Arc<AtomicBool>,
) -> impl futures_util::Stream<Item = Event> {
    event_stream(100, move |mut output| async move {
        let result =
            export_database_worker(pool, path, database, tables, options, cancel, &mut output)
                .await;
        let _ = output.send(Event::ExportFinished(result)).await;
    })
}

pub(crate) fn import_postgres_stream(
    connection: ConnectionInfo,
    path: PathBuf,
    database: String,
    options: ImportOptions,
    cancel: Arc<AtomicBool>,
) -> impl futures_util::Stream<Item = Event> {
    event_stream(100, move |mut output| async move {
        let result =
            import_postgres_worker(connection, path, database, options, cancel, &mut output).await;
        let _ = output.send(Event::ImportFinished(result)).await;
    })
}

pub(crate) fn export_postgres_stream(
    connection: ConnectionInfo,
    path: PathBuf,
    database: String,
    tables: Vec<String>,
    options: ExportOptions,
    cancel: Arc<AtomicBool>,
) -> impl futures_util::Stream<Item = Event> {
    event_stream(100, move |mut output| async move {
        let result = export_postgres_worker(
            connection,
            path,
            database,
            tables,
            options,
            cancel,
            &mut output,
        )
        .await;
        let _ = output.send(Event::ExportFinished(result)).await;
    })
}

fn postgres_port_or_default(connection: &ConnectionInfo) -> String {
    let trimmed = connection.port.trim();
    if trimmed.is_empty() {
        String::from("5432")
    } else {
        trimmed.to_string()
    }
}

fn apply_postgres_connection_args(command: &mut Command, connection: &ConnectionInfo) {
    let host = connection.host.trim();
    if !host.is_empty() {
        command.arg("--host").arg(host);
    }

    command
        .arg("--port")
        .arg(postgres_port_or_default(connection));

    let username = connection.username.trim();
    if !username.is_empty() {
        command.arg("--username").arg(username);
    }

    command.arg("--no-password");
}

fn postgres_tool_ssl_mode(tls_mode: crate::TlsMode) -> &'static str {
    match tls_mode {
        crate::TlsMode::Disabled => "disable",
        crate::TlsMode::Prefer => "prefer",
        crate::TlsMode::Require => "require",
        crate::TlsMode::VerifyCa => "verify-ca",
        crate::TlsMode::VerifyFull => "verify-full",
    }
}

fn postgres_connection_env(connection: &ConnectionInfo) -> Vec<(String, String)> {
    let mut env = vec![(
        String::from("PGSSLMODE"),
        postgres_tool_ssl_mode(connection.tls_mode).to_string(),
    )];

    let ca_cert_path = connection.tls_ca_cert_path.trim();
    if !ca_cert_path.is_empty() {
        env.push((String::from("PGSSLROOTCERT"), ca_cert_path.to_string()));
    }
    let client_cert_path = connection.tls_client_cert_path.trim();
    if !client_cert_path.is_empty() {
        env.push((String::from("PGSSLCERT"), client_cert_path.to_string()));
    }
    let client_key_path = connection.tls_client_key_path.trim();
    if !client_key_path.is_empty() {
        env.push((String::from("PGSSLKEY"), client_key_path.to_string()));
    }

    env
}

fn apply_postgres_connection_env(command: &mut Command, connection: &ConnectionInfo) {
    for (key, value) in postgres_connection_env(connection) {
        command.env(key, value);
    }
}

fn build_createdb_args(database: &str) -> Vec<String> {
    vec![database.trim().to_string()]
}

fn build_psql_args(database: &str, path: &Path, options: &ImportOptions) -> Vec<String> {
    let mut args = vec![
        String::from("--dbname"),
        database.trim().to_string(),
        String::from("--file"),
        path.display().to_string(),
        String::from("--set"),
        String::from("ON_ERROR_STOP=1"),
    ];

    if options.use_transaction {
        args.push(String::from("--single-transaction"));
    }

    args
}

fn build_pg_dump_args(
    database: &str,
    path: &Path,
    tables: &[String],
    options: &ExportOptions,
) -> Result<Vec<String>, TransferError> {
    let mut args = vec![
        String::from("--dbname"),
        database.trim().to_string(),
        String::from("--format=plain"),
        String::from("--file"),
        path.display().to_string(),
        String::from("--no-owner"),
        String::from("--no-privileges"),
    ];

    match (options.include_create, options.include_inserts) {
        (true, true) => {}
        (true, false) => args.push(String::from("--schema-only")),
        (false, true) => args.push(String::from("--data-only")),
        (false, false) => {
            return Err(TransferError::Failed(String::from(
                "Enable at least one export mode (schema or data).",
            )));
        }
    }

    if options.include_drop {
        args.push(String::from("--clean"));
        args.push(String::from("--if-exists"));
    }

    for table in tables {
        args.push(String::from("--table"));
        args.push(sql_quote_table_reference(DatabaseDriver::PostgreSql, table));
    }

    Ok(args)
}

fn spawn_postgres_tool(tool: &str) -> Result<Command, TransferError> {
    if tool.trim().is_empty() {
        return Err(TransferError::Failed(String::from(
            "PostgreSQL transfer tool name is empty.",
        )));
    }
    Ok(Command::new(tool))
}

fn map_postgres_tool_spawn_error(tool: &str, error: std::io::Error) -> TransferError {
    if error.kind() == ErrorKind::NotFound {
        TransferError::Failed(format!(
            "`{tool}` is not installed or not in PATH. Install PostgreSQL client tools (psql/pg_dump) and try again."
        ))
    } else {
        TransferError::Failed(error.to_string())
    }
}

fn spawn_stderr_reader(stderr: Option<ChildStderr>) -> tokio_mpsc::UnboundedReceiver<String> {
    let (tx, rx) = tokio_mpsc::unbounded_channel();
    if let Some(stderr) = stderr {
        tokio::spawn(async move {
            let mut lines = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                if tx.send(line).is_err() {
                    break;
                }
            }
        });
    }
    rx
}

async fn ensure_postgres_database_created_if_requested(
    connection: &ConnectionInfo,
    database: &str,
) -> Result<(), TransferError> {
    let pgpass = create_pgpass_file(connection).await?;
    let mut command = spawn_postgres_tool("createdb")?;
    apply_postgres_connection_args(&mut command, connection);
    apply_postgres_connection_env(&mut command, connection);
    if let Some(file) = pgpass.as_ref() {
        command.env("PGPASSFILE", file.path());
    }
    for arg in build_createdb_args(database) {
        command.arg(arg);
    }
    command.stdout(Stdio::null()).stderr(Stdio::piped());

    let output = command
        .output()
        .await
        .map_err(|error| map_postgres_tool_spawn_error("createdb", error))?;

    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if stderr.to_ascii_lowercase().contains("already exists") {
        return Ok(());
    }

    Err(TransferError::Failed(if stderr.is_empty() {
        format!("`createdb` failed with status {}.", output.status)
    } else {
        format!("`createdb` failed: {stderr}")
    }))
}

async fn export_postgres_worker(
    connection: ConnectionInfo,
    path: PathBuf,
    database: String,
    tables: Vec<String>,
    options: ExportOptions,
    cancel: Arc<AtomicBool>,
    output: &mut mpsc::Sender<Event>,
) -> Result<ExportSummary, TransferError> {
    let start = Instant::now();

    if cancel.load(Ordering::Relaxed) {
        return Err(TransferError::Cancelled);
    }

    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|error| TransferError::Failed(error.to_string()))?;
    }

    let pgpass = create_pgpass_file(&connection).await?;
    let mut command = spawn_postgres_tool("pg_dump")?;
    apply_postgres_connection_args(&mut command, &connection);
    apply_postgres_connection_env(&mut command, &connection);
    if let Some(file) = pgpass.as_ref() {
        command.env("PGPASSFILE", file.path());
    }
    for arg in build_pg_dump_args(database.trim(), &path, &tables, &options)? {
        command.arg(arg);
    }

    command.stdout(Stdio::null()).stderr(Stdio::piped());

    let mut child = command
        .spawn()
        .map_err(|error| map_postgres_tool_spawn_error("pg_dump", error))?;
    let mut stderr = spawn_stderr_reader(child.stderr.take());

    let mut last_stderr = String::new();
    let mut throttle = ProgressThrottle::new();
    let mut progress = 0.05_f32;

    loop {
        while let Ok(line) = stderr.try_recv() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            last_stderr = trimmed.to_string();
            let _ = output
                .send(Event::ExportProgress {
                    progress,
                    status: format!("pg_dump: {trimmed}"),
                })
                .await;
        }

        if cancel.load(Ordering::Relaxed) {
            let _ = child.kill().await;
            let _ = child.wait().await;
            return Err(TransferError::Cancelled);
        }

        if let Some(status) = child
            .try_wait()
            .map_err(|error| TransferError::Failed(error.to_string()))?
        {
            if !status.success() {
                let detail = if last_stderr.is_empty() {
                    format!("`pg_dump` exited with status {status}.")
                } else {
                    format!("`pg_dump` failed: {last_stderr}")
                };
                return Err(TransferError::Failed(detail));
            }
            break;
        }

        if throttle.should_emit(progress) {
            let status = if last_stderr.is_empty() {
                String::from("Exporting with pg_dump...")
            } else {
                format!("pg_dump: {last_stderr}")
            };
            let _ = output
                .send(Event::ExportProgress { progress, status })
                .await;
        }

        progress = (progress + 0.02).min(0.92);
        sleep(TokioDuration::from_millis(220)).await;
    }

    let bytes_written = tokio::fs::metadata(&path)
        .await
        .map(|metadata| metadata.len())
        .unwrap_or(0);

    let _ = output
        .send(Event::ExportProgress {
            progress: 0.98,
            status: String::from("Finalizing export..."),
        })
        .await;

    Ok(ExportSummary {
        tables: tables.len(),
        rows: 0,
        duration: start.elapsed(),
        bytes_written,
    })
}

async fn import_postgres_worker(
    connection: ConnectionInfo,
    path: PathBuf,
    database: String,
    options: ImportOptions,
    cancel: Arc<AtomicBool>,
    output: &mut mpsc::Sender<Event>,
) -> Result<ImportSummary, TransferError> {
    let start = Instant::now();

    if cancel.load(Ordering::Relaxed) {
        return Err(TransferError::Cancelled);
    }

    if !path.is_file() {
        return Err(TransferError::Failed(format!(
            "Import file `{}` does not exist.",
            path.display()
        )));
    }

    if options.create_database {
        let _ = output
            .send(Event::ImportProgress {
                progress: 0.02,
                status: String::from("Ensuring target database exists..."),
            })
            .await;
        ensure_postgres_database_created_if_requested(&connection, database.trim()).await?;
    }

    let pgpass = create_pgpass_file(&connection).await?;
    let mut command = spawn_postgres_tool("psql")?;
    apply_postgres_connection_args(&mut command, &connection);
    apply_postgres_connection_env(&mut command, &connection);
    if let Some(file) = pgpass.as_ref() {
        command.env("PGPASSFILE", file.path());
    }
    for arg in build_psql_args(database.trim(), &path, &options) {
        command.arg(arg);
    }

    command.stdout(Stdio::null()).stderr(Stdio::piped());

    let mut child = command
        .spawn()
        .map_err(|error| map_postgres_tool_spawn_error("psql", error))?;
    let mut stderr = spawn_stderr_reader(child.stderr.take());

    let mut last_stderr = String::new();
    let mut throttle = ProgressThrottle::new();
    let mut progress = 0.05_f32;

    loop {
        while let Ok(line) = stderr.try_recv() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            last_stderr = trimmed.to_string();
            let _ = output
                .send(Event::ImportProgress {
                    progress,
                    status: format!("psql: {trimmed}"),
                })
                .await;
        }

        if cancel.load(Ordering::Relaxed) {
            let _ = child.kill().await;
            let _ = child.wait().await;
            return Err(TransferError::Cancelled);
        }

        if let Some(status) = child
            .try_wait()
            .map_err(|error| TransferError::Failed(error.to_string()))?
        {
            if !status.success() {
                let detail = if last_stderr.is_empty() {
                    format!("`psql` exited with status {status}.")
                } else {
                    format!("`psql` failed: {last_stderr}")
                };
                return Err(TransferError::Failed(detail));
            }
            break;
        }

        if throttle.should_emit(progress) {
            let status = if last_stderr.is_empty() {
                String::from("Importing with psql...")
            } else {
                format!("psql: {last_stderr}")
            };
            let _ = output
                .send(Event::ImportProgress { progress, status })
                .await;
        }

        progress = (progress + 0.02).min(0.92);
        sleep(TokioDuration::from_millis(220)).await;
    }

    let _ = output
        .send(Event::ImportProgress {
            progress: 0.98,
            status: String::from("Finalizing import..."),
        })
        .await;

    Ok(ImportSummary {
        statements: 0,
        duration: start.elapsed(),
    })
}

struct StatementParser {
    delimiter: String,
    buffer: String,
    in_single: bool,
    in_double: bool,
    in_backtick: bool,
    in_block_comment: bool,
    escape_next: bool,
}

impl StatementParser {
    fn new() -> Self {
        Self {
            delimiter: String::from(";"),
            buffer: String::new(),
            in_single: false,
            in_double: false,
            in_backtick: false,
            in_block_comment: false,
            escape_next: false,
        }
    }

    fn can_change_delimiter(&self) -> bool {
        !self.in_single && !self.in_double && !self.in_backtick && !self.in_block_comment
    }

    fn push_line(&mut self, line: &str) -> Vec<String> {
        let mut statements = Vec::new();
        if self.can_change_delimiter() {
            let trimmed = line.trim_start();
            let mut parts = trimmed.split_whitespace();
            if let Some(first) = parts.next()
                && first.eq_ignore_ascii_case("delimiter")
            {
                if let Some(next) = parts.next() {
                    self.delimiter = next.to_string();
                }
                return statements;
            }
        }

        let delimiter_len = self.delimiter.chars().count();
        let mut last_was_space = true;
        let mut iter = line.char_indices().peekable();

        while let Some((idx, ch)) = iter.peek().copied() {
            if self.in_block_comment {
                iter.next();
                if ch == '*'
                    && let Some((_, next)) = iter.peek()
                    && *next == '/'
                {
                    iter.next();
                    self.in_block_comment = false;
                }
                continue;
            }

            if self.in_single {
                iter.next();
                self.buffer.push(ch);
                if self.escape_next {
                    self.escape_next = false;
                } else if ch == '\\' {
                    self.escape_next = true;
                } else if ch == '\'' {
                    self.in_single = false;
                }
                last_was_space = ch.is_whitespace();
                continue;
            }

            if self.in_double {
                iter.next();
                self.buffer.push(ch);
                if self.escape_next {
                    self.escape_next = false;
                } else if ch == '\\' {
                    self.escape_next = true;
                } else if ch == '"' {
                    self.in_double = false;
                }
                last_was_space = ch.is_whitespace();
                continue;
            }

            if self.in_backtick {
                iter.next();
                self.buffer.push(ch);
                if ch == '`' {
                    self.in_backtick = false;
                }
                last_was_space = ch.is_whitespace();
                continue;
            }

            if line[idx..].starts_with(&self.delimiter) {
                for _ in 0..delimiter_len {
                    iter.next();
                }
                let statement = self.buffer.trim();
                if !statement.is_empty() {
                    statements.push(statement.to_string());
                }
                self.buffer.clear();
                last_was_space = true;
                continue;
            }

            if ch == '#' && last_was_space {
                break;
            }

            if ch == '-' && last_was_space {
                let mut lookahead = iter.clone();
                lookahead.next();
                if let Some((_, next)) = lookahead.next()
                    && next == '-'
                {
                    let after = lookahead.next().map(|(_, c)| c);
                    if after.is_none_or(|c| c.is_whitespace()) {
                        break;
                    }
                }
            }

            if ch == '/' {
                let mut lookahead = iter.clone();
                lookahead.next();
                if let Some((_, next)) = lookahead.next()
                    && next == '*'
                {
                    let is_versioned = lookahead.next().map(|(_, c)| c == '!').unwrap_or(false);
                    if !is_versioned {
                        iter.next();
                        iter.next();
                        self.in_block_comment = true;
                        continue;
                    }
                }
            }

            if ch == '\'' {
                self.in_single = true;
                self.escape_next = false;
                iter.next();
                self.buffer.push(ch);
                last_was_space = false;
                continue;
            }

            if ch == '"' {
                self.in_double = true;
                self.escape_next = false;
                iter.next();
                self.buffer.push(ch);
                last_was_space = false;
                continue;
            }

            if ch == '`' {
                self.in_backtick = true;
                iter.next();
                self.buffer.push(ch);
                last_was_space = false;
                continue;
            }

            iter.next();
            self.buffer.push(ch);
            last_was_space = ch.is_whitespace();
        }

        statements
    }

    fn finish(&mut self) -> Option<String> {
        let statement = self.buffer.trim();
        if statement.is_empty() {
            None
        } else {
            let result = Some(statement.to_string());
            self.buffer.clear();
            result
        }
    }
}

async fn import_database_worker(
    pool: MySqlPool,
    path: PathBuf,
    database: String,
    options: ImportOptions,
    cancel: Arc<AtomicBool>,
    output: &mut mpsc::Sender<Event>,
) -> Result<ImportSummary, TransferError> {
    let start = Instant::now();
    let mut conn = pool
        .acquire()
        .await
        .map_err(|error| TransferError::Failed(error.to_string()))?;

    if cancel.load(Ordering::Relaxed) {
        return Err(TransferError::Cancelled);
    }

    if options.create_database {
        let create_stmt = format!(
            "CREATE DATABASE IF NOT EXISTS `{}`",
            escape_mysql_identifier(database.trim())
        );
        (&mut *conn)
            .execute(AssertSqlSafe(create_stmt))
            .await
            .map_err(|error| TransferError::Failed(error.to_string()))?;
    }

    let use_stmt = format!("USE `{}`", escape_mysql_identifier(database.trim()));
    (&mut *conn)
        .execute(AssertSqlSafe(use_stmt))
        .await
        .map_err(|error| TransferError::Failed(error.to_string()))?;

    let original_settings = fetch_mysql_import_session_settings(&mut conn).await?;
    let mut total_bytes = 1u64;
    let mut bytes_read = 0u64;
    let import_result: Result<ImportSummary, TransferError> = 'import: {
        if let Err(error) = apply_innodb_settings(&mut conn, options.innodb_version)
            .await
            .map_err(TransferError::Failed)
        {
            break 'import Err(error);
        }

        if options.drop_existing
            && let Err(error) = drop_database_objects(&mut conn, database.trim()).await
        {
            break 'import Err(error);
        }

        if options.disable_foreign_keys
            && let Err(error) = (&mut *conn)
                .execute("SET FOREIGN_KEY_CHECKS = 0")
                .await
                .map_err(|error| TransferError::Failed(error.to_string()))
        {
            break 'import Err(error);
        }

        let metadata = match tokio::fs::metadata(&path).await {
            Ok(metadata) => metadata,
            Err(error) => break 'import Err(TransferError::Failed(error.to_string())),
        };
        total_bytes = metadata.len().max(1);
        let file = match tokio::fs::File::open(&path).await {
            Ok(file) => file,
            Err(error) => break 'import Err(TransferError::Failed(error.to_string())),
        };
        let mut reader = BufReader::new(file);
        let mut parser = StatementParser::new();
        let mut buffer = String::new();
        let mut statements = 0usize;
        let mut throttle = ProgressThrottle::new();

        if options.use_transaction {
            let mut tx = match conn
                .begin()
                .await
                .map_err(|error| TransferError::Failed(error.to_string()))
            {
                Ok(tx) => tx,
                Err(error) => break 'import Err(error),
            };

            'transaction: {
                loop {
                    if cancel.load(Ordering::Relaxed) {
                        let _ = tx.rollback().await;
                        break 'transaction Err(TransferError::Cancelled);
                    }

                    buffer.clear();
                    let read = match reader.read_line(&mut buffer).await {
                        Ok(read) => read,
                        Err(error) => {
                            let _ = tx.rollback().await;
                            break 'transaction Err(TransferError::Failed(error.to_string()));
                        }
                    };
                    if read == 0 {
                        break;
                    }

                    bytes_read += read as u64;
                    let statements_batch = parser.push_line(&buffer);
                    for statement in statements_batch {
                        if cancel.load(Ordering::Relaxed) {
                            let _ = tx.rollback().await;
                            break 'transaction Err(TransferError::Cancelled);
                        }
                        if should_skip_import_statement(&statement) {
                            continue;
                        }
                        if let Err(error) = execute_statement(&mut tx, &statement).await {
                            let _ = tx.rollback().await;
                            break 'transaction Err(error);
                        }
                        statements += 1;
                        let progress = (bytes_read as f32 / total_bytes as f32).clamp(0.0, 1.0);
                        if throttle.should_emit(progress) {
                            let status = format!(
                                "Importing... {statements} statements ({:.0}% read)",
                                progress * 100.0
                            );
                            let _ = output
                                .send(Event::ImportProgress { progress, status })
                                .await;
                        }
                    }
                }

                if let Some(statement) = parser.finish()
                    && !should_skip_import_statement(&statement)
                {
                    if let Err(error) = execute_statement(&mut tx, &statement).await {
                        let _ = tx.rollback().await;
                        break 'transaction Err(error);
                    }
                    statements += 1;
                }

                if let Err(error) = tx
                    .commit()
                    .await
                    .map_err(|error| TransferError::Failed(error.to_string()))
                {
                    break 'transaction Err(error);
                }

                Ok(ImportSummary {
                    statements,
                    duration: start.elapsed(),
                })
            }
        } else {
            'plain: {
                loop {
                    if cancel.load(Ordering::Relaxed) {
                        break 'plain Err(TransferError::Cancelled);
                    }

                    buffer.clear();
                    let read = match reader.read_line(&mut buffer).await {
                        Ok(read) => read,
                        Err(error) => {
                            break 'plain Err(TransferError::Failed(error.to_string()));
                        }
                    };
                    if read == 0 {
                        break;
                    }

                    bytes_read += read as u64;
                    let statements_batch = parser.push_line(&buffer);
                    for statement in statements_batch {
                        if cancel.load(Ordering::Relaxed) {
                            break 'plain Err(TransferError::Cancelled);
                        }
                        if should_skip_import_statement(&statement) {
                            continue;
                        }
                        if let Err(error) = execute_statement(&mut conn, &statement).await {
                            break 'plain Err(error);
                        }
                        statements += 1;
                        let progress = (bytes_read as f32 / total_bytes as f32).clamp(0.0, 1.0);
                        if throttle.should_emit(progress) {
                            let status = format!(
                                "Importing... {statements} statements ({:.0}% read)",
                                progress * 100.0
                            );
                            let _ = output
                                .send(Event::ImportProgress { progress, status })
                                .await;
                        }
                    }
                }

                if let Some(statement) = parser.finish()
                    && !should_skip_import_statement(&statement)
                {
                    if let Err(error) = execute_statement(&mut conn, &statement).await {
                        break 'plain Err(error);
                    }
                    statements += 1;
                }

                Ok(ImportSummary {
                    statements,
                    duration: start.elapsed(),
                })
            }
        }
    };

    let summary = finish_mysql_import_attempt(import_result, || async {
        restore_mysql_import_session_settings(&mut conn, &original_settings).await
    })
    .await?;

    let progress = (bytes_read as f32 / total_bytes as f32).clamp(0.0, 1.0);
    let _ = output
        .send(Event::ImportProgress {
            progress,
            status: String::from("Finalizing import..."),
        })
        .await;

    Ok(summary)
}

struct MySqlImportSessionSettings {
    foreign_key_checks: i64,
    sql_mode: String,
    innodb_strict_mode: i64,
}

async fn fetch_mysql_import_session_settings(
    conn: &mut MySqlConnection,
) -> Result<MySqlImportSessionSettings, TransferError> {
    let foreign_key_checks: i64 = sqlx::query_scalar("SELECT @@SESSION.foreign_key_checks")
        .fetch_one(&mut *conn)
        .await
        .map_err(|error| TransferError::Failed(error.to_string()))?;
    let sql_mode: String = sqlx::query_scalar("SELECT @@SESSION.sql_mode")
        .fetch_one(&mut *conn)
        .await
        .map_err(|error| TransferError::Failed(error.to_string()))?;
    let innodb_strict_mode: i64 = sqlx::query_scalar("SELECT @@SESSION.innodb_strict_mode")
        .fetch_one(&mut *conn)
        .await
        .map_err(|error| TransferError::Failed(error.to_string()))?;
    Ok(MySqlImportSessionSettings {
        foreign_key_checks,
        sql_mode,
        innodb_strict_mode,
    })
}

async fn restore_mysql_import_session_settings(
    conn: &mut MySqlConnection,
    settings: &MySqlImportSessionSettings,
) -> Result<(), TransferError> {
    let foreign_key_stmt = format!(
        "SET SESSION FOREIGN_KEY_CHECKS = {}",
        if settings.foreign_key_checks == 0 {
            0
        } else {
            1
        }
    );
    (&mut *conn)
        .execute(AssertSqlSafe(foreign_key_stmt))
        .await
        .map_err(|error| TransferError::Failed(error.to_string()))?;

    let sql_mode_stmt = format!(
        "SET SESSION sql_mode = {}",
        crate::sql_quote_string_literal(crate::DatabaseDriver::MySql, &settings.sql_mode)
    );
    (&mut *conn)
        .execute(AssertSqlSafe(sql_mode_stmt))
        .await
        .map_err(|error| TransferError::Failed(error.to_string()))?;

    let strict_stmt = format!(
        "SET SESSION innodb_strict_mode = {}",
        if settings.innodb_strict_mode == 0 {
            0
        } else {
            1
        }
    );
    (&mut *conn)
        .execute(AssertSqlSafe(strict_stmt))
        .await
        .map_err(|error| TransferError::Failed(error.to_string()))?;
    Ok(())
}

async fn finish_mysql_import_attempt<T, Restore, RestoreFuture>(
    import_result: Result<T, TransferError>,
    restore: Restore,
) -> Result<T, TransferError>
where
    Restore: FnOnce() -> RestoreFuture,
    RestoreFuture: std::future::Future<Output = Result<(), TransferError>>,
{
    let restore_result = restore().await;
    if import_result.is_ok() {
        restore_result?;
    }
    import_result
}

pub(crate) fn import_sqlite_stream(
    source_path: PathBuf,
    target_path: PathBuf,
    options: ImportOptions,
    cancel: Arc<AtomicBool>,
) -> impl futures_util::Stream<Item = Event> {
    event_stream(32, move |mut output| async move {
        let result = if sqlite_path_uses_sql_script(&source_path) {
            sqlite_script_import_worker(source_path, target_path, options, cancel, &mut output)
                .await
        } else {
            sqlite_copy_worker(source_path, target_path, cancel, true, &mut output).await
        };
        let _ = output.send(Event::ImportFinished(result)).await;
    })
}

pub(crate) fn export_sqlite_stream(
    source_path: PathBuf,
    target_path: PathBuf,
    cancel: Arc<AtomicBool>,
) -> impl futures_util::Stream<Item = Event> {
    event_stream(32, move |mut output| async move {
        let result = if sqlite_path_uses_sql_script(&target_path) {
            sqlite_export_sql_worker(source_path, target_path, cancel, &mut output).await
        } else {
            sqlite_export_copy_worker(source_path, target_path, cancel, &mut output).await
        };
        let _ = output.send(Event::ExportFinished(result)).await;
    })
}

fn sqlite_path_uses_sql_script(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("sql"))
}

async fn sqlite_copy_worker(
    source_path: PathBuf,
    target_path: PathBuf,
    cancel: Arc<AtomicBool>,
    is_import: bool,
    output: &mut mpsc::Sender<Event>,
) -> Result<ImportSummary, TransferError> {
    let start = Instant::now();
    if cancel.load(Ordering::Relaxed) {
        return Err(TransferError::Cancelled);
    }
    if !source_path.is_file() {
        return Err(TransferError::Failed(format!(
            "SQLite source file `{}` does not exist.",
            source_path.display()
        )));
    }
    if source_path == target_path {
        return Err(TransferError::Failed(String::from(
            "Source and destination SQLite files must be different.",
        )));
    }
    if let Some(parent) = target_path.parent()
        && !parent.as_os_str().is_empty()
    {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|error| TransferError::Failed(error.to_string()))?;
    }
    let status = if is_import {
        "Importing SQLite file..."
    } else {
        "Exporting SQLite file..."
    };
    let _ = if is_import {
        output
            .send(Event::ImportProgress {
                progress: 0.1,
                status: status.to_string(),
            })
            .await
    } else {
        output
            .send(Event::ExportProgress {
                progress: 0.1,
                status: status.to_string(),
            })
            .await
    };

    tokio::fs::copy(&source_path, &target_path)
        .await
        .map_err(|error| TransferError::Failed(error.to_string()))?;

    if cancel.load(Ordering::Relaxed) {
        let _ = tokio::fs::remove_file(&target_path).await;
        return Err(TransferError::Cancelled);
    }

    let bytes = tokio::fs::metadata(&target_path)
        .await
        .map(|metadata| metadata.len())
        .unwrap_or(0);

    let _ = if is_import {
        output
            .send(Event::ImportProgress {
                progress: 1.0,
                status: String::from("SQLite import completed."),
            })
            .await
    } else {
        output
            .send(Event::ExportProgress {
                progress: 1.0,
                status: String::from("SQLite export completed."),
            })
            .await
    };

    Ok(ImportSummary {
        statements: bytes as usize,
        duration: start.elapsed(),
    })
}

async fn sqlite_export_copy_worker(
    source_path: PathBuf,
    target_path: PathBuf,
    cancel: Arc<AtomicBool>,
    output: &mut mpsc::Sender<Event>,
) -> Result<ExportSummary, TransferError> {
    let start = Instant::now();
    if cancel.load(Ordering::Relaxed) {
        return Err(TransferError::Cancelled);
    }
    if !source_path.is_file() {
        return Err(TransferError::Failed(format!(
            "SQLite source file `{}` does not exist.",
            source_path.display()
        )));
    }
    if source_path == target_path {
        return Err(TransferError::Failed(String::from(
            "Source and destination SQLite files must be different.",
        )));
    }
    if let Some(parent) = target_path.parent()
        && !parent.as_os_str().is_empty()
    {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|error| TransferError::Failed(error.to_string()))?;
    }
    let _ = output
        .send(Event::ExportProgress {
            progress: 0.1,
            status: String::from("Exporting SQLite file..."),
        })
        .await;
    tokio::fs::copy(&source_path, &target_path)
        .await
        .map_err(|error| TransferError::Failed(error.to_string()))?;
    if cancel.load(Ordering::Relaxed) {
        let _ = tokio::fs::remove_file(&target_path).await;
        return Err(TransferError::Cancelled);
    }
    let bytes_written = tokio::fs::metadata(&target_path)
        .await
        .map(|metadata| metadata.len())
        .unwrap_or(0);
    let _ = output
        .send(Event::ExportProgress {
            progress: 1.0,
            status: String::from("SQLite export completed."),
        })
        .await;
    Ok(ExportSummary {
        tables: 1,
        rows: 0,
        duration: start.elapsed(),
        bytes_written,
    })
}

struct SqliteSchemaObject {
    kind: String,
    name: String,
    sql: String,
}

async fn sqlite_script_import_worker(
    source_path: PathBuf,
    target_path: PathBuf,
    options: ImportOptions,
    cancel: Arc<AtomicBool>,
    output: &mut mpsc::Sender<Event>,
) -> Result<ImportSummary, TransferError> {
    let start = Instant::now();
    if cancel.load(Ordering::Relaxed) {
        return Err(TransferError::Cancelled);
    }
    if !source_path.is_file() {
        return Err(TransferError::Failed(format!(
            "SQLite SQL source file `{}` does not exist.",
            source_path.display()
        )));
    }

    let (pool, _) = connect_sqlite_profile(target_path.display().to_string())
        .await
        .map_err(TransferError::Failed)?;
    let mut conn = pool
        .acquire()
        .await
        .map_err(|error| TransferError::Failed(error.to_string()))?;

    let metadata = tokio::fs::metadata(&source_path)
        .await
        .map_err(|error| TransferError::Failed(error.to_string()))?;
    let total_bytes = metadata.len().max(1);
    let file = tokio::fs::File::open(&source_path)
        .await
        .map_err(|error| TransferError::Failed(error.to_string()))?;
    let mut reader = BufReader::new(file);
    let mut parser = StatementParser::new();
    let mut buffer = String::new();
    let mut bytes_read = 0u64;
    let mut statements = 0usize;
    let mut throttle = ProgressThrottle::new();

    let _ = output
        .send(Event::ImportProgress {
            progress: 0.05,
            status: String::from("Preparing SQLite SQL import..."),
        })
        .await;

    if options.disable_foreign_keys {
        (&mut *conn)
            .execute("PRAGMA foreign_keys = OFF")
            .await
            .map_err(|error| TransferError::Failed(error.to_string()))?;
    }

    if options.drop_existing {
        drop_sqlite_objects(&mut conn).await?;
    }

    let import_result: Result<ImportSummary, TransferError> = 'import: {
        if options.use_transaction
            && let Err(error) = (&mut *conn)
                .execute("BEGIN IMMEDIATE")
                .await
                .map_err(|error| TransferError::Failed(error.to_string()))
        {
            break 'import Err(error);
        }

        loop {
            if cancel.load(Ordering::Relaxed) {
                if options.use_transaction {
                    let _ = (&mut *conn).execute("ROLLBACK").await;
                }
                break 'import Err(TransferError::Cancelled);
            }

            buffer.clear();
            let read = match reader.read_line(&mut buffer).await {
                Ok(read) => read,
                Err(error) => {
                    if options.use_transaction {
                        let _ = (&mut *conn).execute("ROLLBACK").await;
                    }
                    break 'import Err(TransferError::Failed(error.to_string()));
                }
            };
            if read == 0 {
                break;
            }

            bytes_read += read as u64;
            let statements_batch = parser.push_line(&buffer);
            for statement in statements_batch {
                if cancel.load(Ordering::Relaxed) {
                    if options.use_transaction {
                        let _ = (&mut *conn).execute("ROLLBACK").await;
                    }
                    break 'import Err(TransferError::Cancelled);
                }
                if let Err(error) = execute_sqlite_statement(&mut conn, &statement).await {
                    if options.use_transaction {
                        let _ = (&mut *conn).execute("ROLLBACK").await;
                    }
                    break 'import Err(error);
                }
                statements += 1;
                let progress = (bytes_read as f32 / total_bytes as f32).clamp(0.0, 1.0);
                if throttle.should_emit(progress) {
                    let status = format!(
                        "Importing SQLite SQL... {statements} statements ({:.0}% read)",
                        progress * 100.0
                    );
                    let _ = output
                        .send(Event::ImportProgress { progress, status })
                        .await;
                }
            }
        }

        if let Some(statement) = parser.finish() {
            if let Err(error) = execute_sqlite_statement(&mut conn, &statement).await {
                if options.use_transaction {
                    let _ = (&mut *conn).execute("ROLLBACK").await;
                }
                break 'import Err(error);
            }
            statements += 1;
        }

        if options.use_transaction
            && let Err(error) = (&mut *conn)
                .execute("COMMIT")
                .await
                .map_err(|error| TransferError::Failed(error.to_string()))
        {
            break 'import Err(error);
        }

        Ok(ImportSummary {
            statements,
            duration: start.elapsed(),
        })
    };

    let summary = import_result?;
    let progress = (bytes_read as f32 / total_bytes as f32).clamp(0.0, 1.0);
    let _ = output
        .send(Event::ImportProgress {
            progress,
            status: String::from("Finalizing SQLite import..."),
        })
        .await;

    Ok(summary)
}

async fn sqlite_export_sql_worker(
    source_path: PathBuf,
    target_path: PathBuf,
    cancel: Arc<AtomicBool>,
    output: &mut mpsc::Sender<Event>,
) -> Result<ExportSummary, TransferError> {
    let start = Instant::now();
    if cancel.load(Ordering::Relaxed) {
        return Err(TransferError::Cancelled);
    }
    if !source_path.is_file() {
        return Err(TransferError::Failed(format!(
            "SQLite source file `{}` does not exist.",
            source_path.display()
        )));
    }
    if source_path == target_path {
        return Err(TransferError::Failed(String::from(
            "Source and destination SQLite files must be different.",
        )));
    }
    if let Some(parent) = target_path.parent()
        && !parent.as_os_str().is_empty()
    {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|error| TransferError::Failed(error.to_string()))?;
    }

    let (pool, _) = connect_sqlite_profile(source_path.display().to_string())
        .await
        .map_err(TransferError::Failed)?;
    let mut conn = pool
        .acquire()
        .await
        .map_err(|error| TransferError::Failed(error.to_string()))?;

    let file = tokio::fs::OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(&target_path)
        .await
        .map_err(|error| TransferError::Failed(error.to_string()))?;
    let mut writer = tokio::io::BufWriter::new(file);

    let schema_objects = fetch_sqlite_schema_objects(&mut conn).await?;
    let mut table_row_counts = Vec::new();
    let mut total_units = schema_objects.len() as u64;
    for object in &schema_objects {
        if object.kind.eq_ignore_ascii_case("table") {
            let rows = fetch_sqlite_table_row_count(&mut conn, &object.name).await?;
            total_units = total_units.saturating_add(rows);
            table_row_counts.push((object.name.clone(), rows));
        }
    }
    total_units = total_units.max(1);

    let mut processed_units = 0u64;
    let mut rows_exported = 0usize;
    let mut tables_exported = 0usize;
    let mut bytes_written = 0u64;
    let mut throttle = ProgressThrottle::new();

    let _ = output
        .send(Event::ExportProgress {
            progress: 0.05,
            status: String::from("Preparing SQLite SQL export..."),
        })
        .await;

    bytes_written += write_line(&mut writer, "-- CryoDB SQLite export\n").await?;
    bytes_written += write_line(&mut writer, "PRAGMA foreign_keys=OFF;\n\n").await?;

    for object in &schema_objects {
        if cancel.load(Ordering::Relaxed) {
            let _ = tokio::fs::remove_file(&target_path).await;
            return Err(TransferError::Cancelled);
        }

        let drop_stmt = sqlite_drop_statement(&object.kind, &object.name);
        if !drop_stmt.is_empty() {
            bytes_written += write_line(&mut writer, &drop_stmt).await?;
        }
        bytes_written += write_line(&mut writer, &object.sql).await?;
        bytes_written += write_line(&mut writer, ";\n\n").await?;

        processed_units = processed_units.saturating_add(1);
        let progress = (processed_units as f32 / total_units as f32).clamp(0.0, 1.0);
        if throttle.should_emit(progress) {
            let status = format!("Exporting SQLite schema object `{}`...", object.name);
            let _ = output
                .send(Event::ExportProgress { progress, status })
                .await;
        }

        if !object.kind.eq_ignore_ascii_case("table") {
            continue;
        }

        tables_exported += 1;
        let columns = fetch_sqlite_table_columns(&mut conn, &object.name).await?;
        if columns.is_empty() {
            continue;
        }

        let columns_list = columns
            .iter()
            .map(|column| format!("\"{}\"", escape_sqlite_identifier(column)))
            .collect::<Vec<_>>()
            .join(", ");
        let insert_prefix = format!(
            "INSERT INTO \"{}\" ({}) VALUES ",
            escape_sqlite_identifier(&object.name),
            columns_list
        );
        let values_expression = build_sqlite_values_expression(&columns);
        let select_stmt = format!(
            "SELECT {} FROM \"{}\"",
            values_expression,
            escape_sqlite_identifier(&object.name)
        );
        let expected_rows = table_row_counts
            .iter()
            .find(|(name, _)| name == &object.name)
            .map(|(_, rows)| *rows)
            .unwrap_or(0);
        let mut rows =
            sqlx::query_scalar::<_, String>(AssertSqlSafe(select_stmt)).fetch(&mut *conn);
        let mut batch = Vec::with_capacity(EXPORT_INSERT_BATCH_SIZE);
        let mut row_index = 0u64;

        while let Some(values) = rows
            .try_next()
            .await
            .map_err(|error| TransferError::Failed(error.to_string()))?
        {
            if cancel.load(Ordering::Relaxed) {
                let _ = tokio::fs::remove_file(&target_path).await;
                return Err(TransferError::Cancelled);
            }

            row_index += 1;
            rows_exported += 1;
            processed_units = processed_units.saturating_add(1);
            batch.push(values);
            if batch.len() >= EXPORT_INSERT_BATCH_SIZE {
                bytes_written += write_insert_batch(&mut writer, &insert_prefix, &batch).await?;
                batch.clear();
            }

            let progress = (processed_units as f32 / total_units as f32).clamp(0.0, 1.0);
            if throttle.should_emit(progress) {
                let status = format!(
                    "Exporting SQLite table `{}` ({row_index}/{expected_rows} rows)...",
                    object.name
                );
                let _ = output
                    .send(Event::ExportProgress { progress, status })
                    .await;
            }
        }

        if !batch.is_empty() {
            bytes_written += write_insert_batch(&mut writer, &insert_prefix, &batch).await?;
        }
        bytes_written += write_line(&mut writer, "\n").await?;
    }

    bytes_written += write_line(&mut writer, "PRAGMA foreign_keys=ON;\n").await?;
    writer
        .flush()
        .await
        .map_err(|error| TransferError::Failed(error.to_string()))?;

    let progress = (processed_units as f32 / total_units as f32).clamp(0.0, 1.0);
    let _ = output
        .send(Event::ExportProgress {
            progress,
            status: String::from("Finalizing SQLite export..."),
        })
        .await;

    Ok(ExportSummary {
        tables: tables_exported,
        rows: rows_exported,
        duration: start.elapsed(),
        bytes_written,
    })
}

async fn execute_sqlite_statement(
    executor: &mut SqliteConnection,
    statement: &str,
) -> Result<(), TransferError> {
    if query_returns_rows(statement) {
        let mut rows = executor.fetch(AssertSqlSafe(statement));
        while rows
            .try_next()
            .await
            .map_err(|error| {
                TransferError::Failed(format!(
                    "Statement failed: {}\n\n{}",
                    error,
                    truncate_statement(statement)
                ))
            })?
            .is_some()
        {}
    } else {
        executor
            .execute(AssertSqlSafe(statement))
            .await
            .map_err(|error| {
                TransferError::Failed(format!(
                    "Statement failed: {}\n\n{}",
                    error,
                    truncate_statement(statement)
                ))
            })?;
    }
    Ok(())
}

async fn drop_sqlite_objects(conn: &mut SqliteConnection) -> Result<(), TransferError> {
    let mut objects = Vec::new();
    {
        let mut rows = sqlx::query(
            "SELECT type, name \
            FROM sqlite_master \
            WHERE type IN ('view', 'table') \
            AND name NOT LIKE 'sqlite_%' \
            ORDER BY CASE type WHEN 'view' THEN 0 ELSE 1 END, name",
        )
        .fetch(&mut *conn);

        while let Some(row) = rows
            .try_next()
            .await
            .map_err(|error| TransferError::Failed(error.to_string()))?
        {
            let kind: String = row
                .try_get(0)
                .map_err(|error| TransferError::Failed(error.to_string()))?;
            let name: String = row
                .try_get(1)
                .map_err(|error| TransferError::Failed(error.to_string()))?;
            objects.push((kind, name));
        }
    }

    for (kind, name) in objects {
        let statement = sqlite_drop_statement(&kind, &name);
        if statement.is_empty() {
            continue;
        }
        (&mut *conn)
            .execute(AssertSqlSafe(statement))
            .await
            .map_err(|error| TransferError::Failed(error.to_string()))?;
    }

    Ok(())
}

async fn fetch_sqlite_schema_objects(
    conn: &mut SqliteConnection,
) -> Result<Vec<SqliteSchemaObject>, TransferError> {
    let mut rows = sqlx::query(
        "SELECT type, name, sql \
        FROM sqlite_master \
        WHERE type IN ('table', 'index', 'trigger', 'view') \
        AND sql IS NOT NULL \
        AND name NOT LIKE 'sqlite_%' \
        ORDER BY CASE type \
            WHEN 'table' THEN 0 \
            WHEN 'index' THEN 1 \
            WHEN 'trigger' THEN 2 \
            WHEN 'view' THEN 3 \
            ELSE 4 END, \
            name",
    )
    .fetch(&mut *conn);

    let mut objects = Vec::new();
    while let Some(row) = rows
        .try_next()
        .await
        .map_err(|error| TransferError::Failed(error.to_string()))?
    {
        let kind: String = row
            .try_get(0)
            .map_err(|error| TransferError::Failed(error.to_string()))?;
        let name: String = row
            .try_get(1)
            .map_err(|error| TransferError::Failed(error.to_string()))?;
        let sql: String = row
            .try_get(2)
            .map_err(|error| TransferError::Failed(error.to_string()))?;
        objects.push(SqliteSchemaObject { kind, name, sql });
    }

    Ok(objects)
}

async fn fetch_sqlite_table_row_count(
    conn: &mut SqliteConnection,
    table: &str,
) -> Result<u64, TransferError> {
    let statement = format!(
        "SELECT COUNT(*) FROM \"{}\"",
        escape_sqlite_identifier(table)
    );
    let rows = sqlx::query_scalar::<_, i64>(AssertSqlSafe(statement))
        .fetch_one(&mut *conn)
        .await
        .map_err(|error| TransferError::Failed(error.to_string()))?;
    Ok(rows.max(0) as u64)
}

async fn fetch_sqlite_table_columns(
    conn: &mut SqliteConnection,
    table: &str,
) -> Result<Vec<String>, TransferError> {
    let statement = format!(
        "PRAGMA table_info('{}')",
        escape_sqlite_string_literal(table)
    );
    let mut rows = sqlx::query(AssertSqlSafe(statement)).fetch(&mut *conn);
    let mut columns = Vec::new();
    while let Some(row) = rows
        .try_next()
        .await
        .map_err(|error| TransferError::Failed(error.to_string()))?
    {
        let name: String = row
            .try_get(1)
            .map_err(|error| TransferError::Failed(error.to_string()))?;
        columns.push(name);
    }
    Ok(columns)
}

fn build_sqlite_values_expression(columns: &[String]) -> String {
    if columns.is_empty() {
        return String::from("'()'");
    }

    let mut expression = String::from("'('");
    for (index, column) in columns.iter().enumerate() {
        if index == 0 {
            expression.push_str(" || ");
        } else {
            expression.push_str(" || ', ' || ");
        }
        expression.push_str(&format!("quote(\"{}\")", escape_sqlite_identifier(column)));
    }
    expression.push_str(" || ')' ");
    expression.trim_end().to_string()
}

fn sqlite_drop_statement(kind: &str, name: &str) -> String {
    let escaped = escape_sqlite_identifier(name);
    match kind {
        "table" => format!("DROP TABLE IF EXISTS \"{}\";\n", escaped),
        "view" => format!("DROP VIEW IF EXISTS \"{}\";\n", escaped),
        "trigger" => format!("DROP TRIGGER IF EXISTS \"{}\";\n", escaped),
        "index" => format!("DROP INDEX IF EXISTS \"{}\";\n", escaped),
        _ => String::new(),
    }
}

async fn export_database_worker(
    pool: MySqlPool,
    path: PathBuf,
    database: String,
    tables: Vec<String>,
    options: ExportOptions,
    cancel: Arc<AtomicBool>,
    output: &mut mpsc::Sender<Event>,
) -> Result<ExportSummary, TransferError> {
    let start = Instant::now();
    let mut conn = pool
        .acquire()
        .await
        .map_err(|error| TransferError::Failed(error.to_string()))?;

    if cancel.load(Ordering::Relaxed) {
        return Err(TransferError::Cancelled);
    }

    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|error| TransferError::Failed(error.to_string()))?;
    }

    let file = tokio::fs::OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(&path)
        .await
        .map_err(|error| TransferError::Failed(error.to_string()))?;
    let mut writer = tokio::io::BufWriter::new(file);

    let use_stmt = format!("USE `{}`", escape_mysql_identifier(database.trim()));
    (&mut *conn)
        .execute(AssertSqlSafe(use_stmt))
        .await
        .map_err(|error| TransferError::Failed(error.to_string()))?;

    let mut table_meta = Vec::new();
    let mut estimated_rows = 0u64;
    for table in &tables {
        let row = sqlx::query(
            "SELECT table_type, CAST(COALESCE(table_rows, 0) AS SIGNED) \
            FROM information_schema.tables \
            WHERE table_schema = ? AND table_name = ?",
        )
        .bind(database.trim())
        .bind(table)
        .fetch_optional(&mut *conn)
        .await
        .map_err(|error| TransferError::Failed(error.to_string()))?;
        let (table_type, rows) = if let Some(row) = row {
            let table_type: String = row
                .try_get(0)
                .map_err(|error| TransferError::Failed(error.to_string()))?;
            let rows: i64 = row
                .try_get(1)
                .map_err(|error| TransferError::Failed(error.to_string()))?;
            (table_type, rows.max(0) as u64)
        } else {
            (String::from("BASE TABLE"), 0)
        };
        estimated_rows += rows;
        table_meta.push((table.clone(), table_type, rows));
    }

    let (routines, events) = if options.include_routines {
        (
            fetch_routines(&mut conn, database.trim()).await?,
            fetch_events(&mut conn, database.trim()).await?,
        )
    } else {
        (Vec::new(), Vec::new())
    };
    let triggers = if options.include_create {
        fetch_table_triggers_for_export(&mut conn, database.trim(), &tables).await?
    } else {
        Vec::new()
    };

    let mut total_units = tables.len() as u64;
    if options.include_inserts {
        total_units = total_units.saturating_add(estimated_rows);
    }
    total_units = total_units
        .saturating_add(triggers.len() as u64)
        .saturating_add(routines.len() as u64)
        .saturating_add(events.len() as u64);
    if total_units == 0 {
        total_units = 1;
    }

    let mut processed_units = 0u64;
    let mut rows_exported = 0u64;
    let mut bytes_written = 0u64;
    let mut throttle = ProgressThrottle::new();

    bytes_written += write_line(&mut writer, "-- CryoDB export\n").await?;
    bytes_written += write_line(
        &mut writer,
        &format!("-- Database: `{}`\n\n", database.trim()),
    )
    .await?;
    bytes_written += write_line(&mut writer, "SET NAMES utf8mb4;\n").await?;
    if options.include_drop || options.include_create || options.include_inserts {
        bytes_written += write_line(&mut writer, "SET FOREIGN_KEY_CHECKS=0;\n\n").await?;
    }

    for (table, table_type, table_rows) in &table_meta {
        if cancel.load(Ordering::Relaxed) {
            return Err(TransferError::Cancelled);
        }
        let is_view = table_type.eq_ignore_ascii_case("VIEW");

        if options.include_drop {
            let drop_stmt = if is_view {
                format!(
                    "DROP VIEW IF EXISTS `{}`;\n",
                    escape_mysql_identifier(table)
                )
            } else {
                format!(
                    "DROP TABLE IF EXISTS `{}`;\n",
                    escape_mysql_identifier(table)
                )
            };
            bytes_written += write_line(&mut writer, &drop_stmt).await?;
        }

        if options.include_create {
            let create_stmt = fetch_create_statement(&mut conn, table).await?;
            bytes_written += write_line(&mut writer, &create_stmt).await?;
            bytes_written += write_line(&mut writer, ";\n\n").await?;
        }

        processed_units = processed_units.saturating_add(1);
        let progress = (processed_units as f32 / total_units as f32).clamp(0.0, 1.0);
        if throttle.should_emit(progress) {
            let status = format!("Exporting schema for `{table}`...");
            let _ = output
                .send(Event::ExportProgress { progress, status })
                .await;
        }

        if options.include_inserts && !is_view {
            let columns = fetch_table_columns(&mut conn, database.trim(), table).await?;
            if columns.is_empty() {
                continue;
            }

            let columns_list = columns
                .iter()
                .map(|column| format!("`{}`", escape_mysql_identifier(&column.name)))
                .collect::<Vec<_>>()
                .join(", ");

            let insert_prefix = format!(
                "INSERT INTO `{}` ({}) VALUES ",
                escape_mysql_identifier(table),
                columns_list
            );

            let select_stmt = format!("SELECT * FROM `{}`", escape_mysql_identifier(table));
            let mut stream = sqlx::query(AssertSqlSafe(select_stmt)).fetch(&mut *conn);
            let mut batch = Vec::with_capacity(EXPORT_INSERT_BATCH_SIZE);
            let mut row_index = 0u64;

            while let Some(row) = stream
                .try_next()
                .await
                .map_err(|error| TransferError::Failed(error.to_string()))?
            {
                if cancel.load(Ordering::Relaxed) {
                    return Err(TransferError::Cancelled);
                }
                row_index += 1;
                rows_exported += 1;
                processed_units = processed_units.saturating_add(1);

                let values = build_values_row(&row, &columns);
                if options.use_values {
                    batch.push(values);
                    if batch.len() >= EXPORT_INSERT_BATCH_SIZE {
                        bytes_written +=
                            write_insert_batch(&mut writer, &insert_prefix, &batch).await?;
                        batch.clear();
                    }
                } else {
                    let statement = format!("{insert_prefix}{values};\n");
                    bytes_written += write_line(&mut writer, &statement).await?;
                }

                let progress = (processed_units as f32 / total_units as f32).clamp(0.0, 1.0);
                if throttle.should_emit(progress) {
                    let status = format!("Exporting `{table}` ({row_index}/{table_rows} rows)...");
                    let _ = output
                        .send(Event::ExportProgress { progress, status })
                        .await;
                }
            }

            if options.use_values && !batch.is_empty() {
                bytes_written += write_insert_batch(&mut writer, &insert_prefix, &batch).await?;
                batch.clear();
            }

            bytes_written += write_line(&mut writer, "\n").await?;
        }
    }

    let has_delimited_objects = !triggers.is_empty()
        || (options.include_routines && (!routines.is_empty() || !events.is_empty()));
    if has_delimited_objects {
        bytes_written += write_line(&mut writer, "DELIMITER $$\n").await?;
    }

    for trigger in &triggers {
        if cancel.load(Ordering::Relaxed) {
            return Err(TransferError::Cancelled);
        }
        if options.include_drop {
            let drop_stmt = format!(
                "DROP TRIGGER IF EXISTS `{}` $$\n",
                escape_mysql_identifier(&trigger.name)
            );
            bytes_written += write_line(&mut writer, &drop_stmt).await?;
        }
        let create_stmt = fetch_trigger_statement(&mut conn, &trigger.name).await?;
        bytes_written += write_line(&mut writer, &create_stmt).await?;
        bytes_written += write_line(&mut writer, " $$\n").await?;
        processed_units = processed_units.saturating_add(1);
        let progress = (processed_units as f32 / total_units as f32).clamp(0.0, 1.0);
        if throttle.should_emit(progress) {
            let status = format!(
                "Exporting trigger `{}` (table `{}`)...",
                trigger.name, trigger.table
            );
            let _ = output
                .send(Event::ExportProgress { progress, status })
                .await;
        }
    }

    if options.include_routines {
        for routine in &routines {
            if cancel.load(Ordering::Relaxed) {
                return Err(TransferError::Cancelled);
            }
            let create_stmt =
                fetch_routine_statement(&mut conn, &routine.name, &routine.kind).await?;
            bytes_written += write_line(&mut writer, &create_stmt).await?;
            bytes_written += write_line(&mut writer, " $$\n").await?;
            processed_units = processed_units.saturating_add(1);
            let progress = (processed_units as f32 / total_units as f32).clamp(0.0, 1.0);
            if throttle.should_emit(progress) {
                let status = format!("Exporting routine `{}`...", routine.name);
                let _ = output
                    .send(Event::ExportProgress { progress, status })
                    .await;
            }
        }

        for event in &events {
            if cancel.load(Ordering::Relaxed) {
                return Err(TransferError::Cancelled);
            }
            let create_stmt = fetch_event_statement(&mut conn, event).await?;
            bytes_written += write_line(&mut writer, &create_stmt).await?;
            bytes_written += write_line(&mut writer, " $$\n").await?;
            processed_units = processed_units.saturating_add(1);
            let progress = (processed_units as f32 / total_units as f32).clamp(0.0, 1.0);
            if throttle.should_emit(progress) {
                let status = format!("Exporting event `{event}`...");
                let _ = output
                    .send(Event::ExportProgress { progress, status })
                    .await;
            }
        }
    }

    if has_delimited_objects {
        bytes_written += write_line(&mut writer, "DELIMITER ;\n\n").await?;
    }

    if options.include_drop || options.include_create || options.include_inserts {
        bytes_written += write_line(&mut writer, "SET FOREIGN_KEY_CHECKS=1;\n").await?;
    }

    writer
        .flush()
        .await
        .map_err(|error| TransferError::Failed(error.to_string()))?;

    let progress = (processed_units as f32 / total_units as f32).clamp(0.0, 1.0);
    let _ = output
        .send(Event::ExportProgress {
            progress,
            status: String::from("Finalizing export..."),
        })
        .await;

    Ok(ExportSummary {
        tables: tables.len(),
        rows: rows_exported as usize,
        duration: start.elapsed(),
        bytes_written,
    })
}

async fn execute_statement(
    executor: &mut MySqlConnection,
    statement: &str,
) -> Result<(), TransferError> {
    if query_returns_rows(statement) {
        let mut rows = executor.fetch(AssertSqlSafe(statement));
        while rows
            .try_next()
            .await
            .map_err(|error| {
                TransferError::Failed(format!(
                    "Statement failed: {}\n\n{}",
                    error,
                    truncate_statement(statement)
                ))
            })?
            .is_some()
        {}
    } else {
        executor
            .execute(AssertSqlSafe(statement))
            .await
            .map_err(|error| {
                TransferError::Failed(format!(
                    "Statement failed: {}\n\n{}",
                    error,
                    truncate_statement(statement)
                ))
            })?;
    }
    Ok(())
}

fn should_skip_import_statement(statement: &str) -> bool {
    let trimmed = statement.trim_start();
    let mut parts = trimmed.split_whitespace();
    let keyword = parts.next().unwrap_or("").to_ascii_lowercase();
    if keyword == "use" {
        return true;
    }
    if (keyword == "create" || keyword == "drop")
        && let Some(next) = parts.next()
    {
        let next = next.to_ascii_lowercase();
        return next == "database" || next == "schema";
    }
    false
}

fn truncate_statement(statement: &str) -> String {
    let trimmed = statement.trim();
    if trimmed.len() <= 400 {
        trimmed.to_string()
    } else {
        format!("{}...", &trimmed[..400])
    }
}

async fn apply_innodb_settings(
    conn: &mut MySqlConnection,
    version: InnoDbVersion,
) -> Result<(), String> {
    match version {
        InnoDbVersion::Default => Ok(()),
        InnoDbVersion::V57 => {
            (&mut *conn)
                .execute("SET SESSION innodb_strict_mode = 0")
                .await
                .map_err(|error| error.to_string())?;
            (&mut *conn)
                .execute("SET SESSION sql_mode = 'NO_ENGINE_SUBSTITUTION'")
                .await
                .map_err(|error| error.to_string())?;
            Ok(())
        }
        InnoDbVersion::V80 => {
            (&mut *conn)
                .execute("SET SESSION innodb_strict_mode = 1")
                .await
                .map_err(|error| error.to_string())?;
            (&mut *conn)
                .execute("SET SESSION sql_mode = 'STRICT_TRANS_TABLES,NO_ENGINE_SUBSTITUTION'")
                .await
                .map_err(|error| error.to_string())?;
            Ok(())
        }
    }
}

async fn drop_database_objects(
    conn: &mut MySqlConnection,
    database: &str,
) -> Result<(), TransferError> {
    let mut views = Vec::new();
    let mut tables = Vec::new();
    {
        let mut rows = sqlx::query(
            "SELECT table_name, table_type \
            FROM information_schema.tables \
            WHERE table_schema = ?",
        )
        .bind(database)
        .fetch(&mut *conn);

        while let Some(row) = rows
            .try_next()
            .await
            .map_err(|error| TransferError::Failed(error.to_string()))?
        {
            let name: String = row
                .try_get(0)
                .map_err(|error| TransferError::Failed(error.to_string()))?;
            let table_type: String = row
                .try_get(1)
                .map_err(|error| TransferError::Failed(error.to_string()))?;
            if table_type.eq_ignore_ascii_case("VIEW") {
                views.push(name);
            } else {
                tables.push(name);
            }
        }
    }

    for name in &views {
        let drop_stmt = format!("DROP VIEW IF EXISTS `{}`", escape_mysql_identifier(name));
        (&mut *conn)
            .execute(AssertSqlSafe(drop_stmt))
            .await
            .map_err(|error| TransferError::Failed(error.to_string()))?;
    }

    for name in &tables {
        let drop_stmt = format!("DROP TABLE IF EXISTS `{}`", escape_mysql_identifier(name));
        (&mut *conn)
            .execute(AssertSqlSafe(drop_stmt))
            .await
            .map_err(|error| TransferError::Failed(error.to_string()))?;
    }

    Ok(())
}

async fn fetch_create_statement(
    conn: &mut MySqlConnection,
    table: &str,
) -> Result<String, TransferError> {
    let statement = format!("SHOW CREATE TABLE `{}`", escape_mysql_identifier(table));
    let row = (&mut *conn)
        .fetch_one(AssertSqlSafe(statement))
        .await
        .map_err(|error| TransferError::Failed(error.to_string()))?;
    row.try_get::<String, _>(1)
        .map_err(|error| TransferError::Failed(error.to_string()))
}

async fn fetch_table_columns(
    conn: &mut MySqlConnection,
    database: &str,
    table: &str,
) -> Result<Vec<ColumnSpec>, TransferError> {
    let mut rows = sqlx::query(
        "SELECT column_name, column_type \
        FROM information_schema.columns \
        WHERE table_schema = ? \
        AND table_name = ? \
        ORDER BY ordinal_position",
    )
    .bind(database)
    .bind(table)
    .fetch(&mut *conn);

    let mut columns = Vec::new();
    while let Some(row) = rows
        .try_next()
        .await
        .map_err(|error| TransferError::Failed(error.to_string()))?
    {
        let name: String = row
            .try_get(0)
            .map_err(|error| TransferError::Failed(error.to_string()))?;
        let column_type: String = row
            .try_get(1)
            .map_err(|error| TransferError::Failed(error.to_string()))?;
        let kind = column_kind_from_type_name(&column_type);
        columns.push(ColumnSpec { name, kind });
    }

    Ok(columns)
}

struct RoutineMeta {
    name: String,
    kind: String,
}

struct TriggerMeta {
    table: String,
    name: String,
}

async fn fetch_table_triggers_for_export(
    conn: &mut MySqlConnection,
    database: &str,
    tables: &[String],
) -> Result<Vec<TriggerMeta>, TransferError> {
    let mut triggers = Vec::new();

    for table in tables {
        let mut rows = sqlx::query(
            "SELECT trigger_name \
            FROM information_schema.triggers \
            WHERE trigger_schema = ? \
            AND event_object_table = ? \
            ORDER BY trigger_name",
        )
        .bind(database)
        .bind(table)
        .fetch(&mut *conn);

        while let Some(row) = rows
            .try_next()
            .await
            .map_err(|error| TransferError::Failed(error.to_string()))?
        {
            let name: String = row
                .try_get(0)
                .map_err(|error| TransferError::Failed(error.to_string()))?;
            triggers.push(TriggerMeta {
                table: table.clone(),
                name,
            });
        }
    }

    Ok(triggers)
}

async fn fetch_routines(
    conn: &mut MySqlConnection,
    database: &str,
) -> Result<Vec<RoutineMeta>, TransferError> {
    let mut rows = sqlx::query(
        "SELECT routine_name, routine_type \
        FROM information_schema.routines \
        WHERE routine_schema = ? \
        ORDER BY routine_name",
    )
    .bind(database)
    .fetch(&mut *conn);

    let mut routines = Vec::new();
    while let Some(row) = rows
        .try_next()
        .await
        .map_err(|error| TransferError::Failed(error.to_string()))?
    {
        let name: String = row
            .try_get(0)
            .map_err(|error| TransferError::Failed(error.to_string()))?;
        let kind: String = row
            .try_get(1)
            .map_err(|error| TransferError::Failed(error.to_string()))?;
        routines.push(RoutineMeta { name, kind });
    }

    Ok(routines)
}

async fn fetch_events(
    conn: &mut MySqlConnection,
    database: &str,
) -> Result<Vec<String>, TransferError> {
    let mut rows = sqlx::query(
        "SELECT event_name \
        FROM information_schema.events \
        WHERE event_schema = ? \
        ORDER BY event_name",
    )
    .bind(database)
    .fetch(&mut *conn);

    let mut events = Vec::new();
    while let Some(row) = rows
        .try_next()
        .await
        .map_err(|error| TransferError::Failed(error.to_string()))?
    {
        let name: String = row
            .try_get(0)
            .map_err(|error| TransferError::Failed(error.to_string()))?;
        events.push(name);
    }

    Ok(events)
}

async fn fetch_routine_statement(
    conn: &mut MySqlConnection,
    routine: &str,
    kind: &str,
) -> Result<String, TransferError> {
    let statement = if kind.eq_ignore_ascii_case("FUNCTION") {
        format!(
            "SHOW CREATE FUNCTION `{}`",
            escape_mysql_identifier(routine)
        )
    } else {
        format!(
            "SHOW CREATE PROCEDURE `{}`",
            escape_mysql_identifier(routine)
        )
    };
    let row = (&mut *conn)
        .fetch_one(AssertSqlSafe(statement))
        .await
        .map_err(|error| TransferError::Failed(error.to_string()))?;
    row.try_get::<String, _>(2)
        .map_err(|error| TransferError::Failed(error.to_string()))
}

async fn fetch_event_statement(
    conn: &mut MySqlConnection,
    event: &str,
) -> Result<String, TransferError> {
    let statement = format!("SHOW CREATE EVENT `{}`", escape_mysql_identifier(event));
    let row = (&mut *conn)
        .fetch_one(AssertSqlSafe(statement))
        .await
        .map_err(|error| TransferError::Failed(error.to_string()))?;
    row.try_get::<String, _>(3)
        .map_err(|error| TransferError::Failed(error.to_string()))
}

async fn fetch_trigger_statement(
    conn: &mut MySqlConnection,
    trigger: &str,
) -> Result<String, TransferError> {
    let statement = format!("SHOW CREATE TRIGGER `{}`", escape_mysql_identifier(trigger));
    let row = (&mut *conn)
        .fetch_one(AssertSqlSafe(statement))
        .await
        .map_err(|error| TransferError::Failed(error.to_string()))?;
    row.try_get::<String, _>(2)
        .map_err(|error| TransferError::Failed(error.to_string()))
}

fn build_values_row(row: &MySqlRow, columns: &[ColumnSpec]) -> String {
    let mut values = String::from("(");
    for (index, column) in columns.iter().enumerate() {
        if index > 0 {
            values.push_str(", ");
        }
        values.push_str(&sql_literal_from_row(row, index, column.kind));
    }
    values.push(')');
    values
}

async fn write_insert_batch(
    writer: &mut tokio::io::BufWriter<tokio::fs::File>,
    prefix: &str,
    batch: &[String],
) -> Result<u64, TransferError> {
    if batch.is_empty() {
        return Ok(0);
    }
    let mut statement =
        String::with_capacity(prefix.len() + batch.iter().map(|row| row.len() + 2).sum::<usize>());
    statement.push_str(prefix);
    for (index, row) in batch.iter().enumerate() {
        if index > 0 {
            statement.push_str(",\n");
        }
        statement.push_str(row);
    }
    statement.push_str(";\n");
    write_line(writer, &statement).await
}

async fn write_line(
    writer: &mut tokio::io::BufWriter<tokio::fs::File>,
    line: &str,
) -> Result<u64, TransferError> {
    writer
        .write_all(line.as_bytes())
        .await
        .map_err(|error| TransferError::Failed(error.to_string()))?;
    Ok(line.len() as u64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicBool;

    fn sample_import_options() -> ImportOptions {
        ImportOptions {
            create_database: true,
            drop_existing: false,
            disable_foreign_keys: false,
            use_transaction: true,
            innodb_version: InnoDbVersion::Default,
        }
    }

    fn sample_export_options() -> ExportOptions {
        ExportOptions {
            include_drop: true,
            include_create: true,
            include_inserts: true,
            use_values: true,
            include_routines: false,
        }
    }

    fn sample_connection(port: &str) -> ConnectionInfo {
        ConnectionInfo {
            driver: crate::DatabaseDriver::PostgreSql,
            host: String::from("localhost"),
            port: port.to_string(),
            database: String::from("postgres"),
            username: String::from("postgres"),
            password: String::new(),
            sqlite_path: String::new(),
            tls_mode: crate::TlsMode::Prefer,
            tls_ca_cert_path: String::new(),
            tls_client_cert_path: String::new(),
            tls_client_key_path: String::new(),
        }
    }

    #[test]
    fn postgres_connection_env_maps_tls_mode() {
        let env = postgres_connection_env(&sample_connection("5432"));

        assert!(
            env.iter()
                .any(|(key, value)| key == "PGSSLMODE" && value == "prefer")
        );
        assert!(!env.iter().any(|(key, _)| key == "PGSSLROOTCERT"));
    }

    #[test]
    fn postgres_connection_env_includes_ca_cert_when_present() {
        let mut connection = sample_connection("5432");
        connection.tls_mode = crate::TlsMode::VerifyFull;
        connection.tls_ca_cert_path = String::from("/tmp/cryodb-ca.pem");

        let env = postgres_connection_env(&connection);

        assert!(
            env.iter()
                .any(|(key, value)| key == "PGSSLMODE" && value == "verify-full")
        );
        assert!(
            env.iter()
                .any(|(key, value)| { key == "PGSSLROOTCERT" && value == "/tmp/cryodb-ca.pem" })
        );
    }

    #[test]
    fn postgres_port_defaults_to_5432_when_empty() {
        assert_eq!(postgres_port_or_default(&sample_connection("")), "5432");
        assert_eq!(postgres_port_or_default(&sample_connection("5433")), "5433");
    }

    #[test]
    fn psql_args_include_single_transaction_when_enabled() {
        let args = build_psql_args(
            "mydb",
            &PathBuf::from("/tmp/input.sql"),
            &sample_import_options(),
        );
        assert!(args.iter().any(|arg| arg == "--single-transaction"));
        assert!(args.iter().any(|arg| arg == "--set"));
        assert!(args.iter().any(|arg| arg == "ON_ERROR_STOP=1"));
    }

    #[test]
    fn pg_dump_args_map_schema_and_data_modes() {
        let path = PathBuf::from("/tmp/out.sql");

        let mut options = sample_export_options();
        options.include_create = true;
        options.include_inserts = false;
        let schema_only =
            build_pg_dump_args("mydb", &path, &Vec::new(), &options).expect("schema-only args");
        assert!(schema_only.iter().any(|arg| arg == "--schema-only"));
        assert!(!schema_only.iter().any(|arg| arg == "--data-only"));

        options.include_create = false;
        options.include_inserts = true;
        let data_only =
            build_pg_dump_args("mydb", &path, &Vec::new(), &options).expect("data-only args");
        assert!(data_only.iter().any(|arg| arg == "--data-only"));
        assert!(!data_only.iter().any(|arg| arg == "--schema-only"));

        options.include_create = false;
        options.include_inserts = false;
        let error = build_pg_dump_args("mydb", &path, &Vec::new(), &options);
        assert!(error.is_err());
    }

    #[test]
    fn pg_dump_args_include_drop_and_table_filters() {
        let mut options = sample_export_options();
        options.include_drop = true;

        let tables = vec![String::from("public.users"), String::from("public.events")];
        let args = build_pg_dump_args("mydb", &PathBuf::from("/tmp/out.sql"), &tables, &options)
            .expect("pg_dump args");

        assert!(args.iter().any(|arg| arg == "--clean"));
        assert!(args.iter().any(|arg| arg == "--if-exists"));

        let table_flags = args
            .windows(2)
            .filter(|pair| pair[0] == "--table")
            .map(|pair| pair[1].clone())
            .collect::<Vec<_>>();
        assert_eq!(
            table_flags,
            vec![
                String::from("\"public\".\"users\""),
                String::from("\"public\".\"events\"")
            ]
        );
    }

    #[test]
    fn pg_dump_args_quotes_table_references_for_case_sensitivity_and_schemas() {
        let options = sample_export_options();
        let path = PathBuf::from("/tmp/out.sql");

        let tables_normal = vec![String::from("public.users")];
        let args_normal =
            build_pg_dump_args("mydb", &path, &tables_normal, &options).expect("pg_dump args");
        assert!(
            args_normal
                .windows(2)
                .any(|pair| pair[0] == "--table" && pair[1] == "\"public\".\"users\"")
        );

        let tables_uppercase = vec![String::from("public.Clientes")];
        let args_uppercase =
            build_pg_dump_args("mydb", &path, &tables_uppercase, &options).expect("pg_dump args");
        assert!(
            args_uppercase
                .windows(2)
                .any(|pair| pair[0] == "--table" && pair[1] == "\"public\".\"Clientes\"")
        );

        let tables_custom_schema = vec![String::from("billing.Orders")];
        let args_custom_schema = build_pg_dump_args("mydb", &path, &tables_custom_schema, &options)
            .expect("pg_dump args");
        assert!(
            args_custom_schema
                .windows(2)
                .any(|pair| pair[0] == "--table" && pair[1] == "\"billing\".\"Orders\"")
        );

        let tables_mixed = vec![
            String::from("public.users"),
            String::from("public.Clientes"),
            String::from("billing.Orders"),
        ];
        let args_mixed =
            build_pg_dump_args("mydb", &path, &tables_mixed, &options).expect("pg_dump args");
        let mixed_table_flags = args_mixed
            .windows(2)
            .filter(|pair| pair[0] == "--table")
            .map(|pair| pair[1].clone())
            .collect::<Vec<_>>();
        assert_eq!(
            mixed_table_flags,
            vec![
                String::from("\"public\".\"users\""),
                String::from("\"public\".\"Clientes\""),
                String::from("\"billing\".\"Orders\""),
            ]
        );
    }

    #[test]
    fn postgres_tool_spawn_error_message_mentions_missing_tools() {
        let not_found = std::io::Error::new(ErrorKind::NotFound, "missing");
        let error = map_postgres_tool_spawn_error("pg_dump", not_found);
        let TransferError::Failed(message) = error else {
            panic!("expected failed transfer error");
        };
        assert!(message.contains("pg_dump"));
        assert!(message.contains("not installed"));
    }

    #[test]
    fn finish_mysql_import_attempt_restores_on_cancelled_result() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("tokio runtime");

        runtime.block_on(async {
            let restored = Arc::new(AtomicBool::new(false));
            let restore_flag = Arc::clone(&restored);

            let result = finish_mysql_import_attempt::<ImportSummary, _, _>(
                Err(TransferError::Cancelled),
                move || {
                    let restore_flag = Arc::clone(&restore_flag);
                    async move {
                        restore_flag.store(true, Ordering::Relaxed);
                        Ok(())
                    }
                },
            )
            .await;

            assert!(matches!(result, Err(TransferError::Cancelled)));
            assert!(restored.load(Ordering::Relaxed));
        });
    }

    #[test]
    fn finish_mysql_import_attempt_restores_on_failed_result() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("tokio runtime");

        runtime.block_on(async {
            let restored = Arc::new(AtomicBool::new(false));
            let restore_flag = Arc::clone(&restored);

            let result = finish_mysql_import_attempt::<ImportSummary, _, _>(
                Err(TransferError::Failed(String::from("import failed"))),
                move || {
                    let restore_flag = Arc::clone(&restore_flag);
                    async move {
                        restore_flag.store(true, Ordering::Relaxed);
                        Err(TransferError::Failed(String::from("restore failed")))
                    }
                },
            )
            .await;

            match result {
                Err(TransferError::Failed(message)) => assert_eq!(message, "import failed"),
                _ => panic!("expected original import failure"),
            }
            assert!(restored.load(Ordering::Relaxed));
        });
    }

    #[test]
    fn sqlite_copy_worker_fails_when_source_missing() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("tokio runtime");

        runtime.block_on(async {
            let source_path = PathBuf::from("/tmp/cryodb-missing-source.db");
            let target_path = PathBuf::from("/tmp/cryodb-target.db");
            let cancel = Arc::new(AtomicBool::new(false));
            let (mut tx, _rx) = mpsc::channel(4);

            let result = sqlite_copy_worker(source_path, target_path, cancel, true, &mut tx).await;
            match result {
                Err(TransferError::Failed(message)) => {
                    assert!(message.contains("does not exist"));
                }
                _ => panic!("expected sqlite copy worker to fail for missing source"),
            }
        });
    }

    #[test]
    fn sqlite_copy_worker_honors_cancel_before_copy() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("tokio runtime");

        runtime.block_on(async {
            let temp_dir = tempfile::tempdir().expect("temp dir");
            let source_path = temp_dir.path().join("source.db");
            tokio::fs::write(&source_path, b"sqlite-test")
                .await
                .expect("write source");

            let target_path = temp_dir.path().join("target.db");
            let cancel = Arc::new(AtomicBool::new(true));
            let (mut tx, _rx) = mpsc::channel(4);

            let result =
                sqlite_copy_worker(source_path, target_path.clone(), cancel, true, &mut tx).await;
            assert!(matches!(result, Err(TransferError::Cancelled)));
            assert!(!target_path.exists());
        });
    }

    #[test]
    fn sqlite_path_uses_sql_script_detects_sql_extension() {
        assert!(sqlite_path_uses_sql_script(Path::new("/tmp/dump.sql")));
        assert!(sqlite_path_uses_sql_script(Path::new("/tmp/dump.SQL")));
        assert!(!sqlite_path_uses_sql_script(Path::new(
            "/tmp/database.sqlite"
        )));
    }

    #[test]
    fn pgpass_file_is_written_with_escaped_password() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("tokio runtime");

        runtime.block_on(async {
            let mut connection = sample_connection("5432");
            connection.password = String::from("pa:ss\\word");
            connection.username = String::from("alice");
            connection.host = String::from("db.local");

            let file = create_pgpass_file(&connection)
                .await
                .expect("create pgpass")
                .expect("pgpass file");
            let contents = tokio::fs::read_to_string(file.path())
                .await
                .expect("read pgpass");
            assert!(contents.contains("db.local:5432:*:alice:pa\\:ss\\\\word"));
        });
    }

    #[test]
    fn sqlite_copy_worker_import_success_copies_file_contents() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("tokio runtime");

        runtime.block_on(async {
            let temp_dir = tempfile::tempdir().expect("temp dir");
            let source_path = temp_dir.path().join("import-source.db");
            let target_path = temp_dir.path().join("import-target.db");
            let payload = b"sqlite-import-payload";
            tokio::fs::write(&source_path, payload)
                .await
                .expect("write source");

            let cancel = Arc::new(AtomicBool::new(false));
            let (mut tx, _rx) = mpsc::channel(4);
            let summary = sqlite_copy_worker(
                source_path.clone(),
                target_path.clone(),
                cancel,
                true,
                &mut tx,
            )
            .await
            .expect("sqlite import copy success");

            assert_eq!(summary.statements as u64, payload.len() as u64);
            let copied = tokio::fs::read(&target_path).await.expect("read target");
            assert_eq!(copied, payload);
        });
    }

    #[test]
    fn sqlite_export_copy_worker_success_copies_file_contents() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("tokio runtime");

        runtime.block_on(async {
            let temp_dir = tempfile::tempdir().expect("temp dir");
            let source_path = temp_dir.path().join("export-source.db");
            let target_path = temp_dir.path().join("export-target.db");
            let payload = b"sqlite-export-payload";
            tokio::fs::write(&source_path, payload)
                .await
                .expect("write source");

            let cancel = Arc::new(AtomicBool::new(false));
            let (mut tx, _rx) = mpsc::channel(4);
            let summary =
                sqlite_export_copy_worker(source_path, target_path.clone(), cancel, &mut tx)
                    .await
                    .expect("sqlite export copy success");

            assert_eq!(summary.bytes_written, payload.len() as u64);
            let copied = tokio::fs::read(&target_path).await.expect("read target");
            assert_eq!(copied, payload);
        });
    }

    #[test]
    fn sqlite_logical_export_and_import_round_trip() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("tokio runtime");

        runtime.block_on(async {
            let temp_dir = tempfile::tempdir().expect("temp dir");
            let source_path = temp_dir.path().join("logical-source.db");
            let target_path = temp_dir.path().join("logical-target.db");
            let dump_path = temp_dir.path().join("logical-export.sql");

            let source_pool = sqlx::SqlitePool::connect_with(
                sqlx::sqlite::SqliteConnectOptions::new()
                    .filename(&source_path)
                    .create_if_missing(true),
            )
            .await
            .expect("create source sqlite db");
            sqlx::query("CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT NOT NULL, active INTEGER NOT NULL)")
                .execute(&source_pool)
                .await
                .expect("create source schema");
            sqlx::query("INSERT INTO users (name, active) VALUES ('Ada', 1), ('Grace', 0)")
                .execute(&source_pool)
                .await
                .expect("insert source rows");
            source_pool.close().await;

            let target_pool = sqlx::SqlitePool::connect_with(
                sqlx::sqlite::SqliteConnectOptions::new()
                    .filename(&target_path)
                    .create_if_missing(true),
            )
            .await
            .expect("create target sqlite db");
            target_pool.close().await;

            let cancel = Arc::new(AtomicBool::new(false));
            let (mut export_tx, _export_rx) = mpsc::channel(16);
            let export_summary = sqlite_export_sql_worker(
                source_path.clone(),
                dump_path.clone(),
                Arc::clone(&cancel),
                &mut export_tx,
            )
            .await
            .expect("sqlite logical export success");
            assert_eq!(export_summary.tables, 1);
            assert_eq!(export_summary.rows, 2);

            let dump = tokio::fs::read_to_string(&dump_path)
                .await
                .expect("read sqlite dump");
            assert!(dump.contains("CREATE TABLE users"));
            assert!(dump.contains("INSERT INTO \"users\""));

            let (mut import_tx, _import_rx) = mpsc::channel(16);
            let import_summary = sqlite_script_import_worker(
                dump_path,
                target_path.clone(),
                sample_import_options(),
                Arc::new(AtomicBool::new(false)),
                &mut import_tx,
            )
            .await
            .expect("sqlite logical import success");
            assert!(import_summary.statements > 0);

            let verify_pool = sqlx::SqlitePool::connect_with(
                sqlx::sqlite::SqliteConnectOptions::new()
                    .filename(&target_path)
                    .create_if_missing(false),
            )
            .await
            .expect("open imported sqlite db");
            let rows = sqlx::query_scalar::<_, String>(
                "SELECT name FROM users ORDER BY id",
            )
            .fetch_all(&verify_pool)
            .await
            .expect("fetch imported rows");
            assert_eq!(rows, vec![String::from("Ada"), String::from("Grace")]);
            verify_pool.close().await;
        });
    }

    #[test]
    fn sqlite_export_copy_worker_honors_cancel_before_copy() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("tokio runtime");

        runtime.block_on(async {
            let temp_dir = tempfile::tempdir().expect("temp dir");
            let source_path = temp_dir.path().join("source.db");
            tokio::fs::write(&source_path, b"sqlite-export-cancel")
                .await
                .expect("write source");
            let target_path = temp_dir.path().join("target.db");

            let cancel = Arc::new(AtomicBool::new(true));
            let (mut tx, _rx) = mpsc::channel(4);
            let result =
                sqlite_export_copy_worker(source_path, target_path.clone(), cancel, &mut tx).await;
            assert!(matches!(result, Err(TransferError::Cancelled)));
            assert!(!target_path.exists());
        });
    }
}
