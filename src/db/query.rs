use crate::db::codecs::{row_to_strings, row_to_strings_postgres, row_to_strings_sqlite};
use crate::db::metadata::fetch_table_column_nullability;
use crate::{
    DatabasePool, QueryOutput, ResultSet, column_kind_from_type_name, escape_mysql_identifier,
};
use futures_util::TryStreamExt;
use sqlx::mysql::MySqlRow;
use sqlx::pool::PoolConnection;
use sqlx::postgres::PgRow;
use sqlx::sqlite::SqliteRow;
use sqlx::{AssertSqlSafe, Column, Executor, MySql, Postgres, Row, Sqlite, TypeInfo};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

fn query_cancelled(cancel_flag: Option<&Arc<AtomicBool>>) -> bool {
    cancel_flag
        .map(|flag| flag.load(Ordering::Relaxed))
        .unwrap_or(false)
}

pub(crate) fn query_returns_rows(query: &str) -> bool {
    let effective = crate::utils::sql_parse::strip_leading_sql_comments(query);
    let keyword = effective
        .split_whitespace()
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();

    if matches!(
        keyword.as_str(),
        "select"
            | "show"
            | "describe"
            | "desc"
            | "with"
            | "explain"
            | "pragma"
            | "values"
            | "table"
    ) {
        return true;
    }

    if matches!(keyword.as_str(), "insert" | "update" | "delete") {
        let normalized = effective
            .trim()
            .trim_end_matches(';')
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .to_ascii_lowercase();

        return normalized.contains(" returning ") || normalized.ends_with(" returning");
    }

    false
}

fn mysql_columns_and_kinds(row: &MySqlRow) -> (Vec<String>, Vec<crate::ColumnKind>) {
    let mut names = Vec::with_capacity(row.columns().len());
    let mut kinds = Vec::with_capacity(row.columns().len());
    for column in row.columns() {
        names.push(column.name().to_string());
        kinds.push(column_kind_from_type_name(column.type_info().name()));
    }
    (names, kinds)
}

fn sqlite_columns_and_kinds(row: &SqliteRow) -> (Vec<String>, Vec<crate::ColumnKind>) {
    let mut names = Vec::with_capacity(row.columns().len());
    let mut kinds = Vec::with_capacity(row.columns().len());
    for column in row.columns() {
        names.push(column.name().to_string());
        kinds.push(column_kind_from_type_name(column.type_info().name()));
    }
    (names, kinds)
}

fn postgres_columns_and_kinds(row: &PgRow) -> (Vec<String>, Vec<crate::ColumnKind>, Vec<String>) {
    let mut names = Vec::with_capacity(row.columns().len());
    let mut kinds = Vec::with_capacity(row.columns().len());
    let mut type_names = Vec::with_capacity(row.columns().len());
    for column in row.columns() {
        names.push(column.name().to_string());
        kinds.push(column_kind_from_type_name(column.type_info().name()));
        type_names.push(column.type_info().name().to_string());
    }
    (names, kinds, type_names)
}

async fn fetch_mysql_result_rows(
    conn: &mut PoolConnection<MySql>,
    query: &str,
    cancel_flag: Option<&Arc<AtomicBool>>,
    max_rows: Option<usize>,
) -> Result<(Vec<String>, Vec<crate::ColumnKind>, Vec<Vec<String>>), String> {
    let mut stream = sqlx::query(AssertSqlSafe(query)).fetch(&mut **conn);
    let mut columns = Vec::new();
    let mut column_kinds = Vec::new();
    let mut data_rows = Vec::new();

    while let Some(row) = stream.try_next().await.map_err(|error| error.to_string())? {
        if query_cancelled(cancel_flag) {
            return Err(String::from("Query cancelled."));
        }
        if columns.is_empty() {
            let (names, kinds) = mysql_columns_and_kinds(&row);
            columns = names;
            column_kinds = kinds;
        }

        if let Some(limit) = max_rows
            && data_rows.len() >= limit
        {
            break;
        }

        let column_count = columns.len();
        data_rows.push(row_to_strings(&row, &column_kinds, column_count));
    }

    if query_cancelled(cancel_flag) {
        return Err(String::from("Query cancelled."));
    }

    Ok((columns, column_kinds, data_rows))
}

