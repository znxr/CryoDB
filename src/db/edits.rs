use crate::db::codecs::{coerce_sql_cell_value, sql_cell_function_expression};
use crate::db::metadata::{fetch_primary_keys, fetch_table_column_nullability};
use crate::utils::helpers::{
    escape_mysql_identifier, escape_sqlite_identifier, escape_sqlite_string_literal, is_null_value,
    sql_bind_placeholder, sql_quote_identifier, sql_quote_identifier_path,
};
use crate::{
    ColumnKind, DatabaseDriver, DatabasePool, SchemaChange, SchemaScriptError, TableCommand,
    foreign_key_name,
};
use futures_util::TryStreamExt;
use sqlx::Executor;
use sqlx::PgPool;
use sqlx::Row;
use sqlx::SqliteConnection;
use sqlx::{Acquire, AssertSqlSafe};
use std::collections::HashMap;

fn postgres_table_reference_parts(table: &str) -> (Option<String>, String) {
    let trimmed = table.trim();
    if let Some((schema, name)) = trimmed.split_once('.') {
        let schema = schema.trim().trim_matches('"').to_string();
        let name = name.trim().trim_matches('"').to_string();
        if !schema.is_empty() && !name.is_empty() {
            return (Some(schema), name);
        }
    }

    (None, trimmed.trim_matches('"').to_string())
}

fn postgres_qualified_table_name(table: &str) -> Result<String, String> {
    let (schema, table_name) = postgres_table_reference_parts(table);
    if table_name.trim().is_empty() {
        return Err(String::from("Table name is empty."));
    }

    Ok(match schema {
        Some(schema) => {
            sql_quote_identifier_path(DatabaseDriver::PostgreSql, &[&schema, &table_name])
        }
        None => sql_quote_identifier(DatabaseDriver::PostgreSql, &table_name),
    })
}

fn postgres_placeholder_expr(kind: ColumnKind, placeholder: &str) -> String {
    match kind {
        ColumnKind::Integer | ColumnKind::Unsigned => format!("CAST({placeholder} AS BIGINT)"),
        ColumnKind::Float => format!("CAST({placeholder} AS DOUBLE PRECISION)"),
        ColumnKind::Decimal => format!("CAST({placeholder} AS NUMERIC)"),
        ColumnKind::Bool => format!("CAST({placeholder} AS BOOLEAN)"),
        ColumnKind::DateTime => format!("CAST({placeholder} AS TIMESTAMPTZ)"),
        ColumnKind::Date => format!("CAST({placeholder} AS DATE)"),
        ColumnKind::Time => format!("CAST({placeholder} AS TIME)"),
        ColumnKind::Binary => format!("CAST({placeholder} AS BYTEA)"),
        ColumnKind::Text | ColumnKind::Unknown => placeholder.to_string(),
    }
}

fn postgres_placeholder_expr_with_type(
    kind: ColumnKind,
    placeholder: &str,
    udt_name: Option<&str>,
) -> String {
    let udt_name = udt_name
        .map(|value| value.trim().trim_matches('"').to_ascii_lowercase())
        .unwrap_or_default();

    if !udt_name.is_empty() {
        if matches!(udt_name.as_str(), "int2" | "int4" | "int8") {
            return format!("CAST({placeholder} AS BIGINT)");
        }
        if matches!(udt_name.as_str(), "float4" | "float8") {
            return format!("CAST({placeholder} AS DOUBLE PRECISION)");
        }
        if matches!(udt_name.as_str(), "numeric" | "money") {
            return format!("CAST({placeholder} AS NUMERIC)");
        }
        if udt_name == "bool" {
            return format!("CAST({placeholder} AS BOOLEAN)");
        }
        if matches!(
            udt_name.as_str(),
            "date" | "time" | "timetz" | "timestamp" | "timestamptz"
        ) {
            return format!("CAST({placeholder} AS {udt_name})");
        }
        if matches!(
            udt_name.as_str(),
            "vector" | "halfvec" | "sparsevec" | "json" | "jsonb" | "uuid" | "bytea"
        ) {
            return format!("CAST({placeholder} AS {udt_name})");
        }
        if udt_name.starts_with('_') {
            return format!("CAST({placeholder} AS {udt_name})");
        }
    }

    postgres_placeholder_expr(kind, placeholder)
}

async fn ensure_postgres_current_database(pool: &PgPool, database: &str) -> Result<(), String> {
    if database.trim().is_empty() {
        return Err(String::from("Select a database first."));
    }

    let current_database: String = sqlx::query_scalar("SELECT current_database()")
        .fetch_one(pool)
        .await
        .map_err(|error| error.to_string())?;

    if current_database != database.trim() {
        return Err(format!(
            "Connected to PostgreSQL database `{current_database}`. Switch database by reconnecting for now."
        ));
    }

    Ok(())
}

async fn fetch_postgres_column_udt_names(
    pool: &PgPool,
    table: &str,
) -> Result<HashMap<String, String>, String> {
    let (schema, table_name) = postgres_table_reference_parts(table);
    if table_name.trim().is_empty() {
        return Ok(HashMap::new());
    }

    let mut rows = sqlx::query(
        "SELECT column_name, udt_name \
        FROM information_schema.columns \
        WHERE table_schema = COALESCE($1::text, current_schema()) \
        AND table_name = $2",
    )
    .bind(schema)
    .bind(table_name)
    .fetch(pool);

    let mut map = HashMap::new();
    while let Some(row) = rows.try_next().await.map_err(|error| error.to_string())? {
        let column: String = row.try_get(0).map_err(|error| error.to_string())?;
        let udt_name: String = row.try_get(1).map_err(|error| error.to_string())?;
        map.insert(column, udt_name);
    }

    Ok(map)
}

