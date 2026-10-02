use crate::{
    ConnectionInfo, DEFAULT_POOL_IDLE_TIMEOUT_SECS, DEFAULT_POOL_MAX_CONNECTIONS,
    DEFAULT_POOL_MAX_LIFETIME_SECS, DEFAULT_POOL_MIN_CONNECTIONS, DatabaseDriver, TlsMode,
};
use sqlx::mysql::{MySqlConnectOptions, MySqlPoolOptions, MySqlSslMode};
use sqlx::postgres::{PgConnectOptions, PgPoolOptions, PgSslMode};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{MySqlPool, PgPool, SqlitePool};
use std::net::IpAddr;
use std::path::PathBuf;
use std::time::Duration;

const MYSQL_RSA_AUTH_DISABLED_MESSAGE: &str = "This MySQL/MariaDB server requires RSA password authentication over a non-TLS connection. CryoDB official builds do not support insecure RSA authentication. Enable TLS for this connection and try again.";

fn mysql_ssl_mode(tls_mode: TlsMode) -> MySqlSslMode {
    match tls_mode {
        TlsMode::Disabled => MySqlSslMode::Disabled,
        TlsMode::Prefer => MySqlSslMode::Preferred,
        TlsMode::Require => MySqlSslMode::Required,
        TlsMode::VerifyCa => MySqlSslMode::VerifyCa,
        TlsMode::VerifyFull => MySqlSslMode::VerifyIdentity,
    }
}

fn mysql_effective_tls_mode(info: &ConnectionInfo) -> TlsMode {
    if matches!(info.tls_mode, TlsMode::Prefer) && !is_local_mysql_host(&info.host) {
        TlsMode::Require
    } else {
        info.tls_mode
    }
}

fn is_local_mysql_host(host: &str) -> bool {
    let host = host.trim().trim_matches(['[', ']']);
    host.eq_ignore_ascii_case("localhost")
        || host
            .parse::<IpAddr>()
            .is_ok_and(|address| address.is_loopback())
}

fn driver_label(driver: DatabaseDriver) -> &'static str {
    match driver {
        DatabaseDriver::MySql => "MySQL",
        DatabaseDriver::MariaDb => "MariaDB",
        DatabaseDriver::Sqlite => "SQLite",
        DatabaseDriver::PostgreSql => "PostgreSQL",
    }
}

fn contains_any(haystack: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| haystack.contains(needle))
}

fn gui_connection_error_message(driver: DatabaseDriver, error: String) -> String {
    let raw = error.trim();
    let lower = raw.to_ascii_lowercase();
    let label = driver_label(driver);

    if matches!(driver, DatabaseDriver::MySql | DatabaseDriver::MariaDb)
        && (raw.contains("RSA auth backend disabled") || lower.contains("mysql-rsa"))
    {
        MYSQL_RSA_AUTH_DISABLED_MESSAGE.to_string()
    } else if contains_any(
        &lower,
        &[
            "access denied for user",
            "password authentication failed",
            "authentication failed",
            "invalid password",
        ],
    ) {
        format!("{label} authentication failed. Check the username and password.")
    } else if contains_any(
        &lower,
        &[
            "failed to lookup address information",
            "name or service not known",
            "temporary failure in name resolution",
            "nodename nor servname provided",
            "could not translate host name",
        ],
    ) {
        format!("Could not resolve the {label} host. Check the host name and network/DNS settings.")
    } else if contains_any(&lower, &["connection refused", "os error 111"]) {
        format!(
            "{label} refused the connection. Check that the server is running and accepting this host/port."
        )
    } else if contains_any(&lower, &["timed out", "timeout", "deadline has elapsed"]) {
        format!(
            "{label} connection timed out. Check the host, port, firewall, and VPN/network path."
        )
    } else if matches!(driver, DatabaseDriver::Sqlite)
        && contains_any(&lower, &["database is locked", "database table is locked"])
    {
        String::from(
            "SQLite database is locked. Close other writers or retry after the active transaction finishes.",
        )
    } else if contains_any(&lower, &["permission denied", "access is denied"]) {
        format!(
            "{label} connection failed because the target is not accessible with current permissions."
        )
    } else if contains_any(&lower, &["certificate", "x509", "cert"])
        && contains_any(
            &lower,
            &["tls", "ssl", "verify", "invalid", "unknown issuer"],
        )
    {
        format!("{label} TLS certificate validation failed. Check TLS mode and certificate paths.")
    } else if contains_any(
        &lower,
        &[
            "bytes at eof",
            "unexpected end of file",
            "connection reset by peer",
            "os error 104",
            "broken pipe",
        ],
    ) {
        format!(
            "{label} closed the connection before the handshake. Check that the port reaches the real server (Docker port forwarding, SSH tunnel, proxy) and that the server is not rejecting this host."
        )
    } else if contains_any(
        &lower,
        &["no route to host", "os error 113", "network is unreachable"],
    ) {
        format!(
            "{label} host is unreachable at the network level. Check routes, Docker bridge networks, VPN, and firewall."
        )
    } else {
        raw.to_string()
    }
}