async fn fetch_sqlite_result_rows(
    conn: &mut PoolConnection<Sqlite>,
    query: &str,
    cancel_flag: Option<&Arc<AtomicBool>>,
    max_rows: Option<usize>,
) -> Result<(Vec<String>, Vec<crate::ColumnKind>, Vec<Vec<String>>), String> {
    let mut stream = sqlx::query(AssertSqlSafe(query)).fetch(&mut **conn);
    let mut columns = Vec::new();
    let mut column_kinds = Vec::new();
    let mut data_rows = Vec::new();

    while let Some(row) = stream.try_next().await.map_err(|error| error.to_string())? {
        if query_cancelled(cancel_flag) {
            return Err(String::from("Query cancelled."));
        }
        if columns.is_empty() {
            let (names, kinds) = sqlite_columns_and_kinds(&row);
            columns = names;
            column_kinds = kinds;
        }

        if let Some(limit) = max_rows
            && data_rows.len() >= limit
        {
            break;
        }

        let column_count = columns.len();
        data_rows.push(row_to_strings_sqlite(&row, &column_kinds, column_count));
    }

    if query_cancelled(cancel_flag) {
        return Err(String::from("Query cancelled."));
    }

    Ok((columns, column_kinds, data_rows))
}

async fn fetch_postgres_result_rows(
    conn: &mut PoolConnection<Postgres>,
    query: &str,
    cancel_flag: Option<&Arc<AtomicBool>>,
    max_rows: Option<usize>,
) -> Result<(Vec<String>, Vec<crate::ColumnKind>, Vec<Vec<String>>), String> {
    let mut stream = sqlx::query(AssertSqlSafe(query)).fetch(&mut **conn);
    let mut columns = Vec::new();
    let mut column_kinds = Vec::new();
    let mut column_type_names = Vec::new();
    let mut data_rows = Vec::new();

    while let Some(row) = stream.try_next().await.map_err(|error| error.to_string())? {
        if query_cancelled(cancel_flag) {
            return Err(String::from("Query cancelled."));
        }
        if columns.is_empty() {
            let (names, kinds, type_names) = postgres_columns_and_kinds(&row);
            columns = names;
            column_kinds = kinds;
            column_type_names = type_names;
        }

        if let Some(limit) = max_rows
            && data_rows.len() >= limit
        {
            break;
        }

        let column_count = columns.len();
        data_rows.push(row_to_strings_postgres(
            &row,
            &column_kinds,
            &column_type_names,
            column_count,
        ));
    }

    if query_cancelled(cancel_flag) {
        return Err(String::from("Query cancelled."));
    }

    Ok((columns, column_kinds, data_rows))
}

async fn ensure_postgres_current_database(
    conn: &mut PoolConnection<Postgres>,
    database: Option<String>,
) -> Result<(), String> {
    let Some(target_database) = database.filter(|name| !name.trim().is_empty()) else {
        return Ok(());
    };

    let current_database: String = sqlx::query_scalar("SELECT current_database()")
        .fetch_one(&mut **conn)
        .await
        .map_err(|error| error.to_string())?;

    if current_database != target_database.trim() {
        return Err(format!(
            "Connected to PostgreSQL database `{current_database}`. Switch database by reconnecting for now."
        ));
    }

    Ok(())
}

