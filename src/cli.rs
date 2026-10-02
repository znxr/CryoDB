use crate::db::codecs::{row_to_strings, row_to_strings_postgres, row_to_strings_sqlite};
use crate::db::connect::{connect_mysql, connect_postgres, connect_sqlite_profile};
use crate::db::metadata::{
    fetch_databases, fetch_query_suggestion_columns, fetch_sidebar_relations, fetch_table_info,
    fetch_table_relations,
};
use crate::db::query::{query_returns_rows, run_query_with_control as run_query_with_control_db};
use crate::storage::{
    config_artifact_path, load_connection_secret, load_connection_store,
    sanitize_connection_store_secrets, save_connection_store,
};
use crate::utils::helpers::{
    escape_mysql_identifier, sql_quote_identifier, sql_quote_identifier_path,
    sql_quote_string_literal, sql_quote_table_reference,
};
use crate::utils::sql_parse::{SqlToken, parse_single_table_select, sql_tokens};
use crate::{
    ColumnKind, ConnectionInfo, ConnectionStore, DatabaseDriver, DatabasePool, QueryOutput,
    ResultSet, StoredConnection, TlsMode,
};
use clap::{Args, CommandFactory, Parser, Subcommand, ValueEnum};
use clap_complete::{generate, shells};
use csv::WriterBuilder;
use futures_util::TryStreamExt;
use glob::glob;
use owo_colors::OwoColorize;
use rustyline::completion::{Completer, Pair};
use rustyline::error::ReadlineError;
use rustyline::highlight::Highlighter;
use rustyline::hint::Hinter;
use rustyline::history::DefaultHistory;
use rustyline::validate::Validator;
use rustyline::{CompletionType, Config as LineConfig, Context as LineContext, Editor, Helper};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{Acquire, AssertSqlSafe, Column, Executor, MySql, Postgres, Row, Sqlite, TypeInfo};
use std::collections::BTreeMap;
use std::fs::OpenOptions;
use std::io::{self, IsTerminal, Read, Write};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering as AtomicOrdering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::process::Command;
use url::Url;

const CLI_HISTORY_FILE: &str = "cli-history.jsonl";
const CLI_SHELL_HISTORY_FILE: &str = "cli-shell-history.txt";
const CLI_AUDIT_FILE: &str = "cli-audit.jsonl";
const CLI_DEFAULT_CI_OUTPUT_LIMIT: usize = 16 * 1024 * 1024;
const CLI_EXAMPLES: &str = r#"Examples:
  PostgreSQL: cryodb query --url postgres://user@localhost/app "SELECT 1"
  MySQL:      cryodb q --url mysql://user@localhost/app "SHOW TABLES"
  SQLite:     cryodb tables --sqlite ./app.db --format raw --no-header
  Scripting:  cryodb q -c prod --format ndjson --max-rows 100 --timeout 30 "SELECT * FROM users"
  Export:     cryodb dump -c prod --schema-only --out schema.sql
  Import:     cryodb restore -c staging --transaction --yes dump.sql
  Docs:       cryodb docs -c prod --output html --out schema.html
  Completion: cryodb completion bash --dynamic > ~/.local/share/bash-completion/completions/cryodb

Exit codes: 0 ok, 1 general, 2 connection, 3 SQL, 4 validation, 5 safety, 6 timeout, 7 file-not-found.
"#;

static CLI_QUERY_TIMEOUT_SECONDS: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CliExitCode {
    Success = 0,
    General = 1,
    Connection = 2,
    Sql = 3,
    Validation = 4,
    Safety = 5,
    Timeout = 6,
    FileNotFound = 7,
}

impl CliExitCode {
    fn code(self) -> i32 {
        self as i32
    }
}

#[derive(Debug)]
struct CliError {
    code: CliExitCode,
    message: String,
}

impl CliError {
    fn new(code: CliExitCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: redact_sensitive_text(&message.into()),
        }
    }

    fn general(message: impl Into<String>) -> Self {
        Self::new(CliExitCode::General, message)
    }

    fn connection(message: impl Into<String>) -> Self {
        let message = message.into();
        Self::new(
            if classify_error_category(&message) == "timeout" {
                CliExitCode::Timeout
            } else {
                CliExitCode::Connection
            },
            decorate_classified_error("connection", &message),
        )
    }

    fn sql(message: impl Into<String>) -> Self {
        let message = message.into();
        if classify_error_category(&message) == "timeout" {
            return Self::new(
                CliExitCode::Timeout,
                decorate_classified_error("sql", &message),
            );
        }
        Self::new(CliExitCode::Sql, decorate_classified_error("sql", &message))
    }

    fn validation(message: impl Into<String>) -> Self {
        Self::new(CliExitCode::Validation, message)
    }

    fn safety(message: impl Into<String>) -> Self {
        Self::new(CliExitCode::Safety, message)
    }

    fn file_not_found(path: impl Into<String>) -> Self {
        Self::new(
            CliExitCode::FileNotFound,
            format!("File not found: {}", path.into()),
        )
    }
}

fn classify_error_category(message: &str) -> &'static str {
    let lower = message.to_ascii_lowercase();
    if lower.contains("timed out") || lower.contains("timeout") || lower.contains("deadline") {
        "timeout"
    } else if lower.contains("could not resolve")
        || lower.contains("name or service not known")
        || lower.contains("nodename nor servname")
        || lower.contains("dns")
    {
        "dns"
    } else if lower.contains("password authentication failed")
        || lower.contains("access denied")
        || lower.contains("authentication")
        || lower.contains("login failed")
        || lower.contains("permission denied for user")
    {
        "auth"
    } else if lower.contains("permission denied")
        || lower.contains("not authorized")
        || lower.contains("insufficient privilege")
        || lower.contains("access denied")
    {
        "permission"
    } else if lower.contains("syntax error")
        || lower.contains("you have an error in your sql syntax")
        || lower.contains("near \"")
    {
        "syntax"
    } else if lower.contains("connection refused")
        || lower.contains("connection reset")
        || lower.contains("connection closed")
        || lower.contains("no such host")
        || lower.contains("network is unreachable")
    {
        "network"
    } else {
        "unknown"
    }
}

fn decorate_classified_error(scope: &str, message: &str) -> String {
    let trimmed = message.trim();
    let category = classify_error_category(trimmed);
    if trimmed.starts_with('[') {
        trimmed.to_string()
    } else {
        format!("[{scope}:{category}] {trimmed}")
    }
}

#[derive(Debug, Clone)]
struct AppEnv {
    from_file: BTreeMap<String, String>,
}

impl AppEnv {
    fn load(path: Option<&Path>) -> Result<Self, CliError> {
        let mut from_file = BTreeMap::new();
        let config_path = std::env::var_os("CRYODB_CONFIG_FILE")
            .map(PathBuf::from)
            .unwrap_or_else(|| config_artifact_path("config.toml"));
        merge_toml_env_config(&mut from_file, &config_path)?;

        if let Some(path) = path {
            let contents = std::fs::read_to_string(path).map_err(|error| {
                if error.kind() == std::io::ErrorKind::NotFound {
                    CliError::file_not_found(path.display().to_string())
                } else {
                    CliError::general(error.to_string())
                }
            })?;

            for raw_line in contents.lines() {
                let line = raw_line.trim();
                if line.is_empty() || line.starts_with('#') {
                    continue;
                }
                let Some((key, value)) = line.split_once('=') else {
                    continue;
                };
                let key = key.trim();
                if key.is_empty() {
                    continue;
                }
                let value = value
                    .trim()
                    .trim_matches('"')
                    .trim_matches('\'')
                    .to_string();
                from_file.insert(key.to_string(), value);
            }
        }

        Ok(Self { from_file })
    }

    fn get(&self, key: &str) -> Option<String> {
        std::env::var(key)
            .ok()
            .or_else(|| self.from_file.get(key).cloned())
    }

    fn get_bool(&self, key: &str) -> bool {
        self.get(key)
            .map(|value| {
                matches!(
                    value.trim().to_ascii_lowercase().as_str(),
                    "1" | "true" | "yes" | "on"
                )
            })
            .unwrap_or(false)
    }
}

fn merge_toml_env_config(map: &mut BTreeMap<String, String>, path: &Path) -> Result<(), CliError> {
    let contents = match std::fs::read_to_string(path) {
        Ok(contents) => contents,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(CliError::general(error.to_string())),
    };
    let value = toml::from_str::<toml::Value>(&contents)
        .map_err(|error| CliError::validation(error.to_string()))?;
    if let Some(table) = value.get("cli").and_then(toml::Value::as_table) {
        for (key, value) in table {
            if let Some(env_key) = config_key_to_env(key) {
                insert_toml_config_value(map, env_key, value);
            }
        }
    }
    if let Some(table) = value.as_table() {
        for (key, value) in table {
            if key == "cli" || key == "shell" {
                continue;
            }
            if let Some(env_key) = config_key_to_env(key) {
                insert_toml_config_value(map, env_key, value);
            }
        }
    }
    Ok(())
}

fn config_key_to_env(key: &str) -> Option<&'static str> {
    match key.trim() {
        "connection" => Some("CRYODB_CONNECTION"),
        "format" => Some("CRYODB_FORMAT"),
        "limit" => Some("CRYODB_LIMIT"),
        "max_rows" => Some("CRYODB_MAX_ROWS"),
        "max_output_size" => Some("CRYODB_MAX_OUTPUT_SIZE"),
        "timeout" => Some("CRYODB_TIMEOUT"),
        "quiet" => Some("CRYODB_QUIET"),
        "verbose" => Some("CRYODB_VERBOSE"),
        "debug" => Some("CRYODB_DEBUG"),
        "ci" => Some("CRYODB_CI"),
        "yes" | "assume_yes" => Some("CRYODB_YES"),
        "out_temp" => Some("CRYODB_OUT_TEMP"),
        "structured_logs" => Some("CRYODB_STRUCTURED_LOGS"),
        _ => None,
    }
}

fn insert_toml_config_value(
    map: &mut BTreeMap<String, String>,
    key: &'static str,
    value: &toml::Value,
) {
    let value = match value {
        toml::Value::String(value) => value.clone(),
        toml::Value::Integer(value) => value.to_string(),
        toml::Value::Boolean(value) => value.to_string(),
        toml::Value::Float(value) => value.to_string(),
        _ => return,
    };
    map.insert(key.to_string(), value);
}

#[derive(Debug, Clone)]
struct RuntimeContext {
    env: AppEnv,
    no_color: bool,
    quiet: bool,
    non_interactive: bool,
    assume_yes: bool,
    verbose: bool,
    debug: bool,
    ci: bool,
    structured_logs: bool,
    timeout_secs: Option<u64>,
    pager: bool,
}

impl RuntimeContext {
    fn colors_enabled(&self) -> bool {
        !self.no_color && !self.ci && std::io::stdout().is_terminal()
    }

    fn print_error(&self, message: &str) {
        let text = redact_sensitive_text(message);
        let _ = if self.colors_enabled() {
            writeln!(io::stderr().lock(), "{}", text.red().bold())
        } else {
            writeln!(io::stderr().lock(), "{}", text)
        };
    }

    fn print_info(&self, message: &str) {
        if !self.quiet {
            let text = redact_sensitive_text(message);
            let _ = if self.colors_enabled() {
                writeln!(io::stderr().lock(), "{}", text.cyan())
            } else {
                writeln!(io::stderr().lock(), "{}", text)
            };
        }
    }

    fn print_verbose(&self, message: &str) {
        if !self.quiet && (self.verbose || self.debug) {
            let text = redact_sensitive_text(message);
            let _ = if self.colors_enabled() {
                writeln!(io::stderr().lock(), "{}", text.dimmed())
            } else {
                writeln!(io::stderr().lock(), "{}", text)
            };
        }
    }

    fn print_debug(&self, message: &str) {
        if !self.quiet && self.debug {
            let _ = writeln!(
                io::stderr().lock(),
                "debug: {}",
                redact_sensitive_text(message)
            );
        }
    }

