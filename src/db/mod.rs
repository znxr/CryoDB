use sqlx::{MySqlPool, PgPool, SqlitePool};

#[derive(Debug, Clone)]
pub(crate) enum DatabasePool {
    MySql(MySqlPool),
    Postgres(PgPool),
    Sqlite(SqlitePool),
}

pub(crate) mod codecs;
pub(crate) mod connect;
pub(crate) mod edits;
pub(crate) mod metadata;
pub(crate) mod query;
pub(crate) mod transfer;

#[cfg(test)]
mod tests {
    use super::connect::{connect_mysql, connect_postgres};
    use super::edits::{apply_table_changes, delete_table_rows, execute_table_command};
    use super::metadata::{
        fetch_databases, fetch_primary_keys, fetch_table_column_nullability, fetch_table_info,
        fetch_table_relations, fetch_table_triggers, fetch_tables,
    };
    use super::query::{run_query, run_table_query_with_control};
    use crate::{
        ColumnKind, ConnectionInfo, DatabaseDriver, DatabasePool, QueryOutput, TableCommand,
        TlsMode,
    };
    use sqlx::AssertSqlSafe;
    use std::collections::HashMap;
    use std::env;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn postgres_env_connection_info() -> Option<ConnectionInfo> {
        let host = env::var("CRYODB_TEST_POSTGRES_HOST").ok()?;
        let database = env::var("CRYODB_TEST_POSTGRES_DB").ok()?;
        let username = env::var("CRYODB_TEST_POSTGRES_USER").ok()?;
        let port = env::var("CRYODB_TEST_POSTGRES_PORT").unwrap_or_else(|_| String::from("5432"));
        let password = env::var("CRYODB_TEST_POSTGRES_PASSWORD").unwrap_or_default();

        Some(ConnectionInfo {
            driver: DatabaseDriver::PostgreSql,
            host,
            port,
            database,
            username,
            password,
            sqlite_path: String::new(),
            tls_mode: TlsMode::Prefer,
            tls_ca_cert_path: String::new(),
            tls_client_cert_path: String::new(),
            tls_client_key_path: String::new(),
        })
    }

    fn mysql_like_env_connection_info(
        prefix: &str,
        driver: DatabaseDriver,
    ) -> Option<ConnectionInfo> {
        let host = env::var(format!("CRYODB_TEST_{prefix}_HOST")).ok()?;
        let database = env::var(format!("CRYODB_TEST_{prefix}_DB")).ok()?;
        let username = env::var(format!("CRYODB_TEST_{prefix}_USER")).ok()?;
        let port =
            env::var(format!("CRYODB_TEST_{prefix}_PORT")).unwrap_or_else(|_| String::from("3306"));
        let password = env::var(format!("CRYODB_TEST_{prefix}_PASSWORD")).unwrap_or_default();

        Some(ConnectionInfo {
            driver,
            host,
            port,
            database,
            username,
            password,
            sqlite_path: String::new(),
            tls_mode: TlsMode::Prefer,
            tls_ca_cert_path: String::new(),
            tls_client_cert_path: String::new(),
            tls_client_key_path: String::new(),
        })
    }

    fn unique_suffix() -> String {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        nanos.to_string()
    }