pub(crate) async fn run_readonly_query(
    pool: DatabasePool,
    database: Option<String>,
    query: String,
    cancel_flag: Option<Arc<AtomicBool>>,
    max_rows: Option<usize>,
) -> Result<QueryOutput, String> {
    let trimmed = query.trim().to_string();
    if !query_returns_rows(&trimmed) {
        return Err(String::from(
            "Blocked: only statements that return rows can run in read-only mode.",
        ));
    }

    match pool {
        DatabasePool::MySql(pool) => {
            let mut conn = pool.acquire().await.map_err(|error| error.to_string())?;

            if let Some(database) = database.filter(|name| !name.trim().is_empty()) {
                let statement = format!("USE `{}`", escape_mysql_identifier(database.trim()));
                conn.execute(AssertSqlSafe(statement))
                    .await
                    .map_err(|error| error.to_string())?;
            }

            conn.execute("START TRANSACTION READ ONLY")
                .await
                .map_err(|error| error.to_string())?;

            let rows =
                fetch_mysql_result_rows(&mut conn, &trimmed, cancel_flag.as_ref(), max_rows).await;
            let _ = conn.execute("ROLLBACK").await;

            let (columns, column_kinds, data_rows) = rows?;
            let column_count = columns.len();
            Ok(QueryOutput::Rows(ResultSet {
                columns,
                column_kinds,
                column_nullable: vec![false; column_count],
                rows: data_rows,
            }))
        }
        DatabasePool::Postgres(pool) => {
            let mut conn = pool.acquire().await.map_err(|error| error.to_string())?;

            conn.execute("BEGIN TRANSACTION READ ONLY")
                .await
                .map_err(|error| error.to_string())?;

            let rows =
                fetch_postgres_result_rows(&mut conn, &trimmed, cancel_flag.as_ref(), max_rows)
                    .await;
            let _ = conn.execute("ROLLBACK").await;

            let (columns, column_kinds, data_rows) = rows?;
            let column_count = columns.len();
            Ok(QueryOutput::Rows(ResultSet {
                columns,
                column_kinds,
                column_nullable: vec![false; column_count],
                rows: data_rows,
            }))
        }
        DatabasePool::Sqlite(pool) => {
            let mut conn = pool.acquire().await.map_err(|error| error.to_string())?;

            conn.execute("PRAGMA query_only = ON")
                .await
                .map_err(|error| error.to_string())?;

            let rows =
                fetch_sqlite_result_rows(&mut conn, &trimmed, cancel_flag.as_ref(), max_rows).await;
            let _ = conn.execute("PRAGMA query_only = OFF").await;

            let (columns, column_kinds, data_rows) = rows?;
            let column_count = columns.len();
            Ok(QueryOutput::Rows(ResultSet {
                columns,
                column_kinds,
                column_nullable: vec![false; column_count],
                rows: data_rows,
            }))
        }
    }
}

pub(crate) async fn run_query(
    pool: DatabasePool,
    database: Option<String>,
    query: String,
) -> Result<QueryOutput, String> {
    run_query_with_control(pool, database, query, None, None).await
}

pub(crate) async fn run_query_with_control(
    pool: DatabasePool,
    database: Option<String>,
    query: String,
    cancel_flag: Option<Arc<AtomicBool>>,
    max_rows: Option<usize>,
) -> Result<QueryOutput, String> {
    match pool {
        DatabasePool::MySql(pool) => {
            let trimmed = query.trim();
            let mut conn = pool.acquire().await.map_err(|error| error.to_string())?;

            if let Some(database) = database.filter(|name| !name.trim().is_empty()) {
                let statement = format!("USE `{}`", escape_mysql_identifier(database.trim()));
                conn.execute(AssertSqlSafe(statement))
                    .await
                    .map_err(|error| error.to_string())?;
            }

            if query_returns_rows(trimmed) {
                let (columns, column_kinds, data_rows) =
                    fetch_mysql_result_rows(&mut conn, trimmed, cancel_flag.as_ref(), max_rows)
                        .await?;
                let column_count = columns.len();

                Ok(QueryOutput::Rows(ResultSet {
                    columns,
                    column_kinds,
                    column_nullable: vec![false; column_count],
                    rows: data_rows,
                }))
            } else {
                let result = sqlx::query(AssertSqlSafe(trimmed))
                    .execute(&mut *conn)
                    .await
                    .map_err(|error| error.to_string())?;

                Ok(QueryOutput::Affected(result.rows_affected()))
            }
        }
        DatabasePool::Sqlite(pool) => {
            let trimmed = query.trim();
            let mut conn = pool.acquire().await.map_err(|error| error.to_string())?;

            if query_returns_rows(trimmed) {
                let (columns, column_kinds, data_rows) =
                    fetch_sqlite_result_rows(&mut conn, trimmed, cancel_flag.as_ref(), max_rows)
                        .await?;
                let column_count = columns.len();

                Ok(QueryOutput::Rows(ResultSet {
                    columns,
                    column_kinds,
                    column_nullable: vec![false; column_count],
                    rows: data_rows,
                }))
            } else {
                let result = sqlx::query(AssertSqlSafe(trimmed))
                    .execute(&mut *conn)
                    .await
                    .map_err(|error| error.to_string())?;
                Ok(QueryOutput::Affected(result.rows_affected()))
            }
        }
        DatabasePool::Postgres(pool) => {
            let trimmed = query.trim();
            let mut conn = pool.acquire().await.map_err(|error| error.to_string())?;

            ensure_postgres_current_database(&mut conn, database).await?;

            if query_returns_rows(trimmed) {
                let (columns, column_kinds, data_rows) =
                    fetch_postgres_result_rows(&mut conn, trimmed, cancel_flag.as_ref(), max_rows)
                        .await?;
                let column_count = columns.len();

                Ok(QueryOutput::Rows(ResultSet {
                    columns,
                    column_kinds,
                    column_nullable: vec![false; column_count],
                    rows: data_rows,
                }))
            } else {
                let result = sqlx::query(AssertSqlSafe(trimmed))
                    .execute(&mut *conn)
                    .await
                    .map_err(|error| error.to_string())?;
                Ok(QueryOutput::Affected(result.rows_affected()))
            }
        }
    }
}