    fn print_structured_log(&self, level: &str, event: &str, detail: Value) {
        if !self.quiet && self.structured_logs {
            let value = json!({
                "timestamp": sqlx::types::chrono::Utc::now().to_rfc3339(),
                "level": level,
                "event": event,
                "detail": detail,
            });
            let _ = writeln!(
                io::stderr().lock(),
                "{}",
                redact_sensitive_text(&value.to_string())
            );
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum OutputFormat {
    Table,
    Json,
    Ndjson,
    Csv,
    Tsv,
    Markdown,
    Raw,
    Pretty,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum SchemaOutputFormat {
    Sql,
    Markdown,
    Json,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum DocsOutputFormat {
    Markdown,
    Html,
    Json,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum CliDriver {
    Postgres,
    Mysql,
    Mariadb,
    Sqlite,
}

impl From<CliDriver> for DatabaseDriver {
    fn from(value: CliDriver) -> Self {
        match value {
            CliDriver::Postgres => DatabaseDriver::PostgreSql,
            CliDriver::Mysql => DatabaseDriver::MySql,
            CliDriver::Mariadb => DatabaseDriver::MariaDb,
            CliDriver::Sqlite => DatabaseDriver::Sqlite,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum CliTlsMode {
    Disabled,
    Prefer,
    Require,
    VerifyCa,
    VerifyFull,
}

impl From<CliTlsMode> for TlsMode {
    fn from(value: CliTlsMode) -> Self {
        match value {
            CliTlsMode::Disabled => TlsMode::Disabled,
            CliTlsMode::Prefer => TlsMode::Prefer,
            CliTlsMode::Require => TlsMode::Require,
            CliTlsMode::VerifyCa => TlsMode::VerifyCa,
            CliTlsMode::VerifyFull => TlsMode::VerifyFull,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum CompletionShell {
    Bash,
    Zsh,
    Fish,
}

#[derive(Parser, Debug)]
#[command(name = "cryodb", version, after_help = CLI_EXAMPLES)]
struct Cli {
    #[arg(long, global = true)]
    env_file: Option<PathBuf>,
    #[arg(long, global = true, action = clap::ArgAction::SetTrue)]
    no_color: bool,
    #[arg(long, global = true, action = clap::ArgAction::SetTrue)]
    quiet: bool,
    #[arg(long, global = true, action = clap::ArgAction::SetTrue)]
    non_interactive: bool,
    #[arg(long, global = true, action = clap::ArgAction::SetTrue)]
    ci: bool,
    #[arg(long, global = true, action = clap::ArgAction::SetTrue)]
    yes: bool,
    #[arg(long, global = true, value_parser = clap::value_parser!(u64).range(1..))]
    timeout: Option<u64>,
    #[arg(long, global = true, action = clap::ArgAction::SetTrue)]
    structured_logs: bool,
    #[arg(long, global = true, action = clap::ArgAction::SetTrue)]
    verbose: bool,
    #[arg(long, global = true, action = clap::ArgAction::SetTrue)]
    debug: bool,
    #[arg(long, global = true, action = clap::ArgAction::SetTrue)]
    pager: bool,
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    #[command(alias = "q")]
    Query(QueryCommand),
    Run(RunCommand),
    #[command(alias = "sh")]
    Shell(ShellCommand),
    Tui(TuiCommand),
    #[command(alias = "cx")]
    Conn(ConnectionCommand),
    Databases(ListSimpleCommand),
    Db(DbCommand),
    Schemas(ListSimpleCommand),
    Tables(ListObjectsCommand),
    Views(ListObjectsCommand),
    #[command(alias = "desc")]
    Describe(DescribeCommand),
    Schema(SchemaCommand),
    Docs(DocsCommand),
    Indexes(IndexesCommand),
    Relations(RelationsCommand),
    Constraints(ConstraintsCommand),
    Size(SizeCommand),
    Search(SearchCommand),
    Dump(DumpCommand),
    Restore(RestoreCommand),
    Validate(ValidateCommand),
    Lint(LintCommand),
    Format(FormatCommand),
    Explain(ExplainCommand),
    Value(ValueCommand),
    Ping(ConnectionSelectorArgs),
    Health(HealthCommand),
    ServerInfo(ListSimpleCommand),
    History(HistoryCommand),
    Doctor(DoctorCommand),
    Completion(CompletionCommand),
    #[command(name = "__complete", hide = true)]
    Complete(CompleteCommand),
    Version,
}

#[derive(Args, Debug, Clone)]
struct ConnectionSelectorArgs {
    #[arg(short = 'c', long = "connection")]
    connection: Option<String>,
    #[arg(long)]
    url: Option<String>,
    #[arg(long)]
    sqlite: Option<PathBuf>,
    #[arg(long)]
    database: Option<String>,
}

#[derive(Args, Debug, Clone)]
struct RenderArgs {
    #[arg(long)]
    format: Option<OutputFormat>,
    #[arg(long)]
    out: Option<PathBuf>,
    #[arg(long, action = clap::ArgAction::SetTrue)]
    raw: bool,
    #[arg(long, action = clap::ArgAction::SetTrue)]
    no_header: bool,
    #[arg(long, action = clap::ArgAction::SetTrue)]
    single_value: bool,
    #[arg(long)]
    max_rows: Option<usize>,
    #[arg(long)]
    max_output_size: Option<usize>,
    #[arg(long, action = clap::ArgAction::SetTrue)]
    out_temp: bool,
    #[arg(long, action = clap::ArgAction::SetTrue)]
    single_result: bool,
}

#[derive(Args, Debug)]
struct QueryCommand {
    #[command(flatten)]
    connection: ConnectionSelectorArgs,
    #[command(flatten)]
    render: RenderArgs,
    #[arg(long, action = clap::ArgAction::SetTrue)]
    transaction: bool,
    #[arg(long, action = clap::ArgAction::SetTrue)]
    continue_on_error: bool,
    #[arg(value_name = "SQL")]
    sql: Option<String>,
}

#[derive(Args, Debug)]
struct RunCommand {
    #[command(flatten)]
    connection: ConnectionSelectorArgs,
    #[command(flatten)]
    render: RenderArgs,
    #[arg(long, action = clap::ArgAction::SetTrue)]
    transaction: bool,
    #[arg(long, action = clap::ArgAction::SetTrue)]
    stop_on_error: bool,
    #[arg(long, action = clap::ArgAction::SetTrue)]
    continue_on_error: bool,
    #[arg(long, action = clap::ArgAction::SetTrue)]
    dry_run: bool,
    #[arg(value_name = "FILE")]
    file: Option<PathBuf>,
}

#[derive(Args, Debug)]
struct ShellCommand {
    #[command(flatten)]
    connection: ConnectionSelectorArgs,
    #[arg(long)]
    format: Option<OutputFormat>,
}

#[derive(Args, Debug)]
struct TuiCommand {
    #[command(flatten)]
    connection: ConnectionSelectorArgs,
    #[arg(long)]
    format: Option<OutputFormat>,
    #[arg(long)]
    max_rows: Option<usize>,
}

#[derive(Subcommand, Debug)]
enum ConnectionSubcommands {
    List,
    Add(ConnectionUpsertCommand),
    Edit(ConnectionEditCommand),
    Remove(ConnectionNamedCommand),
    Test(ConnectionNamedCommand),
    Show(ConnectionNamedCommand),
    SetDefault(ConnectionNamedCommand),
}

#[derive(Args, Debug)]
struct ConnectionCommand {
    #[command(subcommand)]
    command: ConnectionSubcommands,
}

#[derive(Args, Debug, Clone)]
struct ConnectionInputArgs {
    #[arg(long)]
    driver: Option<CliDriver>,
    #[arg(long)]
    host: Option<String>,
    #[arg(long)]
    port: Option<u16>,
    #[arg(long)]
    database: Option<String>,
    #[arg(long)]
    username: Option<String>,
    #[arg(long)]
    password: Option<String>,
    #[arg(long = "password-env")]
    password_env: Option<String>,
    #[arg(long, action = clap::ArgAction::SetTrue)]
    password_prompt: bool,
    #[arg(long)]
    sqlite: Option<PathBuf>,
    #[arg(long)]
    url: Option<String>,
    #[arg(long)]
    tls_mode: Option<CliTlsMode>,
    #[arg(long)]
    tls_ca_cert_path: Option<PathBuf>,
    #[arg(long)]
    tls_client_cert_path: Option<PathBuf>,
    #[arg(long)]
    tls_client_key_path: Option<PathBuf>,
}

#[derive(Args, Debug)]
struct ConnectionUpsertCommand {
    name: String,
    #[command(flatten)]
    input: ConnectionInputArgs,
}

#[derive(Args, Debug)]
struct ConnectionEditCommand {
    name: String,
    #[arg(long)]
    rename: Option<String>,
    #[command(flatten)]
    input: ConnectionInputArgs,
}

#[derive(Args, Debug)]
struct ConnectionNamedCommand {
    name: String,
}

#[derive(Subcommand, Debug)]
enum DbSubcommands {
    List(ListSimpleCommand),
}

#[derive(Args, Debug)]
struct DbCommand {
    #[command(subcommand)]
    command: DbSubcommands,
}

#[derive(Args, Debug)]
struct ListSimpleCommand {
    #[command(flatten)]
    connection: ConnectionSelectorArgs,
    #[command(flatten)]
    render: RenderArgs,
}

#[derive(Args, Debug)]
struct ListObjectsCommand {
    #[command(flatten)]
    connection: ConnectionSelectorArgs,
    #[command(flatten)]
    render: RenderArgs,
    #[arg(long)]
    schema: Option<String>,
    #[arg(long)]
    pattern: Option<String>,
}

#[derive(Args, Debug)]
struct DescribeCommand {
    #[command(flatten)]
    connection: ConnectionSelectorArgs,
    #[command(flatten)]
    render: RenderArgs,
    table: String,
}

#[derive(Args, Debug)]
struct SchemaCommand {
    #[command(flatten)]
    connection: ConnectionSelectorArgs,
    #[arg(long, value_enum, default_value = "markdown")]
    output: SchemaOutputFormat,
    table: Option<String>,
}

#[derive(Args, Debug)]
struct DocsCommand {
    #[command(flatten)]
    connection: ConnectionSelectorArgs,
    #[arg(long, value_enum, default_value = "markdown")]
    output: DocsOutputFormat,
    #[arg(long)]
    out: Option<PathBuf>,
    #[arg(long, action = clap::ArgAction::SetTrue)]
    out_temp: bool,
    table: Option<String>,
}

#[derive(Args, Debug)]
struct IndexesCommand {
    #[command(flatten)]
    connection: ConnectionSelectorArgs,
    #[command(flatten)]
    render: RenderArgs,
    table: Option<String>,
}

#[derive(Args, Debug)]
struct RelationsCommand {
    #[command(flatten)]
    connection: ConnectionSelectorArgs,
    #[command(flatten)]
    render: RenderArgs,
}

#[derive(Args, Debug)]
struct ConstraintsCommand {
    #[command(flatten)]
    connection: ConnectionSelectorArgs,
    #[command(flatten)]
    render: RenderArgs,
    table: Option<String>,
}

#[derive(Args, Debug)]
struct SizeCommand {
    #[command(flatten)]
    connection: ConnectionSelectorArgs,
    #[command(flatten)]
    render: RenderArgs,
}

#[derive(Args, Debug)]
struct SearchCommand {
    #[command(flatten)]
    connection: ConnectionSelectorArgs,
    #[command(flatten)]
    render: RenderArgs,
    needle: String,
}

#[derive(Args, Debug)]
struct DumpCommand {
    #[command(flatten)]
    connection: ConnectionSelectorArgs,
    #[arg(long, action = clap::ArgAction::SetTrue)]
    schema_only: bool,
    #[arg(long, action = clap::ArgAction::SetTrue)]
    data_only: bool,
    #[arg(long)]
    table: Option<String>,
    #[arg(long)]
    out: Option<PathBuf>,
    #[arg(long, action = clap::ArgAction::SetTrue)]
    out_temp: bool,
}

#[derive(Args, Debug)]
struct RestoreCommand {
    #[command(flatten)]
    connection: ConnectionSelectorArgs,
    #[arg(long, action = clap::ArgAction::SetTrue)]
    transaction: bool,
    #[arg(long, action = clap::ArgAction::SetTrue)]
    stop_on_error: bool,
    #[arg(long, action = clap::ArgAction::SetTrue)]
    continue_on_error: bool,
    #[arg(long, action = clap::ArgAction::SetTrue)]
    dry_run: bool,
    #[arg(value_name = "FILE")]
    file: Option<PathBuf>,
}

#[derive(Args, Debug)]
struct ValidateCommand {
    #[command(flatten)]
    connection: ConnectionSelectorArgs,
    #[command(flatten)]
    render: RenderArgs,
    #[arg(value_name = "FILES")]
    files: Vec<String>,
}

#[derive(Args, Debug)]
struct LintCommand {
    #[command(flatten)]
    render: RenderArgs,
    #[arg(value_name = "FILES")]
    files: Vec<String>,
}

#[derive(Args, Debug)]
struct FormatCommand {
    #[arg(long, action = clap::ArgAction::SetTrue)]
    write: bool,
    #[arg(value_name = "FILES")]
    files: Vec<String>,
}

#[derive(Args, Debug)]
struct ExplainCommand {
    #[command(flatten)]
    connection: ConnectionSelectorArgs,
    #[command(flatten)]
    render: RenderArgs,
    #[arg(long, action = clap::ArgAction::SetTrue)]
    analyze: bool,
    #[arg(value_name = "SQL")]
    sql: Option<String>,
}

#[derive(Args, Debug)]
struct ValueCommand {
    #[command(flatten)]
    connection: ConnectionSelectorArgs,
    #[arg(value_name = "SQL")]
    sql: Option<String>,
}

#[derive(Args, Debug)]
struct HealthCommand {
    #[command(flatten)]
    connection: ConnectionSelectorArgs,
    #[command(flatten)]
    render: RenderArgs,
}

#[derive(Args, Debug)]
struct HistoryCommand {
    #[arg(short = 'c', long = "connection")]
    connection: Option<String>,
    #[arg(long)]
    search: Option<String>,
    #[arg(long)]
    run: Option<usize>,
    #[arg(long, action = clap::ArgAction::SetTrue)]
    clear: bool,
    #[arg(long, default_value_t = 50)]
    limit: usize,
    #[command(flatten)]
    render: RenderArgs,
}

#[derive(Args, Debug)]
struct DoctorCommand {
    #[arg(value_enum)]
    driver: Option<CliDriver>,
    #[command(flatten)]
    render: RenderArgs,
}

#[derive(Args, Debug)]
struct CompletionCommand {
    shell: CompletionShell,
    #[arg(long, action = clap::ArgAction::SetTrue)]
    dynamic: bool,
}

#[derive(Args, Debug)]
struct CompleteCommand {
    #[arg(long)]
    line: String,
    #[arg(long)]
    position: Option<usize>,
}

#[derive(Debug, Clone)]
pub(crate) struct ResolvedConnection {
    pub(crate) driver: DatabaseDriver,
    pub(crate) pool: DatabasePool,
    pub(crate) database: Option<String>,
    pub(crate) profile_name: Option<String>,
    pub(crate) label: String,
    pub(crate) info: ConnectionInfo,
}

#[derive(Debug, Clone)]
struct ResolvedRender {
    format: OutputFormat,
    out: Option<PathBuf>,
    no_header: bool,
    single_value: bool,
    max_rows: Option<usize>,
    max_output_size: Option<usize>,
    out_temp: bool,
    single_result: bool,
    pager: bool,
    no_color: bool,
}

#[derive(Debug)]
struct StatementResult {
    statement: String,
    output: QueryOutput,
    elapsed_ms: u128,
}

#[derive(Debug)]
struct StatementBatch {
    results: Vec<StatementResult>,
    errors: Vec<String>,
}

impl StatementBatch {
    fn into_result(self) -> Result<Vec<StatementResult>, CliError> {
        if self.errors.is_empty() {
            Ok(self.results)
        } else {
            Err(CliError::sql(self.errors.join("\n")))
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct HistoryRecord {
    timestamp: String,
    connection: String,
    driver: String,
    source: String,
    status: String,
    sql: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AuditRecord {
    timestamp: String,
    event: String,
    connection: String,
    driver: String,
    source: String,
    statement_kind: String,
    sql_preview: String,
}

#[derive(Debug, Clone)]
struct SqlInput {
    label: String,
    path: Option<PathBuf>,
    sql: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum SavedConnectionSource {
    Profile,
    Favorite,
    Recent,
}

impl SavedConnectionSource {
    fn label(self) -> &'static str {
        match self {
            SavedConnectionSource::Profile => "profile",
            SavedConnectionSource::Favorite => "favorite",
            SavedConnectionSource::Recent => "recent",
        }
    }
}

#[derive(Debug, Clone)]
struct CatalogConnection {
    identity_key: String,
    entry: StoredConnection,
    selector: String,
    sources: Vec<SavedConnectionSource>,
}

#[derive(Debug, Clone)]
struct ResolvedSavedConnection {
    entry: StoredConnection,
    selector: String,
    sources: Vec<SavedConnectionSource>,
    exact_named: bool,
}

#[derive(Debug, Default)]
struct DescribeBundle {
    details: Option<TableInfoRows>,
    columns: Option<QueryOutput>,
    indexes: Option<QueryOutput>,
    relations: Option<QueryOutput>,
    constraints: Option<QueryOutput>,
}

#[derive(Debug, Clone)]
struct TableInfoRows {
    rows: Vec<(String, String)>,
}

#[derive(Debug, Default)]
struct ShellCompletionState {
    commands: Vec<String>,
    profiles: Vec<String>,
    objects: Vec<String>,
}

#[derive(Debug, Clone)]
struct ShellHelper {
    state: Arc<Mutex<ShellCompletionState>>,
}

impl Helper for ShellHelper {}
impl Hinter for ShellHelper {
    type Hint = String;
}
impl Highlighter for ShellHelper {}
impl Validator for ShellHelper {}

impl Completer for ShellHelper {
    type Candidate = Pair;

    fn complete(
        &self,
        line: &str,
        pos: usize,
        _ctx: &LineContext<'_>,
    ) -> rustyline::Result<(usize, Vec<Pair>)> {
        let prefix = line.get(..pos).unwrap_or(line);
        let trimmed = prefix.trim_start();
        let state = self.state.lock().ok();
        let Some(state) = state.as_deref() else {
            return Ok((0, Vec::new()));
        };

        if !trimmed.starts_with('\\') {
            return Ok((0, Vec::new()));
        }

        let pieces = trimmed.split_whitespace().collect::<Vec<_>>();
        if pieces.len() <= 1 && !trimmed.contains(' ') {
            let start = prefix.rfind('\\').unwrap_or(0);
            let needle = prefix.get(start..pos).unwrap_or("");
            let pairs = state
                .commands
                .iter()
                .filter(|item| item.starts_with(needle))
                .map(|item| Pair {
                    display: item.clone(),
                    replacement: item.clone(),
                })
                .collect::<Vec<_>>();
            return Ok((start, pairs));
        }

        let first = pieces.first().copied().unwrap_or("");
        let last = pieces.last().copied().unwrap_or("");
        let replace = if prefix.ends_with(' ') { "" } else { last };
        let start = pos.saturating_sub(replace.len());
        let source = match first {
            "\\connect" => &state.profiles,
            "\\describe" | "\\schema" | "\\indexes" | "\\constraints" => &state.objects,
            _ => &state.commands,
        };
        let pairs = source
            .iter()
            .filter(|item| item.starts_with(replace))
            .map(|item| Pair {
                display: item.clone(),
                replacement: item.clone(),
            })
            .collect::<Vec<_>>();
        Ok((start, pairs))
    }
}

pub(crate) fn should_run_cli() -> bool {
    std::env::args_os().nth(1).is_some()
}

pub(crate) fn run() -> i32 {
    match run_inner() {
        Ok(()) => CliExitCode::Success.code(),
        Err(error) => {
            let _ = writeln!(io::stderr().lock(), "{}", error.message);
            error.code.code()
        }
    }
}

fn run_inner() -> Result<(), CliError> {
    let cli = Cli::parse();
    let env = AppEnv::load(cli.env_file.as_deref())?;
    let ci = cli.ci || env.get_bool("CRYODB_CI") || env.get_bool("CI");
    let debug = cli.debug || env.get_bool("CRYODB_DEBUG");
    let timeout_secs = cli.timeout.or_else(|| {
        env.get("CRYODB_TIMEOUT")
            .and_then(|value| parse_positive_u64(&value))
    });
    let context = RuntimeContext {
        no_color: cli.no_color,
        pager: cli.pager || ci || env.get_bool("CRYODB_NO_COLOR") || env.get_bool("NO_COLOR"),
        quiet: cli.quiet || env.get_bool("CRYODB_QUIET"),
        non_interactive: cli.non_interactive || ci,
        assume_yes: cli.yes || env.get_bool("CRYODB_YES") || env.get_bool("CRYODB_ASSUME_YES"),
        verbose: cli.verbose || debug || env.get_bool("CRYODB_VERBOSE"),
        debug,
        ci,
        structured_logs: cli.structured_logs
            || env.get_bool("CRYODB_STRUCTURED_LOGS")
            || env.get_bool("CRYODB_LOG_JSON"),
        timeout_secs,
        env,
    };
    set_cli_query_timeout(context.timeout_secs);

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| CliError::general(error.to_string()))?;

    runtime.block_on(async move { dispatch(context, cli.command).await })
}

async fn dispatch(context: RuntimeContext, command: Commands) -> Result<(), CliError> {
    match command {
        Commands::Query(command) => handle_query(&context, command, "query").await,
        Commands::Run(command) => handle_run(&context, command).await,
        Commands::Shell(command) => handle_shell(&context, command).await,
        Commands::Tui(command) => handle_tui(&context, command).await,
        Commands::Conn(command) => handle_conn(&context, command).await,
        Commands::Databases(command) => handle_databases(&context, command).await,
        Commands::Db(command) => match command.command {
            DbSubcommands::List(command) => handle_databases(&context, command).await,
        },
        Commands::Schemas(command) => handle_schemas(&context, command).await,
        Commands::Tables(command) => handle_tables(&context, command).await,
        Commands::Views(command) => handle_views(&context, command).await,
        Commands::Describe(command) => handle_describe(&context, command).await,
        Commands::Schema(command) => handle_schema(&context, command).await,
        Commands::Docs(command) => handle_docs(&context, command).await,
        Commands::Indexes(command) => handle_indexes(&context, command).await,
        Commands::Relations(command) => handle_relations(&context, command).await,
        Commands::Constraints(command) => handle_constraints(&context, command).await,
        Commands::Size(command) => handle_size(&context, command).await,
        Commands::Search(command) => handle_search(&context, command).await,
        Commands::Dump(command) => handle_dump(&context, command).await,
        Commands::Restore(command) => handle_restore(&context, command).await,
        Commands::Validate(command) => handle_validate(&context, command).await,
        Commands::Lint(command) => handle_lint(&context, command).await,
        Commands::Format(command) => handle_format(&context, command).await,
        Commands::Explain(command) => handle_explain(&context, command).await,
        Commands::Value(command) => handle_value(&context, command).await,
        Commands::Ping(connection) => handle_ping(&context, connection).await,
        Commands::Health(command) => handle_health(&context, command).await,
        Commands::ServerInfo(command) => handle_server_info(&context, command).await,
        Commands::History(command) => handle_history(&context, command).await,
        Commands::Doctor(command) => handle_doctor(&context, command).await,
        Commands::Completion(command) => handle_completion(command),
        Commands::Complete(command) => handle_complete(&context, command).await,
        Commands::Version => writeln!(io::stdout().lock(), "{}", env!("CARGO_PKG_VERSION"))
            .map_err(|error| CliError::general(error.to_string())),
    }
}

fn resolve_render(context: &RuntimeContext, render: RenderArgs) -> ResolvedRender {
    let format = if render.raw {
        OutputFormat::Raw
    } else {
        render
            .format
            .or_else(|| {
                context
                    .env
                    .get("CRYODB_FORMAT")
                    .and_then(|value| parse_output_format(&value))
            })
            .unwrap_or(if context.ci {
                OutputFormat::Json
            } else {
                OutputFormat::Table
            })
    };

    ResolvedRender {
        format,
        out: render.out,
        no_header: render.no_header,
        single_value: render.single_value,
        max_rows: render
            .max_rows
            .or_else(|| parse_env_usize(context, "CRYODB_LIMIT"))
            .or_else(|| parse_env_usize(context, "CRYODB_MAX_ROWS")),
        max_output_size: render
            .max_output_size
            .or_else(|| parse_env_usize(context, "CRYODB_MAX_OUTPUT_SIZE"))
            .or_else(|| context.ci.then_some(CLI_DEFAULT_CI_OUTPUT_LIMIT)),
        out_temp: render.out_temp || context.env.get_bool("CRYODB_OUT_TEMP"),
        single_result: render.single_result,
        pager: context.pager || context.env.get_bool("CRYODB_PAGER"),
        no_color: context.no_color,
    }
}

fn parse_env_usize(context: &RuntimeContext, key: &str) -> Option<usize> {
    context.env.get(key)?.trim().parse::<usize>().ok()
}

fn parse_positive_u64(value: &str) -> Option<u64> {
    let parsed = value.trim().parse::<u64>().ok()?;
    (parsed > 0).then_some(parsed)
}

fn set_cli_query_timeout(timeout_secs: Option<u64>) {
    CLI_QUERY_TIMEOUT_SECONDS.store(timeout_secs.unwrap_or(0), AtomicOrdering::Relaxed);
}

fn cli_query_timeout() -> Option<Duration> {
    let seconds = CLI_QUERY_TIMEOUT_SECONDS.load(AtomicOrdering::Relaxed);
    (seconds > 0).then(|| Duration::from_secs(seconds))
}

async fn run_query_with_control(
    pool: DatabasePool,
    database: Option<String>,
    query: String,
    cancel_flag: Option<Arc<std::sync::atomic::AtomicBool>>,
    max_rows: Option<usize>,
) -> Result<QueryOutput, String> {
    if let Some(timeout) = cli_query_timeout() {
        return match tokio::time::timeout(
            timeout,
            run_query_with_control_db(pool, database, query, cancel_flag, max_rows),
        )
        .await
        {
            Ok(result) => result,
            Err(_) => Err(format!(
                "Query timed out after {} seconds.",
                timeout.as_secs()
            )),
        };
    }

    run_query_with_control_db(pool, database, query, cancel_flag, max_rows).await
}

fn parse_output_format(value: &str) -> Option<OutputFormat> {
    match value.trim().to_ascii_lowercase().as_str() {
        "table" => Some(OutputFormat::Table),
        "json" => Some(OutputFormat::Json),
        "ndjson" => Some(OutputFormat::Ndjson),
        "csv" => Some(OutputFormat::Csv),
        "tsv" => Some(OutputFormat::Tsv),
        "markdown" | "md" => Some(OutputFormat::Markdown),
        "raw" => Some(OutputFormat::Raw),
        "pretty" => Some(OutputFormat::Pretty),
        _ => None,
    }
}

fn redact_sensitive_text(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut token = String::new();
    let mut redact_next_token = false;

    for ch in input.chars() {
        if ch.is_whitespace() {
            if !token.is_empty() {
                if redact_next_token {
                    output.push_str(&redact_standalone_secret_value(&token));
                    redact_next_token = false;
                } else {
                    output.push_str(&redact_sensitive_token(&token));
                    redact_next_token = token_requests_next_secret_redaction(&token);
                }
                token.clear();
            }
            output.push(ch);
        } else {
            token.push(ch);
        }
    }

    if !token.is_empty() {
        if redact_next_token {
            output.push_str(&redact_standalone_secret_value(&token));
        } else {
            output.push_str(&redact_sensitive_token(&token));
        }
    }

    output
}

fn token_requests_next_secret_redaction(token: &str) -> bool {
    let candidate = token.trim_matches(|ch: char| matches!(ch, '"' | '\'' | '{' | '[' | ','));
    for separator in ['=', ':'] {
        if let Some((key, value)) = candidate.split_once(separator) {
            let normalized_key = key
                .trim()
                .trim_start_matches('-')
                .trim_matches(|ch: char| matches!(ch, '"' | '\'' | '{' | '['))
                .trim_end_matches(['"', '\''])
                .to_ascii_lowercase();
            if is_sensitive_query_key(&normalized_key) && value.trim().is_empty() {
                return true;
            }
        }
    }
    false
}

fn redact_standalone_secret_value(token: &str) -> String {
    let leading_len = token
        .char_indices()
        .find(|(_, ch)| ch.is_ascii_alphanumeric() || matches!(ch, '"' | '\''))
        .map(|(index, _)| index)
        .unwrap_or(token.len());
    let trailing_start = token
        .char_indices()
        .rev()
        .find(|(_, ch)| ch.is_ascii_alphanumeric() || matches!(ch, '"' | '\''))
        .map(|(index, ch)| index + ch.len_utf8())
        .unwrap_or(leading_len);
    format!(
        "{}REDACTED{}",
        &token[..leading_len],
        &token[trailing_start..]
    )
}

fn redact_sensitive_token(token: &str) -> String {
    let leading_len = token
        .char_indices()
        .find(|(_, ch)| ch.is_ascii_alphanumeric())
        .map(|(index, _)| index)
        .unwrap_or(token.len());
    let trailing_start = token
        .char_indices()
        .rev()
        .find(|(_, ch)| ch.is_ascii_alphanumeric() || *ch == '/')
        .map(|(index, ch)| index + ch.len_utf8())
        .unwrap_or(leading_len);

    if leading_len >= trailing_start {
        return token.to_string();
    }

    let leading = &token[..leading_len];
    let candidate = &token[leading_len..trailing_start];
    let trailing = &token[trailing_start..];
    if let Some(redacted) = redact_assignment_candidate(candidate) {
        return format!("{leading}{redacted}{trailing}");
    }
    let Some(redacted) = redact_url_candidate(candidate) else {
        return token.to_string();
    };

    format!("{leading}{redacted}{trailing}")
}

fn redact_assignment_candidate(candidate: &str) -> Option<String> {
    for separator in ['=', ':'] {
        let Some((key, value)) = candidate.split_once(separator) else {
            continue;
        };
        let normalized_key = key
            .trim()
            .trim_start_matches('-')
            .trim_matches(|ch: char| matches!(ch, '"' | '\'' | '{' | '['))
            .trim_end_matches(['"', '\''])
            .to_ascii_lowercase();
        if !is_sensitive_query_key(&normalized_key) {
            continue;
        }
        let quote = value
            .chars()
            .find(|ch| !ch.is_whitespace())
            .filter(|ch| matches!(ch, '"' | '\''));
        let replacement = match quote {
            Some(quote) => format!("{key}{separator}{quote}REDACTED{quote}"),
            None => format!("{key}{separator}REDACTED"),
        };
        return Some(replacement);
    }

    None
}

fn redact_url_candidate(candidate: &str) -> Option<String> {
    let mut url = Url::parse(candidate).ok()?;
    let mut changed = false;

    if url.password().is_some() && url.set_password(Some("REDACTED")).is_ok() {
        changed = true;
    }

    if url.query().is_some() {
        let pairs = url
            .query_pairs()
            .map(|(key, value)| {
                let redacted = if is_sensitive_query_key(&key) {
                    changed = true;
                    String::from("REDACTED")
                } else {
                    value.into_owned()
                };
                (key.into_owned(), redacted)
            })
            .collect::<Vec<_>>();

        if changed {
            url.query_pairs_mut().clear().extend_pairs(pairs);
        }
    }

    changed.then(|| url.to_string())
}

fn is_sensitive_query_key(key: &str) -> bool {
    let normalized = key.trim().to_ascii_lowercase();
    matches!(
        normalized.as_str(),
        "password"
            | "passwd"
            | "pwd"
            | "passphrase"
            | "token"
            | "secret"
            | "api_key"
            | "apikey"
            | "access_token"
            | "refresh_token"
            | "client_secret"
            | "private_key"
            | "sslkey"
            | "ssl_key"
            | "ssl-key"
            | "tls_client_key"
            | "tls_client_key_path"
            | "client_key"
            | "client_key_path"
    ) || normalized.contains("password")
        || normalized.contains("secret")
        || normalized.ends_with("token")
        || normalized.contains("private_key")
        || normalized.contains("client_key")
}

async fn handle_query(
    context: &RuntimeContext,
    command: QueryCommand,
    source: &str,
) -> Result<(), CliError> {
    let render = resolve_render(context, command.render);
    let connection = resolve_connection(context, &command.connection).await?;
    let sql = read_query_input(command.sql.as_deref())?;
    let statements = split_sql_statements(&sql);
    if statements.is_empty() {
        return Err(CliError::validation("SQL input is empty."));
    }
    if command.transaction && command.continue_on_error {
        return Err(CliError::validation(
            "`--continue-on-error` is not compatible with `--transaction`.",
        ));
    }
    confirm_destructive_execution(context, &connection, source, &statements)?;

    context.print_debug(&format!(
        "running {} statement(s) from {source}",
        statements.len()
    ));

    let results = if command.transaction {
        run_statements_transactional(&connection, &statements, render.max_rows).await?
    } else if command.continue_on_error {
        let batch = run_statements_non_transactional_batch(
            &connection,
            &statements,
            false,
            render.max_rows,
            source,
        )
        .await;
        append_history_batch(
            &connection,
            source,
            &statements,
            if batch.errors.is_empty() {
                "ok"
            } else {
                "error"
            },
        )?;
        render_statement_results(&batch.results, &render)?;
        return batch.into_result().map(|_| ());
    } else {
        run_statements_non_transactional(&connection, &statements, true, render.max_rows, source)
            .await?
    };

    append_history_batch(&connection, source, &statements, "ok")?;
    render_statement_results(&results, &render)
}

async fn handle_run(context: &RuntimeContext, command: RunCommand) -> Result<(), CliError> {
    let render = resolve_render(context, command.render);
    let connection = resolve_connection(context, &command.connection).await?;
    let sql = read_run_input(command.file.as_deref())?;
    let statements = split_sql_statements(&sql);
    if statements.is_empty() {
        return Err(CliError::validation("SQL input is empty."));
    }
    if command.stop_on_error && command.continue_on_error {
        return Err(CliError::validation(
            "`--stop-on-error` and `--continue-on-error` cannot be used together.",
        ));
    }
    if command.transaction && command.continue_on_error {
        return Err(CliError::validation(
            "`--continue-on-error` is not compatible with `--transaction`.",
        ));
    }

    context.print_debug(&format!(
        "running {} statement(s) from run",
        statements.len()
    ));

    if command.dry_run {
        let preview = statements
            .iter()
            .enumerate()
            .map(|(index, statement)| {
                vec![
                    (index + 1).to_string(),
                    statement_kind(statement).to_string(),
                    preview_sql(statement),
                ]
            })
            .collect::<Vec<_>>();
        return render_query_output(
            QueryOutput::Rows(simple_result_set(
                vec!["statement", "kind", "preview"],
                preview,
            )),
            &render,
        );
    }
    confirm_destructive_execution(context, &connection, "run", &statements)?;

    if command.continue_on_error {
        let batch = run_statements_non_transactional_batch(
            &connection,
            &statements,
            false,
            render.max_rows,
            "run",
        )
        .await;
        append_history_batch(
            &connection,
            "run",
            &statements,
            if batch.errors.is_empty() {
                "ok"
            } else {
                "error"
            },
        )?;
        render_statement_results(&batch.results, &render)?;
        return batch.into_result().map(|_| ());
    }

    let result = if command.transaction {
        run_statements_transactional(&connection, &statements, render.max_rows).await
    } else {
        run_statements_non_transactional(
            &connection,
            &statements,
            command.stop_on_error,
            render.max_rows,
            "run",
        )
        .await
    };

    match result {
        Ok(results) => {
            append_history_batch(&connection, "run", &statements, "ok")?;
            render_statement_results(&results, &render)
        }
        Err(error) => {
            append_history_batch(&connection, "run", &statements, "error")?;
            Err(error)
        }
    }
}

async fn handle_shell(context: &RuntimeContext, command: ShellCommand) -> Result<(), CliError> {
    let mut connection = resolve_connection(context, &command.connection).await?;
    let mut format = command.format.unwrap_or(OutputFormat::Table);
    let history_path = config_artifact_path(CLI_SHELL_HISTORY_FILE);
    let completion_state = Arc::new(Mutex::new(ShellCompletionState::default()));
    let helper = ShellHelper {
        state: completion_state.clone(),
    };
    let config = LineConfig::builder()
        .completion_type(CompletionType::List)
        .build();
    let mut editor = Editor::<ShellHelper, DefaultHistory>::with_config(config)
        .map_err(|error| CliError::general(error.to_string()))?;
    editor.set_helper(Some(helper));
    let _ = editor.load_history(&history_path);

    let profiles = saved_connection_selectors(&load_connection_store().0);
    let objects = shell_objects(&connection).await.unwrap_or_default();
    if let Ok(mut state) = completion_state.lock() {
        state.commands = shell_commands();
        state.profiles = profiles;
        state.objects = objects;
    }

    let mut timing = false;

    loop {
        let line = editor.readline("cryo> ");
        match line {
            Ok(line) => {
                let input = line.trim();
                if input.is_empty() {
                    continue;
                }
                let _ = editor.add_history_entry(input);

                if input.starts_with('\\') {
                    let action =
                        handle_shell_meta(context, &connection, input, format, timing).await?;
                    match action {
                        ShellAction::Continue => {}
                        ShellAction::Exit => break,
                        ShellAction::SetFormat(next) => format = next,
                        ShellAction::SetTiming(next) => timing = next,
                        ShellAction::Reconnect(next) => {
                            connection = *next;
                            let objects = shell_objects(&connection).await.unwrap_or_default();
                            if let Ok(mut state) = completion_state.lock() {
                                state.objects = objects;
                            }
                        }
                    }
                    continue;
                }

                let sql = input.to_string();
                if let Err(error) = confirm_destructive_execution(
                    context,
                    &connection,
                    "shell",
                    std::slice::from_ref(&sql),
                ) {
                    context.print_error(&error.message);
                    continue;
                }
                let start = Instant::now();
                let statement_result = run_statements_non_transactional(
                    &connection,
                    std::slice::from_ref(&sql),
                    true,
                    None,
                    "shell",
                )
                .await;

                match statement_result {
                    Ok(results) => {
                        append_history_batch(&connection, "shell", &[sql], "ok")?;
                        render_statement_results(
                            &results,
                            &ResolvedRender {
                                format,
                                out: None,
                                no_header: false,
                                single_value: false,
                                max_rows: None,
                                max_output_size: None,
                                out_temp: false,
                                single_result: false,
                                pager: false,
                                no_color: false,
                            },
                        )?;
                        if timing {
                            context.print_info(&format!("{} ms", start.elapsed().as_millis()));
                        }
                    }
                    Err(error) => {
                        append_history_batch(&connection, "shell", &[sql], "error")?;
                        context.print_error(&error.message);
                    }
                }
            }
            Err(ReadlineError::Interrupted) => continue,
            Err(ReadlineError::Eof) => break,
            Err(error) => return Err(CliError::general(error.to_string())),
        }
    }

    let _ = editor.save_history(&history_path);
    Ok(())
}

async fn handle_tui(context: &RuntimeContext, command: TuiCommand) -> Result<(), CliError> {
    let has_connection = command.connection.connection.is_some()
        || command.connection.url.is_some()
        || command.connection.sqlite.is_some()
        || context.env.get("CRYODB_CONNECTION").is_some();
    let connection = if has_connection {
        Some(resolve_connection(context, &command.connection).await?)
    } else {
        None
    };
    crate::tui::run_tui(connection, command.max_rows)
        .await
        .map_err(|error| CliError::general(error.to_string()))
}

enum ShellAction {
    Continue,
    Exit,
    SetFormat(OutputFormat),
    SetTiming(bool),
    Reconnect(Box<ResolvedConnection>),
}

async fn handle_shell_meta(
    context: &RuntimeContext,
    connection: &ResolvedConnection,
    input: &str,
    current_format: OutputFormat,
    current_timing: bool,
) -> Result<ShellAction, CliError> {
    let parts = input.split_whitespace().collect::<Vec<_>>();
    let command = parts.first().copied().unwrap_or("");
    match command {
        "\\exit" | "\\q" => Ok(ShellAction::Exit),
        "\\clear" => {
            write!(io::stdout().lock(), "\x1b[2J\x1b[H")
                .map_err(|error| CliError::general(error.to_string()))?;
            Ok(ShellAction::Continue)
        }
        "\\tables" => {
            let render = ResolvedRender {
                format: current_format,
                out: None,
                no_header: false,
                single_value: false,
                max_rows: None,
                max_output_size: None,
                out_temp: false,
                single_result: false,
                pager: false,
                no_color: false,
            };
            let output = run_query_with_control(
                connection.pool.clone(),
                connection.database.clone(),
                tables_query(
                    connection.driver,
                    connection.database.as_deref(),
                    None,
                    None,
                ),
                None,
                None,
            )
            .await
            .map_err(CliError::sql)?;
            render_query_output(output, &render)?;
            Ok(ShellAction::Continue)
        }
        "\\views" => {
            let render = ResolvedRender {
                format: current_format,
                out: None,
                no_header: false,
                single_value: false,
                max_rows: None,
                max_output_size: None,
                out_temp: false,
                single_result: false,
                pager: false,
                no_color: false,
            };
            let output = run_query_with_control(
                connection.pool.clone(),
                connection.database.clone(),
                views_query(
                    connection.driver,
                    connection.database.as_deref(),
                    None,
                    None,
                ),
                None,
                None,
            )
            .await
            .map_err(CliError::sql)?;
            render_query_output(output, &render)?;
            Ok(ShellAction::Continue)
        }
        "\\schemas" => {
            let render = ResolvedRender {
                format: current_format,
                out: None,
                no_header: false,
                single_value: false,
                max_rows: None,
                max_output_size: None,
                out_temp: false,
                single_result: false,
                pager: false,
                no_color: false,
            };
            let output = run_query_with_control(
                connection.pool.clone(),
                connection.database.clone(),
                schemas_query(connection.driver),
                None,
                None,
            )
            .await
            .map_err(CliError::sql)?;
            render_query_output(output, &render)?;
            Ok(ShellAction::Continue)
        }
        "\\describe" => {
            let table = parts
                .get(1)
                .copied()
                .ok_or_else(|| CliError::validation("Usage: \\describe <table>"))?;
            let bundle = describe_bundle(connection, table).await?;
            render_describe_bundle(
                table,
                bundle,
                &ResolvedRender {
                    format: current_format,
                    out: None,
                    no_header: false,
                    single_value: false,
                    max_rows: None,
                    max_output_size: None,
                    out_temp: false,
                    single_result: false,
                    pager: false,
                    no_color: false,
                },
            )?;
            Ok(ShellAction::Continue)
        }
        "\\schema" => {
            let table = parts.get(1).copied();
            if let Some(table) = table {
                let bundle = describe_bundle(connection, table).await?;
                render_schema_bundle_markdown(table, &bundle, &mut io::stdout().lock())
                    .map_err(|error| CliError::general(error.to_string()))?;
            } else {
                let objects = shell_objects(connection).await?;
                let mut stdout = io::stdout().lock();
                for (index, object) in objects.iter().enumerate() {
                    if index > 0 {
                        writeln!(stdout).map_err(|error| CliError::general(error.to_string()))?;
                    }
                    let bundle = describe_bundle(connection, object).await?;
                    render_schema_bundle_markdown(object, &bundle, &mut stdout)
                        .map_err(|error| CliError::general(error.to_string()))?;
                }
            }
            Ok(ShellAction::Continue)
        }
        "\\indexes" => {
            let table = parts.get(1).copied();
            let render = ResolvedRender {
                format: current_format,
                out: None,
                no_header: false,
                single_value: false,
                max_rows: None,
                max_output_size: None,
                out_temp: false,
                single_result: false,
                pager: false,
                no_color: false,
            };
            let output = run_query_with_control(
                connection.pool.clone(),
                connection.database.clone(),
                indexes_query(connection.driver, connection.database.as_deref(), table),
                None,
                None,
            )
            .await
            .map_err(CliError::sql)?;
            render_query_output(output, &render)?;
            Ok(ShellAction::Continue)
        }
        "\\relations" => {
            let render = ResolvedRender {
                format: current_format,
                out: None,
                no_header: false,
                single_value: false,
                max_rows: None,
                max_output_size: None,
                out_temp: false,
                single_result: false,
                pager: false,
                no_color: false,
            };
            let rows = fetch_sidebar_relations(
                connection.pool.clone(),
                connection.database.clone().unwrap_or_default(),
            )
            .await
            .map_err(CliError::sql)?
            .into_iter()
            .map(|entry| {
                vec![
                    entry.table,
                    entry.relation.column,
                    entry.relation.referenced_table,
                    entry.relation.referenced_column,
                ]
            })
            .collect::<Vec<_>>();
            render_query_output(
                QueryOutput::Rows(simple_result_set(
                    vec!["table", "column", "referenced_table", "referenced_column"],
                    rows,
                )),
                &render,
            )?;
            Ok(ShellAction::Continue)
        }
        "\\constraints" => {
            let table = parts.get(1).copied();
            let render = ResolvedRender {
                format: current_format,
                out: None,
                no_header: false,
                single_value: false,
                max_rows: None,
                max_output_size: None,
                out_temp: false,
                single_result: false,
                pager: false,
                no_color: false,
            };
            let output = match connection.driver {
                DatabaseDriver::Sqlite => sqlite_constraints_output(connection, table).await?,
                _ => run_query_with_control(
                    connection.pool.clone(),
                    connection.database.clone(),
                    constraints_query(connection.driver, connection.database.as_deref(), table),
                    None,
                    None,
                )
                .await
                .map_err(CliError::sql)?,
            };
            render_query_output(output, &render)?;
            Ok(ShellAction::Continue)
        }
        "\\size" => {
            let render = ResolvedRender {
                format: current_format,
                out: None,
                no_header: false,
                single_value: false,
                max_rows: None,
                max_output_size: None,
                out_temp: false,
                single_result: false,
                pager: false,
                no_color: false,
            };
            let output = match connection.driver {
                DatabaseDriver::Sqlite => sqlite_size_output(connection).await?,
                _ => run_query_with_control(
                    connection.pool.clone(),
                    connection.database.clone(),
                    size_query(connection.driver, connection.database.as_deref()),
                    None,
                    None,
                )
                .await
                .map_err(CliError::sql)?,
            };
            render_query_output(output, &render)?;
            Ok(ShellAction::Continue)
        }
        "\\search" => {
            let needle = parts
                .get(1)
                .copied()
                .ok_or_else(|| CliError::validation("Usage: \\search <text>"))?;
            let render = ResolvedRender {
                format: current_format,
                out: None,
                no_header: false,
                single_value: false,
                max_rows: None,
                max_output_size: None,
                out_temp: false,
                single_result: false,
                pager: false,
                no_color: false,
            };
            let output = match connection.driver {
                DatabaseDriver::Sqlite => sqlite_search_output(connection, needle).await?,
                _ => run_query_with_control(
                    connection.pool.clone(),
                    connection.database.clone(),
                    search_query(connection.driver, connection.database.as_deref(), needle),
                    None,
                    None,
                )
                .await
                .map_err(CliError::sql)?,
            };
            render_query_output(output, &render)?;
            Ok(ShellAction::Continue)
        }
        "\\help" | "\\?" => {
            for command in shell_commands() {
                writeln!(io::stdout().lock(), "{command}")
                    .map_err(|error| CliError::general(error.to_string()))?;
            }
            Ok(ShellAction::Continue)
        }
        "\\format" => {
            let raw = parts.get(1).copied().unwrap_or("");
            let format = parse_output_format(raw).ok_or_else(|| {
                CliError::validation(
                    "Supported formats: table, json, ndjson, csv, tsv, markdown, raw, pretty",
                )
            })?;
            Ok(ShellAction::SetFormat(format))
        }
        "\\timing" => {
            let next = parts
                .get(1)
                .map(|value| value.eq_ignore_ascii_case("on"))
                .unwrap_or(!current_timing);
            Ok(ShellAction::SetTiming(next))
        }
        "\\history" => {
            let command = HistoryCommand {
                connection: connection.profile_name.clone(),
                search: None,
                run: None,
                clear: false,
                limit: 20,
                render: RenderArgs {
                    format: Some(current_format),
                    out: None,
                    raw: false,
                    no_header: false,
                    single_value: false,
                    max_rows: None,
                    max_output_size: None,
                    out_temp: false,
                    single_result: false,
                },
            };
            handle_history(context, command).await?;
            Ok(ShellAction::Continue)
        }
        "\\connect" => {
            let name = parts
                .get(1)
                .copied()
                .ok_or_else(|| CliError::validation("Usage: \\connect <connection>"))?;
            let next = resolve_connection(
                context,
                &ConnectionSelectorArgs {
                    connection: Some(name.to_string()),
                    url: None,
                    sqlite: None,
                    database: None,
                },
            )
            .await?;
            Ok(ShellAction::Reconnect(Box::new(next)))
        }
        _ => Err(CliError::validation("Unknown shell command.")),
    }
}

async fn handle_conn(context: &RuntimeContext, command: ConnectionCommand) -> Result<(), CliError> {
    match command.command {
        ConnectionSubcommands::List => handle_conn_list().await,
        ConnectionSubcommands::Add(command) => handle_conn_add(context, command).await,
        ConnectionSubcommands::Edit(command) => handle_conn_edit(context, command).await,
        ConnectionSubcommands::Remove(command) => handle_conn_remove(context, command).await,
        ConnectionSubcommands::Test(command) => handle_conn_test(context, command).await,
        ConnectionSubcommands::Show(command) => handle_conn_show(command).await,
        ConnectionSubcommands::SetDefault(command) => handle_conn_set_default(command).await,
    }
}

async fn handle_conn_list() -> Result<(), CliError> {
    let (store, warning) = load_connection_store();
    if let Some(warning) = warning {
        let _ = writeln!(io::stderr().lock(), "{}", warning);
    }
    let rows = connection_catalog(&store)
        .iter()
        .map(|entry| {
            let has_secret = stored_connection_has_secret(&entry.entry);
            vec![
                entry.selector.clone(),
                entry.entry.driver.to_string(),
                entry.entry.display_label(),
                saved_connection_source_text(&entry.sources),
                if store
                    .default_connection
                    .as_deref()
                    .is_some_and(|value| value.eq_ignore_ascii_case(&entry.selector))
                {
                    String::from("yes")
                } else {
                    String::new()
                },
                if has_secret {
                    String::from("yes")
                } else {
                    String::from("no")
                },
            ]
        })
        .collect::<Vec<_>>();
    render_query_output(
        QueryOutput::Rows(simple_result_set(
            vec!["name", "driver", "target", "source", "default", "password"],
            rows,
        )),
        &ResolvedRender {
            format: OutputFormat::Table,
            out: None,
            no_header: false,
            single_value: false,
            max_rows: None,
            max_output_size: None,
            out_temp: false,
            single_result: false,
            pager: false,
            no_color: false,
        },
    )
}

async fn handle_conn_add(
    context: &RuntimeContext,
    command: ConnectionUpsertCommand,
) -> Result<(), CliError> {
    let mut store = load_connection_store().0;
    let entry = build_profile_entry(context, command.name, None, command.input).await?;
    if entry.profile_name().is_none() {
        return Err(CliError::validation("Profile name is required."));
    }
    upsert_profile(&mut store, entry);
    persist_connection_store_file(&mut store)?;
    Ok(())
}

async fn handle_conn_edit(
    context: &RuntimeContext,
    command: ConnectionEditCommand,
) -> Result<(), CliError> {
    let mut store = load_connection_store().0;
    let selection = resolve_saved_connection(&store, &command.name)?;
    let current = selection.entry.clone();
    let default_matches = default_targets_connection(store.default_connection.as_deref(), &current);
    let name = command
        .rename
        .clone()
        .unwrap_or_else(|| current.profile_name().unwrap_or("").to_string());
    let entry = build_profile_entry(context, name, Some(current.clone()), command.input).await?;

    if selection.exact_named && selection.sources.contains(&SavedConnectionSource::Profile) {
        let profile_name = current.profile_name().unwrap_or("").to_string();
        let index = store
            .profiles
            .iter()
            .position(|item| {
                item.profile_name()
                    .is_some_and(|value| value.eq_ignore_ascii_case(&profile_name))
            })
            .ok_or_else(|| {
                CliError::validation(format!("Profile `{}` was not found.", command.name))
            })?;
        store.profiles.remove(index);
        upsert_profile(&mut store, entry.clone());
    } else {
        replace_gui_connections_by_identity(&mut store, &current, &entry);
    }

    clear_removed_connection_secret_if_unused(&store, &current);
    if default_matches {
        store.default_connection = Some(entry.profile_label());
    }
    persist_connection_store_file(&mut store)?;
    Ok(())
}

async fn handle_conn_remove(
    context: &RuntimeContext,
    command: ConnectionNamedCommand,
) -> Result<(), CliError> {
    let mut store = load_connection_store().0;
    let selection = resolve_saved_connection(&store, &command.name)?;
    let removed = selection.entry.clone();
    let default_matches = default_targets_connection(store.default_connection.as_deref(), &removed);
    confirm_action(
        context,
        &format!("Remove connection profile `{}`", selection.selector),
        "connection_remove",
    )?;

    if selection.exact_named && selection.sources.contains(&SavedConnectionSource::Profile) {
        let profile_name = removed.profile_name().unwrap_or("").to_string();
        store.profiles.retain(|entry| {
            !entry
                .profile_name()
                .is_some_and(|value| value.eq_ignore_ascii_case(&profile_name))
        });
    } else {
        remove_gui_connections_by_identity(&mut store, &removed);
    }

    clear_removed_connection_secret_if_unused(&store, &removed);
    if default_matches {
        store.default_connection = None;
    }
    persist_connection_store_file(&mut store)?;
    Ok(())
}

async fn handle_conn_test(
    context: &RuntimeContext,
    command: ConnectionNamedCommand,
) -> Result<(), CliError> {
    let connection = resolve_connection(
        context,
        &ConnectionSelectorArgs {
            connection: Some(command.name),
            url: None,
            sqlite: None,
            database: None,
        },
    )
    .await?;
    let start = Instant::now();
    run_query_with_control(
        connection.pool,
        connection.database,
        String::from("SELECT 1"),
        None,
        Some(1),
    )
    .await
    .map_err(CliError::sql)?;
    writeln!(io::stdout().lock(), "ok\t{}ms", start.elapsed().as_millis())
        .map_err(|error| CliError::general(error.to_string()))
}

async fn handle_conn_show(command: ConnectionNamedCommand) -> Result<(), CliError> {
    let store = load_connection_store().0;
    let selection = resolve_saved_connection(&store, &command.name)?;
    let entry = &selection.entry;
    let has_secret = stored_connection_has_secret(entry);
    let rows = vec![
        vec![String::from("name"), entry.profile_label()],
        vec![
            String::from("source"),
            saved_connection_source_text(&selection.sources),
        ],
        vec![String::from("driver"), entry.driver.to_string()],
        vec![String::from("host"), entry.host.clone()],
        vec![String::from("port"), entry.port.clone()],
        vec![String::from("database"), entry.database.clone()],
        vec![String::from("username"), entry.username.clone()],
        vec![String::from("sqlite"), entry.sqlite_path.clone()],
        vec![String::from("tls_mode"), entry.tls_mode.to_string()],
        vec![
            String::from("tls_ca_cert_path"),
            entry.tls_ca_cert_path.clone(),
        ],
        vec![
            String::from("tls_client_cert_path"),
            entry.tls_client_cert_path.clone(),
        ],
        vec![
            String::from("tls_client_key_path"),
            redact_sensitive_text(&entry.tls_client_key_path),
        ],
        vec![
            String::from("password"),
            if has_secret {
                String::from("stored")
            } else {
                String::from("missing")
            },
        ],
    ];
    render_query_output(
        QueryOutput::Rows(simple_result_set(vec!["field", "value"], rows)),
        &ResolvedRender {
            format: OutputFormat::Table,
            out: None,
            no_header: false,
            single_value: false,
            max_rows: None,
            max_output_size: None,
            out_temp: false,
            single_result: false,
            pager: false,
            no_color: false,
        },
    )
}

async fn handle_conn_set_default(command: ConnectionNamedCommand) -> Result<(), CliError> {
    let mut store = load_connection_store().0;
    let selection = resolve_saved_connection(&store, &command.name)?;
    store.default_connection = Some(selection.selector);
    persist_connection_store_file(&mut store)?;
    Ok(())
}

async fn handle_databases(
    context: &RuntimeContext,
    command: ListSimpleCommand,
) -> Result<(), CliError> {
    let render = resolve_render(context, command.render);
    let connection = resolve_connection(context, &command.connection).await?;
    let rows = fetch_databases(connection.pool)
        .await
        .map_err(CliError::sql)?
        .into_iter()
        .map(|name| vec![name])
        .collect::<Vec<_>>();
    render_query_output(
        QueryOutput::Rows(simple_result_set(vec!["database"], rows)),
        &render,
    )
}

async fn handle_schemas(
    context: &RuntimeContext,
    command: ListSimpleCommand,
) -> Result<(), CliError> {
    let render = resolve_render(context, command.render);
    let connection = resolve_connection(context, &command.connection).await?;
    let output = run_query_with_control(
        connection.pool,
        connection.database,
        schemas_query(connection.driver),
        None,
        render.max_rows,
    )
    .await
    .map_err(CliError::sql)?;
    render_query_output(output, &render)
}

async fn handle_tables(
    context: &RuntimeContext,
    command: ListObjectsCommand,
) -> Result<(), CliError> {
    let render = resolve_render(context, command.render);
    let connection = resolve_connection(context, &command.connection).await?;
    let output = run_query_with_control(
        connection.pool,
        connection.database.clone(),
        tables_query(
            connection.driver,
            connection.database.as_deref(),
            command.schema.as_deref(),
            command.pattern.as_deref(),
        ),
        None,
        render.max_rows,
    )
    .await
    .map_err(CliError::sql)?;
    render_query_output(output, &render)
}

async fn handle_views(
    context: &RuntimeContext,
    command: ListObjectsCommand,
) -> Result<(), CliError> {
    let render = resolve_render(context, command.render);
    let connection = resolve_connection(context, &command.connection).await?;
    let output = run_query_with_control(
        connection.pool,
        connection.database.clone(),
        views_query(
            connection.driver,
            connection.database.as_deref(),
            command.schema.as_deref(),
            command.pattern.as_deref(),
        ),
        None,
        render.max_rows,
    )
    .await
    .map_err(CliError::sql)?;
    render_query_output(output, &render)
}

async fn handle_describe(
    context: &RuntimeContext,
    command: DescribeCommand,
) -> Result<(), CliError> {
    let render = resolve_render(context, command.render);
    let connection = resolve_connection(context, &command.connection).await?;
    let bundle = describe_bundle(&connection, &command.table).await?;
    render_describe_bundle(&command.table, bundle, &render)
}

async fn handle_schema(context: &RuntimeContext, command: SchemaCommand) -> Result<(), CliError> {
    let connection = resolve_connection(context, &command.connection).await?;
    match command.output {
        SchemaOutputFormat::Sql => {
            let ddl = schema_sql_output(&connection, command.table.as_deref()).await?;
            writeln!(io::stdout().lock(), "{}", ddl)
                .map_err(|error| CliError::general(error.to_string()))
        }
        SchemaOutputFormat::Markdown => {
            let mut stdout = io::stdout().lock();
            if let Some(table) = command.table.as_deref() {
                let bundle = describe_bundle(&connection, table).await?;
                render_schema_bundle_markdown(table, &bundle, &mut stdout)
                    .map_err(|error| CliError::general(error.to_string()))?;
            } else {
                let objects = shell_objects(&connection).await?;
                for (index, object) in objects.iter().enumerate() {
                    if index > 0 {
                        writeln!(stdout).map_err(|error| CliError::general(error.to_string()))?;
                    }
                    let bundle = describe_bundle(&connection, object).await?;
                    render_schema_bundle_markdown(object, &bundle, &mut stdout)
                        .map_err(|error| CliError::general(error.to_string()))?;
                }
            }
            Ok(())
        }
        SchemaOutputFormat::Json => {
            let value = if let Some(table) = command.table.as_deref() {
                let bundle = describe_bundle(&connection, table).await?;
                json!({ table: describe_bundle_to_json(&bundle) })
            } else {
                let objects = shell_objects(&connection).await?;
                let mut map = serde_json::Map::new();
                for object in objects {
                    let bundle = describe_bundle(&connection, &object).await?;
                    map.insert(object, describe_bundle_to_json(&bundle));
                }
                Value::Object(map)
            };
            serde_json::to_writer_pretty(io::stdout().lock(), &value)
                .map_err(|error| CliError::general(error.to_string()))?;
            writeln!(io::stdout().lock()).map_err(|error| CliError::general(error.to_string()))
        }
    }
}

async fn handle_docs(context: &RuntimeContext, command: DocsCommand) -> Result<(), CliError> {
    let connection = resolve_connection(context, &command.connection).await?;
    let docs = collect_docs_bundles(&connection, command.table.as_deref()).await?;
    match command.output {
        DocsOutputFormat::Markdown => {
            let mut buffer = Vec::new();
            writeln!(buffer, "# CryoDB Database Documentation")
                .map_err(|error| CliError::general(error.to_string()))?;
            writeln!(buffer).map_err(|error| CliError::general(error.to_string()))?;
            writeln!(
                buffer,
                "- generated_at: {}",
                sqlx::types::chrono::Utc::now().to_rfc3339()
            )
            .map_err(|error| CliError::general(error.to_string()))?;
            writeln!(buffer, "- connection: {}", connection.label)
                .map_err(|error| CliError::general(error.to_string()))?;
            writeln!(buffer).map_err(|error| CliError::general(error.to_string()))?;
            for (index, (object, bundle)) in docs.iter().enumerate() {
                if index > 0 {
                    writeln!(buffer).map_err(|error| CliError::general(error.to_string()))?;
                }
                render_schema_bundle_markdown(object, bundle, &mut buffer)
                    .map_err(|error| CliError::general(error.to_string()))?;
            }
            write_document_bytes(
                &buffer,
                command.out.as_deref(),
                command.out_temp || context.env.get_bool("CRYODB_OUT_TEMP"),
                ".md",
            )
        }
        DocsOutputFormat::Html => {
            let html = render_docs_html(&connection, &docs)?;
            write_document_bytes(
                html.as_bytes(),
                command.out.as_deref(),
                command.out_temp || context.env.get_bool("CRYODB_OUT_TEMP"),
                ".html",
            )
        }
        DocsOutputFormat::Json => {
            let mut objects = serde_json::Map::new();
            for (object, bundle) in docs {
                objects.insert(object, describe_bundle_to_json(&bundle));
            }
            let value = json!({
                "generated_at": sqlx::types::chrono::Utc::now().to_rfc3339(),
                "connection": connection.label,
                "driver": connection.driver.storage_key(),
                "objects": Value::Object(objects),
            });
            let render = ResolvedRender {
                format: OutputFormat::Json,
                out: command.out,
                no_header: false,
                single_value: false,
                max_rows: None,
                max_output_size: None,
                out_temp: command.out_temp || context.env.get_bool("CRYODB_OUT_TEMP"),
                single_result: false,
                pager: false,
                no_color: false,
            };
            write_value_to_output(&value, &render)
        }
    }
}

async fn collect_docs_bundles(
    connection: &ResolvedConnection,
    table: Option<&str>,
) -> Result<Vec<(String, DescribeBundle)>, CliError> {
    let objects = if let Some(table) = table {
        vec![table.to_string()]
    } else {
        shell_objects(connection).await?
    };
    let mut docs = Vec::new();
    for object in objects {
        docs.push((object.clone(), describe_bundle(connection, &object).await?));
    }
    Ok(docs)
}

fn write_document_bytes(
    bytes: &[u8],
    out: Option<&Path>,
    out_temp: bool,
    suffix: &str,
) -> Result<(), CliError> {
    if out_temp && out.is_none() {
        let file = tempfile::Builder::new()
            .prefix("cryodb-docs-")
            .suffix(suffix)
            .tempfile()
            .map_err(|error| CliError::general(error.to_string()))?;
        let path = file.path().to_path_buf();
        std::fs::write(&path, bytes).map_err(|error| CliError::general(error.to_string()))?;
        let kept = file
            .into_temp_path()
            .keep()
            .map_err(|error| CliError::general(error.error.to_string()))?;
        return writeln!(io::stdout().lock(), "{}", kept.display())
            .map_err(|error| CliError::general(error.to_string()));
    }
    if let Some(path) = out.filter(|path| *path != Path::new("-")) {
        std::fs::write(path, bytes).map_err(|error| CliError::general(error.to_string()))
    } else {
        io::stdout()
            .lock()
            .write_all(bytes)
            .map_err(|error| CliError::general(error.to_string()))
    }
}

fn render_docs_html(
    connection: &ResolvedConnection,
    docs: &[(String, DescribeBundle)],
) -> Result<String, CliError> {
    let mut html = String::from(
        "<!doctype html><html><head><meta charset=\"utf-8\"><title>CryoDB Docs</title><style>body{font-family:system-ui,sans-serif;max-width:1100px;margin:40px auto;padding:0 24px;line-height:1.45;color:#18202a}table{border-collapse:collapse;width:100%;margin:12px 0 28px}th,td{border:1px solid #ccd3dd;padding:6px 8px;text-align:left;vertical-align:top}th{background:#edf2f7}code{background:#edf2f7;padding:2px 4px;border-radius:4px}h1,h2,h3{line-height:1.1}</style></head><body>",
    );
    html.push_str("<h1>CryoDB Database Documentation</h1>");
    html.push_str(&format!(
        "<p><strong>Generated:</strong> {}</p><p><strong>Connection:</strong> <code>{}</code></p>",
        escape_html(&sqlx::types::chrono::Utc::now().to_rfc3339()),
        escape_html(&connection.label)
    ));
    for (object, bundle) in docs {
        html.push_str(&format!("<h2>{}</h2>", escape_html(object)));
        if let Some(details) = &bundle.details {
            html.push_str("<h3>Details</h3><ul>");
            for (field, value) in &details.rows {
                if !value.is_empty() {
                    html.push_str(&format!(
                        "<li><strong>{}</strong>: {}</li>",
                        escape_html(field),
                        escape_html(value)
                    ));
                }
            }
            html.push_str("</ul>");
        }
        if let Some(columns) = &bundle.columns {
            html.push_str("<h3>Columns</h3>");
            html.push_str(&query_output_to_html(columns));
        }
        if let Some(indexes) = &bundle.indexes {
            html.push_str("<h3>Indexes</h3>");
            html.push_str(&query_output_to_html(indexes));
        }
        if let Some(relations) = &bundle.relations {
            html.push_str("<h3>Relations</h3>");
            html.push_str(&query_output_to_html(relations));
        }
        if let Some(constraints) = &bundle.constraints {
            html.push_str("<h3>Constraints</h3>");
            html.push_str(&query_output_to_html(constraints));
        }
    }
    html.push_str("</body></html>\n");
    Ok(html)
}

fn query_output_to_html(output: &QueryOutput) -> String {
    match output {
        QueryOutput::Affected(count) => format!("<p>Rows affected: {count}</p>"),
        QueryOutput::Rows(rows) => {
            let mut html = String::from("<table><thead><tr>");
            for column in &rows.columns {
                html.push_str(&format!("<th>{}</th>", escape_html(column)));
            }
            html.push_str("</tr></thead><tbody>");
            for row in &rows.rows {
                html.push_str("<tr>");
                for value in row {
                    html.push_str(&format!("<td>{}</td>", escape_html(value)));
                }
                html.push_str("</tr>");
            }
            html.push_str("</tbody></table>");
            html
        }
    }
}

fn escape_html(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

async fn handle_indexes(context: &RuntimeContext, command: IndexesCommand) -> Result<(), CliError> {
    let render = resolve_render(context, command.render);
    let connection = resolve_connection(context, &command.connection).await?;
    let output = run_query_with_control(
        connection.pool,
        connection.database.clone(),
        indexes_query(
            connection.driver,
            connection.database.as_deref(),
            command.table.as_deref(),
        ),
        None,
        render.max_rows,
    )
    .await
    .map_err(CliError::sql)?;
    render_query_output(output, &render)
}

async fn handle_relations(
    context: &RuntimeContext,
    command: RelationsCommand,
) -> Result<(), CliError> {
    let render = resolve_render(context, command.render);
    let connection = resolve_connection(context, &command.connection).await?;
    let rows = fetch_sidebar_relations(connection.pool, connection.database.unwrap_or_default())
        .await
        .map_err(CliError::sql)?
        .into_iter()
        .map(|entry| {
            vec![
                entry.table,
                entry.relation.column,
                entry.relation.referenced_table,
                entry.relation.referenced_column,
            ]
        })
        .collect::<Vec<_>>();
    render_query_output(
        QueryOutput::Rows(simple_result_set(
            vec!["table", "column", "referenced_table", "referenced_column"],
            rows,
        )),
        &render,
    )
}

async fn handle_constraints(
    context: &RuntimeContext,
    command: ConstraintsCommand,
) -> Result<(), CliError> {
    let render = resolve_render(context, command.render);
    let connection = resolve_connection(context, &command.connection).await?;
    let output = match connection.driver {
        DatabaseDriver::Sqlite => {
            sqlite_constraints_output(&connection, command.table.as_deref()).await?
        }
        _ => {
            let query = constraints_query(
                connection.driver,
                connection.database.as_deref(),
                command.table.as_deref(),
            );
            run_query_with_control(
                connection.pool,
                connection.database,
                query,
                None,
                render.max_rows,
            )
            .await
            .map_err(CliError::sql)?
        }
    };
    render_query_output(output, &render)
}

async fn handle_size(context: &RuntimeContext, command: SizeCommand) -> Result<(), CliError> {
    let render = resolve_render(context, command.render);
    let connection = resolve_connection(context, &command.connection).await?;
    let output = match connection.driver {
        DatabaseDriver::Sqlite => sqlite_size_output(&connection).await?,
        _ => {
            let query = size_query(connection.driver, connection.database.as_deref());
            run_query_with_control(
                connection.pool,
                connection.database,
                query,
                None,
                render.max_rows,
            )
            .await
            .map_err(CliError::sql)?
        }
    };
    render_query_output(output, &render)
}

async fn handle_search(context: &RuntimeContext, command: SearchCommand) -> Result<(), CliError> {
    let render = resolve_render(context, command.render);
    let connection = resolve_connection(context, &command.connection).await?;
    let output = match connection.driver {
        DatabaseDriver::Sqlite => sqlite_search_output(&connection, &command.needle).await?,
        _ => {
            let query = search_query(
                connection.driver,
                connection.database.as_deref(),
                &command.needle,
            );
            run_query_with_control(
                connection.pool,
                connection.database,
                query,
                None,
                render.max_rows,
            )
            .await
            .map_err(CliError::sql)?
        }
    };
    render_query_output(output, &render)
}

async fn handle_dump(context: &RuntimeContext, command: DumpCommand) -> Result<(), CliError> {
    let connection = resolve_connection(context, &command.connection).await?;
    let (include_schema, include_data) = dump_modes(command.schema_only, command.data_only)?;
    let bytes = dump_connection(
        &connection,
        command.table.as_deref(),
        include_schema,
        include_data,
    )
    .await?;

    let render = ResolvedRender {
        format: OutputFormat::Raw,
        out: command.out,
        no_header: true,
        single_value: false,
        max_rows: None,
        max_output_size: None,
        out_temp: command.out_temp || context.env.get_bool("CRYODB_OUT_TEMP"),
        single_result: false,
        pager: false,
        no_color: false,
    };
    write_bytes(&bytes, &render)
}

async fn handle_restore(context: &RuntimeContext, command: RestoreCommand) -> Result<(), CliError> {
    handle_run(
        context,
        RunCommand {
            connection: command.connection,
            render: RenderArgs {
                format: None,
                out: None,
                raw: false,
                no_header: false,
                single_value: false,
                max_rows: None,
                max_output_size: None,
                out_temp: false,
                single_result: false,
            },
            transaction: command.transaction,
            stop_on_error: command.stop_on_error,
            continue_on_error: command.continue_on_error,
            dry_run: command.dry_run,
            file: command.file,
        },
    )
    .await
}

async fn handle_validate(
    context: &RuntimeContext,
    command: ValidateCommand,
) -> Result<(), CliError> {
    let render = resolve_render(context, command.render);
    let inputs = collect_sql_inputs(&command.files, true)?;
    let connection = resolve_connection(context, &command.connection).await?;
    let mut rows = Vec::new();

    for input in inputs {
        let statements = split_sql_statements(&input.sql);
        if statements.is_empty() {
            rows.push(vec![
                input.label.clone(),
                String::from("0"),
                String::from("error"),
                String::from("empty"),
                String::from("No SQL statements found."),
            ]);
            continue;
        }

        let mut had_issue = false;
        for (index, statement) in statements.iter().enumerate() {
            let compatibility = compatibility_issues(connection.driver, statement);
            for issue in compatibility {
                had_issue = true;
                rows.push(vec![
                    input.label.clone(),
                    (index + 1).to_string(),
                    String::from("error"),
                    String::from("driver_compatibility"),
                    issue,
                ]);
            }

            let syntax_issues = validate_statement_syntax(&connection, statement).await?;
            for issue in syntax_issues {
                had_issue = true;
                rows.push(vec![
                    input.label.clone(),
                    (index + 1).to_string(),
                    String::from("error"),
                    String::from("syntax_semantic"),
                    issue,
                ]);
            }

            let schema_issues = validate_statement_against_schema(&connection, statement).await?;
            for issue in schema_issues {
                had_issue = true;
                rows.push(vec![
                    input.label.clone(),
                    (index + 1).to_string(),
                    String::from("error"),
                    String::from("schema"),
                    issue,
                ]);
            }
        }

        if !had_issue {
            rows.push(vec![
                input.label,
                String::from("*"),
                String::from("ok"),
                String::from("validated"),
                String::from("No validation issues found for supported checks."),
            ]);
        }
    }

    render_query_output(
        QueryOutput::Rows(simple_result_set(
            vec!["input", "statement", "status", "rule", "message"],
            rows,
        )),
        &render,
    )
}

async fn handle_lint(context: &RuntimeContext, command: LintCommand) -> Result<(), CliError> {
    let render = resolve_render(context, command.render);
    let inputs = collect_sql_inputs(&command.files, true)?;
    let mut rows = Vec::new();

    for input in inputs {
        let statements = split_sql_statements(&input.sql);
        if statements.is_empty() {
            rows.push(vec![
                input.label.clone(),
                String::from("0"),
                String::from("warn"),
                String::from("empty"),
                String::from("No SQL statements found."),
            ]);
            continue;
        }

        let mut had_issue = false;
        for (index, statement) in statements.iter().enumerate() {
            for (rule, message) in lint_statement(statement) {
                had_issue = true;
                rows.push(vec![
                    input.label.clone(),
                    (index + 1).to_string(),
                    String::from("warn"),
                    rule,
                    message,
                ]);
            }
        }

        if !had_issue {
            rows.push(vec![
                input.label,
                String::from("*"),
                String::from("ok"),
                String::from("clean"),
                String::from("No lint issues found for supported rules."),
            ]);
        }
    }

    render_query_output(
        QueryOutput::Rows(simple_result_set(
            vec!["input", "statement", "status", "rule", "message"],
            rows,
        )),
        &render,
    )
}

async fn handle_format(_context: &RuntimeContext, command: FormatCommand) -> Result<(), CliError> {
    let inputs = collect_sql_inputs(&command.files, true)?;
    if command.write && inputs.iter().any(|input| input.path.is_none()) {
        return Err(CliError::validation(
            "`format --write` requires real file paths and does not support stdin.",
        ));
    }

    let mut stdout = io::stdout().lock();
    for (index, input) in inputs.iter().enumerate() {
        let formatted = format_sql_text(&input.sql);
        if command.write {
            if let Some(path) = input.path.as_deref() {
                std::fs::write(path, formatted.as_bytes())
                    .map_err(|error| CliError::general(error.to_string()))?;
            }
            continue;
        }

        if index > 0 {
            writeln!(stdout).map_err(|error| CliError::general(error.to_string()))?;
        }
        stdout
            .write_all(formatted.as_bytes())
            .map_err(|error| CliError::general(error.to_string()))?;
        if !formatted.ends_with('\n') {
            writeln!(stdout).map_err(|error| CliError::general(error.to_string()))?;
        }
    }

    Ok(())
}

async fn handle_explain(context: &RuntimeContext, command: ExplainCommand) -> Result<(), CliError> {
    let render = resolve_render(context, command.render);
    let connection = resolve_connection(context, &command.connection).await?;
    let sql = read_query_input(command.sql.as_deref())?;
    let statement = explain_query(connection.driver, &sql, command.analyze, render.format);
    let output = run_query_with_control(
        connection.pool,
        connection.database,
        statement,
        None,
        render.max_rows,
    )
    .await
    .map_err(CliError::sql)?;
    render_query_output(
        normalize_explain_output(connection.driver, command.analyze, output),
        &render,
    )
}

async fn handle_value(context: &RuntimeContext, command: ValueCommand) -> Result<(), CliError> {
    let connection = resolve_connection(context, &command.connection).await?;
    let sql = read_query_input(command.sql.as_deref())?;
    let output = run_query_with_control(connection.pool, connection.database, sql, None, Some(1))
        .await
        .map_err(CliError::sql)?;
    let value = match output {
        QueryOutput::Rows(rows) => rows
            .rows
            .first()
            .and_then(|row| row.first())
            .cloned()
            .unwrap_or_default(),
        QueryOutput::Affected(count) => count.to_string(),
    };
    writeln!(io::stdout().lock(), "{}", value).map_err(|error| CliError::general(error.to_string()))
}

async fn handle_ping(
    context: &RuntimeContext,
    connection_args: ConnectionSelectorArgs,
) -> Result<(), CliError> {
    let connection = resolve_connection(context, &connection_args).await?;
    let start = Instant::now();
    run_query_with_control(
        connection.pool,
        connection.database,
        String::from("SELECT 1"),
        None,
        Some(1),
    )
    .await
    .map_err(CliError::sql)?;
    writeln!(io::stdout().lock(), "ok\t{}ms", start.elapsed().as_millis())
        .map_err(|error| CliError::general(error.to_string()))
}

async fn handle_health(context: &RuntimeContext, command: HealthCommand) -> Result<(), CliError> {
    let render = resolve_render(context, command.render);
    let connect_start = Instant::now();
    let connection = resolve_connection(context, &command.connection).await?;
    let connect_ms = connect_start.elapsed().as_millis();

    let query_start = Instant::now();
    run_query_with_control(
        connection.pool.clone(),
        connection.database.clone(),
        String::from("SELECT 1"),
        None,
        Some(1),
    )
    .await
    .map_err(CliError::sql)?;
    let query_ms = query_start.elapsed().as_millis();

    let metadata_start = Instant::now();
    let metadata = run_query_with_control(
        connection.pool.clone(),
        connection.database.clone(),
        tables_query(
            connection.driver,
            connection.database.as_deref(),
            None,
            None,
        ),
        None,
        Some(1),
    )
    .await
    .map_err(CliError::sql)?;
    let metadata_ms = metadata_start.elapsed().as_millis();
    let metadata_detail = match metadata {
        QueryOutput::Rows(rows) => rows
            .rows
            .first()
            .and_then(|row| row.first())
            .map(|value| format!("visible object: {value}"))
            .unwrap_or_else(|| String::from("metadata query ok")),
        QueryOutput::Affected(count) => format!("metadata query affected {count} rows"),
    };

    let server_info_start = Instant::now();
    let server_info = run_query_with_control(
        connection.pool.clone(),
        connection.database.clone(),
        server_info_query(connection.driver),
        None,
        Some(1),
    )
    .await
    .map_err(CliError::sql)?;
    let server_info_ms = server_info_start.elapsed().as_millis();
    let server_info_detail = match server_info {
        QueryOutput::Rows(rows) => rows
            .rows
            .first()
            .map(|row| row.join(" | "))
            .unwrap_or_else(|| String::from("server info query ok")),
        QueryOutput::Affected(count) => format!("server info affected {count} rows"),
    };

    let permissions_start = Instant::now();
    let permissions = run_query_with_control(
        connection.pool.clone(),
        connection.database.clone(),
        permissions_probe_query(connection.driver),
        None,
        Some(1),
    )
    .await
    .map_err(CliError::sql)?;
    let permissions_ms = permissions_start.elapsed().as_millis();
    let permissions_detail = match permissions {
        QueryOutput::Rows(rows) => rows
            .rows
            .first()
            .map(|row| row.join(" | "))
            .unwrap_or_else(|| String::from("permissions probe ok")),
        QueryOutput::Affected(count) => format!("permissions probe affected {count} rows"),
    };

    let rows = vec![
        vec![
            String::from("connect"),
            String::from("ok"),
            connection.label.clone(),
            connect_ms.to_string(),
        ],
        vec![
            String::from("query"),
            String::from("ok"),
            String::from("SELECT 1"),
            query_ms.to_string(),
        ],
        vec![
            String::from("metadata"),
            String::from("ok"),
            metadata_detail,
            metadata_ms.to_string(),
        ],
        vec![
            String::from("server_info"),
            String::from("ok"),
            server_info_detail,
            server_info_ms.to_string(),
        ],
        vec![
            String::from("permissions"),
            String::from("ok"),
            permissions_detail,
            permissions_ms.to_string(),
        ],
    ];

    render_query_output(
        QueryOutput::Rows(simple_result_set(
            vec!["check", "status", "detail", "elapsed_ms"],
            rows,
        )),
        &render,
    )
}

async fn handle_server_info(
    context: &RuntimeContext,
    command: ListSimpleCommand,
) -> Result<(), CliError> {
    let render = resolve_render(context, command.render);
    let connection = resolve_connection(context, &command.connection).await?;
    let output = run_query_with_control(
        connection.pool,
        connection.database,
        server_info_query(connection.driver),
        None,
        render.max_rows,
    )
    .await
    .map_err(CliError::sql)?;
    render_query_output(output, &render)
}

async fn handle_history(context: &RuntimeContext, command: HistoryCommand) -> Result<(), CliError> {
    if command.clear {
        confirm_action(context, "Clear CLI query history", "history_clear")?;
        let path = config_artifact_path(CLI_HISTORY_FILE);
        match std::fs::remove_file(&path) {
            Ok(()) => return Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(CliError::general(error.to_string())),
        }
    }

    let render = resolve_render(context, command.render);
    let mut rows = load_history_records()?;
    if let Some(connection) = command.connection.as_deref() {
        rows.retain(|entry| entry.connection.eq_ignore_ascii_case(connection));
    }
    if let Some(search) = command.search.as_deref() {
        let search = search.to_ascii_lowercase();
        rows.retain(|entry| entry.sql.to_ascii_lowercase().contains(&search));
    }
    rows.reverse();
    if let Some(index) = command.run {
        if index == 0 {
            return Err(CliError::validation(
                "`--run` expects a 1-based history index.",
            ));
        }
        let Some(entry) = rows.get(index - 1).cloned() else {
            return Err(CliError::validation(format!(
                "History entry {} was not found for the current filters.",
                index
            )));
        };
        let connection = resolve_connection(
            context,
            &ConnectionSelectorArgs {
                connection: Some(entry.connection.clone()),
                url: None,
                sqlite: None,
                database: None,
            },
        )
        .await?;
        confirm_destructive_execution(
            context,
            &connection,
            "history-rerun",
            std::slice::from_ref(&entry.sql),
        )?;

        let result = run_query_with_control(
            connection.pool.clone(),
            connection.database.clone(),
            entry.sql.clone(),
            None,
            render.max_rows,
        )
        .await;

        return match result {
            Ok(output) => {
                append_history_batch(&connection, "history-rerun", &[entry.sql], "ok")?;
                render_query_output(output, &render)
            }
            Err(error) => {
                append_history_batch(&connection, "history-rerun", &[entry.sql], "error")?;
                Err(CliError::sql(error))
            }
        };
    }

    rows.truncate(command.limit);
    let rows = rows
        .into_iter()
        .enumerate()
        .map(|(index, entry)| {
            vec![
                (index + 1).to_string(),
                entry.timestamp,
                entry.connection,
                entry.driver,
                entry.source,
                entry.status,
                preview_sql(&entry.sql),
            ]
        })
        .collect::<Vec<_>>();
    render_query_output(
        QueryOutput::Rows(simple_result_set(
            vec![
                "index",
                "timestamp",
                "connection",
                "driver",
                "source",
                "status",
                "sql",
            ],
            rows,
        )),
        &render,
    )
}

async fn handle_doctor(context: &RuntimeContext, command: DoctorCommand) -> Result<(), CliError> {
    let render = resolve_render(context, command.render);
    let (store, warning) = load_connection_store();
    let current_exe = std::env::current_exe()
        .map(|path| path.display().to_string())
        .unwrap_or_else(|error| format!("<unavailable: {error}>"));
    let history_path = config_artifact_path(CLI_HISTORY_FILE);
    let config_dir = history_path
        .parent()
        .map(|path| path.display().to_string())
        .unwrap_or_else(|| String::from("<unknown>"));

    let config_dir_status = match std::fs::create_dir_all(&config_dir) {
        Ok(()) => (
            String::from("ok"),
            String::from("config directory writable"),
        ),
        Err(error) => (String::from("error"), error.to_string()),
    };

    let mut rows = vec![
        vec![
            String::from("version"),
            String::from("ok"),
            env!("CARGO_PKG_VERSION").to_string(),
        ],
        vec![String::from("binary"), String::from("ok"), current_exe],
        vec![String::from("config_dir"), String::from("ok"), config_dir],
        vec![
            String::from("config_dir_access"),
            config_dir_status.0,
            config_dir_status.1,
        ],
        vec![
            String::from("connections"),
            if warning.is_some() {
                String::from("warn")
            } else {
                String::from("ok")
            },
            format!(
                "profiles={}, favorites={}, recents={}, default={}",
                store.profiles.len(),
                store.favorites.len(),
                store.recents.len(),
                store.default_connection.as_deref().unwrap_or("<none>")
            ),
        ],
    ];

    if let Some(warning) = warning {
        rows.push(vec![
            String::from("connection_store_warning"),
            String::from("warn"),
            warning,
        ]);
    }

    let drivers = if let Some(driver) = command.driver {
        vec![driver]
    } else {
        vec![
            CliDriver::Postgres,
            CliDriver::Mysql,
            CliDriver::Mariadb,
            CliDriver::Sqlite,
        ]
    };

    for driver in drivers {
        let detail = match driver {
            CliDriver::Postgres => "compiled with PostgreSQL driver support",
            CliDriver::Mysql => "compiled with MySQL driver support",
            CliDriver::Mariadb => "compiled with MariaDB driver support",
            CliDriver::Sqlite => "compiled with SQLite driver support",
        };
        rows.push(vec![
            format!("driver.{}", DatabaseDriver::from(driver).storage_key()),
            String::from("ok"),
            String::from(detail),
        ]);
    }

    for tool in ["pg_dump", "psql", "mysqldump", "mysql"] {
        let available = command_available(tool).await;
        rows.push(vec![
            format!("tool.{tool}"),
            if available {
                String::from("ok")
            } else {
                String::from("warn")
            },
            if available {
                format!("{tool} available in PATH")
            } else {
                format!("{tool} not found in PATH; related dump/restore flows will use built-in fallback when possible")
            },
        ]);
    }

    render_query_output(
        QueryOutput::Rows(simple_result_set(vec!["check", "status", "detail"], rows)),
        &render,
    )
}

async fn command_available(name: &str) -> bool {
    Command::new(name)
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await
        .map(|status| status.success())
        .unwrap_or(false)
}

fn handle_completion(command: CompletionCommand) -> Result<(), CliError> {
    if command.dynamic {
        return write!(
            io::stdout().lock(),
            "{}",
            dynamic_completion_script(command.shell)
        )
        .map_err(|error| CliError::general(error.to_string()));
    }

    let mut app = Cli::command();
    match command.shell {
        CompletionShell::Bash => generate(shells::Bash, &mut app, "cryodb", &mut io::stdout()),
        CompletionShell::Zsh => generate(shells::Zsh, &mut app, "cryodb", &mut io::stdout()),
        CompletionShell::Fish => generate(shells::Fish, &mut app, "cryodb", &mut io::stdout()),
    }
    Ok(())
}

async fn handle_complete(
    context: &RuntimeContext,
    command: CompleteCommand,
) -> Result<(), CliError> {
    let line = command.line;
    let position = command.position.unwrap_or(line.len()).min(line.len());
    let prefix = &line[..position];
    let current = if prefix.chars().last().is_some_and(char::is_whitespace) {
        ""
    } else {
        prefix.split_whitespace().last().unwrap_or("")
    };
    let words = split_completion_words(prefix);
    let args = completion_args(&words);
    let previous = if current.is_empty() {
        args.last().map(String::as_str).unwrap_or("")
    } else {
        args.len()
            .checked_sub(2)
            .and_then(|index| args.get(index))
            .map(String::as_str)
            .unwrap_or("")
    };

    let suggestions = if matches!(previous, "-c" | "--connection") {
        saved_connection_selectors(&load_connection_store().0)
    } else if args.len() <= 2 && !current.starts_with('-') {
        system_command_names()
    } else if current.starts_with('-') {
        system_flag_names()
    } else if args.get(1).is_some_and(|value| value == "conn") && args.len() <= 3 {
        vec![
            String::from("list"),
            String::from("add"),
            String::from("edit"),
            String::from("remove"),
            String::from("test"),
            String::from("show"),
            String::from("set-default"),
        ]
    } else if args
        .get(1)
        .is_some_and(|value| object_completion_command(value))
    {
        let selector = completion_connection_selector(&args);
        let connection_args = ConnectionSelectorArgs {
            connection: selector,
            url: None,
            sqlite: None,
            database: None,
        };
        match resolve_connection(context, &connection_args).await {
            Ok(connection) => shell_objects(&connection).await.unwrap_or_default(),
            Err(_) => Vec::new(),
        }
    } else {
        Vec::new()
    };

    let mut suggestions = suggestions
        .into_iter()
        .filter(|item| item.starts_with(current))
        .collect::<Vec<_>>();
    suggestions.sort();
    suggestions.dedup();
    let mut stdout = io::stdout().lock();
    for suggestion in suggestions {
        writeln!(stdout, "{suggestion}").map_err(|error| CliError::general(error.to_string()))?;
    }
    Ok(())
}

fn dynamic_completion_script(shell: CompletionShell) -> &'static str {
    match shell {
        CompletionShell::Bash => {
            r#"_cryodb_complete() {
  local line="${COMP_LINE}"
  local point="${COMP_POINT:-${#COMP_LINE}}"
  mapfile -t COMPREPLY < <(cryodb __complete --line "$line" --position "$point" 2>/dev/null)
}
complete -F _cryodb_complete cryodb
"#
        }
        CompletionShell::Zsh => {
            r#"#compdef cryodb
_cryodb_complete() {
  local -a suggestions
  suggestions=(${(f)$(cryodb __complete --line "$BUFFER" --position "$CURSOR" 2>/dev/null)})
  compadd -- $suggestions
}
compdef _cryodb_complete cryodb
"#
        }
        CompletionShell::Fish => {
            r#"complete -c cryodb -f -a '(cryodb __complete --line (commandline -cp) --position (string length -- (commandline -cp)) 2>/dev/null)'
"#
        }
    }
}

fn split_completion_words(input: &str) -> Vec<String> {
    input
        .split_whitespace()
        .map(std::string::ToString::to_string)
        .collect()
}

fn completion_args(words: &[String]) -> Vec<String> {
    if words
        .first()
        .is_some_and(|value| value.ends_with("cryodb") || value == "cryodb")
    {
        words.to_vec()
    } else {
        let mut args = vec![String::from("cryodb")];
        args.extend(words.iter().cloned());
        args
    }
}

fn completion_connection_selector(args: &[String]) -> Option<String> {
    args.windows(2).find_map(|window| {
        matches!(window[0].as_str(), "-c" | "--connection").then(|| window[1].clone())
    })
}

fn object_completion_command(command: &str) -> bool {
    matches!(
        command,
        "describe" | "desc" | "schema" | "docs" | "indexes" | "constraints" | "size"
    )
}

fn system_command_names() -> Vec<String> {
    vec![
        "query",
        "q",
        "run",
        "shell",
        "sh",
        "conn",
        "cx",
        "databases",
        "db",
        "schemas",
        "tables",
        "views",
        "describe",
        "desc",
        "schema",
        "docs",
        "indexes",
        "relations",
        "constraints",
        "size",
        "search",
        "dump",
        "restore",
        "validate",
        "lint",
        "format",
        "explain",
        "value",
        "ping",
        "health",
        "server-info",
        "history",
        "doctor",
        "completion",
        "version",
    ]
    .into_iter()
    .map(String::from)
    .collect()
}

fn system_flag_names() -> Vec<String> {
    vec![
        "--connection",
        "-c",
        "--url",
        "--sqlite",
        "--database",
        "--format",
        "--out",
        "--out-temp",
        "--raw",
        "--no-header",
        "--single-value",
        "--max-rows",
        "--limit",
        "--max-output-size",
        "--timeout",
        "--yes",
        "--non-interactive",
        "--ci",
        "--quiet",
        "--verbose",
        "--debug",
    ]
    .into_iter()
    .map(String::from)
    .collect()
}

fn stored_connection_has_secret(entry: &StoredConnection) -> bool {
    load_connection_secret(entry).is_some()
}

fn merge_saved_connection_source(
    sources: &mut Vec<SavedConnectionSource>,
    source: SavedConnectionSource,
) {
    if !sources.contains(&source) {
        sources.push(source);
        sources.sort();
    }
}

fn saved_connection_source_text(sources: &[SavedConnectionSource]) -> String {
    sources
        .iter()
        .map(|source| source.label())
        .collect::<Vec<_>>()
        .join(",")
}

fn catalog_connection_key(entry: &StoredConnection, source: SavedConnectionSource) -> String {
    if matches!(source, SavedConnectionSource::Profile)
        && let Some(name) = entry.profile_name()
    {
        return format!("profile::{}", name.to_ascii_lowercase());
    }

    format!("gui::{}", entry.secret_lookup_key())
}

fn add_catalog_connection(
    map: &mut BTreeMap<String, CatalogConnection>,
    entry: &StoredConnection,
    source: SavedConnectionSource,
) {
    let key = catalog_connection_key(entry, source);
    let identity_key = entry.secret_lookup_key();

    if let Some(existing) = map.get_mut(&key) {
        merge_saved_connection_source(&mut existing.sources, source);
        if existing.entry.profile_name().is_none() && entry.profile_name().is_some() {
            existing.entry.name = entry.name.clone();
            existing.selector = entry.profile_label();
        }
        return;
    }

    map.insert(
        key,
        CatalogConnection {
            identity_key,
            entry: entry.clone(),
            selector: entry.profile_label(),
            sources: vec![source],
        },
    );
}

fn connection_catalog(store: &ConnectionStore) -> Vec<CatalogConnection> {
    let mut map = BTreeMap::new();

    for entry in &store.profiles {
        add_catalog_connection(&mut map, entry, SavedConnectionSource::Profile);
    }
    for entry in &store.favorites {
        add_catalog_connection(&mut map, entry, SavedConnectionSource::Favorite);
    }
    for entry in &store.recents {
        add_catalog_connection(&mut map, entry, SavedConnectionSource::Recent);
    }

    map.into_values().collect()
}

fn resolve_catalog_candidates(
    candidates: Vec<CatalogConnection>,
    lookup: &str,
    exact_named: bool,
) -> Result<ResolvedSavedConnection, CliError> {
    if candidates.is_empty() {
        return Err(CliError::validation(format!(
            "Connection `{lookup}` was not found."
        )));
    }

    let mut grouped = BTreeMap::<String, ResolvedSavedConnection>::new();
    for candidate in candidates {
        match grouped.get_mut(&candidate.identity_key) {
            Some(existing) => {
                for source in candidate.sources {
                    merge_saved_connection_source(&mut existing.sources, source);
                }
                if exact_named
                    && existing.entry.profile_name().is_none()
                    && candidate.entry.profile_name().is_some()
                {
                    existing.entry = candidate.entry.clone();
                    existing.selector = candidate.selector.clone();
                }
            }
            None => {
                grouped.insert(
                    candidate.identity_key.clone(),
                    ResolvedSavedConnection {
                        entry: candidate.entry.clone(),
                        selector: if exact_named {
                            candidate.selector.clone()
                        } else {
                            candidate.entry.display_label()
                        },
                        sources: candidate.sources.clone(),
                        exact_named,
                    },
                );
            }
        }
    }

    if grouped.len() == 1 {
        return Ok(grouped.into_values().next().expect("single grouped entry"));
    }

    let choices = grouped
        .into_values()
        .map(|entry| {
            format!(
                "{} [{}]",
                entry.entry.profile_label(),
                saved_connection_source_text(&entry.sources)
            )
        })
        .collect::<Vec<_>>()
        .join(", ");
    Err(CliError::validation(format!(
        "Connection `{lookup}` is ambiguous. Matches: {choices}"
    )))
}

fn resolve_saved_connection(
    store: &ConnectionStore,
    lookup: &str,
) -> Result<ResolvedSavedConnection, CliError> {
    let lookup = lookup.trim();
    if lookup.is_empty() {
        return Err(CliError::validation("Connection name cannot be empty."));
    }

    let catalog = connection_catalog(store);
    let named_matches = catalog
        .iter()
        .filter(|candidate| {
            candidate
                .entry
                .profile_name()
                .is_some_and(|name| name.eq_ignore_ascii_case(lookup))
        })
        .cloned()
        .collect::<Vec<_>>();
    if !named_matches.is_empty() {
        return resolve_catalog_candidates(named_matches, lookup, true);
    }

    let display_matches = catalog
        .iter()
        .filter(|candidate| candidate.entry.display_label().eq_ignore_ascii_case(lookup))
        .cloned()
        .collect::<Vec<_>>();
    resolve_catalog_candidates(display_matches, lookup, false)
}

fn saved_connection_selectors(store: &ConnectionStore) -> Vec<String> {
    let mut names = connection_catalog(store)
        .into_iter()
        .map(|entry| entry.selector)
        .collect::<Vec<_>>();
    names.sort_by(|left, right| {
        left.to_ascii_lowercase()
            .cmp(&right.to_ascii_lowercase())
            .then_with(|| left.cmp(right))
    });
    names.dedup_by(|left, right| left.eq_ignore_ascii_case(right));
    names
}

fn store_contains_identity(store: &ConnectionStore, original: &StoredConnection) -> bool {
    store
        .profiles
        .iter()
        .any(|entry| entry.matches_identity(original))
        || store
            .favorites
            .iter()
            .any(|entry| entry.matches_identity(original))
        || store
            .recents
            .iter()
            .any(|entry| entry.matches_identity(original))
}

fn replace_gui_connections_by_identity(
    store: &mut ConnectionStore,
    original: &StoredConnection,
    replacement: &StoredConnection,
) {
    for entry in &mut store.favorites {
        if entry.matches_identity(original) {
            *entry = replacement.clone();
        }
    }
    for entry in &mut store.recents {
        if entry.matches_identity(original) {
            *entry = replacement.clone();
        }
    }
}

fn remove_gui_connections_by_identity(store: &mut ConnectionStore, original: &StoredConnection) {
    store
        .favorites
        .retain(|entry| !entry.matches_identity(original));
    store
        .recents
        .retain(|entry| !entry.matches_identity(original));
}

fn clear_removed_connection_secret_if_unused(store: &ConnectionStore, original: &StoredConnection) {
    if !store_contains_identity(store, original) {
        let _ = crate::storage::store_connection_secret(original, "");
    }
}

fn default_targets_connection(default_connection: Option<&str>, entry: &StoredConnection) -> bool {
    default_connection.is_some_and(|value| entry.matches_profile(value))
}

async fn resolve_connection(
    context: &RuntimeContext,
    args: &ConnectionSelectorArgs,
) -> Result<ResolvedConnection, CliError> {
    if let Some(path) = args.sqlite.as_deref() {
        let path_text = path.display().to_string();
        context.print_verbose(&format!("Connecting to SQLite file {path_text}"));
        let (pool, normalized) = connect_sqlite_profile(path_text.clone())
            .await
            .map_err(CliError::connection)?;
        return Ok(ResolvedConnection {
            driver: DatabaseDriver::Sqlite,
            pool: DatabasePool::Sqlite(pool),
            database: Some(String::from("main")),
            profile_name: None,
            label: normalized,
            info: ConnectionInfo {
                driver: DatabaseDriver::Sqlite,
                host: String::new(),
                port: String::new(),
                database: String::new(),
                username: String::new(),
                password: String::new(),
                sqlite_path: path_text,
                tls_mode: TlsMode::Disabled,
                tls_ca_cert_path: String::new(),
                tls_client_cert_path: String::new(),
                tls_client_key_path: String::new(),
            },
        });
    }

    if let Some(url) = args.url.as_deref() {
        context.print_verbose(&format!("Connecting with URL {url}"));
        let info = connection_info_from_url(url, args.database.as_deref(), context)?;
        return connect_from_info(info, None).await;
    }

    let (store, warning) = load_connection_store();
    if let Some(warning) = warning {
        context.print_info(&warning);
    }

    let connection_name = args
        .connection
        .clone()
        .or_else(|| context.env.get("CRYODB_CONNECTION"))
        .or_else(|| store.default_connection.clone())
        .ok_or_else(|| {
            CliError::validation("Provide `-c/--connection`, `--url`, or `--sqlite`.")
        })?;

    let selection = resolve_saved_connection(&store, &connection_name)?;
    let entry = selection.entry.clone();
    context.print_verbose(&format!(
        "Connecting with profile {} ({})",
        selection.selector, entry.driver
    ));

    let mut info = info_from_stored_connection(&entry, context, args.database.as_deref())?;
    if matches!(
        info.driver,
        DatabaseDriver::MySql | DatabaseDriver::MariaDb | DatabaseDriver::PostgreSql
    ) && info.password.trim().is_empty()
    {
        info.password = resolve_missing_password(
            context,
            &entry.display_label(),
            Some(&entry),
            &info.username,
        )?;
    }

    connect_from_info(info, Some(selection.selector)).await
}

fn info_from_stored_connection(
    entry: &StoredConnection,
    context: &RuntimeContext,
    database_override: Option<&str>,
) -> Result<ConnectionInfo, CliError> {
    let password = load_connection_secret(entry)
        .or_else(|| context.env.get("CRYODB_PASSWORD"))
        .unwrap_or_default();

    Ok(ConnectionInfo {
        driver: entry.driver,
        host: entry.host.clone(),
        port: entry.port.clone(),
        database: database_override
            .map(std::string::ToString::to_string)
            .unwrap_or_else(|| entry.database.clone()),
        username: entry.username.clone(),
        password,
        sqlite_path: entry.sqlite_path.clone(),
        tls_mode: entry.tls_mode,
        tls_ca_cert_path: entry.tls_ca_cert_path.clone(),
        tls_client_cert_path: entry.tls_client_cert_path.clone(),
        tls_client_key_path: entry.tls_client_key_path.clone(),
    })
}

async fn connect_from_info(
    info: ConnectionInfo,
    profile_name: Option<String>,
) -> Result<ResolvedConnection, CliError> {
    let resolved_info = info.clone();
    let label = match info.driver {
        DatabaseDriver::Sqlite => info.sqlite_path.clone(),
        _ => {
            if info.database.trim().is_empty() {
                format!("{}:{}", info.host, info.port)
            } else {
                format!("{}@{}:{}", info.database, info.host, info.port)
            }
        }
    };

    match info.driver {
        DatabaseDriver::MySql | DatabaseDriver::MariaDb => {
            let database = if info.database.trim().is_empty() {
                None
            } else {
                Some(info.database.clone())
            };
            let driver = info.driver;
            let pool = connect_mysql(info).await.map_err(CliError::connection)?;
            Ok(ResolvedConnection {
                driver,
                pool: DatabasePool::MySql(pool),
                database,
                profile_name,
                label,
                info: resolved_info,
            })
        }
        DatabaseDriver::PostgreSql => {
            let database = if info.database.trim().is_empty() {
                None
            } else {
                Some(info.database.clone())
            };
            let pool = connect_postgres(info).await.map_err(CliError::connection)?;
            Ok(ResolvedConnection {
                driver: DatabaseDriver::PostgreSql,
                pool: DatabasePool::Postgres(pool),
                database,
                profile_name,
                label,
                info: resolved_info,
            })
        }
        DatabaseDriver::Sqlite => {
            let (pool, normalized) = connect_sqlite_profile(info.sqlite_path)
                .await
                .map_err(CliError::connection)?;
            Ok(ResolvedConnection {
                driver: DatabaseDriver::Sqlite,
                pool: DatabasePool::Sqlite(pool),
                database: Some(String::from("main")),
                profile_name,
                label: normalized,
                info: resolved_info,
            })
        }
    }
}

fn connection_info_from_url(
    raw: &str,
    database_override: Option<&str>,
    context: &RuntimeContext,
) -> Result<ConnectionInfo, CliError> {
    let url = Url::parse(raw).map_err(|error| CliError::validation(error.to_string()))?;
    let driver = match url.scheme() {
        "postgres" | "postgresql" => DatabaseDriver::PostgreSql,
        "mysql" => DatabaseDriver::MySql,
        "mariadb" => DatabaseDriver::MariaDb,
        "sqlite" => DatabaseDriver::Sqlite,
        scheme => {
            return Err(CliError::validation(format!(
                "Unsupported URL scheme `{scheme}`."
            )));
        }
    };

    if driver == DatabaseDriver::Sqlite {
        let path = url
            .to_file_path()
            .map_err(|_| CliError::validation("Invalid SQLite URL."))?;
        return Ok(ConnectionInfo {
            driver,
            host: String::new(),
            port: String::new(),
            database: String::new(),
            username: String::new(),
            password: String::new(),
            sqlite_path: path.display().to_string(),
            tls_mode: TlsMode::Disabled,
            tls_ca_cert_path: String::new(),
            tls_client_cert_path: String::new(),
            tls_client_key_path: String::new(),
        });
    }

    let host = url
        .host_str()
        .ok_or_else(|| CliError::validation("Connection URL is missing host."))?
        .to_string();
    let username = url.username().to_string();
    let password = url
        .password()
        .map(std::string::ToString::to_string)
        .or_else(|| context.env.get("CRYODB_PASSWORD"))
        .unwrap_or_default();
    let database = database_override
        .map(std::string::ToString::to_string)
        .or_else(|| {
            let path = url.path().trim_start_matches('/').trim();
            if path.is_empty() {
                None
            } else {
                Some(path.to_string())
            }
        })
        .unwrap_or_default();

    let tls_mode = url
        .query_pairs()
        .find_map(|(key, value)| {
            if key.eq_ignore_ascii_case("sslmode") || key.eq_ignore_ascii_case("tls") {
                parse_tls_mode(&value)
            } else {
                None
            }
        })
        .unwrap_or(TlsMode::Prefer);
    let tls_ca_cert_path = url
        .query_pairs()
        .find_map(|(key, value)| {
            matches!(
                key.to_ascii_lowercase().as_str(),
                "sslrootcert" | "ssl-root-cert" | "ssl_ca" | "ssl-ca" | "sslca"
            )
            .then(|| value.into_owned())
        })
        .unwrap_or_default();
    let tls_client_cert_path = url
        .query_pairs()
        .find_map(|(key, value)| {
            matches!(key.to_ascii_lowercase().as_str(), "sslcert" | "ssl-cert")
                .then(|| value.into_owned())
        })
        .unwrap_or_default();
    let tls_client_key_path = url
        .query_pairs()
        .find_map(|(key, value)| {
            matches!(key.to_ascii_lowercase().as_str(), "sslkey" | "ssl-key")
                .then(|| value.into_owned())
        })
        .unwrap_or_default();

    Ok(ConnectionInfo {
        driver,
        host,
        port: url.port().map(|port| port.to_string()).unwrap_or_default(),
        database,
        username,
        password,
        sqlite_path: String::new(),
        tls_mode,
        tls_ca_cert_path,
        tls_client_cert_path,
        tls_client_key_path,
    })
}

fn parse_tls_mode(value: &str) -> Option<TlsMode> {
    match value.trim().to_ascii_lowercase().as_str() {
        "disable" | "disabled" => Some(TlsMode::Disabled),
        "prefer" => Some(TlsMode::Prefer),
        "require" | "required" => Some(TlsMode::Require),
        "verify-ca" | "verify_ca" => Some(TlsMode::VerifyCa),
        "verify-full" | "verify_full" => Some(TlsMode::VerifyFull),
        _ => None,
    }
}

fn resolve_missing_password(
    context: &RuntimeContext,
    label: &str,
    entry: Option<&StoredConnection>,
    username: &str,
) -> Result<String, CliError> {
    if let Some(entry) = entry
        && let Some(secret) = load_connection_secret(entry)
        && !secret.trim().is_empty()
    {
        return Ok(secret);
    }

    if let Some(password) = context.env.get("CRYODB_PASSWORD")
        && !password.trim().is_empty()
    {
        return Ok(password);
    }

    if context.non_interactive || !io::stdin().is_terminal() {
        return Err(CliError::connection(format!(
            "Password is required for `{label}` (user `{username}`) and interactive prompts are disabled."
        )));
    }

    rpassword::prompt_password(format!("Password for {username}@{label}: "))
        .map_err(|error| CliError::general(error.to_string()))
}

async fn build_profile_entry(
    context: &RuntimeContext,
    name: String,
    current: Option<StoredConnection>,
    input: ConnectionInputArgs,
) -> Result<StoredConnection, CliError> {
    let creating_new = current.is_none();
    let mut entry = current.clone().unwrap_or(StoredConnection {
        name: String::new(),
        driver: DatabaseDriver::MySql,
        sqlite_path: String::new(),
        host: String::new(),
        port: String::new(),
        database: String::new(),
        username: String::new(),
        password: String::new(),
        tls_mode: TlsMode::Prefer,
        tls_ca_cert_path: String::new(),
        tls_client_cert_path: String::new(),
        tls_client_key_path: String::new(),
        tags: Vec::new(),
    });

    if let Some(url) = input.url.as_deref() {
        let info = connection_info_from_url(url, input.database.as_deref(), context)?;
        entry = StoredConnection::from_info(&info);
    }

    if let Some(path) = input.sqlite.as_deref() {
        entry.driver = DatabaseDriver::Sqlite;
        entry.sqlite_path = path.display().to_string();
        entry.host.clear();
        entry.port.clear();
        entry.database.clear();
        entry.username.clear();
        entry.password.clear();
    }

    if let Some(driver) = input.driver {
        entry.driver = driver.into();
    }

    entry.name = name.trim().to_string();

    if entry.driver == DatabaseDriver::Sqlite {
        entry.sqlite_path = first_non_empty(
            input
                .sqlite
                .as_deref()
                .map(|value| value.display().to_string())
                .as_deref(),
            prompt_optional(context, "SQLite file", Some(&entry.sqlite_path))?.as_deref(),
        )
        .unwrap_or_default();
        entry.host.clear();
        entry.port.clear();
        entry.database.clear();
        entry.username.clear();
        entry.tls_mode = TlsMode::Disabled;
        entry.tls_ca_cert_path.clear();
        entry.tls_client_cert_path.clear();
        entry.tls_client_key_path.clear();
        entry.password.clear();
    } else {
        entry.host = first_non_empty(
            input.host.as_deref(),
            prompt_optional(context, "Host", Some(&entry.host))?.as_deref(),
        )
        .unwrap_or_default();
        entry.port = input
            .port
            .map(|value| value.to_string())
            .or_else(|| non_empty_owned(entry.port.clone()))
            .or_else(|| {
                if context.non_interactive {
                    None
                } else {
                    prompt_optional(context, "Port", Some(entry.driver.default_port()))
                        .ok()
                        .flatten()
                }
            })
            .unwrap_or_else(|| entry.driver.default_port().to_string());
        entry.database = first_non_empty(
            input.database.as_deref(),
            prompt_optional(context, "Database", Some(&entry.database))?.as_deref(),
        )
        .unwrap_or_default();
        entry.username = first_non_empty(
            input.username.as_deref(),
            prompt_optional(context, "Username", Some(&entry.username))?.as_deref(),
        )
        .unwrap_or_default();
        entry.tls_mode = input.tls_mode.map(Into::into).unwrap_or(entry.tls_mode);
        entry.tls_ca_cert_path = input
            .tls_ca_cert_path
            .map(|value| value.display().to_string())
            .or_else(|| non_empty_owned(entry.tls_ca_cert_path.clone()))
            .unwrap_or_default();
        entry.tls_client_cert_path = input
            .tls_client_cert_path
            .map(|value| value.display().to_string())
            .or_else(|| non_empty_owned(entry.tls_client_cert_path.clone()))
            .unwrap_or_default();
        entry.tls_client_key_path = input
            .tls_client_key_path
            .map(|value| value.display().to_string())
            .or_else(|| non_empty_owned(entry.tls_client_key_path.clone()))
            .unwrap_or_default();

        let password = if let Some(password) = input.password {
            Some(password)
        } else if let Some(env_name) = input.password_env.as_deref() {
            context.env.get(env_name)
        } else if input.password_prompt {
            Some(resolve_missing_password(
                context,
                &entry.display_label(),
                None,
                &entry.username,
            )?)
        } else if context.non_interactive {
            context.env.get("CRYODB_PASSWORD")
        } else if creating_new {
            let prompted =
                resolve_missing_password(context, &entry.display_label(), None, &entry.username)?;
            if prompted.trim().is_empty() {
                None
            } else {
                Some(prompted)
            }
        } else {
            None
        };

        if let Some(password) = password {
            entry.password = password;
        }
    }

    let entry = entry.normalized();
    if !entry.is_valid() {
        return Err(CliError::validation("Connection profile is incomplete."));
    }
    Ok(entry)
}

fn first_non_empty(first: Option<&str>, second: Option<&str>) -> Option<String> {
    first
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(std::string::ToString::to_string)
        .or_else(|| {
            second
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(std::string::ToString::to_string)
        })
}

fn non_empty_owned(value: String) -> Option<String> {
    if value.trim().is_empty() {
        None
    } else {
        Some(value)
    }
}

fn prompt_optional(
    context: &RuntimeContext,
    label: &str,
    default: Option<&str>,
) -> Result<Option<String>, CliError> {
    if context.non_interactive || !io::stdin().is_terminal() {
        return Ok(default
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(std::string::ToString::to_string));
    }

    let mut stdout = io::stdout().lock();
    if let Some(default) = default.filter(|value| !value.trim().is_empty()) {
        write!(stdout, "{} [{}]: ", label, default)
            .map_err(|error| CliError::general(error.to_string()))?;
    } else {
        write!(stdout, "{}: ", label).map_err(|error| CliError::general(error.to_string()))?;
    }
    stdout
        .flush()
        .map_err(|error| CliError::general(error.to_string()))?;
    let mut input = String::new();
    io::stdin()
        .read_line(&mut input)
        .map_err(|error| CliError::general(error.to_string()))?;
    let value = input.trim();
    if value.is_empty() {
        Ok(default
            .map(str::trim)
            .filter(|item| !item.is_empty())
            .map(std::string::ToString::to_string))
    } else {
        Ok(Some(value.to_string()))
    }
}

fn upsert_profile(store: &mut ConnectionStore, entry: StoredConnection) {
    if let Some(index) = store.profiles.iter().position(|item| {
        item.profile_name().is_some() && item.matches_profile(&entry.profile_label())
    }) {
        store.profiles.remove(index);
    }
    store.profiles.push(entry);
    store.profiles.sort_by(|left, right| {
        left.profile_label()
            .to_ascii_lowercase()
            .cmp(&right.profile_label().to_ascii_lowercase())
            .then_with(|| left.profile_label().cmp(&right.profile_label()))
    });
}

fn persist_connection_store_file(store: &mut ConnectionStore) -> Result<(), CliError> {
    let error = sanitize_connection_store_secrets(store);
    save_connection_store(store).map_err(CliError::general)?;
    if let Some(error) = error {
        return Err(CliError::general(format!(
            "Could not save password to keyring: {error}. Password kept in local fallback."
        )));
    }
    Ok(())
}

fn read_query_input(sql: Option<&str>) -> Result<String, CliError> {
    if let Some(sql) = sql {
        return Ok(sql.to_string());
    }
    if io::stdin().is_terminal() {
        return Err(CliError::validation(
            "Provide SQL as an argument or pipe it through stdin.",
        ));
    }
    read_stdin()
}

fn read_run_input(path: Option<&Path>) -> Result<String, CliError> {
    match path {
        Some(path) if path != Path::new("-") => std::fs::read_to_string(path).map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                CliError::file_not_found(path.display().to_string())
            } else {
                CliError::general(error.to_string())
            }
        }),
        _ => {
            if io::stdin().is_terminal() {
                return Err(CliError::validation(
                    "Provide a SQL file path or pipe SQL through stdin.",
                ));
            }
            read_stdin()
        }
    }
}

fn read_stdin() -> Result<String, CliError> {
    let mut input = String::new();
    io::stdin()
        .read_to_string(&mut input)
        .map_err(|error| CliError::general(error.to_string()))?;
    Ok(input)
}

fn pattern_has_glob_magic(pattern: &str) -> bool {
    pattern.contains('*') || pattern.contains('?') || pattern.contains('[')
}

fn collect_sql_inputs(patterns: &[String], allow_stdin: bool) -> Result<Vec<SqlInput>, CliError> {
    if patterns.is_empty() {
        if !allow_stdin || io::stdin().is_terminal() {
            return Err(CliError::validation(
                "Provide at least one SQL file or pipe SQL through stdin.",
            ));
        }
        return Ok(vec![SqlInput {
            label: String::from("<stdin>"),
            path: None,
            sql: read_stdin()?,
        }]);
    }

    let mut inputs = Vec::new();
    let mut consumed_stdin = false;

    for pattern in patterns {
        if pattern == "-" {
            if !allow_stdin {
                return Err(CliError::validation(
                    "stdin is not supported for this command.",
                ));
            }
            if consumed_stdin {
                return Err(CliError::validation("stdin can only be used once."));
            }
            consumed_stdin = true;
            inputs.push(SqlInput {
                label: String::from("<stdin>"),
                path: None,
                sql: read_stdin()?,
            });
            continue;
        }

        if pattern_has_glob_magic(pattern) {
            let mut matched = false;
            let entries = glob(pattern).map_err(|error| CliError::validation(error.to_string()))?;
            for entry in entries {
                let path = entry.map_err(|error| CliError::validation(error.to_string()))?;
                if !path.is_file() {
                    continue;
                }
                matched = true;
                let sql = std::fs::read_to_string(&path)
                    .map_err(|error| CliError::general(error.to_string()))?;
                inputs.push(SqlInput {
                    label: path.display().to_string(),
                    path: Some(path),
                    sql,
                });
            }
            if !matched {
                return Err(CliError::validation(format!(
                    "Pattern `{pattern}` did not match any files."
                )));
            }
            continue;
        }

        let path = PathBuf::from(pattern);
        if !path.exists() {
            return Err(CliError::file_not_found(path.display().to_string()));
        }
        if !path.is_file() {
            return Err(CliError::validation(format!(
                "Path `{}` is not a file.",
                path.display()
            )));
        }
        let sql =
            std::fs::read_to_string(&path).map_err(|error| CliError::general(error.to_string()))?;
        inputs.push(SqlInput {
            label: path.display().to_string(),
            path: Some(path),
            sql,
        });
    }

    Ok(inputs)
}

const FORMAT_KEYWORDS: &[&str] = &[
    "select",
    "from",
    "where",
    "join",
    "inner",
    "left",
    "right",
    "full",
    "outer",
    "cross",
    "on",
    "group",
    "by",
    "order",
    "limit",
    "offset",
    "insert",
    "into",
    "values",
    "update",
    "set",
    "delete",
    "create",
    "drop",
    "alter",
    "table",
    "view",
    "and",
    "or",
    "not",
    "as",
    "having",
    "union",
    "all",
    "distinct",
    "case",
    "when",
    "then",
    "else",
    "end",
    "explain",
    "analyze",
    "schema",
    "primary",
    "key",
    "foreign",
    "unique",
    "check",
    "null",
    "is",
    "in",
    "exists",
    "returning",
    "truncate",
];

fn is_format_keyword(word: &str) -> bool {
    FORMAT_KEYWORDS.contains(&word.to_ascii_lowercase().as_str())
}

fn transform_sql_words(input: &str, mut transform: impl FnMut(&str) -> String) -> String {
    let mut output = String::with_capacity(input.len());
    let bytes = input.as_bytes();
    let mut index = 0usize;
    let mut word = String::new();

    let flush_word =
        |output: &mut String, word: &mut String, transform: &mut dyn FnMut(&str) -> String| {
            if !word.is_empty() {
                output.push_str(&transform(word));
                word.clear();
            }
        };

    while index < bytes.len() {
        let ch = bytes[index] as char;

        if ch == '-' && index + 1 < bytes.len() && bytes[index + 1] == b'-' {
            flush_word(&mut output, &mut word, &mut transform);
            output.push_str("--");
            index += 2;
            while index < bytes.len() {
                let next = bytes[index] as char;
                output.push(next);
                index += 1;
                if next == '\n' {
                    break;
                }
            }
            continue;
        }

        if ch == '/' && index + 1 < bytes.len() && bytes[index + 1] == b'*' {
            flush_word(&mut output, &mut word, &mut transform);
            output.push_str("/*");
            index += 2;
            while index + 1 < bytes.len() {
                let next = bytes[index] as char;
                output.push(next);
                if next == '*' && bytes[index + 1] == b'/' {
                    output.push('/');
                    index += 2;
                    break;
                }
                index += 1;
            }
            continue;
        }

        if ch == '\'' || ch == '"' || ch == '`' {
            flush_word(&mut output, &mut word, &mut transform);
            let quote = ch;
            output.push(ch);
            index += 1;
            while index < bytes.len() {
                let next = bytes[index] as char;
                output.push(next);
                index += 1;
                if next == '\\' && index < bytes.len() {
                    output.push(bytes[index] as char);
                    index += 1;
                    continue;
                }
                if next == quote {
                    if (quote == '"' || quote == '`')
                        && index < bytes.len()
                        && bytes[index] as char == quote
                    {
                        output.push(bytes[index] as char);
                        index += 1;
                        continue;
                    }
                    break;
                }
            }
            continue;
        }

        if ch.is_ascii_alphanumeric() || ch == '_' || ch == '$' {
            word.push(ch);
            index += 1;
            continue;
        }

        flush_word(&mut output, &mut word, &mut transform);
        output.push(ch);
        index += 1;
    }

    flush_word(&mut output, &mut word, &mut transform);
    output
}

fn uppercase_sql_keywords(input: &str) -> String {
    transform_sql_words(input, |word| {
        if is_format_keyword(word) {
            word.to_ascii_uppercase()
        } else {
            word.to_string()
        }
    })
}

fn collapse_sql_whitespace(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let bytes = input.as_bytes();
    let mut index = 0usize;
    let mut last_space = false;

    while index < bytes.len() {
        let ch = bytes[index] as char;

        if ch == '-' && index + 1 < bytes.len() && bytes[index + 1] == b'-' {
            if output.ends_with(' ') {
                output.pop();
            }
            output.push_str("\n--");
            index += 2;
            while index < bytes.len() {
                let next = bytes[index] as char;
                output.push(next);
                index += 1;
                if next == '\n' {
                    last_space = false;
                    break;
                }
            }
            continue;
        }

        if ch == '/' && index + 1 < bytes.len() && bytes[index + 1] == b'*' {
            if output.ends_with(' ') {
                output.pop();
            }
            output.push_str("\n/*");
            index += 2;
            while index + 1 < bytes.len() {
                let next = bytes[index] as char;
                output.push(next);
                if next == '*' && bytes[index + 1] == b'/' {
                    output.push('/');
                    index += 2;
                    break;
                }
                index += 1;
            }
            last_space = false;
            continue;
        }

        if ch == '\'' || ch == '"' || ch == '`' {
            output.push(ch);
            index += 1;
            while index < bytes.len() {
                let next = bytes[index] as char;
                output.push(next);
                index += 1;
                if next == '\\' && index < bytes.len() {
                    output.push(bytes[index] as char);
                    index += 1;
                    continue;
                }
                if next == ch {
                    if (ch == '"' || ch == '`') && index < bytes.len() && bytes[index] as char == ch
                    {
                        output.push(bytes[index] as char);
                        index += 1;
                        continue;
                    }
                    break;
                }
            }
            last_space = false;
            continue;
        }

        if ch.is_whitespace() {
            if !last_space && !output.is_empty() {
                output.push(' ');
                last_space = true;
            }
            index += 1;
            continue;
        }

        if ch == ',' {
            if output.ends_with(' ') {
                output.pop();
            }
            output.push(',');
            output.push(' ');
            last_space = true;
            index += 1;
            continue;
        }

        if ch == ';' {
            if output.ends_with(' ') {
                output.pop();
            }
            output.push(';');
            last_space = false;
            index += 1;
            continue;
        }

        output.push(ch);
        last_space = false;
        index += 1;
    }

    output.trim().to_string()
}

fn format_sql_text(input: &str) -> String {
    let statements = split_sql_statements(input);
    if statements.is_empty() {
        return collapse_sql_whitespace(&uppercase_sql_keywords(input));
    }

    statements
        .into_iter()
        .map(|statement| {
            let formatted = clause_break_sql(&collapse_sql_whitespace(&uppercase_sql_keywords(
                statement.trim(),
            )));
            if formatted.ends_with(';') {
                formatted
            } else {
                format!("{};", formatted)
            }
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn clause_break_sql(input: &str) -> String {
    const CLAUSES: &[&str] = &[
        "UNION ALL",
        "ORDER BY",
        "GROUP BY",
        "LEFT JOIN",
        "RIGHT JOIN",
        "INNER JOIN",
        "FULL JOIN",
        "CROSS JOIN",
        "RETURNING",
        "HAVING",
        "WHERE",
        "FROM",
        "JOIN",
        "VALUES",
        "LIMIT",
        "OFFSET",
        "UNION",
        "SET",
        "ON",
        "AND",
        "OR",
    ];

    let bytes = input.as_bytes();
    let mut output = String::with_capacity(input.len() + 16);
    let mut index = 0usize;
    let mut quote: Option<u8> = None;

    while index < bytes.len() {
        let ch = bytes[index];
        if let Some(active_quote) = quote {
            output.push(ch as char);
            index += 1;
            if ch == b'\\' && index < bytes.len() {
                output.push(bytes[index] as char);
                index += 1;
                continue;
            }
            if ch == active_quote {
                quote = None;
            }
            continue;
        }

        if matches!(ch, b'\'' | b'"' | b'`') {
            quote = Some(ch);
            output.push(ch as char);
            index += 1;
            continue;
        }

        if ch == b',' {
            while output.ends_with(' ') {
                output.pop();
            }
            output.push(',');
            output.push('\n');
            output.push_str("    ");
            index += 1;
            while index < bytes.len() && bytes[index].is_ascii_whitespace() {
                index += 1;
            }
            continue;
        }

        if let Some(phrase) = CLAUSES
            .iter()
            .find(|phrase| keyword_phrase_at(input, index, phrase))
        {
            while output.ends_with(' ') {
                output.pop();
            }
            if !output.is_empty() && !output.ends_with('\n') {
                output.push('\n');
            }
            if matches!(*phrase, "AND" | "OR" | "ON") {
                output.push_str("  ");
            }
            output.push_str(phrase);
            index += phrase.len();
            continue;
        }

        output.push(ch as char);
        index += 1;
    }

    output
        .lines()
        .map(str::trim_end)
        .collect::<Vec<_>>()
        .join("\n")
}

fn keyword_phrase_at(input: &str, index: usize, phrase: &str) -> bool {
    let end = index.saturating_add(phrase.len());
    let bytes = input.as_bytes();
    if end > bytes.len() || &bytes[index..end] != phrase.as_bytes() {
        return false;
    }
    let before_boundary = index
        .checked_sub(1)
        .and_then(|position| bytes.get(position))
        .map(|ch| !(*ch as char).is_ascii_alphanumeric() && *ch != b'_')
        .unwrap_or(true);
    let after_boundary = bytes
        .get(end)
        .map(|ch| !(*ch as char).is_ascii_alphanumeric() && *ch != b'_')
        .unwrap_or(true);
    before_boundary && after_boundary
}

fn normalized_sql_rule_text(input: &str) -> String {
    let mut normalized = String::with_capacity(input.len());
    let transformed = transform_sql_words(input, |word| word.to_ascii_lowercase());
    for ch in transformed.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' || ch == '*' {
            normalized.push(ch);
        } else {
            normalized.push(' ');
        }
    }
    normalized.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn keyword_casing_issue(input: &str) -> bool {
    let mut issue = false;
    let _ = transform_sql_words(input, |word| {
        if is_format_keyword(word) && word != word.to_ascii_uppercase() {
            issue = true;
        }
        word.to_string()
    });
    issue
}

fn lint_statement(statement: &str) -> Vec<(String, String)> {
    let normalized = normalized_sql_rule_text(statement);
    let mut issues = Vec::new();

    if normalized.contains("select * from") || normalized.contains("select distinct * from") {
        issues.push((
            String::from("select_star"),
            String::from("Avoid `SELECT *`; select explicit columns instead."),
        ));
    }

    if (normalized.starts_with("select ") || normalized.starts_with("with "))
        && !normalized.contains(" limit ")
    {
        issues.push((
            String::from("missing_limit"),
            String::from("SELECT query has no LIMIT."),
        ));
    }

    if normalized.starts_with("delete from ") && !normalized.contains(" where ") {
        issues.push((
            String::from("delete_without_where"),
            String::from("DELETE statement has no WHERE clause."),
        ));
    }

    if normalized.starts_with("update ") && !normalized.contains(" where ") {
        issues.push((
            String::from("update_without_where"),
            String::from("UPDATE statement has no WHERE clause."),
        ));
    }

    if normalized.contains(" join ")
        && !normalized.contains(" on ")
        && !normalized.contains(" using ")
    {
        issues.push((
            String::from("join_without_condition"),
            String::from("JOIN statement has no ON/USING condition."),
        ));
    }

    if has_implicit_comma_join(statement) {
        issues.push((
            String::from("implicit_join"),
            String::from("Comma joins are harder to review; use explicit JOIN syntax."),
        ));
    }

    if normalized.contains(" and ")
        && normalized.contains(" or ")
        && !statement.contains('(')
        && !statement.contains(')')
    {
        issues.push((
            String::from("boolean_precedence"),
            String::from("Mixed AND/OR without parentheses can be ambiguous."),
        ));
    }

    if destructive_statement_kind(statement).is_some() {
        issues.push((
            String::from("destructive_statement"),
            String::from(
                "Statement mutates or removes data/schema; use explicit review/confirmation.",
            ),
        ));
    }

    if format_sql_text(statement).trim() != statement.trim() {
        issues.push((
            String::from("formatter_diff"),
            String::from("Statement differs from `cryodb format` output."),
        ));
    }

    if keyword_casing_issue(statement) {
        issues.push((
            String::from("keyword_casing"),
            String::from("SQL keywords are not consistently uppercased."),
        ));
    }

    issues
}

fn has_implicit_comma_join(statement: &str) -> bool {
    let lower = statement.to_ascii_lowercase();
    let Some(from_start) = lower.find(" from ") else {
        return false;
    };
    let tail = &statement[from_start + 6..];
    let lower_tail = tail.to_ascii_lowercase();
    let end = [" where ", " group ", " order ", " having ", " limit "]
        .iter()
        .filter_map(|needle| lower_tail.find(needle))
        .min()
        .unwrap_or(tail.len());
    tail[..end].contains(',')
}

fn compatibility_issues(driver: DatabaseDriver, statement: &str) -> Vec<String> {
    let normalized = normalized_sql_rule_text(statement);
    let raw = statement.trim();
    let mut issues = Vec::new();

    match driver {
        DatabaseDriver::Sqlite => {
            if normalized.starts_with("show ")
                || normalized.starts_with("describe ")
                || normalized.starts_with("use ")
            {
                issues.push(String::from(
                    "SQLite does not support SHOW/DESCRIBE/USE statements.",
                ));
            }
        }
        DatabaseDriver::PostgreSql => {
            if raw.contains('`') {
                issues.push(String::from(
                    "PostgreSQL does not use backtick identifiers.",
                ));
            }
            if normalized.starts_with("show create ") || normalized.starts_with("describe ") {
                issues.push(String::from(
                    "PostgreSQL does not support MySQL-style SHOW CREATE or DESCRIBE statements.",
                ));
            }
        }
        DatabaseDriver::MySql | DatabaseDriver::MariaDb => {
            if normalized.contains(" ilike ") {
                issues.push(String::from("MySQL/MariaDB do not support ILIKE."));
            }
            if raw.contains("::") {
                issues.push(String::from(
                    "MySQL/MariaDB do not support PostgreSQL-style `::` casts.",
                ));
            }
        }
    }

    issues
}

fn parse_simple_table_reference(
    tokens: &[SqlToken],
    start_index: usize,
) -> Option<(String, usize)> {
    let mut parts = Vec::new();
    let mut index = start_index;
    loop {
        match tokens.get(index)? {
            SqlToken::Word(word) => {
                parts.push(word.clone());
                index += 1;
                if matches!(tokens.get(index), Some(SqlToken::Dot)) {
                    index += 1;
                    if parts.len() >= 2 {
                        return None;
                    }
                    continue;
                }
                break;
            }
            _ => return None,
        }
    }

    Some((parts.join("."), index))
}

fn parse_simple_select_columns_and_table(query: &str) -> Option<(String, Vec<String>)> {
    let table = parse_single_table_select(query)?;
    let tokens = sql_tokens(query);
    let mut depth = 0usize;
    let mut saw_select = false;
    let mut saw_from = false;
    let mut segments: Vec<Vec<SqlToken>> = vec![Vec::new()];

    for token in tokens {
        match &token {
            SqlToken::Word(word)
                if depth == 0 && word.eq_ignore_ascii_case("select") && !saw_select =>
            {
                saw_select = true;
                continue;
            }
            SqlToken::Word(word)
                if depth == 0 && word.eq_ignore_ascii_case("from") && saw_select =>
            {
                saw_from = true;
                break;
            }
            SqlToken::LParen => depth = depth.saturating_add(1),
            SqlToken::RParen => depth = depth.saturating_sub(1),
            SqlToken::Comma if depth == 0 && saw_select => {
                segments.push(Vec::new());
                continue;
            }
            _ => {}
        }

        if saw_select
            && depth == 0
            && let Some(current) = segments.last_mut()
        {
            current.push(token);
        }
    }

    if !saw_select || !saw_from {
        return None;
    }

    let mut columns = Vec::new();
    for segment in segments {
        if segment.is_empty() {
            return None;
        }
        let mut parts = Vec::new();
        for token in segment {
            match token {
                SqlToken::Word(word) => parts.push(word),
                SqlToken::Dot => {}
                _ => return None,
            }
        }
        if parts.is_empty() {
            return None;
        }
        let column = parts.last().cloned().unwrap_or_default();
        if column.eq_ignore_ascii_case("as") {
            return None;
        }
        columns.push(column);
    }

    Some((table, columns))
}

fn parse_simple_table_for_validation(query: &str) -> Option<String> {
    if let Some(table) = parse_single_table_select(query) {
        return Some(table);
    }

    let tokens = sql_tokens(query);
    let first_word = tokens.iter().find_map(|token| match token {
        SqlToken::Word(word) => Some(word.to_ascii_lowercase()),
        _ => None,
    })?;

    match first_word.as_str() {
        "insert" => {
            for (index, token) in tokens.iter().enumerate() {
                if let SqlToken::Word(word) = token
                    && word.eq_ignore_ascii_case("into")
                {
                    return parse_simple_table_reference(&tokens, index + 1)
                        .map(|(table, _)| table);
                }
            }
            None
        }
        "update" => parse_simple_table_reference(&tokens, 1).map(|(table, _)| table),
        "delete" => {
            for (index, token) in tokens.iter().enumerate() {
                if let SqlToken::Word(word) = token
                    && word.eq_ignore_ascii_case("from")
                {
                    return parse_simple_table_reference(&tokens, index + 1)
                        .map(|(table, _)| table);
                }
            }
            None
        }
        _ => None,
    }
}

async fn validate_statement_against_schema(
    connection: &ResolvedConnection,
    statement: &str,
) -> Result<Vec<String>, CliError> {
    let Some(table) = parse_simple_table_for_validation(statement) else {
        return Ok(Vec::new());
    };

    let columns = fetch_query_suggestion_columns(
        connection.pool.clone(),
        connection.database.clone().unwrap_or_default(),
        table.clone(),
    )
    .await;

    let actual_columns = match columns {
        Ok(columns) => columns,
        Err(error) => {
            return Ok(vec![format!(
                "Could not validate table `{table}` against schema: {error}"
            )]);
        }
    };

    if actual_columns.is_empty() {
        return Ok(vec![format!(
            "Table `{table}` was not found or has no visible columns."
        )]);
    }

    if let Some((_, selected_columns)) = parse_simple_select_columns_and_table(statement) {
        let mut issues = Vec::new();
        for column in selected_columns {
            if !actual_columns
                .iter()
                .any(|item| item.eq_ignore_ascii_case(&column))
            {
                issues.push(format!(
                    "Column `{column}` was not found on table `{table}`."
                ));
            }
        }
        return Ok(issues);
    }

    Ok(Vec::new())
}

async fn validate_statement_syntax(
    connection: &ResolvedConnection,
    statement: &str,
) -> Result<Vec<String>, CliError> {
    if !supports_validation_probe(statement) {
        return Ok(Vec::new());
    }
    let probe = validation_probe_query(connection.driver, statement);
    match run_query_with_control(
        connection.pool.clone(),
        connection.database.clone(),
        probe,
        None,
        Some(1),
    )
    .await
    {
        Ok(_) => Ok(Vec::new()),
        Err(error) => Ok(vec![format!(
            "Syntax or semantic probe failed: {}",
            redact_sensitive_text(&error)
        )]),
    }
}

fn supports_validation_probe(statement: &str) -> bool {
    let keyword = normalized_sql_rule_text(statement)
        .split_whitespace()
        .next()
        .unwrap_or("")
        .to_string();
    matches!(
        keyword.as_str(),
        "select" | "with" | "values" | "insert" | "update" | "delete"
    )
}

fn validation_probe_query(driver: DatabaseDriver, statement: &str) -> String {
    match driver {
        DatabaseDriver::Sqlite => format!("EXPLAIN QUERY PLAN {}", statement.trim()),
        DatabaseDriver::MySql | DatabaseDriver::MariaDb | DatabaseDriver::PostgreSql => {
            format!("EXPLAIN {}", statement.trim())
        }
    }
}

fn split_sql_statements(input: &str) -> Vec<String> {
    let mut statements = Vec::new();
    let mut current = String::new();
    let bytes = input.as_bytes();
    let mut index = 0usize;
    let mut single = false;
    let mut double = false;
    let mut backtick = false;
    let mut line_comment = false;
    let mut block_comment = false;
    let mut dollar_tag: Option<String> = None;

    while index < bytes.len() {
        if let Some(tag) = dollar_tag.as_deref() {
            if input[index..].starts_with(tag) {
                current.push_str(tag);
                index += tag.len();
                dollar_tag = None;
                continue;
            }
            current.push(bytes[index] as char);
            index += 1;
            continue;
        }

        let ch = bytes[index] as char;
        let next = bytes.get(index + 1).copied().map(char::from);

        if line_comment {
            current.push(ch);
            index += 1;
            if ch == '\n' {
                line_comment = false;
            }
            continue;
        }

        if block_comment {
            current.push(ch);
            index += 1;
            if ch == '*' && next == Some('/') {
                current.push('/');
                index += 1;
                block_comment = false;
            }
            continue;
        }

        if single {
            current.push(ch);
            index += 1;
            if ch == '\\'
                && let Some(next) = next
            {
                current.push(next);
                index += 1;
                continue;
            }
            if ch == '\'' {
                if next == Some('\'') {
                    current.push('\'');
                    index += 1;
                } else {
                    single = false;
                }
            }
            continue;
        }

        if double {
            current.push(ch);
            index += 1;
            if ch == '"' {
                if next == Some('"') {
                    current.push('"');
                    index += 1;
                } else {
                    double = false;
                }
            }
            continue;
        }

        if backtick {
            current.push(ch);
            index += 1;
            if ch == '`' {
                if next == Some('`') {
                    current.push('`');
                    index += 1;
                } else {
                    backtick = false;
                }
            }
            continue;
        }

        if ch == '$'
            && let Some(tag) = parse_dollar_tag(&input[index..])
        {
            current.push_str(&tag);
            index += tag.len();
            dollar_tag = Some(tag);
            continue;
        }

        if ch == '-' && next == Some('-') {
            current.push('-');
            current.push('-');
            index += 2;
            line_comment = true;
            continue;
        }

        if ch == '#' {
            current.push('#');
            index += 1;
            line_comment = true;
            continue;
        }

        if ch == '/' && next == Some('*') {
            current.push('/');
            current.push('*');
            index += 2;
            block_comment = true;
            continue;
        }

        match ch {
            '\'' => {
                single = true;
                current.push(ch);
                index += 1;
            }
            '"' => {
                double = true;
                current.push(ch);
                index += 1;
            }
            '`' => {
                backtick = true;
                current.push(ch);
                index += 1;
            }
            ';' => {
                let statement = current.trim();
                if !statement.is_empty() {
                    statements.push(statement.to_string());
                }
                current.clear();
                index += 1;
            }
            _ => {
                current.push(ch);
                index += 1;
            }
        }
    }

    let statement = current.trim();
    if !statement.is_empty() {
        statements.push(statement.to_string());
    }
    statements
}

fn parse_dollar_tag(input: &str) -> Option<String> {
    if !input.starts_with('$') {
        return None;
    }
    let bytes = input.as_bytes();
    let mut index = 1usize;
    while index < bytes.len() {
        let ch = bytes[index] as char;
        if ch == '$' {
            return Some(input[..=index].to_string());
        }
        if !(ch.is_ascii_alphanumeric() || ch == '_') {
            return None;
        }
        index += 1;
    }
    None
}

fn statement_kind(query: &str) -> &'static str {
    let keyword = query
        .split_whitespace()
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    match keyword.as_str() {
        "select" => "SELECT",
        "insert" => "INSERT",
        "update" => "UPDATE",
        "delete" => "DELETE",
        "create" => "CREATE",
        "alter" => "ALTER",
        "drop" => "DROP",
        "with" => "WITH",
        "explain" => "EXPLAIN",
        _ => "SQL",
    }
}

fn preview_sql(sql: &str) -> String {
    let compact = sql.split_whitespace().collect::<Vec<_>>().join(" ");
    if compact.len() <= 120 {
        compact
    } else {
        format!("{}...", &compact[..120])
    }
}

fn destructive_statement_kind(query: &str) -> Option<&'static str> {
    let normalized = normalized_sql_rule_text(query);
    let first = normalized.split_whitespace().next().unwrap_or("");
    match first {
        "delete" => Some("DELETE"),
        "update" => Some("UPDATE"),
        "drop" => Some("DROP"),
        "truncate" => Some("TRUNCATE"),
        "alter" => Some("ALTER"),
        "replace" => Some("REPLACE"),
        "insert" if normalized.contains(" on duplicate key update ") => Some("UPSERT"),
        _ => None,
    }
}

fn is_production_connection(connection: &ResolvedConnection) -> bool {
    let label = connection
        .profile_name
        .as_deref()
        .unwrap_or(&connection.label)
        .to_ascii_lowercase();
    label.contains("prod") || label.contains("production") || label.contains("live")
}

fn confirm_destructive_execution(
    context: &RuntimeContext,
    connection: &ResolvedConnection,
    source: &str,
    statements: &[String],
) -> Result<(), CliError> {
    let destructive = statements
        .iter()
        .filter_map(|statement| destructive_statement_kind(statement).map(|kind| (kind, statement)))
        .collect::<Vec<_>>();
    if destructive.is_empty() {
        return Ok(());
    }

    for (kind, statement) in &destructive {
        append_audit_event(context, connection, source, kind, statement)?;
    }

    if context.assume_yes {
        return Ok(());
    }

    let kinds = destructive
        .iter()
        .map(|(kind, _)| *kind)
        .collect::<Vec<_>>()
        .join(", ");
    let production_note = if is_production_connection(connection) {
        " Production-like target detected."
    } else {
        ""
    };
    let message = format!(
        "Destructive SQL detected ({kinds}) on `{}`.{production_note} Re-run with `--yes` or type `yes` to continue.",
        connection.label
    );

    if context.non_interactive || !io::stdin().is_terminal() {
        return Err(CliError::safety(message));
    }

    let mut stderr = io::stderr().lock();
    write!(stderr, "{}\ncontinue? ", message)
        .map_err(|error| CliError::general(error.to_string()))?;
    stderr
        .flush()
        .map_err(|error| CliError::general(error.to_string()))?;
    drop(stderr);

    let mut answer = String::new();
    io::stdin()
        .read_line(&mut answer)
        .map_err(|error| CliError::general(error.to_string()))?;
    if answer.trim().eq_ignore_ascii_case("yes") {
        Ok(())
    } else {
        Err(CliError::safety("Destructive execution aborted."))
    }
}

fn confirm_action(context: &RuntimeContext, action: &str, event: &str) -> Result<(), CliError> {
    context.print_structured_log("warn", event, json!({ "action": action }));
    if context.assume_yes {
        return Ok(());
    }
    if context.non_interactive || !io::stdin().is_terminal() {
        return Err(CliError::safety(format!(
            "{action} requires confirmation. Re-run with `--yes` to continue."
        )));
    }

    let mut stderr = io::stderr().lock();
    write!(stderr, "{action}. Type `yes` to continue: ")
        .map_err(|error| CliError::general(error.to_string()))?;
    stderr
        .flush()
        .map_err(|error| CliError::general(error.to_string()))?;
    drop(stderr);
    let mut answer = String::new();
    io::stdin()
        .read_line(&mut answer)
        .map_err(|error| CliError::general(error.to_string()))?;
    if answer.trim().eq_ignore_ascii_case("yes") {
        Ok(())
    } else {
        Err(CliError::safety("Action aborted."))
    }
}

fn append_audit_event(
    context: &RuntimeContext,
    connection: &ResolvedConnection,
    source: &str,
    statement_kind: &str,
    sql: &str,
) -> Result<(), CliError> {
    let record = AuditRecord {
        timestamp: sqlx::types::chrono::Utc::now().to_rfc3339(),
        event: String::from("destructive_sql"),
        connection: connection
            .profile_name
            .clone()
            .unwrap_or_else(|| connection.label.clone()),
        driver: connection.driver.storage_key().to_string(),
        source: source.to_string(),
        statement_kind: statement_kind.to_string(),
        sql_preview: redact_sensitive_text(&preview_sql(sql)),
    };
    context.print_structured_log(
        "warn",
        "destructive_sql",
        json!({
            "connection": &record.connection,
            "driver": &record.driver,
            "source": &record.source,
            "statement_kind": &record.statement_kind,
            "sql_preview": &record.sql_preview,
        }),
    );

    let path = config_artifact_path(CLI_AUDIT_FILE);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| CliError::general(error.to_string()))?;
    }
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|error| CliError::general(error.to_string()))?;
    serde_json::to_writer(&mut file, &record)
        .map_err(|error| CliError::general(error.to_string()))?;
    writeln!(file).map_err(|error| CliError::general(error.to_string()))
}

async fn run_statements_non_transactional(
    connection: &ResolvedConnection,
    statements: &[String],
    stop_on_error: bool,
    max_rows: Option<usize>,
    source: &str,
) -> Result<Vec<StatementResult>, CliError> {
    run_statements_non_transactional_batch(connection, statements, stop_on_error, max_rows, source)
        .await
        .into_result()
}

async fn run_statements_non_transactional_batch(
    connection: &ResolvedConnection,
    statements: &[String],
    stop_on_error: bool,
    max_rows: Option<usize>,
    _source: &str,
) -> StatementBatch {
    let mut results = Vec::new();
    let mut errors = Vec::new();

    for statement in statements {
        let start = Instant::now();
        match run_query_with_control(
            connection.pool.clone(),
            connection.database.clone(),
            statement.clone(),
            None,
            max_rows,
        )
        .await
        {
            Ok(output) => results.push(StatementResult {
                statement: statement.clone(),
                output,
                elapsed_ms: start.elapsed().as_millis(),
            }),
            Err(error) => {
                errors.push(format!("{}: {}", statement_kind(statement), error));
                if stop_on_error {
                    break;
                }
            }
        }
    }

    StatementBatch { results, errors }
}

async fn run_statements_transactional(
    connection: &ResolvedConnection,
    statements: &[String],
    max_rows: Option<usize>,
) -> Result<Vec<StatementResult>, CliError> {
    match &connection.pool {
        DatabasePool::MySql(pool) => {
            let mut conn = pool
                .acquire()
                .await
                .map_err(|error| CliError::sql(error.to_string()))?;
            if let Some(database) = connection
                .database
                .as_deref()
                .filter(|value| !value.trim().is_empty())
            {
                let use_stmt = format!("USE `{}`", escape_mysql_identifier(database));
                (&mut *conn)
                    .execute(AssertSqlSafe(use_stmt))
                    .await
                    .map_err(|error| CliError::sql(error.to_string()))?;
            }
            let mut tx = conn
                .begin()
                .await
                .map_err(|error| CliError::sql(error.to_string()))?;
            let mut results = Vec::new();
            for statement in statements {
                let start = Instant::now();
                let output = run_mysql_on_with_timeout(&mut *tx, statement, max_rows)
                    .await
                    .map_err(CliError::sql)?;
                results.push(StatementResult {
                    statement: statement.clone(),
                    output,
                    elapsed_ms: start.elapsed().as_millis(),
                });
            }
            tx.commit()
                .await
                .map_err(|error| CliError::sql(error.to_string()))?;
            Ok(results)
        }
        DatabasePool::Sqlite(pool) => {
            let mut conn = pool
                .acquire()
                .await
                .map_err(|error| CliError::sql(error.to_string()))?;
            let mut tx = conn
                .begin()
                .await
                .map_err(|error| CliError::sql(error.to_string()))?;
            let mut results = Vec::new();
            for statement in statements {
                let start = Instant::now();
                let output = run_sqlite_on_with_timeout(&mut *tx, statement, max_rows)
                    .await
                    .map_err(CliError::sql)?;
                results.push(StatementResult {
                    statement: statement.clone(),
                    output,
                    elapsed_ms: start.elapsed().as_millis(),
                });
            }
            tx.commit()
                .await
                .map_err(|error| CliError::sql(error.to_string()))?;
            Ok(results)
        }
        DatabasePool::Postgres(pool) => {
            let mut conn = pool
                .acquire()
                .await
                .map_err(|error| CliError::sql(error.to_string()))?;
            ensure_postgres_database(&mut *conn, connection.database.as_deref())
                .await
                .map_err(CliError::sql)?;
            let mut tx = conn
                .begin()
                .await
                .map_err(|error| CliError::sql(error.to_string()))?;
            let mut results = Vec::new();
            for statement in statements {
                let start = Instant::now();
                let output = run_postgres_on_with_timeout(&mut *tx, statement, max_rows)
                    .await
                    .map_err(CliError::sql)?;
                results.push(StatementResult {
                    statement: statement.clone(),
                    output,
                    elapsed_ms: start.elapsed().as_millis(),
                });
            }
            tx.commit()
                .await
                .map_err(|error| CliError::sql(error.to_string()))?;
            Ok(results)
        }
    }
}

async fn run_mysql_on<E>(
    executor: &mut E,
    query: &str,
    max_rows: Option<usize>,
) -> Result<QueryOutput, String>
where
    for<'e> &'e mut E: Executor<'e, Database = MySql>,
{
    if query_returns_rows(query) {
        let (columns, column_kinds, rows) = fetch_mysql_rows(executor, query, max_rows).await?;
        let count = columns.len();
        Ok(QueryOutput::Rows(ResultSet {
            columns,
            column_kinds,
            column_nullable: vec![false; count],
            rows,
        }))
    } else {
        let result = sqlx::query(AssertSqlSafe(query))
            .execute(&mut *executor)
            .await
            .map_err(|error| error.to_string())?;
        Ok(QueryOutput::Affected(result.rows_affected()))
    }
}

async fn run_mysql_on_with_timeout<E>(
    executor: &mut E,
    query: &str,
    max_rows: Option<usize>,
) -> Result<QueryOutput, String>
where
    for<'e> &'e mut E: Executor<'e, Database = MySql>,
{
    if let Some(timeout) = cli_query_timeout() {
        return match tokio::time::timeout(timeout, run_mysql_on(executor, query, max_rows)).await {
            Ok(result) => result,
            Err(_) => Err(format!(
                "Query timed out after {} seconds.",
                timeout.as_secs()
            )),
        };
    }
    run_mysql_on(executor, query, max_rows).await
}

async fn run_sqlite_on<E>(
    executor: &mut E,
    query: &str,
    max_rows: Option<usize>,
) -> Result<QueryOutput, String>
where
    for<'e> &'e mut E: Executor<'e, Database = Sqlite>,
{
    if query_returns_rows(query) {
        let (columns, column_kinds, rows) = fetch_sqlite_rows(executor, query, max_rows).await?;
        let count = columns.len();
        Ok(QueryOutput::Rows(ResultSet {
            columns,
            column_kinds,
            column_nullable: vec![false; count],
            rows,
        }))
    } else {
        let result = sqlx::query(AssertSqlSafe(query))
            .execute(&mut *executor)
            .await
            .map_err(|error| error.to_string())?;
        Ok(QueryOutput::Affected(result.rows_affected()))
    }
}

async fn run_sqlite_on_with_timeout<E>(
    executor: &mut E,
    query: &str,
    max_rows: Option<usize>,
) -> Result<QueryOutput, String>
where
    for<'e> &'e mut E: Executor<'e, Database = Sqlite>,
{
    if let Some(timeout) = cli_query_timeout() {
        return match tokio::time::timeout(timeout, run_sqlite_on(executor, query, max_rows)).await {
            Ok(result) => result,
            Err(_) => Err(format!(
                "Query timed out after {} seconds.",
                timeout.as_secs()
            )),
        };
    }
    run_sqlite_on(executor, query, max_rows).await
}

async fn run_postgres_on<E>(
    executor: &mut E,
    query: &str,
    max_rows: Option<usize>,
) -> Result<QueryOutput, String>
where
    for<'e> &'e mut E: Executor<'e, Database = Postgres>,
{
    if query_returns_rows(query) {
        let (columns, column_kinds, rows) = fetch_postgres_rows(executor, query, max_rows).await?;
        let count = columns.len();
        Ok(QueryOutput::Rows(ResultSet {
            columns,
            column_kinds,
            column_nullable: vec![false; count],
            rows,
        }))
    } else {
        let result = sqlx::query(AssertSqlSafe(query))
            .execute(&mut *executor)
            .await
            .map_err(|error| error.to_string())?;
        Ok(QueryOutput::Affected(result.rows_affected()))
    }
}

async fn run_postgres_on_with_timeout<E>(
    executor: &mut E,
    query: &str,
    max_rows: Option<usize>,
) -> Result<QueryOutput, String>
where
    for<'e> &'e mut E: Executor<'e, Database = Postgres>,
{
    if let Some(timeout) = cli_query_timeout() {
        return match tokio::time::timeout(timeout, run_postgres_on(executor, query, max_rows)).await
        {
            Ok(result) => result,
            Err(_) => Err(format!(
                "Query timed out after {} seconds.",
                timeout.as_secs()
            )),
        };
    }
    run_postgres_on(executor, query, max_rows).await
}

async fn fetch_mysql_rows<E>(
    executor: &mut E,
    query: &str,
    max_rows: Option<usize>,
) -> Result<(Vec<String>, Vec<ColumnKind>, Vec<Vec<String>>), String>
where
    for<'e> &'e mut E: Executor<'e, Database = MySql>,
{
    let mut stream = sqlx::query(AssertSqlSafe(query)).fetch(&mut *executor);
    let mut columns = Vec::new();
    let mut kinds = Vec::new();
    let mut rows = Vec::new();
    while let Some(row) = stream.try_next().await.map_err(|error| error.to_string())? {
        if columns.is_empty() {
            for column in row.columns() {
                columns.push(column.name().to_string());
                kinds.push(crate::column_kind_from_type_name(column.type_info().name()));
            }
        }
        if let Some(limit) = max_rows
            && rows.len() >= limit
        {
            break;
        }
        rows.push(row_to_strings(&row, &kinds, columns.len()));
    }
    Ok((columns, kinds, rows))
}

async fn fetch_sqlite_rows<E>(
    executor: &mut E,
    query: &str,
    max_rows: Option<usize>,
) -> Result<(Vec<String>, Vec<ColumnKind>, Vec<Vec<String>>), String>
where
    for<'e> &'e mut E: Executor<'e, Database = Sqlite>,
{
    let mut stream = sqlx::query(AssertSqlSafe(query)).fetch(&mut *executor);
    let mut columns = Vec::new();
    let mut kinds = Vec::new();
    let mut rows = Vec::new();
    while let Some(row) = stream.try_next().await.map_err(|error| error.to_string())? {
        if columns.is_empty() {
            for column in row.columns() {
                columns.push(column.name().to_string());
                kinds.push(crate::column_kind_from_type_name(column.type_info().name()));
            }
        }
        if let Some(limit) = max_rows
            && rows.len() >= limit
        {
            break;
        }
        rows.push(row_to_strings_sqlite(&row, &kinds, columns.len()));
    }
    Ok((columns, kinds, rows))
}

async fn fetch_postgres_rows<E>(
    executor: &mut E,
    query: &str,
    max_rows: Option<usize>,
) -> Result<(Vec<String>, Vec<ColumnKind>, Vec<Vec<String>>), String>
where
    for<'e> &'e mut E: Executor<'e, Database = Postgres>,
{
    let mut stream = sqlx::query(AssertSqlSafe(query)).fetch(&mut *executor);
    let mut columns = Vec::new();
    let mut kinds = Vec::new();
    let mut type_names = Vec::new();
    let mut rows = Vec::new();
    while let Some(row) = stream.try_next().await.map_err(|error| error.to_string())? {
        if columns.is_empty() {
            for column in row.columns() {
                columns.push(column.name().to_string());
                kinds.push(crate::column_kind_from_type_name(column.type_info().name()));
                type_names.push(column.type_info().name().to_string());
            }
        }
        if let Some(limit) = max_rows
            && rows.len() >= limit
        {
            break;
        }
        rows.push(row_to_strings_postgres(
            &row,
            &kinds,
            &type_names,
            columns.len(),
        ));
    }
    Ok((columns, kinds, rows))
}

async fn ensure_postgres_database<E>(executor: &mut E, database: Option<&str>) -> Result<(), String>
where
    for<'e> &'e mut E: Executor<'e, Database = Postgres>,
{
    let Some(database) = database.filter(|value| !value.trim().is_empty()) else {
        return Ok(());
    };
    let current = sqlx::query_scalar::<_, String>("SELECT current_database()")
        .fetch_one(&mut *executor)
        .await
        .map_err(|error| error.to_string())?;
    if current != database.trim() {
        return Err(format!(
            "Connected to PostgreSQL database `{current}`. Switch database by reconnecting for now."
        ));
    }
    Ok(())
}

fn simple_result_set(columns: Vec<&str>, rows: Vec<Vec<String>>) -> ResultSet {
    ResultSet {
        column_kinds: vec![ColumnKind::Unknown; columns.len()],
        column_nullable: vec![false; columns.len()],
        columns: columns.into_iter().map(String::from).collect(),
        rows,
    }
}

fn render_statement_results(
    results: &[StatementResult],
    render: &ResolvedRender,
) -> Result<(), CliError> {
    if results.is_empty() {
        return Ok(());
    }

    if results.len() == 1 || render.single_result {
        let result = if render.single_result {
            results.last().unwrap_or(&results[0])
        } else {
            &results[0]
        };
        let bytes = render_query_output_to_bytes(&result.output, render)?;
        return write_bytes(&bytes, render);
    }

    match render.format {
        OutputFormat::Json => {
            let value = Value::Array(
                results
                    .iter()
                    .map(|result| {
                        json!({
                            "statement": result.statement,
                            "elapsed_ms": result.elapsed_ms,
                            "result": query_output_to_json(&result.output),
                        })
                    })
                    .collect(),
            );
            write_value_to_output(&value, render)
        }
        OutputFormat::Ndjson => {
            let mut buffer = Vec::new();
            for result in results {
                let value = json!({
                    "statement": result.statement,
                    "elapsed_ms": result.elapsed_ms,
                    "result": query_output_to_json(&result.output),
                });
                serde_json::to_writer(&mut buffer, &value)
                    .map_err(|error| CliError::general(error.to_string()))?;
                buffer.push(b'\n');
            }
            write_bytes(&buffer, render)
        }
        _ => {
            let mut output = Vec::new();
            for (index, result) in results.iter().enumerate() {
                if index > 0 {
                    output.extend_from_slice(b"\n");
                }
                output.extend_from_slice(format!("Statement {}\n", index + 1).as_bytes());
                output.extend_from_slice(
                    render_query_output_to_bytes(&result.output, render)?.as_slice(),
                );
                output.extend_from_slice(format!("\n{} ms\n", result.elapsed_ms).as_bytes());
            }
            write_bytes(&output, render)
        }
    }
}

fn render_query_output(output: QueryOutput, render: &ResolvedRender) -> Result<(), CliError> {
    let bytes = render_query_output_to_bytes(&output, render)?;
    write_bytes(&bytes, render)
}

fn render_query_output_to_bytes(
    output: &QueryOutput,
    render: &ResolvedRender,
) -> Result<Vec<u8>, CliError> {
    let mut buffer = Vec::new();
    match render.format {
        OutputFormat::Json => {
            serde_json::to_writer_pretty(&mut buffer, &query_output_to_json(output))
                .map_err(|error| CliError::general(error.to_string()))?;
            buffer.push(b'\n');
        }
        OutputFormat::Ndjson => render_ndjson(output, &mut buffer)?,
        OutputFormat::Csv => render_delimited(output, b',', !render.no_header, &mut buffer)?,
        OutputFormat::Tsv => render_delimited(output, b'\t', !render.no_header, &mut buffer)?,
        OutputFormat::Markdown => render_markdown(output, !render.no_header, &mut buffer)?,
        OutputFormat::Raw => {
            render_raw(output, !render.no_header, render.single_value, &mut buffer)?
        }
        OutputFormat::Pretty | OutputFormat::Table => {
            let use_color = !render.no_color && io::stdout().is_terminal();
            render_table(
                output,
                !render.no_header,
                render.single_value,
                use_color,
                &mut buffer,
            )?
        }
    }
    Ok(buffer)
}

fn write_bytes(bytes: &[u8], render: &ResolvedRender) -> Result<(), CliError> {
    if let Some(limit) = render.max_output_size
        && bytes.len() > limit
    {
        return Err(CliError::safety(format!(
            "Output size {} bytes exceeds --max-output-size {} bytes.",
            bytes.len(),
            limit
        )));
    }

    if render.out_temp && render.out.is_none() {
        let file = tempfile::Builder::new()
            .prefix("cryodb-")
            .suffix(output_file_suffix(render.format))
            .tempfile()
            .map_err(|error| CliError::general(error.to_string()))?;
        let path = file.path().to_path_buf();
        std::fs::write(&path, bytes).map_err(|error| CliError::general(error.to_string()))?;
        let kept = file
            .into_temp_path()
            .keep()
            .map_err(|error| CliError::general(error.error.to_string()))?;
        return writeln!(io::stdout().lock(), "{}", kept.display())
            .map_err(|error| CliError::general(error.to_string()));
    }

    if let Some(path) = render.out.as_deref().filter(|path| *path != Path::new("-")) {
        return std::fs::write(path, bytes).map_err(|error| CliError::general(error.to_string()));
    }

    let use_pager = render.pager && !render.no_color && io::stdout().is_terminal();
    if use_pager && bytes.len() > 1024 {
        let pager_cmd = std::env::var("CRYODB_PAGER")
            .or_else(|_| std::env::var("PAGER"))
            .unwrap_or_else(|_| String::from("less -R"));
        let mut parts = pager_cmd.split_whitespace();
        let cmd = parts.next().unwrap_or("less");
        let args: Vec<&str> = parts.collect();
        let mut child = std::process::Command::new(cmd)
            .args(&args)
            .stdin(Stdio::piped())
            .spawn()
            .map_err(|error| CliError::general(format!("Failed to start pager: {error}")))?;
        if let Some(stdin) = child.stdin.take() {
            let mut stdin = stdin;
            let _ = stdin.write_all(bytes);
        }
        let _ = child.wait();
        Ok(())
    } else {
        io::stdout()
            .lock()
            .write_all(bytes)
            .map_err(|error| CliError::general(error.to_string()))
    }
}

fn output_file_suffix(format: OutputFormat) -> &'static str {
    match format {
        OutputFormat::Json => ".json",
        OutputFormat::Ndjson => ".ndjson",
        OutputFormat::Csv => ".csv",
        OutputFormat::Tsv => ".tsv",
        OutputFormat::Markdown => ".md",
        OutputFormat::Raw => ".sql",
        OutputFormat::Pretty | OutputFormat::Table => ".txt",
    }
}

fn write_value_to_output(value: &Value, render: &ResolvedRender) -> Result<(), CliError> {
    let mut buffer = Vec::new();
    serde_json::to_writer_pretty(&mut buffer, value)
        .map_err(|error| CliError::general(error.to_string()))?;
    buffer.push(b'\n');
    write_bytes(&buffer, render)
}

fn query_output_to_json(output: &QueryOutput) -> Value {
    match output {
        QueryOutput::Affected(count) => json!({ "rows_affected": count }),
        QueryOutput::Rows(rows) => Value::Array(
            rows.rows
                .iter()
                .map(|row| {
                    let mut object = serde_json::Map::new();
                    for (index, column) in rows.columns.iter().enumerate() {
                        object.insert(
                            column.clone(),
                            Value::String(row.get(index).cloned().unwrap_or_default()),
                        );
                    }
                    Value::Object(object)
                })
                .collect(),
        ),
    }
}

fn render_ndjson(output: &QueryOutput, writer: &mut Vec<u8>) -> Result<(), CliError> {
    match output {
        QueryOutput::Affected(count) => {
            serde_json::to_writer(&mut *writer, &json!({ "rows_affected": count }))
                .map_err(|error| CliError::general(error.to_string()))?;
            writer.push(b'\n');
        }
        QueryOutput::Rows(rows) => {
            for row in &rows.rows {
                let mut object = serde_json::Map::new();
                for (index, column) in rows.columns.iter().enumerate() {
                    object.insert(
                        column.clone(),
                        Value::String(row.get(index).cloned().unwrap_or_default()),
                    );
                }
                serde_json::to_writer(&mut *writer, &Value::Object(object))
                    .map_err(|error| CliError::general(error.to_string()))?;
                writer.push(b'\n');
            }
        }
    }
    Ok(())
}

fn render_delimited(
    output: &QueryOutput,
    delimiter: u8,
    include_header: bool,
    writer: &mut Vec<u8>,
) -> Result<(), CliError> {
    let mut csv = WriterBuilder::new()
        .delimiter(delimiter)
        .from_writer(writer);
    match output {
        QueryOutput::Affected(count) => {
            if include_header {
                csv.write_record(["rows_affected"])
                    .map_err(|error| CliError::general(error.to_string()))?;
            }
            csv.write_record([count.to_string()])
                .map_err(|error| CliError::general(error.to_string()))?;
        }
        QueryOutput::Rows(rows) => {
            if include_header {
                csv.write_record(rows.columns.iter())
                    .map_err(|error| CliError::general(error.to_string()))?;
            }
            for row in &rows.rows {
                csv.write_record(row.iter())
                    .map_err(|error| CliError::general(error.to_string()))?;
            }
        }
    }
    csv.flush()
        .map_err(|error| CliError::general(error.to_string()))
}

fn render_markdown(
    output: &QueryOutput,
    include_header: bool,
    writer: &mut Vec<u8>,
) -> Result<(), CliError> {
    match output {
        QueryOutput::Affected(count) => {
            writer.extend_from_slice(
                format!("| rows_affected |\n| --- |\n| {} |\n", count).as_bytes(),
            );
        }
        QueryOutput::Rows(rows) => {
            if include_header {
                writer.extend_from_slice(format!("| {} |\n", rows.columns.join(" | ")).as_bytes());
                writer.extend_from_slice(
                    format!(
                        "| {} |\n",
                        rows.columns
                            .iter()
                            .map(|_| "---")
                            .collect::<Vec<_>>()
                            .join(" | ")
                    )
                    .as_bytes(),
                );
            }
            for row in &rows.rows {
                writer.extend_from_slice(format!("| {} |\n", row.join(" | ")).as_bytes());
            }
        }
    }
    Ok(())
}

fn render_raw(
    output: &QueryOutput,
    include_header: bool,
    single_value: bool,
    writer: &mut Vec<u8>,
) -> Result<(), CliError> {
    match output {
        QueryOutput::Affected(count) => {
            writer.extend_from_slice(count.to_string().as_bytes());
            writer.push(b'\n');
        }
        QueryOutput::Rows(rows) => {
            if single_value {
                let value = rows
                    .rows
                    .first()
                    .and_then(|row| row.first())
                    .cloned()
                    .unwrap_or_default();
                writer.extend_from_slice(value.as_bytes());
                writer.push(b'\n');
                return Ok(());
            }
            if include_header {
                writer.extend_from_slice(rows.columns.join("\t").as_bytes());
                writer.push(b'\n');
            }
            for row in &rows.rows {
                writer.extend_from_slice(row.join("\t").as_bytes());
                writer.push(b'\n');
            }
        }
    }
    Ok(())
}

fn render_table(
    output: &QueryOutput,
    include_header: bool,
    single_value: bool,
    use_color: bool,
    writer: &mut Vec<u8>,
) -> Result<(), CliError> {
    match output {
        QueryOutput::Affected(count) => {
            writer.extend_from_slice(format!("rows_affected\n{}\n", count).as_bytes());
        }
        QueryOutput::Rows(rows) => {
            if single_value {
                let value = rows
                    .rows
                    .first()
                    .and_then(|row| row.first())
                    .cloned()
                    .unwrap_or_default();
                writer.extend_from_slice(value.as_bytes());
                writer.push(b'\n');
                return Ok(());
            }

            let mut widths = rows
                .columns
                .iter()
                .map(|value| value.len())
                .collect::<Vec<_>>();
            for row in &rows.rows {
                for (index, value) in row.iter().enumerate() {
                    if let Some(width) = widths.get_mut(index) {
                        *width = (*width).max(value.len());
                    }
                }
            }

            let separator = if use_color {
                widths
                    .iter()
                    .map(|width| format!("+{}", "-".repeat(width.saturating_add(2)).dimmed()))
                    .collect::<String>()
                    + &"+\n".dimmed().to_string()
            } else {
                widths
                    .iter()
                    .map(|width| format!("+{}", "-".repeat(width.saturating_add(2))))
                    .collect::<String>()
                    + "+\n"
            };
            writer.extend_from_slice(separator.as_bytes());
            if include_header {
                let header_row = if use_color {
                    format_table_row_colored(&rows.columns, &widths, true)
                } else {
                    format_table_row(&rows.columns, &widths)
                };
                writer.extend_from_slice(header_row.as_bytes());
                writer.extend_from_slice(separator.as_bytes());
            }
            for row in &rows.rows {
                let data_row = if use_color {
                    format_table_row_colored(row, &widths, false)
                } else {
                    format_table_row(row, &widths)
                };
                writer.extend_from_slice(data_row.as_bytes());
            }
            writer.extend_from_slice(separator.as_bytes());
        }
    }
    Ok(())
}

fn format_table_row(values: &[String], widths: &[usize]) -> String {
    let mut row = String::new();
    for (index, width) in widths.iter().enumerate() {
        let value = values.get(index).cloned().unwrap_or_default();
        row.push('|');
        row.push(' ');
        row.push_str(&format!("{value:<width$}", width = *width));
        row.push(' ');
    }
    row.push('|');
    row.push('\n');
    row
}

fn format_table_row_colored(values: &[String], widths: &[usize], is_header: bool) -> String {
    let mut row = String::new();
    for (index, width) in widths.iter().enumerate() {
        let value = values.get(index).cloned().unwrap_or_default();
        row.push_str(&"| ".dimmed().to_string());
        let display = if is_header {
            format!("{value:<width$}", width = *width)
                .cyan()
                .bold()
                .to_string()
        } else if value == "NULL" {
            format!("{value:<width$}", width = *width)
                .dimmed()
                .italic()
                .to_string()
        } else {
            format!("{value:<width$}", width = *width)
        };
        row.push_str(&display);
        row.push_str(&" ".dimmed().to_string());
    }
    row.push_str(&"|\n".dimmed().to_string());
    row
}

async fn describe_bundle(
    connection: &ResolvedConnection,
    table: &str,
) -> Result<DescribeBundle, CliError> {
    let details = fetch_table_info(
        connection.pool.clone(),
        connection.database.clone().unwrap_or_default(),
        table.to_string(),
    )
    .await
    .ok()
    .map(|details| TableInfoRows {
        rows: vec![
            (String::from("driver"), connection.driver.to_string()),
            (
                String::from("database"),
                connection.database.clone().unwrap_or_default(),
            ),
            (String::from("engine"), details.engine.unwrap_or_default()),
            (
                String::from("rows"),
                details
                    .rows
                    .map(|value| value.to_string())
                    .unwrap_or_default(),
            ),
            (
                String::from("data_size"),
                details
                    .data_size
                    .map(|value| value.to_string())
                    .unwrap_or_default(),
            ),
            (
                String::from("index_size"),
                details
                    .index_size
                    .map(|value| value.to_string())
                    .unwrap_or_default(),
            ),
            (
                String::from("free_size"),
                details
                    .free_size
                    .map(|value| value.to_string())
                    .unwrap_or_default(),
            ),
            (
                String::from("collation"),
                details.collation.unwrap_or_default(),
            ),
            (String::from("charset"), details.charset.unwrap_or_default()),
            (String::from("comment"), details.comment.unwrap_or_default()),
        ],
    });

    let columns = run_query_with_control(
        connection.pool.clone(),
        connection.database.clone(),
        describe_columns_query(connection.driver, connection.database.as_deref(), table),
        None,
        None,
    )
    .await
    .map_err(CliError::sql)
    .ok();

    let indexes = run_query_with_control(
        connection.pool.clone(),
        connection.database.clone(),
        indexes_query(
            connection.driver,
            connection.database.as_deref(),
            Some(table),
        ),
        None,
        None,
    )
    .await
    .map_err(CliError::sql)
    .ok();

    let relations = fetch_table_relations(
        connection.pool.clone(),
        connection.database.clone().unwrap_or_default(),
        table.to_string(),
    )
    .await
    .ok()
    .map(|rows| {
        QueryOutput::Rows(simple_result_set(
            vec!["column", "referenced_table", "referenced_column"],
            rows.into_iter()
                .map(|row| vec![row.column, row.referenced_table, row.referenced_column])
                .collect(),
        ))
    });

    let constraints = match connection.driver {
        DatabaseDriver::Sqlite => sqlite_constraints_output(connection, Some(table))
            .await
            .ok(),
        _ => run_query_with_control(
            connection.pool.clone(),
            connection.database.clone(),
            constraints_query(
                connection.driver,
                connection.database.as_deref(),
                Some(table),
            ),
            None,
            None,
        )
        .await
        .ok(),
    };

    Ok(DescribeBundle {
        details,
        columns,
        indexes,
        relations,
        constraints,
    })
}

fn render_describe_bundle(
    table: &str,
    bundle: DescribeBundle,
    render: &ResolvedRender,
) -> Result<(), CliError> {
    match render.format {
        OutputFormat::Json => {
            write_value_to_output(&json!({ table: describe_bundle_to_json(&bundle) }), render)
        }
        _ => {
            let mut output = Vec::new();
            output.extend_from_slice(format!("Table: {}\n\n", table).as_bytes());
            if let Some(details) = bundle.details {
                output.extend_from_slice(b"Details\n");
                output.extend_from_slice(
                    render_query_output_to_bytes(
                        &QueryOutput::Rows(simple_result_set(
                            vec!["field", "value"],
                            details
                                .rows
                                .into_iter()
                                .filter(|(_, value)| !value.is_empty())
                                .map(|(field, value)| vec![field, value])
                                .collect(),
                        )),
                        render,
                    )?
                    .as_slice(),
                );
                output.extend_from_slice(b"\n");
            }
            if let Some(columns) = bundle.columns {
                output.extend_from_slice(b"Columns\n");
                output
                    .extend_from_slice(render_query_output_to_bytes(&columns, render)?.as_slice());
                output.extend_from_slice(b"\n");
            }
            if let Some(indexes) = bundle.indexes {
                output.extend_from_slice(b"Indexes\n");
                output
                    .extend_from_slice(render_query_output_to_bytes(&indexes, render)?.as_slice());
                output.extend_from_slice(b"\n");
            }
            if let Some(relations) = bundle.relations {
                output.extend_from_slice(b"Relations\n");
                output.extend_from_slice(
                    render_query_output_to_bytes(&relations, render)?.as_slice(),
                );
                output.extend_from_slice(b"\n");
            }
            if let Some(constraints) = bundle.constraints {
                output.extend_from_slice(b"Constraints\n");
                output.extend_from_slice(
                    render_query_output_to_bytes(&constraints, render)?.as_slice(),
                );
            }
            write_bytes(&output, render)
        }
    }
}

fn describe_bundle_to_json(bundle: &DescribeBundle) -> Value {
    json!({
        "details": bundle.details.as_ref().map(|details| {
            let mut map = serde_json::Map::new();
            for (field, value) in &details.rows {
                if !value.is_empty() {
                    map.insert(field.clone(), Value::String(value.clone()));
                }
            }
            Value::Object(map)
        }),
        "columns": bundle.columns.as_ref().map(query_output_to_json).unwrap_or(Value::Null),
        "indexes": bundle.indexes.as_ref().map(query_output_to_json).unwrap_or(Value::Null),
        "relations": bundle.relations.as_ref().map(query_output_to_json).unwrap_or(Value::Null),
        "constraints": bundle.constraints.as_ref().map(query_output_to_json).unwrap_or(Value::Null),
    })
}

fn render_schema_bundle_markdown(
    table: &str,
    bundle: &DescribeBundle,
    writer: &mut dyn Write,
) -> io::Result<()> {
    writeln!(writer, "## {}", table)?;
    writeln!(writer)?;
    if let Some(details) = &bundle.details {
        writeln!(writer, "### Details")?;
        writeln!(writer)?;
        for (field, value) in &details.rows {
            if !value.is_empty() {
                writeln!(writer, "- {}: {}", field, value)?;
            }
        }
        writeln!(writer)?;
    }
    if let Some(columns) = &bundle.columns {
        writeln!(writer, "### Columns")?;
        writeln!(writer)?;
        let bytes = render_query_output_to_bytes(
            columns,
            &ResolvedRender {
                format: OutputFormat::Markdown,
                out: None,
                no_header: false,
                single_value: false,
                max_rows: None,
                max_output_size: None,
                out_temp: false,
                single_result: false,
                pager: false,
                no_color: false,
            },
        )
        .map_err(|error| io::Error::other(error.message))?;
        writer.write_all(bytes.as_slice())?;
        writeln!(writer)?;
    }
    if let Some(indexes) = &bundle.indexes {
        writeln!(writer, "### Indexes")?;
        writeln!(writer)?;
        let bytes = render_query_output_to_bytes(
            indexes,
            &ResolvedRender {
                format: OutputFormat::Markdown,
                out: None,
                no_header: false,
                single_value: false,
                max_rows: None,
                max_output_size: None,
                out_temp: false,
                single_result: false,
                pager: false,
                no_color: false,
            },
        )
        .map_err(|error| io::Error::other(error.message))?;
        writer.write_all(bytes.as_slice())?;
        writeln!(writer)?;
    }
    if let Some(relations) = &bundle.relations {
        writeln!(writer, "### Relations")?;
        writeln!(writer)?;
        let bytes = render_query_output_to_bytes(
            relations,
            &ResolvedRender {
                format: OutputFormat::Markdown,
                out: None,
                no_header: false,
                single_value: false,
                max_rows: None,
                max_output_size: None,
                out_temp: false,
                single_result: false,
                pager: false,
                no_color: false,
            },
        )
        .map_err(|error| io::Error::other(error.message))?;
        writer.write_all(bytes.as_slice())?;
        writeln!(writer)?;
    }
    if let Some(constraints) = &bundle.constraints {
        writeln!(writer, "### Constraints")?;
        writeln!(writer)?;
        let bytes = render_query_output_to_bytes(
            constraints,
            &ResolvedRender {
                format: OutputFormat::Markdown,
                out: None,
                no_header: false,
                single_value: false,
                max_rows: None,
                max_output_size: None,
                out_temp: false,
                single_result: false,
                pager: false,
                no_color: false,
            },
        )
        .map_err(|error| io::Error::other(error.message))?;
        writer.write_all(bytes.as_slice())?;
        writeln!(writer)?;
    }
    Ok(())
}

async fn schema_sql_output(
    connection: &ResolvedConnection,
    table: Option<&str>,
) -> Result<String, CliError> {
    match connection.driver {
        DatabaseDriver::PostgreSql => {
            let bytes = dump_postgres(connection, table, true, false).await?;
            String::from_utf8(bytes).map_err(|error| CliError::general(error.to_string()))
        }
        DatabaseDriver::MySql | DatabaseDriver::MariaDb => {
            if let Some(table) = table {
                let sql = format!(
                    "SHOW CREATE TABLE {}",
                    sql_quote_table_reference(connection.driver, table)
                );
                let output = run_query_with_control(
                    connection.pool.clone(),
                    connection.database.clone(),
                    sql,
                    None,
                    Some(1),
                )
                .await
                .map_err(CliError::sql)?;
                return extract_last_column_text(output);
            }
            let objects = shell_objects(connection).await?;
            let mut statements = Vec::new();
            for object in objects {
                let sql = format!(
                    "SHOW CREATE TABLE {}",
                    sql_quote_table_reference(connection.driver, &object)
                );
                let output = run_query_with_control(
                    connection.pool.clone(),
                    connection.database.clone(),
                    sql,
                    None,
                    Some(1),
                )
                .await
                .map_err(CliError::sql)?;
                statements.push(extract_last_column_text(output)?);
            }
            Ok(statements.join("\n\n"))
        }
        DatabaseDriver::Sqlite => {
            if let Some(table) = table {
                let statement = format!(
                    "SELECT sql FROM sqlite_master WHERE name = {} AND type IN ('table', 'view') LIMIT 1",
                    sql_quote_string_literal(DatabaseDriver::Sqlite, table)
                );
                let output = run_query_with_control(
                    connection.pool.clone(),
                    connection.database.clone(),
                    statement,
                    None,
                    Some(1),
                )
                .await
                .map_err(CliError::sql)?;
                return extract_first_value_text(output);
            }
            let statement = String::from(
                "SELECT sql FROM sqlite_master WHERE type IN ('table', 'view') AND sql IS NOT NULL ORDER BY name",
            );
            let output = run_query_with_control(
                connection.pool.clone(),
                connection.database.clone(),
                statement,
                None,
                None,
            )
            .await
            .map_err(CliError::sql)?;
            match output {
                QueryOutput::Rows(rows) => Ok(rows
                    .rows
                    .iter()
                    .filter_map(|row| row.first().cloned())
                    .collect::<Vec<_>>()
                    .join("\n\n")),
                QueryOutput::Affected(_) => Ok(String::new()),
            }
        }
    }
}

fn extract_last_column_text(output: QueryOutput) -> Result<String, CliError> {
    match output {
        QueryOutput::Rows(rows) => rows
            .rows
            .first()
            .and_then(|row| row.last())
            .cloned()
            .ok_or_else(|| CliError::sql("No schema output returned.")),
        QueryOutput::Affected(_) => Err(CliError::sql("No schema output returned.")),
    }
}

fn extract_first_value_text(output: QueryOutput) -> Result<String, CliError> {
    match output {
        QueryOutput::Rows(rows) => rows
            .rows
            .first()
            .and_then(|row| row.first())
            .cloned()
            .ok_or_else(|| CliError::sql("No value returned.")),
        QueryOutput::Affected(_) => Err(CliError::sql("No value returned.")),
    }
}

fn output_rows(output: QueryOutput) -> Vec<Vec<String>> {
    match output {
        QueryOutput::Rows(rows) => rows.rows,
        QueryOutput::Affected(_) => Vec::new(),
    }
}

fn parse_u64_cell(value: Option<&String>) -> u64 {
    value
        .map(String::as_str)
        .unwrap_or("")
        .trim()
        .parse::<u64>()
        .unwrap_or(0)
}

async fn sqlite_constraints_output(
    connection: &ResolvedConnection,
    table: Option<&str>,
) -> Result<QueryOutput, CliError> {
    let tables = if let Some(table) = table {
        vec![table.to_string()]
    } else {
        output_rows(
            run_query_with_control(
                connection.pool.clone(),
                connection.database.clone(),
                String::from(
                    "SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%' ORDER BY name",
                ),
                None,
                None,
            )
            .await
            .map_err(CliError::sql)?,
        )
        .into_iter()
        .filter_map(|row| row.first().cloned())
        .collect::<Vec<_>>()
    };

    let mut rows = Vec::new();
    for table in tables {
        let query = constraints_query(
            DatabaseDriver::Sqlite,
            connection.database.as_deref(),
            Some(&table),
        );
        rows.extend(output_rows(
            run_query_with_control(
                connection.pool.clone(),
                connection.database.clone(),
                query,
                None,
                None,
            )
            .await
            .map_err(CliError::sql)?,
        ));
    }

    Ok(QueryOutput::Rows(simple_result_set(
        vec![
            "table_name",
            "constraint_kind",
            "constraint_name",
            "column_name",
            "referenced_table",
            "referenced_column",
            "details",
        ],
        rows,
    )))
}

async fn sqlite_size_output(connection: &ResolvedConnection) -> Result<QueryOutput, CliError> {
    let page_count = parse_u64_cell(
        output_rows(
            run_query_with_control(
                connection.pool.clone(),
                connection.database.clone(),
                String::from("PRAGMA page_count"),
                None,
                Some(1),
            )
            .await
            .map_err(CliError::sql)?,
        )
        .first()
        .and_then(|row| row.first()),
    );
    let page_size = parse_u64_cell(
        output_rows(
            run_query_with_control(
                connection.pool.clone(),
                connection.database.clone(),
                String::from("PRAGMA page_size"),
                None,
                Some(1),
            )
            .await
            .map_err(CliError::sql)?,
        )
        .first()
        .and_then(|row| row.first()),
    );
    let freelist_count = parse_u64_cell(
        output_rows(
            run_query_with_control(
                connection.pool.clone(),
                connection.database.clone(),
                String::from("PRAGMA freelist_count"),
                None,
                Some(1),
            )
            .await
            .map_err(CliError::sql)?,
        )
        .first()
        .and_then(|row| row.first()),
    );

    let total_size = page_count.saturating_mul(page_size);
    let data_size = page_count
        .saturating_sub(freelist_count)
        .saturating_mul(page_size);

    let tables = output_rows(
        run_query_with_control(
            connection.pool.clone(),
            connection.database.clone(),
            String::from(
                "SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%' ORDER BY name",
            ),
            None,
            None,
        )
        .await
        .map_err(CliError::sql)?,
    );
    let dbstat_sizes = run_query_with_control(
        connection.pool.clone(),
        connection.database.clone(),
        String::from("SELECT name, SUM(pgsize) AS bytes FROM dbstat GROUP BY name"),
        None,
        None,
    )
    .await
    .ok()
    .map(output_rows)
    .unwrap_or_default()
    .into_iter()
    .filter_map(|row| {
        let name = row.first()?.clone();
        let bytes = parse_u64_cell(row.get(1));
        Some((name, bytes))
    })
    .collect::<BTreeMap<_, _>>();

    let mut rows = vec![vec![
        String::from("database"),
        String::from("database"),
        connection
            .database
            .clone()
            .unwrap_or_else(|| String::from("main")),
        total_size.to_string(),
        data_size.to_string(),
        String::new(),
        String::new(),
        String::from("database_page_count"),
    ]];

    for row in tables {
        let Some(table) = row.first().cloned() else {
            continue;
        };
        let count_sql = format!(
            "SELECT COUNT(*) FROM {}",
            sql_quote_table_reference(DatabaseDriver::Sqlite, &table)
        );
        let count = parse_u64_cell(
            output_rows(
                run_query_with_control(
                    connection.pool.clone(),
                    connection.database.clone(),
                    count_sql,
                    None,
                    Some(1),
                )
                .await
                .map_err(CliError::sql)?,
            )
            .first()
            .and_then(|value| value.first()),
        );

        let table_size = dbstat_sizes.get(&table).copied().unwrap_or(0);
        rows.push(vec![
            String::from("table"),
            String::from("table"),
            table,
            if table_size == 0 {
                String::new()
            } else {
                table_size.to_string()
            },
            if table_size == 0 {
                String::new()
            } else {
                table_size.to_string()
            },
            String::new(),
            count.to_string(),
            String::from("exact_count"),
        ]);
    }

    Ok(QueryOutput::Rows(simple_result_set(
        vec![
            "size_scope",
            "object_type",
            "object_name",
            "total_size_bytes",
            "data_size_bytes",
            "index_size_bytes",
            "approx_rows",
            "row_count_kind",
        ],
        rows,
    )))
}

async fn sqlite_search_output(
    connection: &ResolvedConnection,
    needle: &str,
) -> Result<QueryOutput, CliError> {
    let needle = needle.trim();
    if needle.is_empty() {
        return Err(CliError::validation("Search text cannot be empty."));
    }

    let lowered = needle.to_ascii_lowercase();
    let pattern = format!("%{}%", needle);
    let mut rows = output_rows(
        run_query_with_control(
            connection.pool.clone(),
            connection.database.clone(),
            format!(
                "SELECT type AS object_type, name AS object_name, CASE WHEN type IN ('index', 'trigger') THEN COALESCE(tbl_name, '') ELSE '' END AS parent_object, COALESCE(sql, '') AS details FROM sqlite_master WHERE type IN ('table', 'view', 'index', 'trigger') AND name LIKE {} ORDER BY type, name",
                sql_quote_string_literal(DatabaseDriver::Sqlite, &pattern)
            ),
            None,
            None,
        )
        .await
        .map_err(CliError::sql)?,
    );

    let table_rows = output_rows(
        run_query_with_control(
            connection.pool.clone(),
            connection.database.clone(),
            String::from(
                "SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%' ORDER BY name",
            ),
            None,
            None,
        )
        .await
        .map_err(CliError::sql)?,
    );

    for row in table_rows {
        let Some(table) = row.first().cloned() else {
            continue;
        };

        let column_rows = output_rows(
            run_query_with_control(
                connection.pool.clone(),
                connection.database.clone(),
                format!(
                    "SELECT name, type FROM pragma_table_info({}) ORDER BY cid",
                    sql_quote_string_literal(DatabaseDriver::Sqlite, &table)
                ),
                None,
                None,
            )
            .await
            .map_err(CliError::sql)?,
        );
        for column in column_rows {
            let Some(column_name) = column.first().cloned() else {
                continue;
            };
            if column_name.to_ascii_lowercase().contains(&lowered) {
                rows.push(vec![
                    String::from("column"),
                    column_name,
                    table.clone(),
                    column.get(1).cloned().unwrap_or_default(),
                ]);
            }
        }

        let index_rows = output_rows(
            run_query_with_control(
                connection.pool.clone(),
                connection.database.clone(),
                format!(
                    "SELECT name, origin FROM pragma_index_list({}) ORDER BY name",
                    sql_quote_string_literal(DatabaseDriver::Sqlite, &table)
                ),
                None,
                None,
            )
            .await
            .map_err(CliError::sql)?,
        );
        for index in index_rows {
            let Some(index_name) = index.first().cloned() else {
                continue;
            };
            if index_name.to_ascii_lowercase().contains(&lowered) {
                rows.push(vec![
                    String::from("index"),
                    index_name,
                    table.clone(),
                    index.get(1).cloned().unwrap_or_default(),
                ]);
            }
        }
    }

    rows.sort_by_key(|left| left.join("\u{1f}"));
    rows.dedup();

    Ok(QueryOutput::Rows(simple_result_set(
        vec!["object_type", "object_name", "parent_object", "details"],
        rows,
    )))
}

fn dump_modes(schema_only: bool, data_only: bool) -> Result<(bool, bool), CliError> {
    if schema_only && data_only {
        return Err(CliError::validation(
            "Use either `--schema-only` or `--data-only`, not both.",
        ));
    }

    Ok((!data_only, !schema_only))
}

async fn dump_connection(
    connection: &ResolvedConnection,
    table: Option<&str>,
    include_schema: bool,
    include_data: bool,
) -> Result<Vec<u8>, CliError> {
    match connection.driver {
        DatabaseDriver::PostgreSql => {
            dump_postgres(connection, table, include_schema, include_data).await
        }
        DatabaseDriver::MySql | DatabaseDriver::MariaDb => {
            dump_mysql_like(connection, table, include_schema, include_data).await
        }
        DatabaseDriver::Sqlite => {
            dump_sqlite(connection, table, include_schema, include_data).await
        }
    }
}

async fn dump_postgres(
    connection: &ResolvedConnection,
    table: Option<&str>,
    include_schema: bool,
    include_data: bool,
) -> Result<Vec<u8>, CliError> {
    let temp_dir = tempfile::tempdir().map_err(|error| CliError::general(error.to_string()))?;
    let dump_path = temp_dir.path().join("dump.sql");
    let mut command = Command::new("pg_dump");
    let info = &connection.info;

    if !info.host.trim().is_empty() {
        command.arg("--host").arg(info.host.trim());
    }
    command
        .arg("--port")
        .arg(if info.port.trim().is_empty() {
            "5432"
        } else {
            info.port.trim()
        })
        .arg("--format=plain")
        .arg("--file")
        .arg(&dump_path)
        .arg("--no-owner")
        .arg("--no-privileges")
        .arg("--no-password");

    if !info.username.trim().is_empty() {
        command.arg("--username").arg(info.username.trim());
    }

    if include_schema && !include_data {
        command.arg("--schema-only");
    }
    if include_data && !include_schema {
        command.arg("--data-only");
    }
    if let Some(table) = table {
        command.arg("--table").arg(table);
    }
    command.arg("--dbname").arg(
        connection
            .database
            .as_deref()
            .or_else(|| {
                if info.database.trim().is_empty() {
                    None
                } else {
                    Some(info.database.trim())
                }
            })
            .ok_or_else(|| CliError::validation("PostgreSQL dump requires a database name."))?,
    );

    command.env("PGSSLMODE", postgres_dump_ssl_mode(info.tls_mode));
    if !info.tls_ca_cert_path.trim().is_empty() {
        command.env("PGSSLROOTCERT", info.tls_ca_cert_path.trim());
    }
    if !info.tls_client_cert_path.trim().is_empty() {
        command.env("PGSSLCERT", info.tls_client_cert_path.trim());
    }
    if !info.tls_client_key_path.trim().is_empty() {
        command.env("PGSSLKEY", info.tls_client_key_path.trim());
    }

    let pgpass = postgres_dump_pgpass(info).await?;
    if let Some(path) = pgpass.as_deref() {
        command.env("PGPASSFILE", path);
    }
    command.stdout(Stdio::null()).stderr(Stdio::piped());
    let output = command.output().await.map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            CliError::general(
                "`pg_dump` is not installed or not in PATH. Install PostgreSQL client tools and try again.",
            )
        } else {
            CliError::general(error.to_string())
        }
    })?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(CliError::general(if stderr.is_empty() {
            format!("`pg_dump` failed with status {}.", output.status)
        } else {
            format!("`pg_dump` failed: {stderr}")
        }));
    }

    tokio::fs::read(&dump_path)
        .await
        .map_err(|error| CliError::general(error.to_string()))
}

fn postgres_dump_ssl_mode(tls_mode: TlsMode) -> &'static str {
    match tls_mode {
        TlsMode::Disabled => "disable",
        TlsMode::Prefer => "prefer",
        TlsMode::Require => "require",
        TlsMode::VerifyCa => "verify-ca",
        TlsMode::VerifyFull => "verify-full",
    }
}

async fn postgres_dump_pgpass(info: &ConnectionInfo) -> Result<Option<PathBuf>, CliError> {
    let password = info.password.trim();
    if password.is_empty() || info.username.trim().is_empty() {
        return Ok(None);
    }

    let host = if info.host.trim().is_empty() {
        "localhost"
    } else {
        info.host.trim()
    };
    let port = if info.port.trim().is_empty() {
        "5432"
    } else {
        info.port.trim()
    };
    let escaped_password = password.replace('\\', r"\\").replace(':', r"\:");
    let entry = format!(
        "{host}:{port}:*:{}:{escaped_password}\n",
        info.username.trim()
    );

    let file =
        tempfile::NamedTempFile::new().map_err(|error| CliError::general(error.to_string()))?;
    let path = file.path().to_path_buf();
    tokio::fs::write(&path, entry)
        .await
        .map_err(|error| CliError::general(error.to_string()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let permissions = std::fs::Permissions::from_mode(0o600);
        std::fs::set_permissions(&path, permissions)
            .map_err(|error| CliError::general(error.to_string()))?;
    }
    let persisted = file
        .into_temp_path()
        .keep()
        .map_err(|error| CliError::general(error.error.to_string()))?;
    Ok(Some(persisted))
}

async fn dump_mysql_like(
    connection: &ResolvedConnection,
    table: Option<&str>,
    include_schema: bool,
    include_data: bool,
) -> Result<Vec<u8>, CliError> {
    if env_truthy("CRYODB_USE_MYSQLDUMP") || env_truthy("CRYODB_MYSQLDUMP") {
        return dump_mysql_with_mysqldump(connection, table, include_schema, include_data).await;
    }

    let mut output = String::new();
    let mut first_section = true;
    let tables = mysql_dump_tables(connection, table).await?;

    if include_schema {
        for table_name in &tables {
            let ddl_query = format!(
                "SHOW CREATE TABLE {}",
                sql_quote_table_reference(connection.driver, table_name)
            );
            let ddl = extract_last_column_text(
                run_query_with_control(
                    connection.pool.clone(),
                    connection.database.clone(),
                    ddl_query,
                    None,
                    Some(1),
                )
                .await
                .map_err(CliError::sql)?,
            )?;
            if !first_section {
                output.push('\n');
            }
            output.push_str(&ddl);
            if !ddl.trim_end().ends_with(';') {
                output.push(';');
            }
            output.push('\n');
            first_section = false;
        }
    }

    if include_data {
        for table_name in &tables {
            let insert_block = dump_table_data(connection, table_name).await?;
            if insert_block.trim().is_empty() {
                continue;
            }
            if !first_section {
                output.push('\n');
            }
            output.push_str(&insert_block);
            first_section = false;
        }
    }

    Ok(output.into_bytes())
}

async fn dump_mysql_with_mysqldump(
    connection: &ResolvedConnection,
    table: Option<&str>,
    include_schema: bool,
    include_data: bool,
) -> Result<Vec<u8>, CliError> {
    let info = &connection.info;
    let database = connection
        .database
        .as_deref()
        .or_else(|| {
            if info.database.trim().is_empty() {
                None
            } else {
                Some(info.database.trim())
            }
        })
        .ok_or_else(|| CliError::validation("mysqldump requires a database name."))?;
    let mut command = Command::new("mysqldump");
    if !info.host.trim().is_empty() {
        command.arg("--host").arg(info.host.trim());
    }
    command.arg("--port").arg(if info.port.trim().is_empty() {
        "3306"
    } else {
        info.port.trim()
    });
    if !info.username.trim().is_empty() {
        command.arg("--user").arg(info.username.trim());
    }
    if !include_schema {
        command.arg("--no-create-info");
    }
    if !include_data {
        command.arg("--no-data");
    }
    command
        .arg("--single-transaction")
        .arg("--skip-comments")
        .arg(database);
    if let Some(table) = table {
        command.arg(table);
    }
    if !info.password.trim().is_empty() {
        command.env("MYSQL_PWD", info.password.trim());
    }
    if !info.tls_ca_cert_path.trim().is_empty() {
        command.arg("--ssl-ca").arg(info.tls_ca_cert_path.trim());
    }
    if !info.tls_client_cert_path.trim().is_empty() {
        command
            .arg("--ssl-cert")
            .arg(info.tls_client_cert_path.trim());
    }
    if !info.tls_client_key_path.trim().is_empty() {
        command
            .arg("--ssl-key")
            .arg(info.tls_client_key_path.trim());
    }
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    let output = command.output().await.map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            CliError::general(
                "`mysqldump` is not installed or not in PATH. Unset CRYODB_USE_MYSQLDUMP to use the built-in dumper.",
            )
        } else {
            CliError::general(error.to_string())
        }
    })?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(CliError::general(if stderr.is_empty() {
            format!("`mysqldump` failed with status {}.", output.status)
        } else {
            format!("`mysqldump` failed: {stderr}")
        }));
    }
    Ok(output.stdout)
}