    #[test]
    fn postgres_connect_metadata_query_edit_delete_happy_path_env() {
        let Some(info) = postgres_env_connection_info() else {
            eprintln!(
                "Skipping PostgreSQL integration test: set CRYODB_TEST_POSTGRES_HOST/PORT/DB/USER/PASSWORD"
            );
            return;
        };

        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("tokio runtime");

        runtime.block_on(async move {
            let pool = connect_postgres(info.clone())
                .await
                .expect("connect_postgres should succeed in integration env");

            let database = info.database.clone();
            let suffix = unique_suffix();
            let schema = format!("cryodb_it_{suffix}");
            let table = format!("items_{suffix}");
            let table_qualified = format!("{schema}.{table}");
            let renamed_table = format!("items_{suffix}_renamed");
            let renamed_qualified = format!("{schema}.{renamed_table}");

            let create_schema = format!("CREATE SCHEMA IF NOT EXISTS \"{schema}\"");
            sqlx::query(AssertSqlSafe(create_schema))
                .execute(&pool)
                .await
                .expect("create schema");

            let create_table =
                format!("CREATE TABLE \"{schema}\".\"{table}\" (id BIGINT PRIMARY KEY, name TEXT)");
            sqlx::query(AssertSqlSafe(create_table))
                .execute(&pool)
                .await
                .expect("create table");

            let seed_rows = format!(
                "INSERT INTO \"{schema}\".\"{table}\" (id, name) VALUES (1, 'alpha'), (2, 'beta')"
            );
            sqlx::query(AssertSqlSafe(seed_rows))
                .execute(&pool)
                .await
                .expect("seed rows");

            let databases = fetch_databases(DatabasePool::Postgres(pool.clone()))
                .await
                .expect("fetch_databases");
            assert!(databases.iter().any(|name| name == &database));

            let tables = fetch_tables(DatabasePool::Postgres(pool.clone()), database.clone())
                .await
                .expect("fetch_tables");
            assert!(tables.iter().any(|name| name == &table_qualified));

            let primary_keys = fetch_primary_keys(
                DatabasePool::Postgres(pool.clone()),
                &database,
                &table_qualified,
            )
            .await
            .expect("fetch_primary_keys");
            assert_eq!(primary_keys, vec![String::from("id")]);

            let nullability = fetch_table_column_nullability(
                DatabasePool::Postgres(pool.clone()),
                &database,
                &table_qualified,
            )
            .await
            .expect("fetch_table_column_nullability");
            assert_eq!(nullability.get("id"), Some(&false));
            assert_eq!(nullability.get("name"), Some(&true));

            let info_details = fetch_table_info(
                DatabasePool::Postgres(pool.clone()),
                database.clone(),
                table_qualified.clone(),
            )
            .await
            .expect("fetch_table_info");
            assert!(info_details.engine.is_some());

            let query_output = run_query(
                DatabasePool::Postgres(pool.clone()),
                Some(database.clone()),
                format!("SELECT id, name FROM \"{schema}\".\"{table}\" ORDER BY id"),
            )
            .await
            .expect("run_query select");

            let QueryOutput::Rows(result_set) = query_output else {
                panic!("expected row output for select query");
            };
            assert_eq!(result_set.rows.len(), 2);

            let table_output = run_table_query_with_control(
                DatabasePool::Postgres(pool.clone()),
                Some(database.clone()),
                table_qualified.clone(),
                format!("SELECT id, name FROM \"{schema}\".\"{table}\" ORDER BY id"),
                None,
            )
            .await
            .expect("run_table_query");

            let QueryOutput::Rows(table_rows) = table_output else {
                panic!("expected row output for table query");
            };

            let original_rows = table_rows
                .rows
                .iter()
                .cloned()
                .enumerate()
                .collect::<HashMap<usize, Vec<String>>>();
            let mut edits = HashMap::new();
            edits.insert((0usize, 1usize), String::from("alpha-updated"));

            let changed = apply_table_changes(
                DatabasePool::Postgres(pool.clone()),
                database.clone(),
                table_qualified.clone(),
                table_rows.columns.clone(),
                table_rows.column_kinds.clone(),
                original_rows,
                edits,
            )
            .await
            .expect("apply_table_changes");
            assert_eq!(changed, 1);

            let verify_update = run_query(
                DatabasePool::Postgres(pool.clone()),
                Some(database.clone()),
                format!("SELECT name FROM \"{schema}\".\"{table}\" WHERE id = 1"),
            )
            .await
            .expect("verify updated row");
            let QueryOutput::Rows(updated_rows) = verify_update else {
                panic!("expected updated row query output");
            };
            assert_eq!(
                updated_rows.rows.first().and_then(|row| row.first()),
                Some(&String::from("alpha-updated"))
            );

            let rows_to_delete = table_rows
                .rows
                .iter()
                .filter(|row| row.first().is_some_and(|id| id == "2"))
                .cloned()
                .collect::<Vec<_>>();
            let deleted = delete_table_rows(
                DatabasePool::Postgres(pool.clone()),
                database.clone(),
                table_qualified.clone(),
                table_rows.columns.clone(),
                rows_to_delete,
            )
            .await
            .expect("delete_table_rows");
            assert_eq!(deleted, 1);

            let renamed = execute_table_command(
                DatabasePool::Postgres(pool.clone()),
                database.clone(),
                TableCommand::Rename {
                    table: table_qualified.clone(),
                    new_table: renamed_qualified.clone(),
                },
            )
            .await
            .expect("execute_table_command rename");
            let _ = renamed;

            let tables_after = fetch_tables(DatabasePool::Postgres(pool.clone()), database.clone())
                .await
                .expect("fetch_tables after rename");
            assert!(tables_after.iter().any(|name| name == &renamed_qualified));

            let relations = fetch_table_relations(
                DatabasePool::Postgres(pool.clone()),
                database.clone(),
                renamed_qualified.clone(),
            )
            .await
            .expect("fetch_table_relations");
            assert!(relations.is_empty());

            let triggers = fetch_table_triggers(
                DatabasePool::Postgres(pool.clone()),
                database,
                renamed_qualified.clone(),
            )
            .await
            .expect("fetch_table_triggers");
            assert!(triggers.is_empty());

            let cleanup_drop_renamed =
                format!("DROP TABLE IF EXISTS \"{schema}\".\"{renamed_table}\"");
            sqlx::query(AssertSqlSafe(cleanup_drop_renamed))
                .execute(&pool)
                .await
                .expect("drop renamed test table");
            let cleanup_drop_original = format!("DROP TABLE IF EXISTS \"{schema}\".\"{table}\"");
            sqlx::query(AssertSqlSafe(cleanup_drop_original))
                .execute(&pool)
                .await
                .expect("drop original test table");
            let cleanup_schema = format!("DROP SCHEMA IF EXISTS \"{schema}\"");
            sqlx::query(AssertSqlSafe(cleanup_schema))
                .execute(&pool)
                .await
                .expect("drop test schema");

            pool.close().await;
        });
    }