pub(crate) async fn run_multi_query_with_control(
    pool: DatabasePool,
    database: Option<String>,
    query: String,
    cancel_flag: Option<Arc<AtomicBool>>,
    max_rows: Option<usize>,
) -> Result<Vec<QueryOutput>, String> {
    use crate::utils::sql_parse::split_sql_statements;
    let statements: Vec<String> = split_sql_statements(&query)
        .into_iter()
        .map(|s| s.to_string())
        .collect();
    if statements.is_empty() {
        return Err(String::from("Query is empty."));
    }

    match pool {
        DatabasePool::MySql(pool) => {
            let mut conn = pool.acquire().await.map_err(|error| error.to_string())?;
            if let Some(database) = database.filter(|name| !name.trim().is_empty()) {
                let statement = format!("USE `{}`", escape_mysql_identifier(database.trim()));
                conn.execute(AssertSqlSafe(statement))
                    .await
                    .map_err(|error| error.to_string())?;
            }

            let mut outputs = Vec::with_capacity(statements.len());
            for stmt in statements {
                if query_cancelled(cancel_flag.as_ref()) {
                    return Err(String::from("Query cancelled."));
                }
                let trimmed = stmt.trim();
                let result = if query_returns_rows(trimmed) {
                    let (columns, column_kinds, data_rows) =
                        fetch_mysql_result_rows(&mut conn, trimmed, cancel_flag.as_ref(), max_rows)
                            .await?;
                    let column_count = columns.len();
                    QueryOutput::Rows(ResultSet {
                        columns,
                        column_kinds,
                        column_nullable: vec![false; column_count],
                        rows: data_rows,
                    })
                } else {
                    let res = sqlx::query(AssertSqlSafe(trimmed))
                        .execute(&mut *conn)
                        .await
                        .map_err(|error| error.to_string())?;
                    QueryOutput::Affected(res.rows_affected())
                };
                outputs.push(result);
            }
            Ok(outputs)
        }
        DatabasePool::Sqlite(pool) => {
            let mut conn = pool.acquire().await.map_err(|error| error.to_string())?;
            let mut outputs = Vec::with_capacity(statements.len());
            for stmt in statements {
                if query_cancelled(cancel_flag.as_ref()) {
                    return Err(String::from("Query cancelled."));
                }
                let trimmed = stmt.trim();
                let result = if query_returns_rows(trimmed) {
                    let (columns, column_kinds, data_rows) = fetch_sqlite_result_rows(
                        &mut conn,
                        trimmed,
                        cancel_flag.as_ref(),
                        max_rows,
                    )
                    .await?;
                    let column_count = columns.len();
                    QueryOutput::Rows(ResultSet {
                        columns,
                        column_kinds,
                        column_nullable: vec![false; column_count],
                        rows: data_rows,
                    })
                } else {
                    let res = sqlx::query(AssertSqlSafe(trimmed))
                        .execute(&mut *conn)
                        .await
                        .map_err(|error| error.to_string())?;
                    QueryOutput::Affected(res.rows_affected())
                };
                outputs.push(result);
            }
            Ok(outputs)
        }
        DatabasePool::Postgres(pool) => {
            let mut conn = pool.acquire().await.map_err(|error| error.to_string())?;
            ensure_postgres_current_database(&mut conn, database).await?;
            let mut outputs = Vec::with_capacity(statements.len());
            for stmt in statements {
                if query_cancelled(cancel_flag.as_ref()) {
                    return Err(String::from("Query cancelled."));
                }
                let trimmed = stmt.trim();
                let result = if query_returns_rows(trimmed) {
                    let (columns, column_kinds, data_rows) = fetch_postgres_result_rows(
                        &mut conn,
                        trimmed,
                        cancel_flag.as_ref(),
                        max_rows,
                    )
                    .await?;
                    let column_count = columns.len();
                    QueryOutput::Rows(ResultSet {
                        columns,
                        column_kinds,
                        column_nullable: vec![false; column_count],
                        rows: data_rows,
                    })
                } else {
                    let res = sqlx::query(AssertSqlSafe(trimmed))
                        .execute(&mut *conn)
                        .await
                        .map_err(|error| error.to_string())?;
                    QueryOutput::Affected(res.rows_affected())
                };
                outputs.push(result);
            }
            Ok(outputs)
        }
    }
}