fn env_truthy(key: &str) -> bool {
    std::env::var(key)
        .ok()
        .map(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(false)
}

async fn dump_sqlite(
    connection: &ResolvedConnection,
    table: Option<&str>,
    include_schema: bool,
    include_data: bool,
) -> Result<Vec<u8>, CliError> {
    let mut output = String::new();
    let mut first_section = true;

    if include_schema {
        let ddl = sqlite_schema_dump_output(connection, table).await?;
        if !ddl.trim().is_empty() {
            output.push_str(&ddl);
            if !ddl.ends_with('\n') {
                output.push('\n');
            }
            first_section = false;
        }
    }

    if include_data {
        let tables = sqlite_dump_tables(connection, table).await?;
        for table_name in &tables {
            let insert_block = dump_table_data(connection, table_name).await?;
            if insert_block.trim().is_empty() {
                continue;
            }
            if !first_section {
                output.push('\n');
            }
            output.push_str(&insert_block);
            first_section = false;
        }
    }

    Ok(output.into_bytes())
}

async fn sqlite_schema_dump_output(
    connection: &ResolvedConnection,
    table: Option<&str>,
) -> Result<String, CliError> {
    let statement = if let Some(table) = table {
        format!(
            "SELECT sql FROM sqlite_master WHERE name = {} AND type IN ('table', 'view') AND sql IS NOT NULL ORDER BY type, name",
            sql_quote_string_literal(DatabaseDriver::Sqlite, table)
        )
    } else {
        String::from(
            "SELECT sql FROM sqlite_master WHERE type IN ('table', 'view') AND sql IS NOT NULL ORDER BY type, name",
        )
    };

    let output = run_query_with_control(
        connection.pool.clone(),
        connection.database.clone(),
        statement,
        None,
        None,
    )
    .await
    .map_err(CliError::sql)?;

    let mut sql = String::new();
    for value in output_rows(output)
        .into_iter()
        .filter_map(|row| row.first().cloned())
    {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            continue;
        }
        if !sql.is_empty() {
            sql.push('\n');
            sql.push('\n');
        }
        sql.push_str(trimmed);
        if !trimmed.ends_with(';') {
            sql.push(';');
        }
    }

    Ok(sql)
}