pub(crate) async fn apply_table_changes(
    pool: DatabasePool,
    database: String,
    table: String,
    columns: Vec<String>,
    column_kinds: Vec<ColumnKind>,
    rows: HashMap<usize, Vec<String>>,
    edits: HashMap<(usize, usize), String>,
) -> Result<usize, String> {
    if edits.is_empty() {
        return Ok(0);
    }

    match pool {
        DatabasePool::MySql(pool) => {
            let primary_keys =
                fetch_primary_keys(DatabasePool::MySql(pool.clone()), &database, &table).await?;
            if primary_keys.is_empty() {
                return Err(String::from("Cannot apply changes without a primary key."));
            }

            let column_nullability = fetch_table_column_nullability(
                DatabasePool::MySql(pool.clone()),
                &database,
                &table,
            )
            .await?;

            let mut column_indices = HashMap::new();
            for (index, name) in columns.iter().enumerate() {
                column_indices.insert(name.clone(), index);
            }

            let mut pk_indices = Vec::new();
            for key in &primary_keys {
                let Some(index) = column_indices.get(key).copied() else {
                    return Err(format!(
                        "Primary key column `{}` not present in results.",
                        key
                    ));
                };
                pk_indices.push((key.clone(), index));
            }

            let mut changes_by_row: HashMap<usize, Vec<(usize, String)>> = HashMap::new();
            for ((row, column), value) in edits {
                changes_by_row.entry(row).or_default().push((column, value));
            }

            let mut total_applied = 0usize;
            let database_name = database.trim();
            let table_name = if database_name.is_empty() {
                format!("`{}`", escape_mysql_identifier(&table))
            } else {
                format!(
                    "`{}`.`{}`",
                    escape_mysql_identifier(database_name),
                    escape_mysql_identifier(&table)
                )
            };

            for (row_index, changes) in changes_by_row {
                let Some(original_row) = rows.get(&row_index) else {
                    continue;
                };

                let mut set_clauses = Vec::new();
                let mut values: Vec<(String, bool)> = Vec::new();
                let mut bind_index = 1usize;

                for (column_index, value) in changes {
                    let Some(column_name) = columns.get(column_index) else {
                        continue;
                    };

                    if let Some(expression) = sql_cell_function_expression(&value) {
                        set_clauses.push(format!(
                            "`{}` = {}",
                            escape_mysql_identifier(column_name),
                            expression
                        ));
                    } else {
                        let placeholder = sql_bind_placeholder(DatabaseDriver::MySql, bind_index);
                        bind_index = bind_index.saturating_add(1);
                        set_clauses.push(format!(
                            "`{}` = {}",
                            escape_mysql_identifier(column_name),
                            placeholder
                        ));
                        let kind = column_kinds
                            .get(column_index)
                            .copied()
                            .unwrap_or(ColumnKind::Unknown);
                        let is_nullable = column_nullability
                            .get(column_name)
                            .copied()
                            .unwrap_or(false);
                        values.push((coerce_sql_cell_value(value, kind), is_nullable));
                    }
                }

                if set_clauses.is_empty() {
                    continue;
                }

                let mut where_clauses = Vec::new();
                for (pk_name, pk_index) in &pk_indices {
                    let placeholder = sql_bind_placeholder(DatabaseDriver::MySql, bind_index);
                    bind_index = bind_index.saturating_add(1);
                    where_clauses.push(format!(
                        "`{}` = {}",
                        escape_mysql_identifier(pk_name),
                        placeholder
                    ));
                    let pk_value = original_row
                        .get(*pk_index)
                        .cloned()
                        .unwrap_or_else(|| String::from("NULL"));
                    values.push((pk_value, false));
                }

                let sql = format!(
                    "UPDATE {} SET {} WHERE {} LIMIT 1",
                    table_name,
                    set_clauses.join(", "),
                    where_clauses.join(" AND ")
                );

                let mut query = sqlx::query(AssertSqlSafe(sql.as_str()));
                for (value, empty_is_null) in values {
                    if is_null_value(&value) || (empty_is_null && value.trim().is_empty()) {
                        query = query.bind::<Option<String>>(None);
                    } else {
                        query = query.bind(Some(value));
                    }
                }

                let result = query
                    .execute(&pool)
                    .await
                    .map_err(|error| error.to_string())?;

                total_applied += result.rows_affected() as usize;
            }

            Ok(total_applied)
        }
        DatabasePool::Sqlite(pool) => {
            let primary_keys =
                fetch_primary_keys(DatabasePool::Sqlite(pool.clone()), &database, &table).await?;
            if primary_keys.is_empty() {
                return Err(String::from("Cannot apply changes without a primary key."));
            }

            let column_nullability = fetch_table_column_nullability(
                DatabasePool::Sqlite(pool.clone()),
                &database,
                &table,
            )
            .await?;

            let mut column_indices = HashMap::new();
            for (index, name) in columns.iter().enumerate() {
                column_indices.insert(name.clone(), index);
            }

            let mut pk_indices = Vec::new();
            for key in &primary_keys {
                let Some(index) = column_indices.get(key).copied() else {
                    return Err(format!(
                        "Primary key column `{}` not present in results.",
                        key
                    ));
                };
                pk_indices.push((key.clone(), index));
            }

            let mut changes_by_row: HashMap<usize, Vec<(usize, String)>> = HashMap::new();
            for ((row, column), value) in edits {
                changes_by_row.entry(row).or_default().push((column, value));
            }

            let mut total_applied = 0usize;
            let database_name = if database.trim().is_empty() {
                String::from("main")
            } else {
                database.trim().to_string()
            };
            let table_name = format!(
                "\"{}\".\"{}\"",
                escape_sqlite_identifier(&database_name),
                escape_sqlite_identifier(table.trim())
            );

            for (row_index, changes) in changes_by_row {
                let Some(original_row) = rows.get(&row_index) else {
                    continue;
                };

                let mut set_clauses = Vec::new();
                let mut values: Vec<(String, bool)> = Vec::new();
                let mut bind_index = 1usize;

                for (column_index, value) in changes {
                    let Some(column_name) = columns.get(column_index) else {
                        continue;
                    };

                    if let Some(expression) = sql_cell_function_expression(&value) {
                        set_clauses.push(format!(
                            "\"{}\" = {}",
                            escape_sqlite_identifier(column_name),
                            expression
                        ));
                    } else {
                        let placeholder = sql_bind_placeholder(DatabaseDriver::Sqlite, bind_index);
                        bind_index = bind_index.saturating_add(1);
                        set_clauses.push(format!(
                            "\"{}\" = {}",
                            escape_sqlite_identifier(column_name),
                            placeholder
                        ));
                        let kind = column_kinds
                            .get(column_index)
                            .copied()
                            .unwrap_or(ColumnKind::Unknown);
                        let is_nullable = column_nullability
                            .get(column_name)
                            .copied()
                            .unwrap_or(false);
                        values.push((coerce_sql_cell_value(value, kind), is_nullable));
                    }
                }

                if set_clauses.is_empty() {
                    continue;
                }

                let mut where_clauses = Vec::new();
                for (pk_name, pk_index) in &pk_indices {
                    let placeholder = sql_bind_placeholder(DatabaseDriver::Sqlite, bind_index);
                    bind_index = bind_index.saturating_add(1);
                    where_clauses.push(format!(
                        "\"{}\" = {}",
                        escape_sqlite_identifier(pk_name),
                        placeholder
                    ));
                    let pk_value = original_row
                        .get(*pk_index)
                        .cloned()
                        .unwrap_or_else(|| String::from("NULL"));
                    values.push((pk_value, false));
                }

                let sql = format!(
                    "UPDATE {} SET {} WHERE {}",
                    table_name,
                    set_clauses.join(", "),
                    where_clauses.join(" AND ")
                );

                let mut query = sqlx::query(AssertSqlSafe(sql.as_str()));
                for (value, empty_is_null) in values {
                    if is_null_value(&value) || (empty_is_null && value.trim().is_empty()) {
                        query = query.bind::<Option<String>>(None);
                    } else {
                        query = query.bind(Some(value));
                    }
                }

                let result = query
                    .execute(&pool)
                    .await
                    .map_err(|error| error.to_string())?;

                total_applied += result.rows_affected() as usize;
            }

            Ok(total_applied)
        }
        DatabasePool::Postgres(pool) => {
            ensure_postgres_current_database(&pool, &database).await?;
            let primary_keys =
                fetch_primary_keys(DatabasePool::Postgres(pool.clone()), &database, &table).await?;
            if primary_keys.is_empty() {
                return Err(String::from("Cannot apply changes without a primary key."));
            }

            let column_nullability = fetch_table_column_nullability(
                DatabasePool::Postgres(pool.clone()),
                &database,
                &table,
            )
            .await?;

            let column_udt_names = fetch_postgres_column_udt_names(&pool, &table).await?;

            let mut column_indices = HashMap::new();
            for (index, name) in columns.iter().enumerate() {
                column_indices.insert(name.clone(), index);
            }

            let mut pk_indices = Vec::new();
            for key in &primary_keys {
                let Some(index) = column_indices.get(key).copied() else {
                    return Err(format!(
                        "Primary key column `{}` not present in results.",
                        key
                    ));
                };
                pk_indices.push((key.clone(), index));
            }

            let mut changes_by_row: HashMap<usize, Vec<(usize, String)>> = HashMap::new();
            for ((row, column), value) in edits {
                changes_by_row.entry(row).or_default().push((column, value));
            }

            let mut total_applied = 0usize;
            let table_name = postgres_qualified_table_name(&table)?;

            for (row_index, changes) in changes_by_row {
                let Some(original_row) = rows.get(&row_index) else {
                    continue;
                };

                let mut set_clauses = Vec::new();
                let mut values: Vec<(String, bool)> = Vec::new();
                let mut bind_index = 1usize;

                for (column_index, value) in changes {
                    let Some(column_name) = columns.get(column_index) else {
                        continue;
                    };
                    let quoted_column =
                        sql_quote_identifier(DatabaseDriver::PostgreSql, column_name);

                    if let Some(expression) = sql_cell_function_expression(&value) {
                        set_clauses.push(format!("{} = {}", quoted_column, expression));
                    } else {
                        let placeholder =
                            sql_bind_placeholder(DatabaseDriver::PostgreSql, bind_index);
                        bind_index = bind_index.saturating_add(1);
                        let kind = column_kinds
                            .get(column_index)
                            .copied()
                            .unwrap_or(ColumnKind::Unknown);
                        let udt_name = column_udt_names.get(column_name).map(String::as_str);
                        let value_expr =
                            postgres_placeholder_expr_with_type(kind, &placeholder, udt_name);
                        set_clauses.push(format!("{} = {}", quoted_column, value_expr));

                        let is_nullable = column_nullability
                            .get(column_name)
                            .copied()
                            .unwrap_or(false);
                        values.push((coerce_sql_cell_value(value, kind), is_nullable));
                    }
                }

                if set_clauses.is_empty() {
                    continue;
                }

                let mut where_clauses = Vec::new();
                for (pk_name, pk_index) in &pk_indices {
                    let placeholder = sql_bind_placeholder(DatabaseDriver::PostgreSql, bind_index);
                    bind_index = bind_index.saturating_add(1);

                    let quoted_pk = sql_quote_identifier(DatabaseDriver::PostgreSql, pk_name);
                    let kind = column_kinds
                        .get(*pk_index)
                        .copied()
                        .unwrap_or(ColumnKind::Unknown);
                    let udt_name = column_udt_names.get(pk_name).map(String::as_str);
                    let value_expr =
                        postgres_placeholder_expr_with_type(kind, &placeholder, udt_name);
                    where_clauses.push(format!("{} = {}", quoted_pk, value_expr));

                    let pk_value = original_row
                        .get(*pk_index)
                        .cloned()
                        .unwrap_or_else(|| String::from("NULL"));
                    values.push((pk_value, false));
                }

                let sql = format!(
                    "UPDATE {} SET {} WHERE {}",
                    table_name,
                    set_clauses.join(", "),
                    where_clauses.join(" AND ")
                );

                let mut query = sqlx::query(AssertSqlSafe(sql.as_str()));
                for (value, empty_is_null) in values {
                    if is_null_value(&value) || (empty_is_null && value.trim().is_empty()) {
                        query = query.bind::<Option<String>>(None);
                    } else {
                        query = query.bind(Some(value));
                    }
                }

                let result = query
                    .execute(&pool)
                    .await
                    .map_err(|error| error.to_string())?;

                total_applied += result.rows_affected() as usize;
            }

            Ok(total_applied)
        }
    }
}

pub(crate) async fn delete_table_rows(
    pool: DatabasePool,
    database: String,
    table: String,
    columns: Vec<String>,
    rows: Vec<Vec<String>>,
) -> Result<usize, String> {
    if rows.is_empty() {
        return Ok(0);
    }

    match pool {
        DatabasePool::MySql(pool) => {
            let primary_keys =
                fetch_primary_keys(DatabasePool::MySql(pool.clone()), &database, &table).await?;
            if primary_keys.is_empty() {
                return Err(String::from("Cannot delete rows without a primary key."));
            }

            let mut column_indices = HashMap::new();
            for (index, name) in columns.iter().enumerate() {
                column_indices.insert(name.clone(), index);
            }

            let mut pk_indices = Vec::new();
            for key in &primary_keys {
                let Some(index) = column_indices.get(key).copied() else {
                    return Err(format!(
                        "Primary key column `{}` not present in results.",
                        key
                    ));
                };
                pk_indices.push((key.clone(), index));
            }

            let database_name = database.trim();
            let table_name = if database_name.is_empty() {
                format!("`{}`", escape_mysql_identifier(&table))
            } else {
                format!(
                    "`{}`.`{}`",
                    escape_mysql_identifier(database_name),
                    escape_mysql_identifier(&table)
                )
            };

            let where_clause = pk_indices
                .iter()
                .enumerate()
                .map(|(offset, (name, _))| {
                    let placeholder = sql_bind_placeholder(DatabaseDriver::MySql, offset + 1);
                    format!("`{}` = {}", escape_mysql_identifier(name), placeholder)
                })
                .collect::<Vec<_>>()
                .join(" AND ");

            let sql = format!("DELETE FROM {} WHERE {} LIMIT 1", table_name, where_clause);

            let mut total_deleted = 0usize;

            for row in rows {
                let mut query = sqlx::query(AssertSqlSafe(sql.as_str()));
                for (_, index) in &pk_indices {
                    let value = row
                        .get(*index)
                        .cloned()
                        .unwrap_or_else(|| String::from("NULL"));
                    if is_null_value(&value) {
                        query = query.bind::<Option<String>>(None);
                    } else {
                        query = query.bind(Some(value));
                    }
                }

                let result = query
                    .execute(&pool)
                    .await
                    .map_err(|error| error.to_string())?;
                total_deleted += result.rows_affected() as usize;
            }

            Ok(total_deleted)
        }
        DatabasePool::Sqlite(pool) => {
            let primary_keys =
                fetch_primary_keys(DatabasePool::Sqlite(pool.clone()), &database, &table).await?;
            if primary_keys.is_empty() {
                return Err(String::from("Cannot delete rows without a primary key."));
            }

            let mut column_indices = HashMap::new();
            for (index, name) in columns.iter().enumerate() {
                column_indices.insert(name.clone(), index);
            }

            let mut pk_indices = Vec::new();
            for key in &primary_keys {
                let Some(index) = column_indices.get(key).copied() else {
                    return Err(format!(
                        "Primary key column `{}` not present in results.",
                        key
                    ));
                };
                pk_indices.push((key.clone(), index));
            }

            let database_name = if database.trim().is_empty() {
                String::from("main")
            } else {
                database.trim().to_string()
            };
            let table_name = format!(
                "\"{}\".\"{}\"",
                escape_sqlite_identifier(&database_name),
                escape_sqlite_identifier(table.trim())
            );

            let where_clause = pk_indices
                .iter()
                .enumerate()
                .map(|(offset, (name, _))| {
                    let placeholder = sql_bind_placeholder(DatabaseDriver::Sqlite, offset + 1);
                    format!("\"{}\" = {}", escape_sqlite_identifier(name), placeholder)
                })
                .collect::<Vec<_>>()
                .join(" AND ");

            let sql = format!("DELETE FROM {} WHERE {}", table_name, where_clause);

            let mut total_deleted = 0usize;
            for row in rows {
                let mut query = sqlx::query(AssertSqlSafe(sql.as_str()));
                for (_, index) in &pk_indices {
                    let value = row
                        .get(*index)
                        .cloned()
                        .unwrap_or_else(|| String::from("NULL"));
                    if is_null_value(&value) {
                        query = query.bind::<Option<String>>(None);
                    } else {
                        query = query.bind(Some(value));
                    }
                }

                let result = query
                    .execute(&pool)
                    .await
                    .map_err(|error| error.to_string())?;
                total_deleted += result.rows_affected() as usize;
            }

            Ok(total_deleted)
        }
        DatabasePool::Postgres(pool) => {
            ensure_postgres_current_database(&pool, &database).await?;
            let primary_keys =
                fetch_primary_keys(DatabasePool::Postgres(pool.clone()), &database, &table).await?;
            if primary_keys.is_empty() {
                return Err(String::from("Cannot delete rows without a primary key."));
            }

            let column_udt_names = fetch_postgres_column_udt_names(&pool, &table).await?;

            let mut column_indices = HashMap::new();
            for (index, name) in columns.iter().enumerate() {
                column_indices.insert(name.clone(), index);
            }

            let mut pk_indices = Vec::new();
            for key in &primary_keys {
                let Some(index) = column_indices.get(key).copied() else {
                    return Err(format!(
                        "Primary key column `{}` not present in results.",
                        key
                    ));
                };
                pk_indices.push((key.clone(), index));
            }

            let table_name = postgres_qualified_table_name(&table)?;

            let where_clause = pk_indices
                .iter()
                .enumerate()
                .map(|(offset, (name, _index))| {
                    let placeholder = sql_bind_placeholder(DatabaseDriver::PostgreSql, offset + 1);
                    let quoted_name = sql_quote_identifier(DatabaseDriver::PostgreSql, name);
                    let udt_name = column_udt_names.get(name).map(String::as_str);
                    let value_expr = postgres_placeholder_expr_with_type(
                        ColumnKind::Unknown,
                        &placeholder,
                        udt_name,
                    );
                    format!("{} = {}", quoted_name, value_expr)
                })
                .collect::<Vec<_>>()
                .join(" AND ");

            let sql = format!("DELETE FROM {} WHERE {}", table_name, where_clause);

            let mut total_deleted = 0usize;
            for row in rows {
                let mut query = sqlx::query(AssertSqlSafe(sql.as_str()));
                for (_, index) in &pk_indices {
                    let value = row
                        .get(*index)
                        .cloned()
                        .unwrap_or_else(|| String::from("NULL"));
                    if is_null_value(&value) {
                        query = query.bind::<Option<String>>(None);
                    } else {
                        query = query.bind(Some(value));
                    }
                }

                let result = query
                    .execute(&pool)
                    .await
                    .map_err(|error| error.to_string())?;
                total_deleted += result.rows_affected() as usize;
            }

            Ok(total_deleted)
        }
    }
}

