use crate::*;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub(crate) const CONNECTION_STORE_SCHEMA_VERSION: u32 = 1;

fn default_connection_store_schema_version() -> u32 {
    CONNECTION_STORE_SCHEMA_VERSION
}

fn normalized_sqlite_path(path: &str) -> String {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    std::fs::canonicalize(trimmed)
        .unwrap_or_else(|_| PathBuf::from(trimmed))
        .display()
        .to_string()
}

#[derive(Debug, Clone)]
pub(crate) struct ConnectionInfo {
    pub(crate) driver: DatabaseDriver,
    pub(crate) host: String,
    pub(crate) port: String,
    pub(crate) database: String,
    pub(crate) username: String,
    pub(crate) password: String,
    pub(crate) sqlite_path: String,
    pub(crate) tls_mode: TlsMode,
    pub(crate) tls_ca_cert_path: String,
    pub(crate) tls_client_cert_path: String,
    pub(crate) tls_client_key_path: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub(crate) enum TlsMode {
    Disabled,
    #[default]
    Prefer,
    Require,
    VerifyCa,
    VerifyFull,
}

impl TlsMode {
    pub(crate) const ALL: [TlsMode; 5] = [
        TlsMode::Disabled,
        TlsMode::Prefer,
        TlsMode::Require,
        TlsMode::VerifyCa,
        TlsMode::VerifyFull,
    ];

    pub(crate) fn storage_key(self) -> &'static str {
        match self {
            TlsMode::Disabled => "disabled",
            TlsMode::Prefer => "prefer",
            TlsMode::Require => "require",
            TlsMode::VerifyCa => "verify_ca",
            TlsMode::VerifyFull => "verify_full",
        }
    }
}

impl std::fmt::Display for TlsMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TlsMode::Disabled => f.write_str(&crate::i18n::tr("Disabled")),
            TlsMode::Prefer => f.write_str(&crate::i18n::tr("Prefer")),
            TlsMode::Require => f.write_str(&crate::i18n::tr("Require")),
            TlsMode::VerifyCa => f.write_str(&crate::i18n::tr("Verify CA")),
            TlsMode::VerifyFull => f.write_str(&crate::i18n::tr("Verify Full")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub(crate) enum DatabaseDriver {
    #[serde(rename = "mysql")]
    #[default]
    MySql,
    #[serde(rename = "mariadb")]
    MariaDb,
    #[serde(rename = "sqlite")]
    Sqlite,
    #[serde(rename = "postgresql")]
    PostgreSql,
}

impl DatabaseDriver {
    pub(crate) const ALL: [DatabaseDriver; 4] = [
        DatabaseDriver::MySql,
        DatabaseDriver::MariaDb,
        DatabaseDriver::Sqlite,
        DatabaseDriver::PostgreSql,
    ];

    pub(crate) fn default_query(self) -> &'static str {
        "/*\nWelcome to CryoDB.\nWrite a query to get started.\n*/"
    }

    pub(crate) fn default_port(self) -> &'static str {
        match self {
            DatabaseDriver::PostgreSql => POSTGRES_DEFAULT_PORT,
            _ => MYSQL_DEFAULT_PORT,
        }
    }

    pub(crate) fn icon(self) -> char {
        match self {
            DatabaseDriver::MySql => crate::ICON_DRIVER_MYSQL,
            DatabaseDriver::MariaDb => crate::ICON_DRIVER_MARIADB,
            DatabaseDriver::Sqlite => crate::ICON_DRIVER_SQLITE,
            DatabaseDriver::PostgreSql => crate::ICON_DRIVER_POSTGRESQL,
        }
    }

    pub(crate) fn storage_key(self) -> &'static str {
        match self {
            DatabaseDriver::MySql => "mysql",
            DatabaseDriver::MariaDb => "mariadb",
            DatabaseDriver::Sqlite => "sqlite",
            DatabaseDriver::PostgreSql => "postgresql",
        }
    }
}