async fn mysql_dump_tables(
    connection: &ResolvedConnection,
    table: Option<&str>,
) -> Result<Vec<String>, CliError> {
    if let Some(table) = table {
        return Ok(vec![table.to_string()]);
    }

    let output = run_query_with_control(
        connection.pool.clone(),
        connection.database.clone(),
        tables_query(
            connection.driver,
            connection.database.as_deref(),
            None,
            None,
        ),
        None,
        None,
    )
    .await
    .map_err(CliError::sql)?;
    Ok(output_rows(output)
        .into_iter()
        .filter_map(|row| row.first().cloned())
        .collect())
}

async fn sqlite_dump_tables(
    connection: &ResolvedConnection,
    table: Option<&str>,
) -> Result<Vec<String>, CliError> {
    if let Some(table) = table {
        return Ok(vec![table.to_string()]);
    }

    let output = run_query_with_control(
        connection.pool.clone(),
        connection.database.clone(),
        String::from(
            "SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%' ORDER BY name",
        ),
        None,
        None,
    )
    .await
    .map_err(CliError::sql)?;
    Ok(output_rows(output)
        .into_iter()
        .filter_map(|row| row.first().cloned())
        .collect())
}

async fn dump_table_data(connection: &ResolvedConnection, table: &str) -> Result<String, CliError> {
    let columns_output = run_query_with_control(
        connection.pool.clone(),
        connection.database.clone(),
        describe_columns_query(connection.driver, connection.database.as_deref(), table),
        None,
        None,
    )
    .await
    .map_err(CliError::sql)?;
    let column_rows = output_rows(columns_output);
    let columns = column_rows
        .iter()
        .filter_map(|row| row.get(1).cloned())
        .collect::<Vec<_>>();
    let kinds = column_rows
        .iter()
        .map(|row| crate::column_kind_from_type_name(row.get(2).map(String::as_str).unwrap_or("")))
        .collect::<Vec<_>>();
    if columns.is_empty() {
        return Ok(String::new());
    }

    let select_sql = format!(
        "SELECT * FROM {}",
        sql_quote_table_reference(connection.driver, table)
    );
    let rows_output = run_query_with_control(
        connection.pool.clone(),
        connection.database.clone(),
        select_sql,
        None,
        None,
    )
    .await
    .map_err(CliError::sql)?;

    let QueryOutput::Rows(result_set) = rows_output else {
        return Ok(String::new());
    };

    if result_set.rows.is_empty() {
        return Ok(String::new());
    }

    let table_ref = match connection.driver {
        DatabaseDriver::PostgreSql => {
            let (schema, table_name) = postgres_schema_and_name(table);
            sql_quote_identifier_path(connection.driver, &[schema.as_str(), table_name.as_str()])
        }
        _ => sql_quote_table_reference(connection.driver, table),
    };
    let column_list = columns
        .iter()
        .map(|column| sql_quote_identifier(connection.driver, column))
        .collect::<Vec<_>>()
        .join(", ");
    let prefix = format!("INSERT INTO {} ({}) VALUES ", table_ref, column_list);

    let mut output = String::new();
    let mut batch = Vec::new();
    for row in &result_set.rows {
        let values = row
            .iter()
            .enumerate()
            .map(|(index, value)| {
                let kind = kinds.get(index).copied().unwrap_or(ColumnKind::Unknown);
                crate::db::codecs::sql_literal_from_value_for_driver(value, kind, connection.driver)
            })
            .collect::<Vec<_>>();
        batch.push(format!("({})", values.join(", ")));
        if batch.len() >= crate::EXPORT_INSERT_BATCH_SIZE {
            output.push_str(&prefix);
            output.push_str(&batch.join(",\n"));
            output.push_str(";\n");
            batch.clear();
        }
    }
    if !batch.is_empty() {
        output.push_str(&prefix);
        output.push_str(&batch.join(",\n"));
        output.push_str(";\n");
    }

    Ok(output)
}