pub(crate) fn split_schema_script(script: &str) -> Vec<String> {
    let mut statements = Vec::new();
    let mut current = String::new();
    for line in script.lines() {
        if line.trim_start().starts_with("--") {
            continue;
        }
        current.push_str(line);
        current.push('\n');
        if line.trim_end().ends_with(';') {
            let statement = current.trim().trim_end_matches(';').trim().to_string();
            if !statement.is_empty() {
                statements.push(statement);
            }
            current.clear();
        }
    }
    let tail = current.trim().trim_end_matches(';').trim().to_string();
    if !tail.is_empty() {
        statements.push(tail);
    }
    statements
}

fn validate_sqlite_schema_script(version: &str, statements: &[String]) -> Result<(), String> {
    let mut parts = version
        .split('.')
        .take(2)
        .map(|part| part.parse::<u32>().unwrap_or(0));
    let supported = (parts.next().unwrap_or(0), parts.next().unwrap_or(0)) >= (3, 35);
    if !supported
        && statements.iter().any(|statement| {
            let sql = statement.to_ascii_uppercase();
            sql.starts_with("ALTER TABLE ") && sql.contains(" DROP COLUMN ")
        })
    {
        return Err(format!(
            "Dropping a column requires SQLite 3.35 or newer; this connection uses SQLite {version}."
        ));
    }
    Ok(())
}

fn sqlite_table_body(sql: &str) -> Result<(usize, usize), String> {
    let bytes = sql.as_bytes();
    let mut quote = 0;
    let mut depth = 0;
    let mut open = None;
    let mut index = 0;
    while index < bytes.len() {
        let byte = bytes[index];
        if quote != 0 {
            let end = if quote == b'[' { b']' } else { quote };
            if byte == end {
                if bytes.get(index + 1) == Some(&end) {
                    index += 2;
                    continue;
                }
                quote = 0;
            }
            index += 1;
            continue;
        }
        if matches!(byte, b'\'' | b'"' | b'`' | b'[') {
            quote = byte;
        } else if byte == b'-' && bytes.get(index + 1) == Some(&b'-') {
            index += 2;
            while index < bytes.len() && bytes[index] != b'\n' {
                index += 1;
            }
            continue;
        } else if byte == b'/' && bytes.get(index + 1) == Some(&b'*') {
            index += 2;
            while index + 1 < bytes.len() && !(bytes[index] == b'*' && bytes[index + 1] == b'/') {
                index += 1;
            }
            index += 2;
            continue;
        } else if byte == b'(' {
            if open.is_none() {
                open = Some(index);
            }
            depth += 1;
        } else if byte == b')' && depth > 0 {
            depth -= 1;
            if depth == 0 {
                return Ok((open.unwrap_or(index), index));
            }
        }
        index += 1;
    }
    Err(String::from(
        "Could not parse the stored SQLite table definition.",
    ))
}

fn sqlite_add_foreign_key_ddl(
    ddl: &str,
    schema: &str,
    temporary_table: &str,
    table: &str,
    columns: &[String],
    referenced_table: &str,
    referenced_columns: &[String],
) -> Result<String, String> {
    let (open, close) = sqlite_table_body(ddl)?;
    let body = &ddl[open + 1..close];
    let comma = if body.trim().is_empty() { "" } else { "," };
    let list = |names: &[String]| {
        names
            .iter()
            .map(|name| sql_quote_identifier(DatabaseDriver::Sqlite, name))
            .collect::<Vec<_>>()
            .join(", ")
    };
    Ok(format!(
        "CREATE TABLE {}.{} ({}{}\n  CONSTRAINT {} FOREIGN KEY ({}) REFERENCES {} ({})\n){}",
        sql_quote_identifier(DatabaseDriver::Sqlite, schema),
        sql_quote_identifier(DatabaseDriver::Sqlite, temporary_table),
        body,
        comma,
        sql_quote_identifier(DatabaseDriver::Sqlite, &foreign_key_name(table, columns)),
        list(columns),
        sql_quote_identifier(DatabaseDriver::Sqlite, referenced_table),
        list(referenced_columns),
        &ddl[close + 1..]
    ))
}

struct SqliteToken {
    text: String,
    start: usize,
    end: usize,
}

fn sqlite_tokens(sql: &str) -> Vec<SqliteToken> {
    let bytes = sql.as_bytes();
    let mut tokens = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index].is_ascii_whitespace() {
            index += 1;
            continue;
        }
        if bytes[index] == b'-' && bytes.get(index + 1) == Some(&b'-') {
            index += 2;
            while index < bytes.len() && bytes[index] != b'\n' {
                index += 1;
            }
            continue;
        }
        if bytes[index] == b'/' && bytes.get(index + 1) == Some(&b'*') {
            index += 2;
            while index + 1 < bytes.len() && !(bytes[index] == b'*' && bytes[index + 1] == b'/') {
                index += 1;
            }
            index = (index + 2).min(bytes.len());
            continue;
        }
        if bytes[index] == b'\'' {
            index += 1;
            while index < bytes.len() {
                if bytes[index] == b'\'' {
                    if bytes.get(index + 1) == Some(&b'\'') {
                        index += 2;
                        continue;
                    }
                    index += 1;
                    break;
                }
                index += 1;
            }
            continue;
        }
        let start = index;
        if matches!(bytes[index], b'"' | b'`' | b'[') {
            let opening = bytes[index];
            let closing = if opening == b'[' { b']' } else { opening };
            index += 1;
            let mut text = String::new();
            while index < bytes.len() {
                if bytes[index] == closing {
                    if bytes.get(index + 1) == Some(&closing) {
                        text.push(closing as char);
                        index += 2;
                        continue;
                    }
                    index += 1;
                    break;
                }
                let character = sql[index..].chars().next().unwrap_or_default();
                text.push(character);
                index += character.len_utf8();
            }
            tokens.push(SqliteToken {
                text,
                start,
                end: index,
            });
            continue;
        }
        if matches!(bytes[index], b'(' | b')' | b',' | b'.') {
            index += 1;
        } else {
            while index < bytes.len()
                && !bytes[index].is_ascii_whitespace()
                && !matches!(
                    bytes[index],
                    b'(' | b')' | b',' | b'.' | b'\'' | b'"' | b'`' | b'['
                )
            {
                index += 1;
            }
        }
        tokens.push(SqliteToken {
            text: sql[start..index].to_string(),
            start,
            end: index,
        });
    }
    tokens
}

fn sqlite_token_list(tokens: &[SqliteToken], open: usize) -> (Vec<&str>, usize) {
    let mut values = Vec::new();
    let mut depth = 0;
    let mut index = open;
    while index < tokens.len() {
        match tokens[index].text.as_str() {
            "(" => depth += 1,
            ")" => {
                depth -= 1;
                if depth == 0 {
                    return (values, index + 1);
                }
            }
            "," if depth == 1 => {}
            _ if depth == 1 => values.push(tokens[index].text.as_str()),
            _ => {}
        }
        index += 1;
    }
    (values, index)
}