fn postgres_ssl_mode(tls_mode: TlsMode) -> PgSslMode {
    match tls_mode {
        TlsMode::Disabled => PgSslMode::Disable,
        TlsMode::Prefer => PgSslMode::Prefer,
        TlsMode::Require => PgSslMode::Require,
        TlsMode::VerifyCa => PgSslMode::VerifyCa,
        TlsMode::VerifyFull => PgSslMode::VerifyFull,
    }
}

async fn validated_tls_ca_path(
    info: &ConnectionInfo,
    driver_label: &str,
) -> Result<Option<String>, String> {
    if !matches!(info.tls_mode, TlsMode::VerifyCa | TlsMode::VerifyFull) {
        return Ok(None);
    }

    let ca_path = info.tls_ca_cert_path.trim();
    if ca_path.is_empty() {
        return Err(format!(
            "{driver_label} CA certificate path is required for the selected TLS mode."
        ));
    }

    validate_tls_file_path(ca_path, &format!("{driver_label} CA certificate")).await
}

async fn validate_tls_file_path(raw_path: &str, label: &str) -> Result<Option<String>, String> {
    let path = raw_path.trim();
    if path.is_empty() {
        return Ok(None);
    }

    let metadata = tokio::fs::metadata(path).await.map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            format!("{label} file could not be found: {path}")
        } else {
            format!("Could not access {label} file `{path}`: {error}")
        }
    })?;

    if !metadata.is_file() {
        return Err(format!("{label} path must point to a file: {path}"));
    }

    Ok(Some(path.to_string()))
}

async fn validated_tls_client_paths(
    info: &ConnectionInfo,
    driver_label: &str,
) -> Result<(Option<String>, Option<String>), String> {
    let cert_path = validate_tls_file_path(
        &info.tls_client_cert_path,
        &format!("{driver_label} client certificate"),
    )
    .await?;
    let key_path = validate_tls_file_path(
        &info.tls_client_key_path,
        &format!("{driver_label} client key"),
    )
    .await?;

    if cert_path.is_some() != key_path.is_some() {
        return Err(format!(
            "{driver_label} client certificate and client key paths must be provided together."
        ));
    }

    Ok((cert_path, key_path))
}

pub(crate) async fn create_sqlite_database_file(path: PathBuf) -> Result<String, String> {
    if path.as_os_str().is_empty() {
        return Err(String::from("SQLite file path is required."));
    }

    match tokio::fs::metadata(&path).await {
        Ok(_) => return Err(String::from("SQLite file already exists.")),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.to_string()),
    }

    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|error| error.to_string())?;
    }

    tokio::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&path)
        .await
        .map_err(|error| error.to_string())?;

    Ok(path.display().to_string())
}