fn schemas_query(driver: DatabaseDriver) -> String {
    match driver {
        DatabaseDriver::MySql | DatabaseDriver::MariaDb => String::from(
            "SELECT schema_name AS schema_name FROM information_schema.schemata ORDER BY schema_name",
        ),
        DatabaseDriver::Sqlite => String::from("PRAGMA database_list"),
        DatabaseDriver::PostgreSql => String::from(
            "SELECT schema_name FROM information_schema.schemata WHERE schema_name = 'public' OR (schema_name NOT LIKE 'pg_%' AND schema_name <> 'information_schema') ORDER BY CASE WHEN schema_name = 'public' THEN 0 ELSE 1 END, schema_name",
        ),
    }
}

fn tables_query(
    driver: DatabaseDriver,
    database: Option<&str>,
    schema: Option<&str>,
    pattern: Option<&str>,
) -> String {
    let pattern = pattern.filter(|value| !value.trim().is_empty());
    match driver {
        DatabaseDriver::MySql | DatabaseDriver::MariaDb => {
            let database = database.unwrap_or("");
            let mut sql = format!(
                "SELECT table_name FROM information_schema.tables WHERE table_schema = {} AND table_type = 'BASE TABLE'",
                sql_quote_string_literal(driver, database)
            );
            if let Some(pattern) = pattern {
                sql.push_str(&format!(
                    " AND table_name LIKE {}",
                    sql_quote_string_literal(driver, &format!("%{}%", pattern))
                ));
            }
            sql.push_str(" ORDER BY table_name");
            sql
        }
        DatabaseDriver::Sqlite => {
            let mut sql = String::from(
                "SELECT name AS table_name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%'",
            );
            if let Some(pattern) = pattern {
                sql.push_str(&format!(
                    " AND name LIKE {}",
                    sql_quote_string_literal(driver, &format!("%{}%", pattern))
                ));
            }
            sql.push_str(" ORDER BY name");
            sql
        }
        DatabaseDriver::PostgreSql => {
            let schema = schema.unwrap_or("public");
            let mut sql = format!(
                "SELECT n.nspname || '.' || c.relname AS table_name FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace WHERE c.relkind IN ('r', 'p', 'f') AND n.nspname = {}",
                sql_quote_string_literal(driver, schema)
            );
            if let Some(pattern) = pattern {
                sql.push_str(&format!(
                    " AND c.relname ILIKE {}",
                    sql_quote_string_literal(driver, &format!("%{}%", pattern))
                ));
            }
            sql.push_str(" ORDER BY n.nspname, c.relname");
            sql
        }
    }
}