pub(crate) async fn run_table_query_with_control(
    pool: DatabasePool,
    database: Option<String>,
    table: String,
    query: String,
    cancel_flag: Option<Arc<AtomicBool>>,
) -> Result<QueryOutput, String> {
    match pool {
        DatabasePool::MySql(pool) => {
            let trimmed = query.trim();
            let database = database
                .filter(|name| !name.trim().is_empty())
                .ok_or_else(|| String::from("Select a database to load tables."))?;

            let mut conn = pool.acquire().await.map_err(|error| error.to_string())?;

            let statement = format!("USE `{}`", escape_mysql_identifier(database.trim()));
            conn.execute(AssertSqlSafe(statement))
                .await
                .map_err(|error| error.to_string())?;

            if !query_returns_rows(trimmed) {
                return Err(String::from("Query did not return rows."));
            }

            let (columns, column_kinds, data_rows) =
                fetch_mysql_result_rows(&mut conn, trimmed, cancel_flag.as_ref(), None).await?;

            let nullability =
                fetch_table_column_nullability(DatabasePool::MySql(pool), database.trim(), &table)
                    .await?;

            let column_nullable = columns
                .iter()
                .map(|column| nullability.get(column).copied().unwrap_or(false))
                .collect::<Vec<_>>();

            Ok(QueryOutput::Rows(ResultSet {
                columns,
                column_kinds,
                column_nullable,
                rows: data_rows,
            }))
        }
        DatabasePool::Sqlite(pool) => {
            let trimmed = query.trim();
            let database = database.unwrap_or_else(|| String::from("main"));
            if !query_returns_rows(trimmed) {
                return Err(String::from("Query did not return rows."));
            }

            let mut conn = pool.acquire().await.map_err(|error| error.to_string())?;

            let (columns, column_kinds, data_rows) =
                fetch_sqlite_result_rows(&mut conn, trimmed, cancel_flag.as_ref(), None).await?;

            let nullability = fetch_table_column_nullability(
                DatabasePool::Sqlite(pool),
                database.trim(),
                table.trim(),
            )
            .await?;

            let column_nullable = columns
                .iter()
                .map(|column| nullability.get(column).copied().unwrap_or(false))
                .collect::<Vec<_>>();

            Ok(QueryOutput::Rows(ResultSet {
                columns,
                column_kinds,
                column_nullable,
                rows: data_rows,
            }))
        }
        DatabasePool::Postgres(pool) => {
            let trimmed = query.trim();
            let database = database
                .filter(|name| !name.trim().is_empty())
                .ok_or_else(|| String::from("Select a database to load tables."))?;

            if !query_returns_rows(trimmed) {
                return Err(String::from("Query did not return rows."));
            }

            let mut conn = pool.acquire().await.map_err(|error| error.to_string())?;
            ensure_postgres_current_database(&mut conn, Some(database.clone())).await?;

            let (columns, column_kinds, data_rows) =
                fetch_postgres_result_rows(&mut conn, trimmed, cancel_flag.as_ref(), None).await?;

            let nullability = fetch_table_column_nullability(
                DatabasePool::Postgres(pool),
                database.trim(),
                table.trim(),
            )
            .await?;

            let column_nullable = columns
                .iter()
                .map(|column| nullability.get(column).copied().unwrap_or(false))
                .collect::<Vec<_>>();

            Ok(QueryOutput::Rows(ResultSet {
                columns,
                column_kinds,
                column_nullable,
                rows: data_rows,
            }))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{query_returns_rows, run_multi_query_with_control, run_query_with_control};
    use crate::{DatabasePool, QueryOutput};
    use std::sync::Arc;
    use std::sync::atomic::AtomicBool;

    async fn sqlite_test_pool() -> (tempfile::TempDir, sqlx::SqlitePool) {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let path = temp_dir.path().join("query-tests.db");
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(
                sqlx::sqlite::SqliteConnectOptions::new()
                    .filename(&path)
                    .create_if_missing(true),
            )
            .await
            .expect("open sqlite pool");

        sqlx::query(
            "CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT NOT NULL);\
            INSERT INTO users (name) VALUES ('Ada'), ('Grace'), ('Linus');",
        )
        .execute(&pool)
        .await
        .expect("seed sqlite rows");

        (temp_dir, pool)
    }

    #[test]
    fn query_returns_rows_handles_postgres_keywords_and_returning() {
        assert!(query_returns_rows("SELECT 1"));
        assert!(query_returns_rows("VALUES (1), (2)"));
        assert!(query_returns_rows("TABLE public.users"));
        assert!(query_returns_rows(
            "INSERT INTO users(name) VALUES ('a') RETURNING id"
        ));
        assert!(query_returns_rows(
            "UPDATE users SET name = 'b' WHERE id = 1 RETURNING *"
        ));
        assert!(query_returns_rows(
            "DELETE FROM users WHERE id = 1 RETURNING id"
        ));
        assert!(!query_returns_rows("UPDATE users SET name = 'b'"));
        assert!(!query_returns_rows("DELETE FROM users WHERE id = 1"));
    }

    #[test]
    fn run_query_with_control_sqlite_honors_row_cap() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("tokio runtime");

        runtime.block_on(async {
            let (_temp_dir, pool) = sqlite_test_pool().await;

            let output = run_query_with_control(
                DatabasePool::Sqlite(pool.clone()),
                None,
                String::from("SELECT id, name FROM users ORDER BY id"),
                None,
                Some(2),
            )
            .await
            .expect("query succeeds");

            match output {
                QueryOutput::Rows(result) => {
                    assert_eq!(
                        result.columns,
                        vec![String::from("id"), String::from("name")]
                    );
                    assert_eq!(result.rows.len(), 2);
                    assert_eq!(result.rows[0], vec![String::from("1"), String::from("Ada")]);
                    assert_eq!(
                        result.rows[1],
                        vec![String::from("2"), String::from("Grace")]
                    );
                }
                _ => panic!("expected query rows output"),
            }

            pool.close().await;
        });
    }

    #[test]
    fn run_multi_query_with_control_sqlite_keeps_each_result_set() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("tokio runtime");

        runtime.block_on(async {
            let (_temp_dir, pool) = sqlite_test_pool().await;
            let outputs = run_multi_query_with_control(
                DatabasePool::Sqlite(pool.clone()),
                None,
                String::from(
                    "SELECT name FROM users WHERE id = 1; SELECT name FROM users WHERE id = 2;",
                ),
                None,
                None,
            )
            .await
            .expect("query succeeds");

            assert_eq!(outputs.len(), 2);
            let QueryOutput::Rows(first) = &outputs[0] else {
                panic!("expected first result set");
            };
            let QueryOutput::Rows(second) = &outputs[1] else {
                panic!("expected second result set");
            };
            assert_eq!(first.rows[0][0], "Ada");
            assert_eq!(second.rows[0][0], "Grace");

            pool.close().await;
        });
    }

    #[test]
    fn run_query_with_control_sqlite_reports_cancellation() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("tokio runtime");

        runtime.block_on(async {
            let (_temp_dir, pool) = sqlite_test_pool().await;
            let cancel_flag = Arc::new(AtomicBool::new(true));

            let error = run_query_with_control(
                DatabasePool::Sqlite(pool.clone()),
                None,
                String::from("SELECT id, name FROM users ORDER BY id"),
                Some(cancel_flag),
                None,
            )
            .await
            .expect_err("query should be cancelled");

            assert_eq!(error, "Query cancelled.");
            pool.close().await;
        });
    }
}