impl std::fmt::Display for DatabaseDriver {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DatabaseDriver::MySql => f.write_str(&crate::i18n::tr("MySQL")),
            DatabaseDriver::MariaDb => f.write_str(&crate::i18n::tr("MariaDB")),
            DatabaseDriver::Sqlite => f.write_str(&crate::i18n::tr("SQLite")),
            DatabaseDriver::PostgreSql => f.write_str(&crate::i18n::tr("PostgreSQL")),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct StoredConnection {
    #[serde(default)]
    pub(crate) name: String,
    #[serde(default = "crate::utils::helpers::default_database_driver")]
    pub(crate) driver: DatabaseDriver,
    #[serde(default)]
    pub(crate) sqlite_path: String,
    pub(crate) host: String,
    pub(crate) port: String,
    pub(crate) database: String,
    pub(crate) username: String,
    #[serde(default)]
    pub(crate) password: String,
    #[serde(default)]
    pub(crate) tls_mode: TlsMode,
    #[serde(default)]
    pub(crate) tls_ca_cert_path: String,
    #[serde(default)]
    pub(crate) tls_client_cert_path: String,
    #[serde(default)]
    pub(crate) tls_client_key_path: String,
    #[serde(default)]
    pub(crate) tags: Vec<String>,
}

impl StoredConnection {
    pub(crate) fn from_info(info: &ConnectionInfo) -> Self {
        let driver = info.driver;
        match driver {
            DatabaseDriver::Sqlite => Self {
                name: String::new(),
                driver,
                sqlite_path: normalized_sqlite_path(&info.sqlite_path),
                host: String::new(),
                port: String::new(),
                database: String::new(),
                username: String::new(),
                password: String::new(),
                tls_mode: TlsMode::default(),
                tls_ca_cert_path: String::new(),
                tls_client_cert_path: String::new(),
                tls_client_key_path: String::new(),
                tags: Vec::new(),
            },
            _ => {
                let host = info.host.trim().to_string();
                let port = if info.port.trim().is_empty() {
                    driver.default_port().to_string()
                } else {
                    info.port.trim().to_string()
                };
                let database = info.database.trim().to_string();
                let username = info.username.trim().to_string();
                Self {
                    name: String::new(),
                    driver,
                    sqlite_path: String::new(),
                    host,
                    port,
                    database,
                    username,
                    password: info.password.trim().to_string(),
                    tls_mode: info.tls_mode,
                    tls_ca_cert_path: info.tls_ca_cert_path.trim().to_string(),
                    tls_client_cert_path: info.tls_client_cert_path.trim().to_string(),
                    tls_client_key_path: info.tls_client_key_path.trim().to_string(),
                    tags: Vec::new(),
                }
            }
        }
    }

    pub(crate) fn normalized(self) -> Self {
        match self.driver {
            DatabaseDriver::Sqlite => Self {
                name: self.name.trim().to_string(),
                driver: self.driver,
                sqlite_path: normalized_sqlite_path(&self.sqlite_path),
                host: String::new(),
                port: String::new(),
                database: String::new(),
                username: String::new(),
                password: String::new(),
                tls_mode: TlsMode::default(),
                tls_ca_cert_path: String::new(),
                tls_client_cert_path: String::new(),
                tls_client_key_path: String::new(),
                tags: self.tags.clone(),
            },
            _ => {
                let host = self.host.trim().to_string();
                let port = if self.port.trim().is_empty() {
                    self.driver.default_port().to_string()
                } else {
                    self.port.trim().to_string()
                };
                let database = self.database.trim().to_string();
                let username = self.username.trim().to_string();
                let password = self.password.trim().to_string();
                Self {
                    name: self.name.trim().to_string(),
                    driver: self.driver,
                    sqlite_path: String::new(),
                    host,
                    port,
                    database,
                    username,
                    password,
                    tls_mode: self.tls_mode,
                    tls_ca_cert_path: self.tls_ca_cert_path.trim().to_string(),
                    tls_client_cert_path: self.tls_client_cert_path.trim().to_string(),
                    tls_client_key_path: self.tls_client_key_path.trim().to_string(),
                    tags: self.tags.clone(),
                }
            }
        }
    }

    pub(crate) fn is_valid(&self) -> bool {
        match self.driver {
            DatabaseDriver::Sqlite => !self.sqlite_path.trim().is_empty(),
            _ => !self.host.trim().is_empty() && !self.username.trim().is_empty(),
        }
    }