pub(crate) async fn connect_sqlite_profile(path: String) -> Result<(SqlitePool, String), String> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Err(String::from("SQLite file is required."));
    }

    let path = PathBuf::from(trimmed);
    let metadata = tokio::fs::metadata(&path)
        .await
        .map_err(|error| gui_connection_error_message(DatabaseDriver::Sqlite, error.to_string()))?;

    if !metadata.is_file() {
        return Err(String::from("SQLite path must point to a file."));
    }

    let normalized = tokio::fs::canonicalize(&path).await.unwrap_or(path);
    let normalized_text = normalized.display().to_string();
    let options = SqliteConnectOptions::new()
        .filename(&normalized)
        .create_if_missing(false);

    let connect_future = SqlitePoolOptions::new()
        .max_connections(DEFAULT_POOL_MAX_CONNECTIONS)
        .min_connections(DEFAULT_POOL_MIN_CONNECTIONS)
        .idle_timeout(Duration::from_secs(DEFAULT_POOL_IDLE_TIMEOUT_SECS))
        .max_lifetime(Duration::from_secs(DEFAULT_POOL_MAX_LIFETIME_SECS))
        .connect_with(options);
    let timeout =
        crate::constants::ACTIVE_CONNECTION_TIMEOUT_SECS.load(std::sync::atomic::Ordering::Relaxed);
    let pool = match tokio::time::timeout(Duration::from_secs(timeout), connect_future).await {
        Ok(result) => result.map_err(|error| {
            gui_connection_error_message(DatabaseDriver::Sqlite, error.to_string())
        })?,
        Err(_) => {
            return Err(format!("Connection timed out after {} seconds.", timeout));
        }
    };

    sqlx::query("PRAGMA foreign_keys = ON")
        .execute(&pool)
        .await
        .map_err(|error| gui_connection_error_message(DatabaseDriver::Sqlite, error.to_string()))?;

    Ok((pool, normalized_text))
}

pub(crate) async fn connect_mysql(info: ConnectionInfo) -> Result<MySqlPool, String> {
    let port = if info.port.trim().is_empty() {
        3306
    } else {
        info.port
            .trim()
            .parse::<u16>()
            .map_err(|_| String::from("Port must be a number between 1 and 65535."))?
    };

    let mut options = MySqlConnectOptions::new()
        .host(&info.host)
        .port(port)
        .username(&info.username)
        .ssl_mode(mysql_ssl_mode(mysql_effective_tls_mode(&info)));

    if !info.password.trim().is_empty() {
        options = options.password(&info.password);
    }

    if !info.database.trim().is_empty() {
        options = options.database(&info.database);
    }

    if let Some(ca_path) = validated_tls_ca_path(&info, "MySQL").await? {
        options = options.ssl_ca(&ca_path);
    }
    let (client_cert_path, client_key_path) = validated_tls_client_paths(&info, "MySQL").await?;
    if let Some(client_cert_path) = client_cert_path {
        options = options.ssl_client_cert(&client_cert_path);
    }
    if let Some(client_key_path) = client_key_path {
        options = options.ssl_client_key(&client_key_path);
    }

    let connect_future = MySqlPoolOptions::new()
        .max_connections(DEFAULT_POOL_MAX_CONNECTIONS)
        .min_connections(DEFAULT_POOL_MIN_CONNECTIONS)
        .idle_timeout(Duration::from_secs(DEFAULT_POOL_IDLE_TIMEOUT_SECS))
        .max_lifetime(Duration::from_secs(DEFAULT_POOL_MAX_LIFETIME_SECS))
        .connect_with(options);

    let timeout =
        crate::constants::ACTIVE_CONNECTION_TIMEOUT_SECS.load(std::sync::atomic::Ordering::Relaxed);

    match tokio::time::timeout(Duration::from_secs(timeout), connect_future).await {
        Ok(result) => {
            result.map_err(|error| gui_connection_error_message(info.driver, error.to_string()))
        }
        Err(_) => Err(format!("Connection timed out after {} seconds.", timeout)),
    }
}