fn sqlite_reference(tokens: &[SqliteToken]) -> Option<(usize, &str, Vec<&str>, usize)> {
    let reference = tokens
        .iter()
        .position(|token| token.text.eq_ignore_ascii_case("REFERENCES"))?;
    let mut index = reference + 1;
    let mut table = tokens.get(index)?.text.as_str();
    if tokens.get(index + 1).is_some_and(|token| token.text == ".") {
        index += 2;
        table = tokens.get(index)?.text.as_str();
    }
    index += 1;
    let (columns, mut end) = if tokens.get(index).is_some_and(|token| token.text == "(") {
        sqlite_token_list(tokens, index)
    } else {
        (Vec::new(), index)
    };
    while let Some(token) = tokens.get(end) {
        if token.text.eq_ignore_ascii_case("ON") && tokens.len() > end + 2 {
            end += 3;
            if tokens
                .get(end - 1)
                .is_some_and(|token| token.text.eq_ignore_ascii_case("SET"))
                || tokens
                    .get(end - 1)
                    .is_some_and(|token| token.text.eq_ignore_ascii_case("NO"))
            {
                end += 1;
            }
        } else if token.text.eq_ignore_ascii_case("MATCH") && tokens.len() > end + 1 {
            end += 2;
        } else if token.text.eq_ignore_ascii_case("DEFERRABLE")
            || (token.text.eq_ignore_ascii_case("NOT")
                && tokens
                    .get(end + 1)
                    .is_some_and(|token| token.text.eq_ignore_ascii_case("DEFERRABLE")))
        {
            end += if token.text.eq_ignore_ascii_case("NOT") {
                2
            } else {
                1
            };
            if tokens
                .get(end)
                .is_some_and(|token| token.text.eq_ignore_ascii_case("INITIALLY"))
            {
                end = (end + 2).min(tokens.len());
            }
        } else {
            break;
        }
    }
    Some((reference, table, columns, end))
}

fn sqlite_body_entries(body: &str) -> Vec<(usize, usize)> {
    let mut ranges = Vec::new();
    let mut start = 0;
    let mut depth = 0;
    for token in sqlite_tokens(body) {
        match token.text.as_str() {
            "(" => depth += 1,
            ")" => depth -= 1,
            "," if depth == 0 => {
                ranges.push((start, token.start));
                start = token.end;
            }
            _ => {}
        }
    }
    ranges.push((start, body.len()));
    ranges
}

fn sqlite_column_clause_start(text: &str) -> bool {
    [
        "CONSTRAINT",
        "PRIMARY",
        "NOT",
        "NULL",
        "UNIQUE",
        "CHECK",
        "DEFAULT",
        "COLLATE",
        "REFERENCES",
        "GENERATED",
        "AS",
        "AUTOINCREMENT",
    ]
    .iter()
    .any(|keyword| text.eq_ignore_ascii_case(keyword))
}

fn sqlite_alter_column_ddl(
    ddl: &str,
    schema: &str,
    temporary_table: &str,
    column: &str,
    data_type: &str,
    nullable: bool,
    default: Option<&str>,
) -> Result<String, String> {
    let (open, close) = sqlite_table_body(ddl)?;
    let body = &ddl[open + 1..close];
    let mut found = false;
    let mut entries = Vec::new();
    for (start, end) in sqlite_body_entries(body) {
        let entry = body[start..end].trim();
        let tokens = sqlite_tokens(entry);
        if found
            || !tokens
                .first()
                .is_some_and(|token| token.text.eq_ignore_ascii_case(column))
        {
            entries.push(entry.to_string());
            continue;
        }
        found = true;
        let mut clauses = Vec::new();
        let mut clause = None;
        let mut depth = 0;
        for (index, token) in tokens.iter().enumerate().skip(1) {
            match token.text.as_str() {
                "(" => depth += 1,
                ")" => depth -= 1,
                _ if depth == 0 && sqlite_column_clause_start(&token.text) => {
                    if let Some(begin) = clause.replace(index) {
                        clauses.push((begin, index));
                    }
                }
                _ => {}
            }
        }
        if let Some(begin) = clause {
            clauses.push((begin, tokens.len()));
        }
        let mut definition = format!(
            "{} {}",
            sql_quote_identifier(DatabaseDriver::Sqlite, column),
            data_type.trim()
        );
        if !nullable {
            definition.push_str(" NOT NULL");
        }
        if let Some(value) = default {
            definition.push_str(&format!(" DEFAULT {value}"));
        }
        for (begin, end) in clauses {
            let text = &tokens[begin].text;
            if ["NOT", "NULL", "DEFAULT"]
                .iter()
                .any(|keyword| text.eq_ignore_ascii_case(keyword))
            {
                continue;
            }
            definition.push(' ');
            definition.push_str(&entry[tokens[begin].start..tokens[end - 1].end]);
        }
        entries.push(definition);
    }
    if !found {
        return Err(format!(
            "Could not find {column} in the stored SQLite definition."
        ));
    }
    Ok(format!(
        "CREATE TABLE {}.{} (\n{}\n){}",
        sql_quote_identifier(DatabaseDriver::Sqlite, schema),
        sql_quote_identifier(DatabaseDriver::Sqlite, temporary_table),
        entries.join(",\n"),
        &ddl[close + 1..]
    ))
}

fn sqlite_drop_foreign_key_ddl(
    ddl: &str,
    schema: &str,
    temporary_table: &str,
    column: &str,
    referenced_table: &str,
    referenced_column: &str,
) -> Result<String, String> {
    let (open, close) = sqlite_table_body(ddl)?;
    let body = &ddl[open + 1..close];
    let ranges = sqlite_body_entries(body);

    let mut found = false;
    let mut entries = Vec::new();
    for (start, end) in ranges {
        let entry = body[start..end].trim();
        let tokens = sqlite_tokens(entry);
        let Some((reference, target, referenced_columns, clause_end)) = sqlite_reference(&tokens)
        else {
            entries.push(entry.to_string());
            continue;
        };
        let target_matches = target.eq_ignore_ascii_case(referenced_table)
            && (referenced_columns.is_empty()
                || referenced_columns
                    .iter()
                    .any(|name| name.eq_ignore_ascii_case(referenced_column)));
        if found || !target_matches {
            entries.push(entry.to_string());
            continue;
        }
        if let Some(foreign) = tokens
            .iter()
            .position(|token| token.text.eq_ignore_ascii_case("FOREIGN"))
            && tokens
                .get(foreign + 1)
                .is_some_and(|token| token.text.eq_ignore_ascii_case("KEY"))
            && let Some(open) = tokens[foreign + 2..]
                .iter()
                .position(|token| token.text == "(")
                .map(|index| index + foreign + 2)
        {
            let (columns, _) = sqlite_token_list(&tokens, open);
            let pair_matches = columns
                .iter()
                .position(|name| name.eq_ignore_ascii_case(column))
                .is_some_and(|index| {
                    referenced_columns.is_empty()
                        || referenced_columns
                            .get(index)
                            .is_some_and(|name| name.eq_ignore_ascii_case(referenced_column))
                });
            if pair_matches {
                found = true;
                continue;
            }
        } else if tokens
            .first()
            .is_some_and(|token| token.text.eq_ignore_ascii_case(column))
        {
            let clause_start = tokens[reference].start;
            let clause_end = tokens
                .get(clause_end.saturating_sub(1))
                .map(|token| token.end)
                .unwrap_or(entry.len());
            entries.push(format!(
                "{}{}",
                entry[..clause_start].trim_end(),
                &entry[clause_end..]
            ));
            found = true;
            continue;
        }
        entries.push(entry.to_string());
    }
    if !found {
        return Err(String::from(
            "Could not find the selected foreign key in the stored SQLite definition.",
        ));
    }
    Ok(format!(
        "CREATE TABLE {}.{} (\n{}\n){}",
        sql_quote_identifier(DatabaseDriver::Sqlite, schema),
        sql_quote_identifier(DatabaseDriver::Sqlite, temporary_table),
        entries.join(",\n"),
        &ddl[close + 1..]
    ))
}

fn sqlite_schema_object_ddl(sql: &str, kind: &str, schema: &str) -> Result<String, String> {
    let upper = sql.to_ascii_uppercase();
    let keyword = kind.to_ascii_uppercase();
    let mut start = upper
        .find(&keyword)
        .map(|index| index + keyword.len())
        .ok_or_else(|| format!("Could not parse the stored SQLite {kind} definition."))?;
    while sql
        .as_bytes()
        .get(start)
        .is_some_and(u8::is_ascii_whitespace)
    {
        start += 1;
    }
    if upper[start..].starts_with("IF NOT EXISTS") {
        start += "IF NOT EXISTS".len();
        while sql
            .as_bytes()
            .get(start)
            .is_some_and(u8::is_ascii_whitespace)
        {
            start += 1;
        }
    }
    let quote = sql.as_bytes().get(start).copied().unwrap_or_default();
    let end = if matches!(quote, b'"' | b'`' | b'[') {
        let closing = if quote == b'[' { b']' } else { quote };
        let mut index = start + 1;
        loop {
            let Some(byte) = sql.as_bytes().get(index).copied() else {
                return Err(format!("Could not parse the stored SQLite {kind} name."));
            };
            if byte == closing {
                if sql.as_bytes().get(index + 1) == Some(&closing) {
                    index += 2;
                    continue;
                }
                break index + 1;
            }
            index += 1;
        }
    } else {
        sql[start..]
            .find(char::is_whitespace)
            .map(|length| start + length)
            .unwrap_or(sql.len())
    };
    Ok(format!(
        "{}{}.{}{}",
        &sql[..start],
        sql_quote_identifier(DatabaseDriver::Sqlite, schema),
        &sql[start..end],
        &sql[end..]
    ))
}

fn sqlite_qualified_statement(schema: &str, change: &SchemaChange, sql: &str) -> String {
    let table = match change {
        SchemaChange::CreateTable { table, .. }
        | SchemaChange::DropTable { table }
        | SchemaChange::RenameTable { table, .. }
        | SchemaChange::AddColumn { table, .. }
        | SchemaChange::DropColumn { table, .. } => table,
        _ => return sql.to_string(),
    };
    let table = sql_quote_identifier(DatabaseDriver::Sqlite, table);
    let qualified = format!(
        "{}.{}",
        sql_quote_identifier(DatabaseDriver::Sqlite, schema),
        table
    );
    sql.replacen(&table, &qualified, 1)
}