    pub(crate) fn matches_identity(&self, other: &StoredConnection) -> bool {
        if self.driver != other.driver {
            return false;
        }
        match self.driver {
            DatabaseDriver::Sqlite => {
                normalized_sqlite_path(&self.sqlite_path)
                    == normalized_sqlite_path(&other.sqlite_path)
            }
            _ => {
                self.host == other.host
                    && self.port == other.port
                    && self.database == other.database
                    && self.username == other.username
                    && self.tls_mode == other.tls_mode
                    && self.tls_ca_cert_path == other.tls_ca_cert_path
                    && self.tls_client_cert_path == other.tls_client_cert_path
                    && self.tls_client_key_path == other.tls_client_key_path
            }
        }
    }

    pub(crate) fn secret_lookup_key(&self) -> String {
        match self.driver {
            DatabaseDriver::Sqlite => {
                format!("sqlite|{}", normalized_sqlite_path(&self.sqlite_path))
            }
            _ => {
                let host = self.host.trim().to_ascii_lowercase();
                let port = if self.port.trim().is_empty() {
                    self.driver.default_port().to_string()
                } else {
                    self.port.trim().to_string()
                };
                let database = self.database.trim().to_ascii_lowercase();
                let username = self.username.trim().to_ascii_lowercase();
                let tls_mode = self.tls_mode.storage_key();
                let tls_ca = self.tls_ca_cert_path.trim().to_ascii_lowercase();
                let tls_client_cert = self.tls_client_cert_path.trim().to_ascii_lowercase();
                let tls_client_key = self.tls_client_key_path.trim().to_ascii_lowercase();
                format!(
                    "{}|{}|{}|{}|{}|{}|{}|{}|{}",
                    self.driver.storage_key(),
                    host,
                    port,
                    database,
                    username,
                    tls_mode,
                    tls_ca,
                    tls_client_cert,
                    tls_client_key
                )
            }
        }
    }

    pub(crate) fn display_label(&self) -> String {
        match self.driver {
            DatabaseDriver::Sqlite => {
                let path = self.sqlite_path.trim();
                if path.is_empty() {
                    String::from("SQLite")
                } else {
                    PathBuf::from(path)
                        .file_name()
                        .and_then(|value| value.to_str())
                        .filter(|value| !value.trim().is_empty())
                        .map(std::string::ToString::to_string)
                        .unwrap_or_else(|| path.to_string())
                }
            }
            _ => {
                let host = self.host.trim();
                let database = self.database.trim();
                if database.is_empty() {
                    host.to_string()
                } else {
                    format!("{database}@{host}")
                }
            }
        }
    }

    pub(crate) fn profile_name(&self) -> Option<&str> {
        let name = self.name.trim();
        if name.is_empty() { None } else { Some(name) }
    }

    pub(crate) fn profile_label(&self) -> String {
        self.profile_name()
            .map(std::string::ToString::to_string)
            .unwrap_or_else(|| self.display_label())
    }