fn views_query(
    driver: DatabaseDriver,
    database: Option<&str>,
    schema: Option<&str>,
    pattern: Option<&str>,
) -> String {
    let pattern = pattern.filter(|value| !value.trim().is_empty());
    match driver {
        DatabaseDriver::MySql | DatabaseDriver::MariaDb => {
            let database = database.unwrap_or("");
            let mut sql = format!(
                "SELECT table_name FROM information_schema.views WHERE table_schema = {}",
                sql_quote_string_literal(driver, database)
            );
            if let Some(pattern) = pattern {
                sql.push_str(&format!(
                    " AND table_name LIKE {}",
                    sql_quote_string_literal(driver, &format!("%{}%", pattern))
                ));
            }
            sql.push_str(" ORDER BY table_name");
            sql
        }
        DatabaseDriver::Sqlite => {
            let mut sql =
                String::from("SELECT name AS view_name FROM sqlite_master WHERE type = 'view'");
            if let Some(pattern) = pattern {
                sql.push_str(&format!(
                    " AND name LIKE {}",
                    sql_quote_string_literal(driver, &format!("%{}%", pattern))
                ));
            }
            sql.push_str(" ORDER BY name");
            sql
        }
        DatabaseDriver::PostgreSql => {
            let schema = schema.unwrap_or("public");
            let mut sql = format!(
                "SELECT n.nspname || '.' || c.relname AS view_name FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace WHERE c.relkind IN ('v', 'm') AND n.nspname = {}",
                sql_quote_string_literal(driver, schema)
            );
            if let Some(pattern) = pattern {
                sql.push_str(&format!(
                    " AND c.relname ILIKE {}",
                    sql_quote_string_literal(driver, &format!("%{}%", pattern))
                ));
            }
            sql.push_str(" ORDER BY n.nspname, c.relname");
            sql
        }
    }
}