pub(crate) async fn connect_postgres(info: ConnectionInfo) -> Result<PgPool, String> {
    let port = if info.port.trim().is_empty() {
        5432
    } else {
        info.port
            .trim()
            .parse::<u16>()
            .map_err(|_| String::from("Port must be a number between 1 and 65535."))?
    };

    let mut options = PgConnectOptions::new()
        .host(&info.host)
        .port(port)
        .username(&info.username)
        .ssl_mode(postgres_ssl_mode(info.tls_mode));

    if !info.password.trim().is_empty() {
        options = options.password(&info.password);
    }

    if !info.database.trim().is_empty() {
        options = options.database(&info.database);
    }

    if let Some(ca_path) = validated_tls_ca_path(&info, "PostgreSQL").await? {
        options = options.ssl_root_cert(&ca_path);
    }
    let (client_cert_path, client_key_path) =
        validated_tls_client_paths(&info, "PostgreSQL").await?;
    if let Some(client_cert_path) = client_cert_path {
        options = options.ssl_client_cert(&client_cert_path);
    }
    if let Some(client_key_path) = client_key_path {
        options = options.ssl_client_key(&client_key_path);
    }

    let connect_future = PgPoolOptions::new()
        .max_connections(DEFAULT_POOL_MAX_CONNECTIONS)
        .min_connections(DEFAULT_POOL_MIN_CONNECTIONS)
        .idle_timeout(Duration::from_secs(DEFAULT_POOL_IDLE_TIMEOUT_SECS))
        .max_lifetime(Duration::from_secs(DEFAULT_POOL_MAX_LIFETIME_SECS))
        .connect_with(options);

    let timeout =
        crate::constants::ACTIVE_CONNECTION_TIMEOUT_SECS.load(std::sync::atomic::Ordering::Relaxed);

    match tokio::time::timeout(Duration::from_secs(timeout), connect_future).await {
        Ok(result) => result.map_err(|error| {
            gui_connection_error_message(DatabaseDriver::PostgreSql, error.to_string())
        }),
        Err(_) => Err(format!("Connection timed out after {} seconds.", timeout)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_connection(tls_mode: TlsMode, tls_ca_cert_path: &str) -> ConnectionInfo {
        ConnectionInfo {
            driver: crate::DatabaseDriver::MySql,
            host: String::from("127.0.0.1"),
            port: String::from("3306"),
            database: String::from("sample"),
            username: String::from("user"),
            password: String::from("secret"),
            sqlite_path: String::new(),
            tls_mode,
            tls_ca_cert_path: tls_ca_cert_path.to_string(),
            tls_client_cert_path: String::new(),
            tls_client_key_path: String::new(),
        }
    }

    #[test]
    fn mysql_tls_mode_mapping_matches_expected_values() {
        assert!(matches!(
            mysql_ssl_mode(TlsMode::Disabled),
            MySqlSslMode::Disabled
        ));
        assert!(matches!(
            mysql_ssl_mode(TlsMode::Prefer),
            MySqlSslMode::Preferred
        ));
        assert!(matches!(
            mysql_ssl_mode(TlsMode::Require),
            MySqlSslMode::Required
        ));
        assert!(matches!(
            mysql_ssl_mode(TlsMode::VerifyCa),
            MySqlSslMode::VerifyCa
        ));
        assert!(matches!(
            mysql_ssl_mode(TlsMode::VerifyFull),
            MySqlSslMode::VerifyIdentity
        ));
    }

    #[test]
    fn mysql_prefer_requires_tls_for_remote_hosts() {
        let local = sample_connection(TlsMode::Prefer, "");
        assert_eq!(mysql_effective_tls_mode(&local), TlsMode::Prefer);

        let mut remote = local.clone();
        remote.host = String::from("db.example.com");
        assert_eq!(mysql_effective_tls_mode(&remote), TlsMode::Require);
    }

    #[test]
    fn mysql_rsa_auth_error_is_user_facing() {
        let error = gui_connection_error_message(
            DatabaseDriver::MySql,
            String::from(
                "configuration error: RSA auth backend disabled; enable feature `mysql-rsa` or use TLS.",
            ),
        );

        assert_eq!(error, MYSQL_RSA_AUTH_DISABLED_MESSAGE);
    }

    #[test]
    fn gui_connection_error_mapper_makes_common_failures_actionable() {
        assert_eq!(
            gui_connection_error_message(
                DatabaseDriver::PostgreSql,
                String::from("password authentication failed for user app"),
            ),
            "PostgreSQL authentication failed. Check the username and password."
        );
        assert_eq!(
            gui_connection_error_message(
                DatabaseDriver::MySql,
                String::from("failed to lookup address information: Name or service not known"),
            ),
            "Could not resolve the MySQL host. Check the host name and network/DNS settings."
        );
        assert_eq!(
            gui_connection_error_message(
                DatabaseDriver::MariaDb,
                String::from("Connection refused")
            ),
            "MariaDB refused the connection. Check that the server is running and accepting this host/port."
        );
        assert_eq!(
            gui_connection_error_message(
                DatabaseDriver::Sqlite,
                String::from("database is locked")
            ),
            "SQLite database is locked. Close other writers or retry after the active transaction finishes."
        );
        assert_eq!(
            gui_connection_error_message(
                DatabaseDriver::PostgreSql,
                String::from("TLS error: invalid certificate: unknown issuer"),
            ),
            "PostgreSQL TLS certificate validation failed. Check TLS mode and certificate paths."
        );
        assert_eq!(
            gui_connection_error_message(
                DatabaseDriver::Sqlite,
                std::io::Error::from(std::io::ErrorKind::PermissionDenied).to_string(),
            ),
            "SQLite connection failed because the target is not accessible with current permissions."
        );
    }

    #[test]
    fn postgres_tls_mode_mapping_matches_expected_values() {
        assert!(matches!(
            postgres_ssl_mode(TlsMode::Disabled),
            PgSslMode::Disable
        ));
        assert!(matches!(
            postgres_ssl_mode(TlsMode::Prefer),
            PgSslMode::Prefer
        ));
        assert!(matches!(
            postgres_ssl_mode(TlsMode::Require),
            PgSslMode::Require
        ));
        assert!(matches!(
            postgres_ssl_mode(TlsMode::VerifyCa),
            PgSslMode::VerifyCa
        ));
        assert!(matches!(
            postgres_ssl_mode(TlsMode::VerifyFull),
            PgSslMode::VerifyFull
        ));
    }

    #[test]
    fn validated_tls_ca_path_requires_path_for_verify_modes() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("tokio runtime");

        runtime.block_on(async {
            let info = sample_connection(TlsMode::VerifyCa, "");
            let error = validated_tls_ca_path(&info, "MySQL")
                .await
                .expect_err("missing path should fail");
            assert!(error.contains("CA certificate path is required"));
        });
    }

    #[test]
    fn validated_tls_ca_path_rejects_missing_files() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("tokio runtime");

        runtime.block_on(async {
            let info = sample_connection(TlsMode::VerifyFull, "/tmp/cryodb-missing-ca.pem");
            let error = validated_tls_ca_path(&info, "PostgreSQL")
                .await
                .expect_err("missing file should fail");
            assert!(error.contains("could not be found"));
        });
    }

    #[test]
    fn validated_tls_ca_path_accepts_existing_files() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("tokio runtime");

        runtime.block_on(async {
            let temp_dir = tempfile::tempdir().expect("temp dir");
            let ca_path = temp_dir.path().join("ca.pem");
            tokio::fs::write(&ca_path, b"dummy-ca")
                .await
                .expect("write ca file");

            let info = sample_connection(TlsMode::VerifyCa, &ca_path.display().to_string());
            let result = validated_tls_ca_path(&info, "MySQL")
                .await
                .expect("existing file should pass");
            assert_eq!(result.as_deref(), Some(ca_path.to_string_lossy().as_ref()));
        });
    }
}