    pub(crate) fn matches_profile(&self, lookup: &str) -> bool {
        let lookup = lookup.trim();
        if lookup.is_empty() {
            return false;
        }

        if let Some(name) = self.profile_name()
            && name.eq_ignore_ascii_case(lookup)
        {
            return true;
        }

        self.display_label().eq_ignore_ascii_case(lookup)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ConnectionStore {
    #[serde(default = "default_connection_store_schema_version")]
    pub(crate) schema_version: u32,
    #[serde(default)]
    pub(crate) profiles: Vec<StoredConnection>,
    #[serde(default)]
    pub(crate) default_connection: Option<String>,
    #[serde(default)]
    pub(crate) favorites: Vec<StoredConnection>,
    #[serde(default)]
    pub(crate) recents: Vec<StoredConnection>,
}

impl Default for ConnectionStore {
    fn default() -> Self {
        Self {
            schema_version: CONNECTION_STORE_SCHEMA_VERSION,
            profiles: Vec::new(),
            default_connection: None,
            favorites: Vec::new(),
            recents: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalized_network_connection_preserves_password_fallback() {
        let entry = StoredConnection {
            name: String::new(),
            driver: DatabaseDriver::MySql,
            sqlite_path: String::new(),
            host: String::from(" 127.0.0.1 "),
            port: String::new(),
            database: String::from(" app_db "),
            username: String::from(" root "),
            password: String::from(" topsecret "),
            tls_mode: TlsMode::Prefer,
            tls_ca_cert_path: String::new(),
            tls_client_cert_path: String::new(),
            tls_client_key_path: String::new(),
            tags: Vec::new(),
        };

        let normalized = entry.normalized();

        assert_eq!(normalized.password, "topsecret");
        assert_eq!(normalized.port, DatabaseDriver::MySql.default_port());
        assert_eq!(normalized.host, "127.0.0.1");
        assert_eq!(normalized.database, "app_db");
        assert_eq!(normalized.username, "root");
    }

    fn stored_network(driver: DatabaseDriver, tls_mode: TlsMode) -> StoredConnection {
        StoredConnection {
            name: String::new(),
            driver,
            sqlite_path: String::new(),
            host: String::from("db.example.com"),
            port: String::from("3306"),
            database: String::from("app"),
            username: String::from("root"),
            password: String::new(),
            tls_mode,
            tls_ca_cert_path: String::new(),
            tls_client_cert_path: String::new(),
            tls_client_key_path: String::new(),
            tags: Vec::new(),
        }
    }

    #[test]
    fn network_identity_and_secret_key_include_driver_and_tls() {
        let mysql = stored_network(DatabaseDriver::MySql, TlsMode::Prefer).normalized();
        let mariadb = stored_network(DatabaseDriver::MariaDb, TlsMode::Prefer).normalized();
        let mysql_tls = stored_network(DatabaseDriver::MySql, TlsMode::Require).normalized();

        assert!(!mysql.matches_identity(&mariadb));
        assert!(!mysql.matches_identity(&mysql_tls));
        assert_ne!(mysql.secret_lookup_key(), mariadb.secret_lookup_key());
        assert_ne!(mysql.secret_lookup_key(), mysql_tls.secret_lookup_key());
    }

    #[test]
    fn sqlite_identity_uses_canonical_path_and_filename_label() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let nested = temp_dir.path().join("nested");
        std::fs::create_dir(&nested).expect("nested dir");
        let database = nested.join("app.sqlite");
        std::fs::write(&database, b"").expect("sqlite placeholder");
        let equivalent = nested.join("..").join("nested").join("app.sqlite");

        let left = StoredConnection {
            name: String::new(),
            driver: DatabaseDriver::Sqlite,
            sqlite_path: database.display().to_string(),
            host: String::new(),
            port: String::new(),
            database: String::new(),
            username: String::new(),
            password: String::new(),
            tls_mode: TlsMode::default(),
            tls_ca_cert_path: String::new(),
            tls_client_cert_path: String::new(),
            tls_client_key_path: String::new(),
            tags: Vec::new(),
        };
        let mut right = left.clone();
        right.sqlite_path = equivalent.display().to_string();

        assert!(left.matches_identity(&right));
        assert_eq!(right.normalized().display_label(), "app.sqlite");
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn sqlite_secret_key_preserves_case_on_case_sensitive_filesystems() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let upper = temp_dir.path().join("App.sqlite");
        let lower = temp_dir.path().join("app.sqlite");
        std::fs::write(&upper, b"").expect("upper sqlite placeholder");
        std::fs::write(&lower, b"").expect("lower sqlite placeholder");

        let entry = |path: PathBuf| StoredConnection {
            name: String::new(),
            driver: DatabaseDriver::Sqlite,
            sqlite_path: path.display().to_string(),
            host: String::new(),
            port: String::new(),
            database: String::new(),
            username: String::new(),
            password: String::new(),
            tls_mode: TlsMode::default(),
            tls_ca_cert_path: String::new(),
            tls_client_cert_path: String::new(),
            tls_client_key_path: String::new(),
            tags: Vec::new(),
        };

        assert_ne!(
            entry(upper).secret_lookup_key(),
            entry(lower).secret_lookup_key()
        );
    }

    #[test]
    fn legacy_connection_store_payload_defaults_schema_version() {
        let payload = r#"
[[favorites]]
driver = "mysql"
host = "localhost"
port = "3306"
database = "app"
username = "root"

[[recents]]
driver = "sqlite"
sqlite_path = "/tmp/app.sqlite"
host = ""
port = ""
database = ""
username = ""
"#;

        let store: ConnectionStore = toml::from_str(payload).expect("legacy store payload");

        assert_eq!(store.schema_version, CONNECTION_STORE_SCHEMA_VERSION);
        assert_eq!(store.favorites.len(), 1);
        assert_eq!(store.recents.len(), 1);
    }
}