fn describe_columns_query(driver: DatabaseDriver, database: Option<&str>, table: &str) -> String {
    match driver {
        DatabaseDriver::MySql | DatabaseDriver::MariaDb => {
            let database = database.unwrap_or("");
            format!(
                "SELECT ordinal_position, column_name, column_type, is_nullable, COALESCE(column_default, '') AS column_default, CASE WHEN column_key = 'PRI' THEN 'YES' ELSE '' END AS primary_key, COALESCE(extra, '') AS extra FROM information_schema.columns WHERE table_schema = {} AND table_name = {} ORDER BY ordinal_position",
                sql_quote_string_literal(driver, database),
                sql_quote_string_literal(driver, table)
            )
        }
        DatabaseDriver::Sqlite => format!(
            "SELECT cid + 1 AS ordinal_position, name AS column_name, type AS column_type, CASE WHEN \"notnull\" = 0 THEN 'YES' ELSE 'NO' END AS is_nullable, COALESCE(dflt_value, '') AS column_default, CASE WHEN pk > 0 THEN 'YES' ELSE '' END AS primary_key, '' AS extra FROM pragma_table_info({}) ORDER BY cid",
            sql_quote_string_literal(driver, table)
        ),
        DatabaseDriver::PostgreSql => {
            let (schema, table_name) = postgres_schema_and_name(table);
            format!(
                "SELECT c.ordinal_position, c.column_name, pg_catalog.format_type(a.atttypid, a.atttypmod) AS column_type, c.is_nullable, COALESCE(c.column_default, '') AS column_default, CASE WHEN pk.column_name IS NOT NULL THEN 'YES' ELSE '' END AS primary_key, '' AS extra FROM information_schema.columns c JOIN pg_class cls ON cls.relname = c.table_name JOIN pg_namespace ns ON ns.oid = cls.relnamespace AND ns.nspname = c.table_schema JOIN pg_attribute a ON a.attrelid = cls.oid AND a.attname = c.column_name LEFT JOIN (SELECT kcu.column_name FROM information_schema.table_constraints tc JOIN information_schema.key_column_usage kcu ON tc.constraint_name = kcu.constraint_name AND tc.table_schema = kcu.table_schema WHERE tc.constraint_type = 'PRIMARY KEY' AND tc.table_schema = {} AND tc.table_name = {}) pk ON pk.column_name = c.column_name WHERE c.table_schema = {} AND c.table_name = {} ORDER BY c.ordinal_position",
                sql_quote_string_literal(driver, &schema),
                sql_quote_string_literal(driver, &table_name),
                sql_quote_string_literal(driver, &schema),
                sql_quote_string_literal(driver, &table_name),
            )
        }
    }
}