    async fn run_mysql_like_lifecycle(info: ConnectionInfo) {
        let pool = connect_mysql(info.clone())
            .await
            .expect("connect_mysql should succeed in integration env");

        let database = info.database.clone();
        let suffix = unique_suffix();
        let table = format!("cryodb_it_{suffix}");
        let renamed_table = format!("cryodb_it_{suffix}_renamed");

        let drop_original = format!("DROP TABLE IF EXISTS `{table}`");
        sqlx::query(AssertSqlSafe(drop_original))
            .execute(&pool)
            .await
            .expect("drop original test table");
        let drop_renamed = format!("DROP TABLE IF EXISTS `{renamed_table}`");
        sqlx::query(AssertSqlSafe(drop_renamed))
            .execute(&pool)
            .await
            .expect("drop renamed test table");

        let create_table =
            format!("CREATE TABLE `{table}` (id BIGINT PRIMARY KEY, name VARCHAR(128) NULL)");
        sqlx::query(AssertSqlSafe(create_table))
            .execute(&pool)
            .await
            .expect("create table");

        let seed_rows =
            format!("INSERT INTO `{table}` (id, name) VALUES (1, 'alpha'), (2, 'beta')");
        sqlx::query(AssertSqlSafe(seed_rows))
            .execute(&pool)
            .await
            .expect("seed rows");

        let databases = fetch_databases(DatabasePool::MySql(pool.clone()))
            .await
            .expect("fetch_databases");
        assert!(databases.iter().any(|name| name == &database));

        let tables = fetch_tables(DatabasePool::MySql(pool.clone()), database.clone())
            .await
            .expect("fetch_tables");
        assert!(tables.iter().any(|name| name == &table));

        let primary_keys = fetch_primary_keys(DatabasePool::MySql(pool.clone()), &database, &table)
            .await
            .expect("fetch_primary_keys");
        assert_eq!(primary_keys, vec![String::from("id")]);

        let nullability =
            fetch_table_column_nullability(DatabasePool::MySql(pool.clone()), &database, &table)
                .await
                .expect("fetch_table_column_nullability");
        assert_eq!(nullability.get("id"), Some(&false));
        assert_eq!(nullability.get("name"), Some(&true));

        let info_details = fetch_table_info(
            DatabasePool::MySql(pool.clone()),
            database.clone(),
            table.clone(),
        )
        .await
        .expect("fetch_table_info");
        assert!(info_details.engine.is_some());

        let query_output = run_query(
            DatabasePool::MySql(pool.clone()),
            Some(database.clone()),
            format!("SELECT id, name FROM `{table}` ORDER BY id"),
        )
        .await
        .expect("run_query select");

        let QueryOutput::Rows(result_set) = query_output else {
            panic!("expected row output for select query");
        };
        assert_eq!(result_set.rows.len(), 2);

        let table_output = run_table_query_with_control(
            DatabasePool::MySql(pool.clone()),
            Some(database.clone()),
            table.clone(),
            format!("SELECT id, name FROM `{table}` ORDER BY id"),
            None,
        )
        .await
        .expect("run_table_query");

        let QueryOutput::Rows(table_rows) = table_output else {
            panic!("expected row output for table query");
        };

        let original_rows = table_rows
            .rows
            .iter()
            .cloned()
            .enumerate()
            .collect::<HashMap<usize, Vec<String>>>();
        let mut edits = HashMap::new();
        edits.insert((0usize, 1usize), String::from("alpha-updated"));

        let changed = apply_table_changes(
            DatabasePool::MySql(pool.clone()),
            database.clone(),
            table.clone(),
            table_rows.columns.clone(),
            table_rows.column_kinds.clone(),
            original_rows,
            edits,
        )
        .await
        .expect("apply_table_changes");
        assert_eq!(changed, 1);

        let rows_to_delete = table_rows
            .rows
            .iter()
            .filter(|row| row.first().is_some_and(|id| id == "2"))
            .cloned()
            .collect::<Vec<_>>();
        let deleted = delete_table_rows(
            DatabasePool::MySql(pool.clone()),
            database.clone(),
            table.clone(),
            table_rows.columns.clone(),
            rows_to_delete,
        )
        .await
        .expect("delete_table_rows");
        assert_eq!(deleted, 1);

        let renamed = execute_table_command(
            DatabasePool::MySql(pool.clone()),
            database.clone(),
            TableCommand::Rename {
                table: table.clone(),
                new_table: renamed_table.clone(),
            },
        )
        .await
        .expect("execute_table_command rename");
        let _ = renamed;

        let tables_after = fetch_tables(DatabasePool::MySql(pool.clone()), database.clone())
            .await
            .expect("fetch_tables after rename");
        assert!(tables_after.iter().any(|name| name == &renamed_table));

        let cleanup_drop_renamed = format!("DROP TABLE IF EXISTS `{renamed_table}`");
        sqlx::query(AssertSqlSafe(cleanup_drop_renamed))
            .execute(&pool)
            .await
            .expect("drop renamed test table");
        let cleanup_drop_original = format!("DROP TABLE IF EXISTS `{table}`");
        sqlx::query(AssertSqlSafe(cleanup_drop_original))
            .execute(&pool)
            .await
            .expect("drop original test table");

        pool.close().await;
    }