async fn sqlite_rebuild_table(
    conn: &mut SqliteConnection,
    schema: &str,
    table: &str,
    create: impl FnOnce(&str, &str) -> Result<String, String>,
) -> Result<(), String> {
    let schema_name = sql_quote_identifier(DatabaseDriver::Sqlite, schema);
    let table_name = sql_quote_identifier(DatabaseDriver::Sqlite, table);
    let temporary_table = format!("__cryodb_rebuild_{table}");
    let temporary_name = sql_quote_identifier(DatabaseDriver::Sqlite, &temporary_table);
    let master = format!("{schema_name}.sqlite_master");
    let exists: i64 = sqlx::query_scalar(AssertSqlSafe(format!(
        "SELECT EXISTS(SELECT 1 FROM {master} WHERE type = 'table' AND name = ?)"
    )))
    .bind(&temporary_table)
    .fetch_one(&mut *conn)
    .await
    .map_err(|error| error.to_string())?;
    if exists != 0 {
        return Err(format!(
            "Temporary table {temporary_table} already exists; rename or remove it first."
        ));
    }

    let ddl: String = sqlx::query_scalar(AssertSqlSafe(format!(
        "SELECT sql FROM {master} WHERE type = 'table' AND name = ? AND sql IS NOT NULL"
    )))
    .bind(table)
    .fetch_optional(&mut *conn)
    .await
    .map_err(|error| error.to_string())?
    .ok_or_else(|| format!("Could not load the stored definition for {table}."))?;

    let mut objects = Vec::new();
    let mut rows = sqlx::query(AssertSqlSafe(format!(
        "SELECT type, sql FROM {master} WHERE tbl_name = ? AND type IN ('index', 'trigger') AND sql IS NOT NULL ORDER BY type, name"
    )))
    .bind(table)
    .fetch(&mut *conn);
    while let Some(row) = rows.try_next().await.map_err(|error| error.to_string())? {
        let kind = row
            .try_get::<String, _>(0)
            .map_err(|error| error.to_string())?;
        let sql = row
            .try_get::<String, _>(1)
            .map_err(|error| error.to_string())?;
        objects.push(sqlite_schema_object_ddl(&sql, &kind, schema)?);
    }
    drop(rows);

    let pragma = format!(
        "PRAGMA {schema_name}.table_info('{}')",
        escape_sqlite_string_literal(table)
    );
    let mut rows = sqlx::query(AssertSqlSafe(pragma)).fetch(&mut *conn);
    let mut columns = Vec::new();
    while let Some(row) = rows.try_next().await.map_err(|error| error.to_string())? {
        columns.push(
            row.try_get::<String, _>(1)
                .map_err(|error| error.to_string())?,
        );
    }
    drop(rows);
    if columns.is_empty() {
        return Err(format!("Table {table} has no stored columns to copy."));
    }

    let create = create(&ddl, &temporary_table)?;
    (&mut *conn)
        .execute(AssertSqlSafe(create))
        .await
        .map_err(|error| error.to_string())?;
    let columns = columns
        .iter()
        .map(|column| sql_quote_identifier(DatabaseDriver::Sqlite, column))
        .collect::<Vec<_>>()
        .join(", ");
    (&mut *conn)
        .execute(AssertSqlSafe(format!(
            "INSERT INTO {schema_name}.{temporary_name} ({columns}) SELECT {columns} FROM {schema_name}.{table_name}"
        )))
        .await
        .map_err(|error| error.to_string())?;
    (&mut *conn)
        .execute(AssertSqlSafe(format!(
            "DROP TABLE {schema_name}.{table_name}"
        )))
        .await
        .map_err(|error| error.to_string())?;
    (&mut *conn)
        .execute(AssertSqlSafe(format!(
            "ALTER TABLE {schema_name}.{temporary_name} RENAME TO {table_name}"
        )))
        .await
        .map_err(|error| error.to_string())?;
    for sql in objects {
        (&mut *conn)
            .execute(AssertSqlSafe(sql))
            .await
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

pub(crate) async fn run_sqlite_schema_changes(
    pool: DatabasePool,
    database: String,
    changes: Vec<(SchemaChange, String)>,
) -> Result<usize, String> {
    let DatabasePool::Sqlite(pool) = pool else {
        return Err(String::from(
            "SQLite schema changes require a SQLite connection.",
        ));
    };
    if changes.is_empty() {
        return Err(String::from("Nothing to apply."));
    }
    let schema = if database.trim().is_empty() {
        String::from("main")
    } else {
        database.trim().to_string()
    };
    let statements = changes
        .iter()
        .flat_map(|(_, sql)| split_schema_script(sql))
        .collect::<Vec<_>>();
    let mut conn = pool.acquire().await.map_err(|error| error.to_string())?;
    let version: String = sqlx::query_scalar("SELECT sqlite_version()")
        .fetch_one(&mut *conn)
        .await
        .map_err(|error| error.to_string())?;
    validate_sqlite_schema_script(&version, &statements)?;
    (&mut *conn)
        .execute("PRAGMA foreign_keys = OFF")
        .await
        .map_err(|error| error.to_string())?;
    if let Err(error) = (&mut *conn).execute("BEGIN IMMEDIATE").await {
        let _ = (&mut *conn).execute("PRAGMA foreign_keys = ON").await;
        return Err(error.to_string());
    }

    let result = async {
        for (change, sql) in &changes {
            if let SchemaChange::AddForeignKey {
                table,
                columns,
                referenced_table,
                referenced_columns,
            } = change
            {
                sqlite_rebuild_table(&mut conn, &schema, table, |ddl, temporary_table| {
                    sqlite_add_foreign_key_ddl(
                        ddl,
                        &schema,
                        temporary_table,
                        table,
                        columns,
                        referenced_table,
                        referenced_columns,
                    )
                })
                .await?;
            } else if let SchemaChange::DropForeignKey {
                table,
                column,
                referenced_table,
                referenced_column,
                ..
            } = change
            {
                sqlite_rebuild_table(&mut conn, &schema, table, |ddl, temporary_table| {
                    sqlite_drop_foreign_key_ddl(
                        ddl,
                        &schema,
                        temporary_table,
                        column,
                        referenced_table,
                        referenced_column,
                    )
                })
                .await?;
            } else if let SchemaChange::AlterColumn {
                table,
                column,
                data_type,
                nullable,
                default,
                ..
            } = change
            {
                sqlite_rebuild_table(&mut conn, &schema, table, |ddl, temporary_table| {
                    sqlite_alter_column_ddl(
                        ddl,
                        &schema,
                        temporary_table,
                        column,
                        data_type,
                        *nullable,
                        default.as_deref(),
                    )
                })
                .await?;
            } else {
                for statement in split_schema_script(sql) {
                    let statement = sqlite_qualified_statement(&schema, change, &statement);
                    (&mut *conn)
                        .execute(AssertSqlSafe(statement))
                        .await
                        .map_err(|error| error.to_string())?;
                }
            }
        }
        let check = format!(
            "PRAGMA {}.foreign_key_check",
            sql_quote_identifier(DatabaseDriver::Sqlite, &schema)
        );
        if sqlx::query(AssertSqlSafe(check))
            .fetch_optional(&mut *conn)
            .await
            .map_err(|error| error.to_string())?
            .is_some()
        {
            return Err(String::from(
                "The SQLite foreign key check failed; no schema changes were applied.",
            ));
        }
        Ok(changes.len())
    }
    .await;

    let result = match result {
        Ok(count) => match (&mut *conn).execute("COMMIT").await {
            Ok(_) => Ok(count),
            Err(error) => {
                let _ = (&mut *conn).execute("ROLLBACK").await;
                Err(error.to_string())
            }
        },
        Err(error) => {
            let _ = (&mut *conn).execute("ROLLBACK").await;
            Err(error)
        }
    };
    let restore = (&mut *conn)
        .execute("PRAGMA foreign_keys = ON")
        .await
        .map_err(|error| error.to_string());
    result.and(restore.map(|_| changes.len()))
}

pub(crate) async fn run_schema_script(
    pool: DatabasePool,
    database: String,
    scripts: Vec<String>,
) -> Result<usize, SchemaScriptError> {
    let statements = scripts
        .iter()
        .flat_map(|script| split_schema_script(script))
        .collect::<Vec<_>>();
    if statements.is_empty() {
        return Err(SchemaScriptError::new(0, "Nothing to apply."));
    }

    match pool {
        DatabasePool::MySql(pool) => {
            let database = database.trim().to_string();
            let mut conn = pool
                .acquire()
                .await
                .map_err(|error| SchemaScriptError::new(0, error))?;
            if !database.is_empty() {
                let use_stmt = format!("USE `{}`", escape_mysql_identifier(&database));
                (&mut *conn)
                    .execute(AssertSqlSafe(use_stmt))
                    .await
                    .map_err(|error| SchemaScriptError::new(0, error))?;
            }
            for (applied, script) in scripts.iter().enumerate() {
                for statement in split_schema_script(script) {
                    (&mut *conn)
                        .execute(AssertSqlSafe(statement))
                        .await
                        .map_err(|error| SchemaScriptError::new(applied, error))?;
                }
            }
            Ok(statements.len())
        }
        DatabasePool::Sqlite(pool) => {
            let mut conn = pool
                .acquire()
                .await
                .map_err(|error| SchemaScriptError::new(0, error))?;
            let version: String = sqlx::query_scalar("SELECT sqlite_version()")
                .fetch_one(&mut *conn)
                .await
                .map_err(|error| SchemaScriptError::new(0, error))?;
            validate_sqlite_schema_script(&version, &statements)
                .map_err(|error| SchemaScriptError::new(0, error))?;
            let mut tx = conn
                .begin()
                .await
                .map_err(|error| SchemaScriptError::new(0, error))?;
            for statement in &statements {
                (&mut *tx)
                    .execute(AssertSqlSafe(statement.clone()))
                    .await
                    .map_err(|error| SchemaScriptError::new(0, error))?;
            }
            tx.commit()
                .await
                .map_err(|error| SchemaScriptError::new(0, error))?;
            Ok(statements.len())
        }
        DatabasePool::Postgres(pool) => {
            let mut conn = pool
                .acquire()
                .await
                .map_err(|error| SchemaScriptError::new(0, error))?;
            let mut tx = conn
                .begin()
                .await
                .map_err(|error| SchemaScriptError::new(0, error))?;
            for statement in &statements {
                (&mut *tx)
                    .execute(AssertSqlSafe(statement.clone()))
                    .await
                    .map_err(|error| SchemaScriptError::new(0, error))?;
            }
            tx.commit()
                .await
                .map_err(|error| SchemaScriptError::new(0, error))?;
            Ok(statements.len())
        }
    }
}

pub(crate) async fn execute_table_command(
    pool: DatabasePool,
    database: String,
    command: TableCommand,
) -> Result<usize, String> {
    match pool {
        DatabasePool::MySql(pool) => {
            let database = database.trim().to_string();
            if database.is_empty() {
                return Err(String::from("Select a database first."));
            }

            let mut conn = pool.acquire().await.map_err(|error| error.to_string())?;
            let use_stmt = format!("USE `{}`", escape_mysql_identifier(&database));
            (&mut *conn)
                .execute(AssertSqlSafe(use_stmt))
                .await
                .map_err(|error| error.to_string())?;

            let qualified = |table: &str| {
                format!(
                    "`{}`.`{}`",
                    escape_mysql_identifier(&database),
                    escape_mysql_identifier(table.trim())
                )
            };

            match command {
                TableCommand::AlterTable { table, clause } => {
                    let clause = clause.trim().trim_end_matches(';').trim();
                    if clause.is_empty() {
                        return Err(String::from("ALTER clause is empty."));
                    }
                    let sql = format!("ALTER TABLE {} {}", qualified(&table), clause);
                    let result = (&mut *conn)
                        .execute(AssertSqlSafe(sql))
                        .await
                        .map_err(|error| error.to_string())?;
                    Ok(result.rows_affected() as usize)
                }
                TableCommand::AddColumn { table, definition } => {
                    let definition = definition.trim().trim_end_matches(';').trim();
                    if definition.is_empty() {
                        return Err(String::from("Column definition is empty."));
                    }
                    let sql = format!(
                        "ALTER TABLE {} ADD COLUMN {}",
                        qualified(&table),
                        definition
                    );
                    let result = (&mut *conn)
                        .execute(AssertSqlSafe(sql))
                        .await
                        .map_err(|error| error.to_string())?;
                    Ok(result.rows_affected() as usize)
                }
                TableCommand::Truncate { table } => {
                    let sql = format!("TRUNCATE TABLE {}", qualified(&table));
                    let result = (&mut *conn)
                        .execute(AssertSqlSafe(sql))
                        .await
                        .map_err(|error| error.to_string())?;
                    Ok(result.rows_affected() as usize)
                }
                TableCommand::Drop { table } => {
                    let sql = format!("DROP TABLE {}", qualified(&table));
                    let result = (&mut *conn)
                        .execute(AssertSqlSafe(sql))
                        .await
                        .map_err(|error| error.to_string())?;
                    Ok(result.rows_affected() as usize)
                }
                TableCommand::Duplicate { table, new_table } => {
                    if table.eq_ignore_ascii_case(&new_table) {
                        return Err(String::from(
                            "Source and destination table names must be different.",
                        ));
                    }
                    let create_sql = format!(
                        "CREATE TABLE {} LIKE {}",
                        qualified(&new_table),
                        qualified(&table)
                    );
                    (&mut *conn)
                        .execute(AssertSqlSafe(create_sql))
                        .await
                        .map_err(|error| error.to_string())?;
                    let insert_sql = format!(
                        "INSERT INTO {} SELECT * FROM {}",
                        qualified(&new_table),
                        qualified(&table)
                    );
                    let result = (&mut *conn)
                        .execute(AssertSqlSafe(insert_sql))
                        .await
                        .map_err(|error| error.to_string())?;
                    Ok(result.rows_affected() as usize)
                }
                TableCommand::Rename { table, new_table } => {
                    if table.eq_ignore_ascii_case(&new_table) {
                        return Ok(0);
                    }
                    let sql = format!(
                        "RENAME TABLE {} TO {}",
                        qualified(&table),
                        qualified(&new_table)
                    );
                    let result = (&mut *conn)
                        .execute(AssertSqlSafe(sql))
                        .await
                        .map_err(|error| error.to_string())?;
                    Ok(result.rows_affected() as usize)
                }
            }
        }
        DatabasePool::Sqlite(pool) => {
            let database = if database.trim().is_empty() {
                String::from("main")
            } else {
                database.trim().to_string()
            };
            let mut conn = pool.acquire().await.map_err(|error| error.to_string())?;

            let qualified = |table: &str| {
                format!(
                    "\"{}\".\"{}\"",
                    escape_sqlite_identifier(&database),
                    escape_sqlite_identifier(table.trim())
                )
            };

            match command {
                TableCommand::AlterTable { table, clause } => {
                    let clause = clause.trim().trim_end_matches(';').trim();
                    if clause.is_empty() {
                        return Err(String::from("ALTER clause is empty."));
                    }
                    let sql = format!("ALTER TABLE {} {}", qualified(&table), clause);
                    let result = (&mut *conn)
                        .execute(AssertSqlSafe(sql))
                        .await
                        .map_err(|error| error.to_string())?;
                    Ok(result.rows_affected() as usize)
                }
                TableCommand::AddColumn { table, definition } => {
                    let definition = definition.trim().trim_end_matches(';').trim();
                    if definition.is_empty() {
                        return Err(String::from("Column definition is empty."));
                    }
                    let sql = format!(
                        "ALTER TABLE {} ADD COLUMN {}",
                        qualified(&table),
                        definition
                    );
                    let result = (&mut *conn)
                        .execute(AssertSqlSafe(sql))
                        .await
                        .map_err(|error| error.to_string())?;
                    Ok(result.rows_affected() as usize)
                }
                TableCommand::Truncate { table } => {
                    let sql = format!("DELETE FROM {}", qualified(&table));
                    let result = (&mut *conn)
                        .execute(AssertSqlSafe(sql))
                        .await
                        .map_err(|error| error.to_string())?;
                    Ok(result.rows_affected() as usize)
                }
                TableCommand::Drop { table } => {
                    let sql = format!("DROP TABLE {}", qualified(&table));
                    let result = (&mut *conn)
                        .execute(AssertSqlSafe(sql))
                        .await
                        .map_err(|error| error.to_string())?;
                    Ok(result.rows_affected() as usize)
                }
                TableCommand::Duplicate { table, new_table } => {
                    if table.eq_ignore_ascii_case(&new_table) {
                        return Err(String::from(
                            "Source and destination table names must be different.",
                        ));
                    }
                    let create_sql = format!(
                        "CREATE TABLE {} AS SELECT * FROM {}",
                        qualified(&new_table),
                        qualified(&table)
                    );
                    let result = (&mut *conn)
                        .execute(AssertSqlSafe(create_sql))
                        .await
                        .map_err(|error| error.to_string())?;
                    Ok(result.rows_affected() as usize)
                }
                TableCommand::Rename { table, new_table } => {
                    if table.eq_ignore_ascii_case(&new_table) {
                        return Ok(0);
                    }
                    let sql = format!(
                        "ALTER TABLE {} RENAME TO \"{}\"",
                        qualified(&table),
                        escape_sqlite_identifier(&new_table)
                    );
                    let result = (&mut *conn)
                        .execute(AssertSqlSafe(sql))
                        .await
                        .map_err(|error| error.to_string())?;
                    Ok(result.rows_affected() as usize)
                }
            }
        }
        DatabasePool::Postgres(pool) => {
            ensure_postgres_current_database(&pool, &database).await?;
            let mut conn = pool.acquire().await.map_err(|error| error.to_string())?;

            match command {
                TableCommand::AlterTable { table, clause } => {
                    let clause = clause.trim().trim_end_matches(';').trim();
                    if clause.is_empty() {
                        return Err(String::from("ALTER clause is empty."));
                    }
                    let table_name = postgres_qualified_table_name(&table)?;
                    let sql = format!("ALTER TABLE {} {}", table_name, clause);
                    let result = (&mut *conn)
                        .execute(AssertSqlSafe(sql))
                        .await
                        .map_err(|error| error.to_string())?;
                    Ok(result.rows_affected() as usize)
                }
                TableCommand::AddColumn { table, definition } => {
                    let definition = definition.trim().trim_end_matches(';').trim();
                    if definition.is_empty() {
                        return Err(String::from("Column definition is empty."));
                    }
                    let table_name = postgres_qualified_table_name(&table)?;
                    let sql = format!("ALTER TABLE {} ADD COLUMN {}", table_name, definition);
                    let result = (&mut *conn)
                        .execute(AssertSqlSafe(sql))
                        .await
                        .map_err(|error| error.to_string())?;
                    Ok(result.rows_affected() as usize)
                }
                TableCommand::Truncate { table } => {
                    let table_name = postgres_qualified_table_name(&table)?;
                    let sql = format!("TRUNCATE TABLE {}", table_name);
                    let result = (&mut *conn)
                        .execute(AssertSqlSafe(sql))
                        .await
                        .map_err(|error| error.to_string())?;
                    Ok(result.rows_affected() as usize)
                }
                TableCommand::Drop { table } => {
                    let table_name = postgres_qualified_table_name(&table)?;
                    let sql = format!("DROP TABLE {}", table_name);
                    let result = (&mut *conn)
                        .execute(AssertSqlSafe(sql))
                        .await
                        .map_err(|error| error.to_string())?;
                    Ok(result.rows_affected() as usize)
                }
                TableCommand::Duplicate { table, new_table } => {
                    let (source_schema, source_name) = postgres_table_reference_parts(&table);
                    if source_name.trim().is_empty() {
                        return Err(String::from("Source table name is empty."));
                    }

                    let (target_schema_raw, target_name) =
                        postgres_table_reference_parts(&new_table);
                    if target_name.trim().is_empty() {
                        return Err(String::from("Destination table name is empty."));
                    }

                    let target_schema = target_schema_raw.or(source_schema.clone());
                    if source_name.eq_ignore_ascii_case(&target_name)
                        && source_schema
                            .as_deref()
                            .unwrap_or("public")
                            .eq_ignore_ascii_case(target_schema.as_deref().unwrap_or("public"))
                    {
                        return Err(String::from(
                            "Source and destination table names must be different.",
                        ));
                    }

                    let source_table_name = postgres_qualified_table_name(&table)?;
                    let target_table_name = match target_schema {
                        Some(schema) => sql_quote_identifier_path(
                            DatabaseDriver::PostgreSql,
                            &[schema.as_str(), target_name.as_str()],
                        ),
                        None => sql_quote_identifier(DatabaseDriver::PostgreSql, &target_name),
                    };

                    let create_sql = format!(
                        "CREATE TABLE {} (LIKE {} INCLUDING ALL)",
                        target_table_name, source_table_name
                    );
                    (&mut *conn)
                        .execute(AssertSqlSafe(create_sql))
                        .await
                        .map_err(|error| error.to_string())?;

                    let insert_sql = format!(
                        "INSERT INTO {} SELECT * FROM {}",
                        target_table_name, source_table_name
                    );
                    let result = (&mut *conn)
                        .execute(AssertSqlSafe(insert_sql))
                        .await
                        .map_err(|error| error.to_string())?;
                    Ok(result.rows_affected() as usize)
                }
                TableCommand::Rename { table, new_table } => {
                    let (source_schema, source_name) = postgres_table_reference_parts(&table);
                    if source_name.trim().is_empty() {
                        return Err(String::from("Source table name is empty."));
                    }

                    let (target_schema_raw, target_name) =
                        postgres_table_reference_parts(&new_table);
                    if target_name.trim().is_empty() {
                        return Err(String::from("Destination table name is empty."));
                    }

                    let target_schema = target_schema_raw.or(source_schema.clone());
                    let source_schema_for_compare =
                        source_schema.as_deref().unwrap_or("public").to_string();
                    let target_schema_for_compare =
                        target_schema.as_deref().unwrap_or("public").to_string();

                    if source_name.eq_ignore_ascii_case(&target_name)
                        && source_schema_for_compare
                            .eq_ignore_ascii_case(&target_schema_for_compare)
                    {
                        return Ok(0);
                    }

                    let mut affected = 0usize;
                    let mut current_name = source_name;
                    let current_schema = source_schema;

                    if !current_name.eq_ignore_ascii_case(&target_name) {
                        let current_table = match &current_schema {
                            Some(schema) => sql_quote_identifier_path(
                                DatabaseDriver::PostgreSql,
                                &[schema.as_str(), current_name.as_str()],
                            ),
                            None => sql_quote_identifier(DatabaseDriver::PostgreSql, &current_name),
                        };
                        let rename_sql = format!(
                            "ALTER TABLE {} RENAME TO {}",
                            current_table,
                            sql_quote_identifier(DatabaseDriver::PostgreSql, &target_name)
                        );
                        let result = (&mut *conn)
                            .execute(AssertSqlSafe(rename_sql))
                            .await
                            .map_err(|error| error.to_string())?;
                        affected = affected.saturating_add(result.rows_affected() as usize);
                        current_name = target_name.clone();
                    }

                    let current_schema_name =
                        current_schema.as_deref().unwrap_or("public").to_string();
                    let target_schema_name =
                        target_schema.as_deref().unwrap_or("public").to_string();

                    if !current_schema_name.eq_ignore_ascii_case(&target_schema_name) {
                        let current_table = match &current_schema {
                            Some(schema) => sql_quote_identifier_path(
                                DatabaseDriver::PostgreSql,
                                &[schema.as_str(), current_name.as_str()],
                            ),
                            None => sql_quote_identifier(DatabaseDriver::PostgreSql, &current_name),
                        };
                        let set_schema_sql = format!(
                            "ALTER TABLE {} SET SCHEMA {}",
                            current_table,
                            sql_quote_identifier(DatabaseDriver::PostgreSql, &target_schema_name)
                        );
                        let result = (&mut *conn)
                            .execute(AssertSqlSafe(set_schema_sql))
                            .await
                            .map_err(|error| error.to_string())?;
                        affected = affected.saturating_add(result.rows_affected() as usize);
                    }

                    Ok(affected)
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sqlite_schema_changes_use_the_selected_attached_schema() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("tokio runtime");

        runtime.block_on(async {
            let pool = sqlx::sqlite::SqlitePoolOptions::new()
                .max_connections(1)
                .connect("sqlite::memory:")
                .await
                .expect("open sqlite pool");
            sqlx::query("ATTACH ':memory:' AS aux")
                .execute(&pool)
                .await
                .expect("attach schema");
            sqlx::query("CREATE TABLE main.notes (id INTEGER)")
                .execute(&pool)
                .await
                .expect("create main table");
            sqlx::query("CREATE TABLE aux.notes (id INTEGER)")
                .execute(&pool)
                .await
                .expect("create attached table");

            let change = SchemaChange::DropTable {
                table: String::from("notes"),
            };
            run_sqlite_schema_changes(
                DatabasePool::Sqlite(pool.clone()),
                String::from("aux"),
                vec![(change, String::from("DROP TABLE \"notes\";"))],
            )
            .await
            .expect("apply attached schema change");

            let main_exists: i64 = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM main.sqlite_master WHERE type = 'table' AND name = 'notes')",
            )
            .fetch_one(&pool)
            .await
            .expect("read main schema");
            let aux_exists: i64 = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM aux.sqlite_master WHERE type = 'table' AND name = 'notes')",
            )
            .fetch_one(&pool)
            .await
            .expect("read attached schema");
            assert_eq!(main_exists, 1);
            assert_eq!(aux_exists, 0);
        });
    }

    #[test]
    fn schema_script_rolls_back_sqlite_on_failure() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("tokio runtime");

        runtime.block_on(async {
            let pool = sqlx::sqlite::SqlitePoolOptions::new()
                .max_connections(1)
                .connect("sqlite::memory:")
                .await
                .expect("open sqlite pool");
            let result = run_schema_script(
                DatabasePool::Sqlite(pool.clone()),
                String::new(),
                vec![
                    String::from("CREATE TABLE users (id INTEGER);"),
                    String::from("INVALID SQL;"),
                ],
            )
            .await;

            assert_eq!(result.err().map(|error| error.applied), Some(0));
            let tables: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'users'",
            )
            .fetch_one(&pool)
            .await
            .expect("count tables");
            assert_eq!(tables, 0);
        });
    }

    #[test]
    fn sqlite_drop_column_requires_version_3_35() {
        let statements = vec![String::from("ALTER TABLE users DROP COLUMN name")];

        assert!(validate_sqlite_schema_script("3.34.1", &statements).is_err());
        assert!(validate_sqlite_schema_script("3.35.0", &statements).is_ok());
        assert!(validate_sqlite_schema_script("4.0.0", &statements).is_ok());
        assert!(
            validate_sqlite_schema_script(
                "3.34.1",
                &[String::from("ALTER TABLE users ADD COLUMN name TEXT")]
            )
            .is_ok()
        );
    }

    #[test]
    fn sqlite_foreign_key_rebuild_preserves_schema_objects_and_data() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("tokio runtime");

        runtime.block_on(async {
            let pool = sqlx::sqlite::SqlitePoolOptions::new()
                .max_connections(1)
                .connect("sqlite::memory:")
                .await
                .expect("open sqlite pool");
            sqlx::query(
                "PRAGMA foreign_keys = ON;
                 CREATE TABLE parents (id INTEGER PRIMARY KEY);
                 CREATE TABLE children (
                   id INTEGER PRIMARY KEY AUTOINCREMENT,
                   parent_id INTEGER NOT NULL DEFAULT 1 CHECK(parent_id > 0),
                   code TEXT UNIQUE,
                   label TEXT GENERATED ALWAYS AS (code || ')') STORED
                 ) STRICT;
                 CREATE INDEX children_parent_idx ON children(parent_id);
                 CREATE TABLE audit (count INTEGER NOT NULL);
                 INSERT INTO audit VALUES (0);
                 CREATE TRIGGER children_insert AFTER INSERT ON children
                 BEGIN UPDATE audit SET count = count + 1; END;
                 INSERT INTO parents VALUES (1);
                 INSERT INTO children (parent_id, code) VALUES (1, 'first');",
            )
            .execute(&pool)
            .await
            .expect("seed schema");

            let change = SchemaChange::AddForeignKey {
                table: String::from("children"),
                columns: vec![String::from("parent_id")],
                referenced_table: String::from("parents"),
                referenced_columns: vec![String::from("id")],
            };
            run_sqlite_schema_changes(
                DatabasePool::Sqlite(pool.clone()),
                String::from("main"),
                vec![(change, String::new())],
            )
            .await
            .expect("add foreign key");

            let ddl: String = sqlx::query_scalar(
                "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = 'children'",
            )
            .fetch_one(&pool)
            .await
            .expect("stored ddl");
            assert!(ddl.contains("DEFAULT 1"));
            assert!(ddl.contains("CHECK(parent_id > 0)"));
            assert!(ddl.contains("code TEXT UNIQUE"));
            assert!(ddl.contains("GENERATED ALWAYS"));
            assert!(ddl.ends_with("STRICT"));
            assert!(ddl.contains("FOREIGN KEY (\"parent_id\") REFERENCES \"parents\" (\"id\")"));
            let objects: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM sqlite_master WHERE tbl_name = 'children' AND type IN ('index', 'trigger') AND sql IS NOT NULL",
            )
            .fetch_one(&pool)
            .await
            .expect("schema objects");
            assert_eq!(objects, 2);
            let rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM children")
                .fetch_one(&pool)
                .await
                .expect("child rows");
            assert_eq!(rows, 1);
            let label: String = sqlx::query_scalar("SELECT label FROM children WHERE id = 1")
                .fetch_one(&pool)
                .await
                .expect("generated value");
            assert_eq!(label, "first)");
            assert!(
                sqlx::query("INSERT INTO children (parent_id, code) VALUES (99, 'invalid')")
                    .execute(&pool)
                    .await
                    .is_err()
            );
            sqlx::query("INSERT INTO children (parent_id, code) VALUES (1, 'second')")
                .execute(&pool)
                .await
                .expect("trigger remains active");
            let audit: i64 = sqlx::query_scalar("SELECT count FROM audit")
                .fetch_one(&pool)
                .await
                .expect("audit count");
            assert_eq!(audit, 2);
        });
    }

    #[test]
    fn sqlite_rebuild_writes_a_composite_foreign_key() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("tokio runtime");

        runtime.block_on(async {
            let pool = sqlx::sqlite::SqlitePoolOptions::new()
                .max_connections(1)
                .connect("sqlite::memory:")
                .await
                .expect("open sqlite pool");
            sqlx::query(
                "PRAGMA foreign_keys = ON;
                 CREATE TABLE customers (tenant_id INTEGER, id INTEGER, PRIMARY KEY (tenant_id, id));
                 CREATE TABLE orders (tenant_id INTEGER NOT NULL, customer_id INTEGER NOT NULL);
                 INSERT INTO customers VALUES (1, 10);
                 INSERT INTO orders VALUES (1, 10);",
            )
            .execute(&pool)
            .await
            .expect("seed schema");

            run_sqlite_schema_changes(
                DatabasePool::Sqlite(pool.clone()),
                String::from("main"),
                vec![(
                    SchemaChange::AddForeignKey {
                        table: String::from("orders"),
                        columns: vec![String::from("tenant_id"), String::from("customer_id")],
                        referenced_table: String::from("customers"),
                        referenced_columns: vec![String::from("tenant_id"), String::from("id")],
                    },
                    String::new(),
                )],
            )
            .await
            .expect("add composite foreign key");

            let ddl: String = sqlx::query_scalar(
                "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = 'orders'",
            )
            .fetch_one(&pool)
            .await
            .expect("stored ddl");
            assert!(ddl.contains(
                "FOREIGN KEY (\"tenant_id\", \"customer_id\") REFERENCES \"customers\" (\"tenant_id\", \"id\")"
            ));
            let keys: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM pragma_foreign_key_list('orders')")
                .fetch_one(&pool)
                .await
                .expect("foreign key pairs");
            assert_eq!(keys, 2);
            sqlx::query("INSERT INTO orders VALUES (1, 10)")
                .execute(&pool)
                .await
                .expect("matching pair is accepted");
            assert!(
                sqlx::query("INSERT INTO orders VALUES (1, 99)")
                    .execute(&pool)
                    .await
                    .is_err()
            );
        });
    }

    #[test]
    fn sqlite_alter_column_rebuild_keeps_constraints_and_data() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("tokio runtime");

        runtime.block_on(async {
            let pool = sqlx::sqlite::SqlitePoolOptions::new()
                .max_connections(1)
                .connect("sqlite::memory:")
                .await
                .expect("open sqlite pool");
            sqlx::query(
                "CREATE TABLE notes (
                   id INTEGER PRIMARY KEY AUTOINCREMENT,
                   body TEXT DEFAULT 'old' CHECK(length(body) < 40) COLLATE NOCASE,
                   code TEXT UNIQUE
                 );
                 CREATE INDEX notes_code_idx ON notes(code);
                 INSERT INTO notes (body, code) VALUES ('kept', 'first');",
            )
            .execute(&pool)
            .await
            .expect("seed schema");

            let change = SchemaChange::AlterColumn {
                table: String::from("notes"),
                column: String::from("body"),
                data_type: String::from("VARCHAR(120)"),
                nullable: false,
                default: Some(String::from("'fresh'")),
                attributes: String::new(),
            };
            run_sqlite_schema_changes(
                DatabasePool::Sqlite(pool.clone()),
                String::from("main"),
                vec![(change, String::new())],
            )
            .await
            .expect("alter column");

            let ddl: String = sqlx::query_scalar(
                "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = 'notes'",
            )
            .fetch_one(&pool)
            .await
            .expect("stored ddl");
            assert!(ddl.contains("\"body\" VARCHAR(120) NOT NULL DEFAULT 'fresh'"));
            assert!(ddl.contains("CHECK(length(body) < 40)"));
            assert!(ddl.contains("COLLATE NOCASE"));
            assert!(ddl.contains("code TEXT UNIQUE"));
            assert!(!ddl.contains("DEFAULT 'old'"));
            let body: String = sqlx::query_scalar("SELECT body FROM notes WHERE id = 1")
                .fetch_one(&pool)
                .await
                .expect("row survives");
            assert_eq!(body, "kept");
            let indexes: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM sqlite_master WHERE tbl_name = 'notes' AND type = 'index' AND sql IS NOT NULL",
            )
            .fetch_one(&pool)
            .await
            .expect("index count");
            assert_eq!(indexes, 1);
            assert!(
                sqlx::query("INSERT INTO notes (body, code) VALUES (NULL, 'second')")
                    .execute(&pool)
                    .await
                    .is_err()
            );
        });
    }

    #[test]
    fn sqlite_foreign_key_rebuild_rolls_back_invalid_data() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("tokio runtime");

        runtime.block_on(async {
            let pool = sqlx::sqlite::SqlitePoolOptions::new()
                .max_connections(1)
                .connect("sqlite::memory:")
                .await
                .expect("open sqlite pool");
            sqlx::query(
                "CREATE TABLE parents (id INTEGER PRIMARY KEY);
                 CREATE TABLE children (id INTEGER PRIMARY KEY, parent_id INTEGER);
                 INSERT INTO children VALUES (1, 99);",
            )
            .execute(&pool)
            .await
            .expect("seed invalid relation");

            let result = run_sqlite_schema_changes(
                DatabasePool::Sqlite(pool.clone()),
                String::new(),
                vec![
                    (
                        SchemaChange::AddColumn {
                            table: String::from("children"),
                            column: String::from("marker"),
                            data_type: String::from("TEXT"),
                        },
                        String::from("ALTER TABLE \"children\" ADD COLUMN \"marker\" TEXT;"),
                    ),
                    (
                        SchemaChange::AddForeignKey {
                            table: String::from("children"),
                            columns: vec![String::from("parent_id")],
                            referenced_table: String::from("parents"),
                            referenced_columns: vec![String::from("id")],
                        },
                        String::new(),
                    ),
                ],
            )
            .await;

            assert!(result.is_err());
            let ddl: String = sqlx::query_scalar(
                "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = 'children'",
            )
            .fetch_one(&pool)
            .await
            .expect("original ddl");
            assert!(!ddl.contains("FOREIGN KEY"));
            let parent_id: i64 = sqlx::query_scalar("SELECT parent_id FROM children WHERE id = 1")
                .fetch_one(&pool)
                .await
                .expect("original data");
            assert_eq!(parent_id, 99);
            let marker: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM pragma_table_info('children') WHERE name = 'marker'",
            )
            .fetch_one(&pool)
            .await
            .expect("rolled back column");
            assert_eq!(marker, 0);
            let foreign_keys: i64 = sqlx::query_scalar("PRAGMA foreign_keys")
                .fetch_one(&pool)
                .await
                .expect("foreign key setting");
            assert_eq!(foreign_keys, 1);
        });
    }

    #[test]
    fn sqlite_foreign_key_rebuild_supports_attached_databases() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("tokio runtime");

        runtime.block_on(async {
            let pool = sqlx::sqlite::SqlitePoolOptions::new()
                .max_connections(1)
                .connect("sqlite::memory:")
                .await
                .expect("open sqlite pool");
            sqlx::query(
                "ATTACH ':memory:' AS aux;
                 CREATE TABLE aux.parents (id INTEGER PRIMARY KEY);
                 CREATE TABLE aux.children (id INTEGER PRIMARY KEY, parent_id INTEGER);
                 CREATE INDEX aux.children_parent_idx ON children(parent_id);
                 CREATE TRIGGER aux.children_insert AFTER INSERT ON children BEGIN SELECT 1; END;",
            )
            .execute(&pool)
            .await
            .expect("seed attached schema");

            run_sqlite_schema_changes(
                DatabasePool::Sqlite(pool.clone()),
                String::from("aux"),
                vec![(
                    SchemaChange::AddForeignKey {
                        table: String::from("children"),
                        columns: vec![String::from("parent_id")],
                        referenced_table: String::from("parents"),
                        referenced_columns: vec![String::from("id")],
                    },
                    String::new(),
                )],
            )
            .await
            .expect("add attached foreign key");

            let foreign_key: i64 = sqlx::query_scalar("PRAGMA aux.foreign_key_list('children')")
                .fetch_one(&pool)
                .await
                .expect("attached foreign key");
            assert_eq!(foreign_key, 0);
            let objects: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM aux.sqlite_master WHERE tbl_name = 'children' AND type IN ('index', 'trigger') AND sql IS NOT NULL",
            )
            .fetch_one(&pool)
            .await
            .expect("attached schema objects");
            assert_eq!(objects, 2);

            run_sqlite_schema_changes(
                DatabasePool::Sqlite(pool.clone()),
                String::from("aux"),
                vec![(
                    SchemaChange::DropForeignKey {
                        table: String::from("children"),
                        column: String::from("parent_id"),
                        referenced_table: String::from("parents"),
                        referenced_column: String::from("id"),
                        constraint_name: String::new(),
                    },
                    String::new(),
                )],
            )
            .await
            .expect("drop attached foreign key");
            assert!(
                sqlx::query("PRAGMA aux.foreign_key_list('children')")
                    .fetch_optional(&pool)
                    .await
                    .expect("read attached foreign keys")
                    .is_none()
            );
        });
    }

    #[test]
    fn sqlite_foreign_key_rebuild_drops_table_and_inline_constraints() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("tokio runtime");

        runtime.block_on(async {
            let pool = sqlx::sqlite::SqlitePoolOptions::new()
                .max_connections(1)
                .connect("sqlite::memory:")
                .await
                .expect("open sqlite pool");
            sqlx::query(
                "PRAGMA foreign_keys = ON;
                 CREATE TABLE parents (a INTEGER, b INTEGER, PRIMARY KEY (a, b)) WITHOUT ROWID;
                 CREATE TABLE others (id INTEGER PRIMARY KEY);
                 CREATE TABLE children (
                   id INTEGER PRIMARY KEY,
                   parent_a INTEGER,
                   parent_b INTEGER,
                   inline_parent INTEGER DEFAULT 1 REFERENCES \"others\"(\"id\") ON DELETE CASCADE NOT NULL,
                   CONSTRAINT \"fk pair\" FOREIGN KEY (\"parent_a\", \"parent_b\") REFERENCES \"parents\"(\"a\", \"b\") ON UPDATE CASCADE,
                   CHECK(parent_a > 0)
                 );
                 CREATE INDEX children_parent_idx ON children(parent_a, parent_b);
                 CREATE TRIGGER children_insert AFTER INSERT ON children BEGIN SELECT 1; END;
                 INSERT INTO parents VALUES (1, 2);
                 INSERT INTO others VALUES (1);
                 INSERT INTO children VALUES (1, 1, 2, 1);",
            )
            .execute(&pool)
            .await
            .expect("seed foreign keys");

            run_sqlite_schema_changes(
                DatabasePool::Sqlite(pool.clone()),
                String::new(),
                vec![(
                    SchemaChange::DropForeignKey {
                        table: String::from("children"),
                        column: String::from("parent_a"),
                        referenced_table: String::from("parents"),
                        referenced_column: String::from("a"),
                        constraint_name: String::new(),
                    },
                    String::new(),
                )],
            )
            .await
            .expect("drop composite foreign key");

            let relations: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM pragma_foreign_key_list('children')",
            )
            .fetch_one(&pool)
            .await
            .expect("remaining foreign keys");
            assert_eq!(relations, 1);
            let ddl: String = sqlx::query_scalar(
                "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = 'children'",
            )
            .fetch_one(&pool)
            .await
            .expect("stored ddl");
            assert!(ddl.contains("REFERENCES \"others\""));
            assert!(!ddl.contains("fk pair"));
            assert!(ddl.contains("CHECK(parent_a > 0)"));

            run_sqlite_schema_changes(
                DatabasePool::Sqlite(pool.clone()),
                String::new(),
                vec![(
                    SchemaChange::DropForeignKey {
                        table: String::from("children"),
                        column: String::from("inline_parent"),
                        referenced_table: String::from("others"),
                        referenced_column: String::from("id"),
                        constraint_name: String::new(),
                    },
                    String::new(),
                )],
            )
            .await
            .expect("drop inline foreign key");

            assert!(
                sqlx::query("PRAGMA foreign_key_list('children')")
                    .fetch_optional(&pool)
                    .await
                    .expect("read foreign keys")
                    .is_none()
            );
            let ddl: String = sqlx::query_scalar(
                "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = 'children'",
            )
            .fetch_one(&pool)
            .await
            .expect("stored ddl");
            assert!(!ddl.contains("REFERENCES"));
            assert!(ddl.contains("inline_parent INTEGER DEFAULT 1 NOT NULL"));
            assert!(
                sqlx::query("INSERT INTO children (id, parent_a, parent_b, inline_parent) VALUES (2, 1, 2, NULL)")
                    .execute(&pool)
                    .await
                    .is_err()
            );
            let objects: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM sqlite_master WHERE tbl_name = 'children' AND type IN ('index', 'trigger') AND sql IS NOT NULL",
            )
            .fetch_one(&pool)
            .await
            .expect("schema objects");
            assert_eq!(objects, 2);
            let rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM children")
                .fetch_one(&pool)
                .await
                .expect("stored rows");
            assert_eq!(rows, 1);
        });
    }

    #[test]
    fn postgres_placeholder_expr_uses_udt_for_unknown_kinds() {
        assert_eq!(
            postgres_placeholder_expr_with_type(ColumnKind::Unknown, "$1", Some("int8")),
            "CAST($1 AS BIGINT)"
        );
        assert_eq!(
            postgres_placeholder_expr_with_type(ColumnKind::Unknown, "$1", Some("uuid")),
            "CAST($1 AS uuid)"
        );
        assert_eq!(
            postgres_placeholder_expr_with_type(ColumnKind::Unknown, "$1", Some("text")),
            "$1"
        );
    }
}