fn indexes_query(driver: DatabaseDriver, database: Option<&str>, table: Option<&str>) -> String {
    match driver {
        DatabaseDriver::MySql | DatabaseDriver::MariaDb => {
            let database = database.unwrap_or("");
            let mut sql = format!(
                "SELECT table_name, index_name, CASE WHEN non_unique = 0 THEN 'YES' ELSE '' END AS is_unique, seq_in_index, column_name, index_type FROM information_schema.statistics WHERE table_schema = {}",
                sql_quote_string_literal(driver, database)
            );
            if let Some(table) = table {
                sql.push_str(&format!(
                    " AND table_name = {}",
                    sql_quote_string_literal(driver, table)
                ));
            }
            sql.push_str(" ORDER BY table_name, index_name, seq_in_index");
            sql
        }
        DatabaseDriver::Sqlite => match table {
            Some(table) => format!(
                "SELECT il.tbl_name AS table_name, il.name AS index_name, CASE WHEN il.\"unique\" = 1 THEN 'YES' ELSE '' END AS is_unique, ii.seqno + 1 AS seq_in_index, ii.name AS column_name, il.origin AS index_type FROM pragma_index_list({table}) il JOIN pragma_index_info(il.name) ii ORDER BY il.name, ii.seqno",
                table = sql_quote_string_literal(driver, table)
            ),
            None => String::from(
                "SELECT tbl_name AS table_name, name AS index_name, '' AS is_unique, '' AS seq_in_index, '' AS column_name, COALESCE(sql, '') AS index_type FROM sqlite_master WHERE type = 'index' AND name NOT LIKE 'sqlite_%' ORDER BY tbl_name, name",
            ),
        },
        DatabaseDriver::PostgreSql => {
            if let Some(table) = table {
                let (schema, table_name) = postgres_schema_and_name(table);
                format!(
                    "SELECT ns.nspname || '.' || tbl.relname AS table_name, idx.relname AS index_name, CASE WHEN ind.indisunique THEN 'YES' ELSE '' END AS is_unique, ord.ordinality AS seq_in_index, COALESCE(att.attname, '') AS column_name, pg_get_indexdef(ind.indexrelid) AS index_type FROM pg_class tbl JOIN pg_namespace ns ON ns.oid = tbl.relnamespace JOIN pg_index ind ON ind.indrelid = tbl.oid JOIN pg_class idx ON idx.oid = ind.indexrelid LEFT JOIN LATERAL unnest(ind.indkey) WITH ORDINALITY AS ord(attnum, ordinality) ON true LEFT JOIN pg_attribute att ON att.attrelid = tbl.oid AND att.attnum = ord.attnum WHERE ns.nspname = {} AND tbl.relname = {} ORDER BY idx.relname, ord.ordinality",
                    sql_quote_string_literal(driver, &schema),
                    sql_quote_string_literal(driver, &table_name),
                )
            } else {
                String::from(
                    "SELECT schemaname || '.' || tablename AS table_name, indexname AS index_name, '' AS is_unique, '' AS seq_in_index, '' AS column_name, indexdef AS index_type FROM pg_indexes WHERE schemaname = 'public' OR (schemaname NOT LIKE 'pg_%' AND schemaname <> 'information_schema') ORDER BY schemaname, tablename, indexname",
                )
            }
        }
    }
}

fn constraints_query(
    driver: DatabaseDriver,
    database: Option<&str>,
    table: Option<&str>,
) -> String {
    match driver {
        DatabaseDriver::MySql | DatabaseDriver::MariaDb => {
            let database = database.unwrap_or("");
            let table_filter = table
                .map(|table| {
                    format!(
                        " AND c.table_name = {}",
                        sql_quote_string_literal(driver, table)
                    )
                })
                .unwrap_or_default();
            let table_filter_constraints = table
                .map(|table| {
                    format!(
                        " AND tc.table_name = {}",
                        sql_quote_string_literal(driver, table)
                    )
                })
                .unwrap_or_default();
            format!(
                "SELECT c.table_name, 'NOT NULL' AS constraint_kind, CONCAT(c.table_name, '.', c.column_name, '.not_null') AS constraint_name, c.column_name, '' AS referenced_table, '' AS referenced_column, '' AS details FROM information_schema.columns c WHERE c.table_schema = {schema} AND c.is_nullable = 'NO'{table_filter} UNION ALL SELECT tc.table_name, tc.constraint_type AS constraint_kind, tc.constraint_name, COALESCE(kcu.column_name, '') AS column_name, COALESCE(kcu.referenced_table_name, '') AS referenced_table, COALESCE(kcu.referenced_column_name, '') AS referenced_column, CONCAT('update=', COALESCE(rc.update_rule, ''), '; delete=', COALESCE(rc.delete_rule, ''), CASE WHEN chk.check_clause IS NULL THEN '' ELSE CONCAT('; check=', chk.check_clause) END) AS details FROM information_schema.table_constraints tc LEFT JOIN information_schema.key_column_usage kcu ON kcu.constraint_schema = tc.constraint_schema AND kcu.constraint_name = tc.constraint_name AND kcu.table_name = tc.table_name LEFT JOIN information_schema.referential_constraints rc ON rc.constraint_schema = tc.constraint_schema AND rc.constraint_name = tc.constraint_name LEFT JOIN information_schema.check_constraints chk ON chk.constraint_schema = tc.constraint_schema AND chk.constraint_name = tc.constraint_name WHERE tc.table_schema = {schema}{table_filter_constraints} ORDER BY table_name, constraint_kind, constraint_name, column_name",
                schema = sql_quote_string_literal(driver, database),
                table_filter = table_filter,
                table_filter_constraints = table_filter_constraints,
            )
        }
        DatabaseDriver::Sqlite => {
            let table = table.unwrap_or("");
            format!(
                "SELECT {table_name} AS table_name, 'NOT NULL' AS constraint_kind, name || '.not_null' AS constraint_name, name AS column_name, '' AS referenced_table, '' AS referenced_column, '' AS details FROM pragma_table_info({table}) WHERE \"notnull\" = 1 AND pk = 0 UNION ALL SELECT {table_name} AS table_name, 'PRIMARY KEY' AS constraint_kind, name || '.pk' AS constraint_name, name AS column_name, '' AS referenced_table, '' AS referenced_column, '' AS details FROM pragma_table_info({table}) WHERE pk > 0 UNION ALL SELECT {table_name} AS table_name, 'FOREIGN KEY' AS constraint_kind, {table_name} || '.fk.' || id AS constraint_name, \"from\" AS column_name, \"table\" AS referenced_table, COALESCE(\"to\", '') AS referenced_column, 'update=' || COALESCE(on_update, '') || '; delete=' || COALESCE(on_delete, '') || '; match=' || COALESCE(\"match\", '') AS details FROM pragma_foreign_key_list({table}) UNION ALL SELECT {table_name} AS table_name, 'UNIQUE' AS constraint_kind, il.name AS constraint_name, COALESCE(ii.name, '') AS column_name, '' AS referenced_table, '' AS referenced_column, il.origin AS details FROM pragma_index_list({table}) il LEFT JOIN pragma_index_info(il.name) ii WHERE il.\"unique\" = 1 AND il.origin <> 'pk' UNION ALL SELECT {table_name} AS table_name, 'CHECK' AS constraint_kind, {table_name} || '.check' AS constraint_name, '' AS column_name, '' AS referenced_table, '' AS referenced_column, COALESCE(sql, '') AS details FROM sqlite_master WHERE type = 'table' AND name = {table_literal} AND sql LIKE '%CHECK%' ORDER BY table_name, constraint_kind, constraint_name, column_name",
                table = sql_quote_string_literal(driver, table),
                table_name = sql_quote_string_literal(driver, table),
                table_literal = sql_quote_string_literal(driver, table),
            )
        }
        DatabaseDriver::PostgreSql => {
            let (
                schema,
                table_name,
                schema_filter_columns,
                schema_filter_constraints,
                table_filter_columns,
                table_filter_constraints,
            ) = if let Some(table) = table {
                let (schema, table_name) = postgres_schema_and_name(table);
                (
                    schema.clone(),
                    table_name.clone(),
                    format!(
                        "c.table_schema = {}",
                        sql_quote_string_literal(driver, &schema)
                    ),
                    format!(
                        "tc.table_schema = {}",
                        sql_quote_string_literal(driver, &schema)
                    ),
                    format!(
                        " AND c.table_name = {}",
                        sql_quote_string_literal(driver, &table_name)
                    ),
                    format!(
                        " AND tc.table_name = {}",
                        sql_quote_string_literal(driver, &table_name)
                    ),
                )
            } else {
                (
                    String::new(),
                    String::new(),
                    String::from(
                        "(c.table_schema = 'public' OR (c.table_schema NOT LIKE 'pg_%' AND c.table_schema <> 'information_schema'))",
                    ),
                    String::from(
                        "(tc.table_schema = 'public' OR (tc.table_schema NOT LIKE 'pg_%' AND tc.table_schema <> 'information_schema'))",
                    ),
                    String::new(),
                    String::new(),
                )
            };
            let _ = (&schema, &table_name);
            format!(
                "SELECT c.table_schema || '.' || c.table_name AS table_name, 'NOT NULL' AS constraint_kind, c.table_name || '.' || c.column_name || '.not_null' AS constraint_name, c.column_name, '' AS referenced_table, '' AS referenced_column, '' AS details FROM information_schema.columns c WHERE {schema_filter_columns} AND c.is_nullable = 'NO'{table_filter_columns} UNION ALL SELECT tc.table_schema || '.' || tc.table_name AS table_name, tc.constraint_type AS constraint_kind, tc.constraint_name, COALESCE(kcu.column_name, '') AS column_name, CASE WHEN ccu.table_name IS NULL THEN '' ELSE ccu.table_schema || '.' || ccu.table_name END AS referenced_table, COALESCE(ccu.column_name, '') AS referenced_column, COALESCE(pg_get_constraintdef(pgcon.oid), chk.check_clause, '') || CASE WHEN pgcon.condeferrable THEN '; deferrable' ELSE '' END || CASE WHEN pgcon.condeferred THEN '; initially_deferred' ELSE '' END AS details FROM information_schema.table_constraints tc LEFT JOIN information_schema.key_column_usage kcu ON kcu.constraint_schema = tc.constraint_schema AND kcu.constraint_name = tc.constraint_name AND kcu.table_name = tc.table_name LEFT JOIN information_schema.constraint_column_usage ccu ON ccu.constraint_schema = tc.constraint_schema AND ccu.constraint_name = tc.constraint_name LEFT JOIN information_schema.check_constraints chk ON chk.constraint_schema = tc.constraint_schema AND chk.constraint_name = tc.constraint_name LEFT JOIN pg_namespace pgns ON pgns.nspname = tc.table_schema LEFT JOIN pg_class pgcls ON pgcls.relname = tc.table_name AND pgcls.relnamespace = pgns.oid LEFT JOIN pg_constraint pgcon ON pgcon.conname = tc.constraint_name AND pgcon.conrelid = pgcls.oid WHERE {schema_filter_constraints}{table_filter_constraints} ORDER BY table_name, constraint_kind, constraint_name, column_name",
                schema_filter_columns = schema_filter_columns,
                schema_filter_constraints = schema_filter_constraints,
                table_filter_columns = table_filter_columns,
                table_filter_constraints = table_filter_constraints,
            )
        }
    }
}

fn size_query(driver: DatabaseDriver, database: Option<&str>) -> String {
    match driver {
        DatabaseDriver::MySql | DatabaseDriver::MariaDb => {
            let database = database.unwrap_or("");
            format!(
                "SELECT 'database' AS size_scope, 'database' AS object_type, {schema} AS object_name, SUM(COALESCE(data_length, 0) + COALESCE(index_length, 0)) AS total_size_bytes, SUM(COALESCE(data_length, 0)) AS data_size_bytes, SUM(COALESCE(index_length, 0)) AS index_size_bytes, SUM(COALESCE(table_rows, 0)) AS approx_rows FROM information_schema.tables WHERE table_schema = {schema} UNION ALL SELECT 'table' AS size_scope, LOWER(table_type) AS object_type, table_name AS object_name, COALESCE(data_length, 0) + COALESCE(index_length, 0) AS total_size_bytes, COALESCE(data_length, 0) AS data_size_bytes, COALESCE(index_length, 0) AS index_size_bytes, COALESCE(table_rows, 0) AS approx_rows FROM information_schema.tables WHERE table_schema = {schema} ORDER BY CASE WHEN size_scope = 'database' THEN 0 ELSE 1 END, object_name",
                schema = sql_quote_string_literal(driver, database),
            )
        }
        DatabaseDriver::Sqlite => String::new(),
        DatabaseDriver::PostgreSql => String::from(
            "SELECT 'database' AS size_scope, 'database' AS object_type, current_database() AS object_name, pg_database_size(current_database())::bigint AS total_size_bytes, NULL::bigint AS data_size_bytes, NULL::bigint AS index_size_bytes, NULL::bigint AS approx_rows UNION ALL SELECT 'table' AS size_scope, CASE c.relkind WHEN 'r' THEN 'table' WHEN 'p' THEN 'partitioned_table' WHEN 'v' THEN 'view' WHEN 'm' THEN 'materialized_view' WHEN 'f' THEN 'foreign_table' ELSE 'relation' END AS object_type, n.nspname || '.' || c.relname AS object_name, pg_total_relation_size(c.oid)::bigint AS total_size_bytes, pg_relation_size(c.oid)::bigint AS data_size_bytes, pg_indexes_size(c.oid)::bigint AS index_size_bytes, GREATEST(c.reltuples, 0)::bigint AS approx_rows FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace WHERE (n.nspname = 'public' OR (n.nspname NOT LIKE 'pg_%' AND n.nspname <> 'information_schema')) AND c.relkind IN ('r', 'p', 'v', 'm', 'f') ORDER BY CASE WHEN size_scope = 'database' THEN 0 ELSE 1 END, object_name",
        ),
    }
}

fn search_query(driver: DatabaseDriver, database: Option<&str>, needle: &str) -> String {
    let pattern = format!("%{}%", needle.trim());
    match driver {
        DatabaseDriver::MySql | DatabaseDriver::MariaDb => {
            let database = database.unwrap_or("");
            format!(
                "SELECT 'schema' AS object_type, schema_name AS object_name, '' AS parent_object, '' AS details FROM information_schema.schemata WHERE schema_name LIKE {pattern} UNION ALL SELECT 'table' AS object_type, table_name AS object_name, table_schema AS parent_object, table_type AS details FROM information_schema.tables WHERE table_schema = {schema} AND table_type = 'BASE TABLE' AND table_name LIKE {pattern} UNION ALL SELECT 'view' AS object_type, table_name AS object_name, table_schema AS parent_object, table_type AS details FROM information_schema.tables WHERE table_schema = {schema} AND table_type = 'VIEW' AND table_name LIKE {pattern} UNION ALL SELECT 'column' AS object_type, column_name AS object_name, table_name AS parent_object, column_type AS details FROM information_schema.columns WHERE table_schema = {schema} AND column_name LIKE {pattern} UNION ALL SELECT 'index' AS object_type, index_name AS object_name, table_name AS parent_object, index_type AS details FROM information_schema.statistics WHERE table_schema = {schema} AND index_name LIKE {pattern} UNION ALL SELECT LOWER(routine_type) AS object_type, routine_name AS object_name, routine_schema AS parent_object, COALESCE(dtd_identifier, '') AS details FROM information_schema.routines WHERE routine_schema = {schema} AND routine_name LIKE {pattern} UNION ALL SELECT 'trigger' AS object_type, trigger_name AS object_name, event_object_table AS parent_object, CONCAT(action_timing, ' ', event_manipulation) AS details FROM information_schema.triggers WHERE trigger_schema = {schema} AND trigger_name LIKE {pattern} UNION ALL SELECT LOWER(constraint_type) AS object_type, constraint_name AS object_name, table_name AS parent_object, '' AS details FROM information_schema.table_constraints WHERE table_schema = {schema} AND constraint_name LIKE {pattern} ORDER BY object_type, parent_object, object_name",
                schema = sql_quote_string_literal(driver, database),
                pattern = sql_quote_string_literal(driver, &pattern),
            )
        }
        DatabaseDriver::Sqlite => String::new(),
        DatabaseDriver::PostgreSql => format!(
            "SELECT 'schema' AS object_type, nspname AS object_name, '' AS parent_object, '' AS details FROM pg_namespace WHERE (nspname = 'public' OR (nspname NOT LIKE 'pg_%' AND nspname <> 'information_schema')) AND nspname ILIKE {pattern} UNION ALL SELECT CASE c.relkind WHEN 'r' THEN 'table' WHEN 'p' THEN 'table' WHEN 'v' THEN 'view' WHEN 'm' THEN 'materialized_view' WHEN 'f' THEN 'foreign_table' ELSE 'relation' END AS object_type, c.relname AS object_name, n.nspname AS parent_object, '' AS details FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace WHERE (n.nspname = 'public' OR (n.nspname NOT LIKE 'pg_%' AND n.nspname <> 'information_schema')) AND c.relkind IN ('r', 'p', 'v', 'm', 'f') AND c.relname ILIKE {pattern} UNION ALL SELECT 'column' AS object_type, c.column_name AS object_name, c.table_schema || '.' || c.table_name AS parent_object, c.data_type AS details FROM information_schema.columns c WHERE (c.table_schema = 'public' OR (c.table_schema NOT LIKE 'pg_%' AND c.table_schema <> 'information_schema')) AND c.column_name ILIKE {pattern} UNION ALL SELECT 'index' AS object_type, i.indexname AS object_name, i.schemaname || '.' || i.tablename AS parent_object, i.indexdef AS details FROM pg_indexes i WHERE (i.schemaname = 'public' OR (i.schemaname NOT LIKE 'pg_%' AND i.schemaname <> 'information_schema')) AND i.indexname ILIKE {pattern} UNION ALL SELECT CASE p.prokind WHEN 'p' THEN 'procedure' ELSE 'function' END AS object_type, p.proname AS object_name, n.nspname AS parent_object, pg_get_function_identity_arguments(p.oid) AS details FROM pg_proc p JOIN pg_namespace n ON n.oid = p.pronamespace WHERE (n.nspname = 'public' OR (n.nspname NOT LIKE 'pg_%' AND n.nspname <> 'information_schema')) AND p.proname ILIKE {pattern} UNION ALL SELECT 'trigger' AS object_type, tg.tgname AS object_name, ns.nspname || '.' || cls.relname AS parent_object, pg_get_triggerdef(tg.oid) AS details FROM pg_trigger tg JOIN pg_class cls ON cls.oid = tg.tgrelid JOIN pg_namespace ns ON ns.oid = cls.relnamespace WHERE NOT tg.tgisinternal AND tg.tgname ILIKE {pattern} UNION ALL SELECT LOWER(contype::text) AS object_type, conname AS object_name, ns.nspname || '.' || cls.relname AS parent_object, pg_get_constraintdef(con.oid) AS details FROM pg_constraint con JOIN pg_class cls ON cls.oid = con.conrelid JOIN pg_namespace ns ON ns.oid = cls.relnamespace WHERE con.conname ILIKE {pattern} ORDER BY object_type, parent_object, object_name",
            pattern = sql_quote_string_literal(driver, &pattern),
        ),
    }
}

fn server_info_query(driver: DatabaseDriver) -> String {
    match driver {
        DatabaseDriver::MySql | DatabaseDriver::MariaDb => String::from(
            "SELECT VERSION() AS server_version, CURRENT_USER() AS current_user_name, DATABASE() AS current_database_name, @@character_set_database AS database_encoding, @@collation_database AS database_collation, @@session.time_zone AS session_timezone",
        ),
        DatabaseDriver::Sqlite => String::from(
            "SELECT sqlite_version() AS server_version, '' AS current_user_name, 'main' AS current_database_name, 'UTF-8' AS database_encoding, '' AS database_collation, '' AS session_timezone",
        ),
        DatabaseDriver::PostgreSql => String::from(
            "SELECT version() AS server_version, current_user AS current_user_name, current_database() AS current_database_name, pg_encoding_to_char(encoding) AS database_encoding, datcollate AS database_collation, current_setting('TIMEZONE') AS session_timezone FROM pg_database WHERE datname = current_database()",
        ),
    }
}

fn permissions_probe_query(driver: DatabaseDriver) -> String {
    match driver {
        DatabaseDriver::MySql | DatabaseDriver::MariaDb => String::from(
            "SELECT CURRENT_USER() AS current_user_name, @@read_only AS server_read_only",
        ),
        DatabaseDriver::Sqlite => String::from(
            "SELECT 'main' AS database_name, (SELECT COUNT(*) FROM sqlite_master) AS visible_objects, (SELECT CASE WHEN EXISTS (SELECT 1 FROM pragma_database_list WHERE name = 'main') THEN 'yes' ELSE 'no' END) AS readable",
        ),
        DatabaseDriver::PostgreSql => String::from(
            "SELECT current_user AS current_user_name, has_database_privilege(current_database(), 'CONNECT')::text AS can_connect, has_schema_privilege('public', 'USAGE')::text AS can_use_public_schema",
        ),
    }
}

fn explain_query(driver: DatabaseDriver, sql: &str, analyze: bool, format: OutputFormat) -> String {
    match driver {
        DatabaseDriver::PostgreSql => {
            if matches!(format, OutputFormat::Json) {
                if analyze {
                    format!("EXPLAIN (ANALYZE TRUE, FORMAT JSON) {}", sql.trim())
                } else {
                    format!("EXPLAIN (FORMAT JSON) {}", sql.trim())
                }
            } else if analyze {
                format!("EXPLAIN ANALYZE {}", sql.trim())
            } else {
                format!("EXPLAIN {}", sql.trim())
            }
        }
        DatabaseDriver::MySql | DatabaseDriver::MariaDb => {
            if matches!(format, OutputFormat::Json) {
                format!("EXPLAIN FORMAT=JSON {}", sql.trim())
            } else if analyze {
                format!("EXPLAIN ANALYZE {}", sql.trim())
            } else {
                format!("EXPLAIN {}", sql.trim())
            }
        }
        DatabaseDriver::Sqlite => format!("EXPLAIN QUERY PLAN {}", sql.trim()),
    }
}

fn normalize_explain_output(
    driver: DatabaseDriver,
    analyze: bool,
    output: QueryOutput,
) -> QueryOutput {
    match output {
        QueryOutput::Affected(count) => QueryOutput::Rows(simple_result_set(
            vec!["driver", "analyze", "step", "plan"],
            vec![vec![
                driver.storage_key().to_string(),
                analyze.to_string(),
                String::from("0"),
                format!("rows_affected={count}"),
            ]],
        )),
        QueryOutput::Rows(rows) => {
            let normalized_rows = rows
                .rows
                .iter()
                .enumerate()
                .map(|(index, row)| {
                    let plan = if row.len() == 1 {
                        row.first().cloned().unwrap_or_default()
                    } else {
                        rows.columns
                            .iter()
                            .enumerate()
                            .map(|(column_index, column)| {
                                format!(
                                    "{}={}",
                                    column,
                                    row.get(column_index).cloned().unwrap_or_default()
                                )
                            })
                            .collect::<Vec<_>>()
                            .join(" | ")
                    };
                    vec![
                        driver.storage_key().to_string(),
                        analyze.to_string(),
                        (index + 1).to_string(),
                        plan,
                    ]
                })
                .collect::<Vec<_>>();
            QueryOutput::Rows(simple_result_set(
                vec!["driver", "analyze", "step", "plan"],
                normalized_rows,
            ))
        }
    }
}

fn postgres_schema_and_name(table: &str) -> (String, String) {
    if let Some((schema, name)) = table.split_once('.') {
        let schema = schema.trim().trim_matches('"');
        let name = name.trim().trim_matches('"');
        if !schema.is_empty() && !name.is_empty() {
            return (schema.to_string(), name.to_string());
        }
    }
    (
        String::from("public"),
        table.trim().trim_matches('"').to_string(),
    )
}

async fn shell_objects(connection: &ResolvedConnection) -> Result<Vec<String>, CliError> {
    let tables = run_query_with_control(
        connection.pool.clone(),
        connection.database.clone(),
        tables_query(
            connection.driver,
            connection.database.as_deref(),
            None,
            None,
        ),
        None,
        None,
    )
    .await
    .map_err(CliError::sql)?;
    let views = run_query_with_control(
        connection.pool.clone(),
        connection.database.clone(),
        views_query(
            connection.driver,
            connection.database.as_deref(),
            None,
            None,
        ),
        None,
        None,
    )
    .await
    .map_err(CliError::sql)?;
    let mut objects = Vec::new();
    if let QueryOutput::Rows(rows) = tables {
        objects.extend(rows.rows.into_iter().filter_map(|row| row.first().cloned()));
    }
    if let QueryOutput::Rows(rows) = views {
        objects.extend(rows.rows.into_iter().filter_map(|row| row.first().cloned()));
    }
    objects.sort();
    objects.dedup();
    Ok(objects)
}

fn shell_commands() -> Vec<String> {
    vec![
        String::from("\\connect"),
        String::from("\\tables"),
        String::from("\\views"),
        String::from("\\schemas"),
        String::from("\\describe"),
        String::from("\\schema"),
        String::from("\\indexes"),
        String::from("\\relations"),
        String::from("\\constraints"),
        String::from("\\size"),
        String::from("\\search"),
        String::from("\\format"),
        String::from("\\timing"),
        String::from("\\history"),
        String::from("\\help"),
        String::from("\\?"),
        String::from("\\clear"),
        String::from("\\exit"),
    ]
}

fn append_history_batch(
    connection: &ResolvedConnection,
    source: &str,
    statements: &[String],
    status: &str,
) -> Result<(), CliError> {
    let path = config_artifact_path(CLI_HISTORY_FILE);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| CliError::general(error.to_string()))?;
    }
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|error| CliError::general(error.to_string()))?;
    let timestamp = sqlx::types::chrono::Utc::now().to_rfc3339();
    for statement in statements {
        let record = HistoryRecord {
            timestamp: timestamp.clone(),
            connection: connection
                .profile_name
                .clone()
                .unwrap_or_else(|| connection.label.clone()),
            driver: connection.driver.storage_key().to_string(),
            source: source.to_string(),
            status: status.to_string(),
            sql: statement.clone(),
        };
        serde_json::to_writer(&mut file, &record)
            .map_err(|error| CliError::general(error.to_string()))?;
        writeln!(file).map_err(|error| CliError::general(error.to_string()))?;
    }
    Ok(())
}

fn load_history_records() -> Result<Vec<HistoryRecord>, CliError> {
    let path = config_artifact_path(CLI_HISTORY_FILE);
    let contents = match std::fs::read_to_string(path) {
        Ok(contents) => contents,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(CliError::general(error.to_string())),
    };
    let mut records = Vec::new();
    for line in contents.lines() {
        if line.trim().is_empty() {
            continue;
        }
        if let Ok(record) = serde_json::from_str::<HistoryRecord>(line) {
            records.push(record);
        }
    }
    Ok(records)
}