    #[test]
    fn mysql_connect_metadata_query_edit_delete_happy_path_env() {
        let Some(info) = mysql_like_env_connection_info("MYSQL", DatabaseDriver::MySql) else {
            eprintln!(
                "Skipping MySQL integration test: set CRYODB_TEST_MYSQL_HOST/PORT/DB/USER/PASSWORD"
            );
            return;
        };

        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("tokio runtime");

        runtime.block_on(run_mysql_like_lifecycle(info));
    }

    #[test]
    fn mariadb_connect_metadata_query_edit_delete_happy_path_env() {
        let Some(info) = mysql_like_env_connection_info("MARIADB", DatabaseDriver::MariaDb) else {
            eprintln!(
                "Skipping MariaDB integration test: set CRYODB_TEST_MARIADB_HOST/PORT/DB/USER/PASSWORD"
            );
            return;
        };

        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("tokio runtime");

        runtime.block_on(run_mysql_like_lifecycle(info));
    }

    #[test]
    fn mysql_sqlite_regression_parity_checks() {
        assert_eq!(
            crate::utils::helpers::sql_bind_placeholder(DatabaseDriver::MySql, 3),
            "?"
        );
        assert_eq!(
            crate::utils::helpers::sql_bind_placeholder(DatabaseDriver::Sqlite, 3),
            "?"
        );
        assert_eq!(
            crate::utils::helpers::sql_quote_identifier(DatabaseDriver::MySql, "users"),
            "`users`"
        );
        assert_eq!(
            crate::utils::helpers::sql_quote_identifier(DatabaseDriver::Sqlite, "users"),
            "\"users\""
        );
        assert_eq!(
            crate::utils::helpers::column_kind_from_type_name("BIGINT"),
            ColumnKind::Integer
        );
        assert_eq!(
            crate::utils::helpers::column_kind_from_type_name("BIGINT UNSIGNED"),
            ColumnKind::Unsigned
        );
    }
}
