use crate::utils::format::format_optional_datetime;
use crate::utils::helpers::{
    escape_mysql_identifier, escape_sqlite_identifier, escape_sqlite_string_literal, format_definer,
};
use crate::{
    DatabasePool, DiagramColumn, PostgresObjectKind, PostgresRoleForm, PostgresSidebarObject,
    RelationInfo, SchemaDiagramData, SidebarRelationEntry, SidebarTriggerEntry, TableInfoDetails,
    TriggerInfo,
};
use futures_util::TryStreamExt;
use sqlx::PgPool;
use sqlx::Row;
use sqlx::types::chrono::NaiveDateTime;
use sqlx::{AssertSqlSafe, Executor};
use std::collections::HashMap;
use std::future::Future;
use std::time::Duration;

async fn metadata_timeout<T>(
    label: &str,
    future: impl Future<Output = Result<T, String>>,
) -> Result<T, String> {
    match tokio::time::timeout(Duration::from_secs(crate::METADATA_TIMEOUT_SECS), future).await {
        Ok(result) => result,
        Err(_) => Err(format!(
            "{label} timed out after {} seconds.",
            crate::METADATA_TIMEOUT_SECS
        )),
    }
}

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

async fn ensure_postgres_current_database(pool: &PgPool, database: &str) -> Result<(), String> {
    if database.trim().is_empty() {
        return Err(String::from("Select a database to load metadata."));
    }

    let current_database = sqlx::query_scalar::<_, String>("SELECT current_database()")
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

async fn fetch_postgres_user_schemas(pool: &PgPool) -> Result<Vec<String>, String> {
    let mut rows = sqlx::query(
        "SELECT nspname \
        FROM pg_namespace \
        WHERE nspname = 'public' \
        OR (nspname NOT LIKE 'pg_%' AND nspname <> 'information_schema') \
        ORDER BY CASE WHEN nspname = 'public' THEN 0 ELSE 1 END, nspname",
    )
    .fetch(pool);

    let mut schemas = Vec::new();
    while let Some(row) = rows.try_next().await.map_err(|error| error.to_string())? {
        let schema: String = row.try_get(0).map_err(|error| error.to_string())?;
        if !schema.trim().is_empty() {
            schemas.push(schema);
        }
    }

    if schemas.is_empty() {
        schemas.push(String::from("public"));
    }

    Ok(schemas)
}

fn non_negative_i64_to_u64(value: Option<i64>) -> Option<u64> {
    value.and_then(|raw| if raw >= 0 { Some(raw as u64) } else { None })
}

fn non_negative_f64_to_u64(value: Option<f64>) -> Option<u64> {
    value.and_then(|raw| {
        if raw.is_finite() && raw >= 0.0 {
            Some(raw.round() as u64)
        } else {
            None
        }
    })
}

fn postgres_object_kind_from_str(raw: &str) -> Option<PostgresObjectKind> {
    match raw {
        "table" => Some(PostgresObjectKind::Table),
        "view" => Some(PostgresObjectKind::View),
        "materialized_view" => Some(PostgresObjectKind::MaterializedView),
        "foreign_table" => Some(PostgresObjectKind::ForeignTable),
        "sequence" => Some(PostgresObjectKind::Sequence),
        "extension" => Some(PostgresObjectKind::Extension),
        "role" => Some(PostgresObjectKind::Role),
        _ => None,
    }
}

pub(crate) async fn fetch_tables(
    pool: DatabasePool,
    database: String,
) -> Result<Vec<String>, String> {
    metadata_timeout("Table list load", fetch_tables_inner(pool, database)).await
}

pub(crate) async fn fetch_table_row_counts(
    pool: DatabasePool,
    database: String,
) -> Result<HashMap<String, u64>, String> {
    metadata_timeout(
        "Row count load",
        fetch_table_row_counts_inner(pool, database),
    )
    .await
}

async fn fetch_table_row_counts_inner(
    pool: DatabasePool,
    database: String,
) -> Result<HashMap<String, u64>, String> {
    let mut counts = HashMap::new();

    match pool {
        DatabasePool::MySql(pool) => {
            if database.trim().is_empty() {
                return Ok(counts);
            }
            let mut rows = sqlx::query(
                "SELECT table_name, table_rows                 FROM information_schema.tables                 WHERE table_schema = ?",
            )
            .bind(database)
            .fetch(&pool);

            while let Some(row) = rows.try_next().await.map_err(|error| error.to_string())? {
                let name: String = row.try_get(0).map_err(|error| error.to_string())?;
                let count: Option<u64> = row.try_get(1).unwrap_or(None);
                if let Some(count) = count {
                    counts.insert(name, count);
                }
            }
        }
        DatabasePool::Postgres(pool) => {
            let mut rows = sqlx::query(
                "SELECT n.nspname, c.relname, c.reltuples::bigint                 FROM pg_class c                 JOIN pg_namespace n ON n.oid = c.relnamespace                 WHERE c.relkind IN ('r', 'p', 'v', 'm', 'f')",
            )
            .fetch(&pool);

            while let Some(row) = rows.try_next().await.map_err(|error| error.to_string())? {
                let schema: String = row.try_get(0).map_err(|error| error.to_string())?;
                let name: String = row.try_get(1).map_err(|error| error.to_string())?;
                let count: Option<i64> = row.try_get(2).unwrap_or(None);
                if let Some(count) = count.filter(|count| *count >= 0) {
                    counts.insert(format!("{schema}.{name}"), count as u64);
                }
            }
        }
        DatabasePool::Sqlite(_) => {}
    }

    Ok(counts)
}

async fn fetch_tables_inner(pool: DatabasePool, database: String) -> Result<Vec<String>, String> {
    match pool {
        DatabasePool::MySql(pool) => {
            if database.trim().is_empty() {
                return Err(String::from("Select a database to load tables."));
            }

            let mut rows = sqlx::query(
                "SELECT table_name \
                FROM information_schema.tables \
                WHERE table_schema = ? \
                ORDER BY table_name",
            )
            .bind(database)
            .fetch(&pool);

            let mut tables = Vec::new();
            while let Some(row) = rows.try_next().await.map_err(|error| error.to_string())? {
                let name: String = row.try_get(0).map_err(|error| error.to_string())?;
                tables.push(name);
            }

            Ok(tables)
        }
        DatabasePool::Sqlite(pool) => {
            let schema = if database.trim().is_empty() {
                String::from("main")
            } else {
                database.trim().to_string()
            };
            let statement = format!(
                "SELECT name \
                FROM \"{}\".sqlite_master \
                WHERE type IN ('table', 'view') \
                AND name NOT LIKE 'sqlite_%' \
                ORDER BY name",
                escape_sqlite_identifier(&schema)
            );

            let mut rows = sqlx::query(AssertSqlSafe(statement)).fetch(&pool);

            let mut tables = Vec::new();
            while let Some(row) = rows.try_next().await.map_err(|error| error.to_string())? {
                let name: String = row.try_get(0).map_err(|error| error.to_string())?;
                tables.push(name);
            }

            Ok(tables)
        }
        DatabasePool::Postgres(pool) => {
            ensure_postgres_current_database(&pool, &database).await?;

            let schemas = fetch_postgres_user_schemas(&pool).await?;
            let mut tables = Vec::new();

            for schema in schemas {
                let mut rows = sqlx::query(
                    "SELECT c.relname \
                    FROM pg_class c \
                    JOIN pg_namespace n ON n.oid = c.relnamespace \
                    WHERE n.nspname = $1 \
                    AND c.relkind IN ('r', 'p', 'v', 'm', 'f') \
                    ORDER BY c.relname",
                )
                .bind(&schema)
                .fetch(&pool);

                while let Some(row) = rows.try_next().await.map_err(|error| error.to_string())? {
                    let name: String = row.try_get(0).map_err(|error| error.to_string())?;
                    tables.push(format!("{schema}.{name}"));
                }
            }

            tables.sort_by(|left, right| {
                left.to_ascii_lowercase()
                    .cmp(&right.to_ascii_lowercase())
                    .then_with(|| left.cmp(right))
            });
            tables.dedup();

            Ok(tables)
        }
    }
}

pub(crate) async fn fetch_postgres_sidebar_objects(
    pool: DatabasePool,
    database: String,
) -> Result<Vec<PostgresSidebarObject>, String> {
    metadata_timeout(
        "PostgreSQL sidebar object load",
        fetch_postgres_sidebar_objects_inner(pool, database),
    )
    .await
}

async fn fetch_postgres_sidebar_objects_inner(
    pool: DatabasePool,
    database: String,
) -> Result<Vec<PostgresSidebarObject>, String> {
    let DatabasePool::Postgres(pool) = pool else {
        return Ok(Vec::new());
    };

    ensure_postgres_current_database(&pool, &database).await?;

    let mut rows = sqlx::query(
        "SELECT n.nspname AS schema_name, c.relname AS object_name, \
        CASE c.relkind \
            WHEN 'r' THEN 'table' \
            WHEN 'p' THEN 'table' \
            WHEN 'v' THEN 'view' \
            WHEN 'm' THEN 'materialized_view' \
            WHEN 'f' THEN 'foreign_table' \
            WHEN 'S' THEN 'sequence' \
            ELSE NULL \
        END AS object_kind \
        FROM pg_class c \
        JOIN pg_namespace n ON n.oid = c.relnamespace \
        WHERE (n.nspname = 'public' \
            OR (n.nspname NOT LIKE 'pg_%' AND n.nspname <> 'information_schema')) \
        AND c.relkind IN ('r', 'p', 'v', 'm', 'f', 'S') \
        UNION ALL \
        SELECT 'extensions' AS schema_name, e.extname AS object_name, 'extension' AS object_kind \
        FROM pg_extension e \
        UNION ALL \
        SELECT 'roles' AS schema_name, r.rolname AS object_name, 'role' AS object_kind \
        FROM pg_roles r",
    )
    .fetch(&pool);

    let mut objects = Vec::new();
    while let Some(row) = rows.try_next().await.map_err(|error| error.to_string())? {
        let schema: String = row.try_get(0).map_err(|error| error.to_string())?;
        let name: String = row.try_get(1).map_err(|error| error.to_string())?;
        let kind_name: Option<String> = row.try_get(2).map_err(|error| error.to_string())?;
        let Some(kind) = kind_name.as_deref().and_then(postgres_object_kind_from_str) else {
            continue;
        };

        if schema.trim().is_empty() || name.trim().is_empty() {
            continue;
        }

        objects.push(PostgresSidebarObject { schema, name, kind });
    }

    objects.sort_by(|left, right| {
        left.schema
            .to_ascii_lowercase()
            .cmp(&right.schema.to_ascii_lowercase())
            .then_with(|| {
                let left_label = left.kind.searchable_label();
                let right_label = right.kind.searchable_label();
                left_label.cmp(right_label)
            })
            .then_with(|| {
                left.name
                    .to_ascii_lowercase()
                    .cmp(&right.name.to_ascii_lowercase())
            })
            .then_with(|| left.name.cmp(&right.name))
    });
    objects.dedup_by(|left, right| {
        left.schema.eq_ignore_ascii_case(&right.schema)
            && left.name.eq_ignore_ascii_case(&right.name)
            && left.kind == right.kind
    });

    Ok(objects)
}

pub(crate) async fn fetch_postgres_role_form(
    pool: DatabasePool,
    database: String,
    role_name: String,
) -> Result<PostgresRoleForm, String> {
    metadata_timeout(
        "PostgreSQL role metadata load",
        fetch_postgres_role_form_inner(pool, database, role_name),
    )
    .await
}

async fn fetch_postgres_role_form_inner(
    pool: DatabasePool,
    database: String,
    role_name: String,
) -> Result<PostgresRoleForm, String> {
    let DatabasePool::Postgres(pool) = pool else {
        return Err(String::from(
            "PostgreSQL role properties are only available for PostgreSQL connections.",
        ));
    };

    ensure_postgres_current_database(&pool, &database).await?;

    let role_name = role_name.trim().to_string();
    if role_name.is_empty() {
        return Err(String::from("Select a role first."));
    }

    let row = sqlx::query(
        "SELECT r.rolname, r.rolcanlogin, r.rolsuper, r.rolinherit, r.rolcreatedb, \
        r.rolcreaterole, r.rolreplication, r.rolbypassrls, r.rolconnlimit, \
        COALESCE(r.rolvaliduntil::text, '') AS valid_until, \
        COALESCE(array_to_string(r.rolconfig, E'\\n'), '') AS role_config, \
        COALESCE(shobj_description(r.oid, 'pg_authid'), '') AS role_comment \
        FROM pg_roles r \
        WHERE r.rolname = $1 \
        LIMIT 1",
    )
    .bind(&role_name)
    .fetch_optional(&pool)
    .await
    .map_err(|error| error.to_string())?
    .ok_or_else(|| format!("Role `{role_name}` was not found."))?;

    let role_name: String = row.try_get(0).map_err(|error| error.to_string())?;
    let can_login: bool = row.try_get(1).map_err(|error| error.to_string())?;
    let is_superuser: bool = row.try_get(2).map_err(|error| error.to_string())?;
    let inherit_privileges: bool = row.try_get(3).map_err(|error| error.to_string())?;
    let can_create_db: bool = row.try_get(4).map_err(|error| error.to_string())?;
    let can_create_role: bool = row.try_get(5).map_err(|error| error.to_string())?;
    let can_replicate: bool = row.try_get(6).map_err(|error| error.to_string())?;
    let bypass_rls: bool = row.try_get(7).map_err(|error| error.to_string())?;
    let connection_limit: i32 = row.try_get(8).map_err(|error| error.to_string())?;
    let valid_until: String = row.try_get(9).map_err(|error| error.to_string())?;
    let role_config: String = row.try_get(10).map_err(|error| error.to_string())?;
    let role_comment: String = row.try_get(11).map_err(|error| error.to_string())?;

    let role_lookup = role_name.clone();

    let mut member_of_rows = sqlx::query(
        "SELECT parent.rolname \
        FROM pg_auth_members members \
        JOIN pg_roles parent ON parent.oid = members.roleid \
        JOIN pg_roles child ON child.oid = members.member \
        WHERE child.rolname = $1 \
        ORDER BY parent.rolname",
    )
    .bind(&role_lookup)
    .fetch(&pool);

    let mut member_of = Vec::new();
    while let Some(row) = member_of_rows
        .try_next()
        .await
        .map_err(|error| error.to_string())?
    {
        let value: String = row.try_get(0).map_err(|error| error.to_string())?;
        if !value.trim().is_empty() {
            member_of.push(value);
        }
    }

    let mut members_rows = sqlx::query(
        "SELECT child.rolname \
        FROM pg_auth_members members \
        JOIN pg_roles parent ON parent.oid = members.roleid \
        JOIN pg_roles child ON child.oid = members.member \
        WHERE parent.rolname = $1 \
        ORDER BY child.rolname",
    )
    .bind(&role_lookup)
    .fetch(&pool);

    let mut members = Vec::new();
    while let Some(row) = members_rows
        .try_next()
        .await
        .map_err(|error| error.to_string())?
    {
        let value: String = row.try_get(0).map_err(|error| error.to_string())?;
        if !value.trim().is_empty() {
            members.push(value);
        }
    }

    let mut form = PostgresRoleForm::with_role_name(role_name);
    form.can_login = can_login;
    form.is_superuser = is_superuser;
    form.inherit_privileges = inherit_privileges;
    form.can_create_db = can_create_db;
    form.can_create_role = can_create_role;
    form.can_replicate = can_replicate;
    form.bypass_rls = bypass_rls;
    form.connection_limit = connection_limit.to_string();
    form.valid_until = valid_until.trim().to_string();
    form.member_of_roles = member_of.join(", ");
    form.members = members.join(", ");
    form.comment = role_comment.trim().to_string();

    for line in role_config.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let Some((name, value)) = trimmed.split_once('=') else {
            continue;
        };
        let name = name.trim().to_ascii_lowercase();
        let value = value.trim().to_string();
        match name.as_str() {
            "search_path" => form.search_path = value,
            "work_mem" => form.work_mem = value,
            "maintenance_work_mem" => form.maintenance_work_mem = value,
            "statement_timeout" => form.statement_timeout = value,
            "lock_timeout" => form.lock_timeout = value,
            "idle_in_transaction_session_timeout" => {
                form.idle_in_transaction_session_timeout = value
            }
            _ => {}
        }
    }

    Ok(form)
}

pub(crate) async fn fetch_table_triggers(
    pool: DatabasePool,
    database: String,
    table: String,
) -> Result<Vec<TriggerInfo>, String> {
    metadata_timeout(
        "Table trigger metadata load",
        fetch_table_triggers_inner(pool, database, table),
    )
    .await
}

async fn fetch_table_triggers_inner(
    pool: DatabasePool,
    database: String,
    table: String,
) -> Result<Vec<TriggerInfo>, String> {
    match pool {
        DatabasePool::MySql(pool) => {
            if database.trim().is_empty() {
                return Err(String::from("Select a database to load triggers."));
            }

            let mut rows = sqlx::query(
                "SELECT trigger_name, action_timing, event_manipulation, \
                action_statement, definer \
                FROM information_schema.triggers \
                WHERE trigger_schema = ? \
                AND event_object_table = ? \
                ORDER BY trigger_name",
            )
            .bind(database)
            .bind(table)
            .fetch(&pool);

            let mut triggers = Vec::new();
            while let Some(row) = rows.try_next().await.map_err(|error| error.to_string())? {
                let name: String = row.try_get(0).map_err(|error| error.to_string())?;
                let timing: String = row.try_get(1).map_err(|error| error.to_string())?;
                let event: String = row.try_get(2).map_err(|error| error.to_string())?;
                let statement: String = row.try_get(3).map_err(|error| error.to_string())?;
                let definer: String = row.try_get(4).map_err(|error| error.to_string())?;
                triggers.push(TriggerInfo {
                    name,
                    timing,
                    event,
                    statement,
                    definer,
                });
            }

            Ok(triggers)
        }
        DatabasePool::Sqlite(pool) => {
            let schema = if database.trim().is_empty() {
                String::from("main")
            } else {
                database.trim().to_string()
            };
            let statement = format!(
                "SELECT name, sql \
                FROM \"{}\".sqlite_master \
                WHERE type = 'trigger' AND tbl_name = ? \
                ORDER BY name",
                escape_sqlite_identifier(&schema)
            );
            let mut rows = sqlx::query(AssertSqlSafe(statement))
                .bind(table)
                .fetch(&pool);

            let mut triggers = Vec::new();
            while let Some(row) = rows.try_next().await.map_err(|error| error.to_string())? {
                let name: String = row.try_get(0).map_err(|error| error.to_string())?;
                let statement: Option<String> =
                    row.try_get(1).map_err(|error| error.to_string())?;
                let sql = statement.unwrap_or_default();
                let upper = sql.to_ascii_uppercase();
                let timing = if upper.contains(" BEFORE ") {
                    String::from("BEFORE")
                } else if upper.contains(" AFTER ") {
                    String::from("AFTER")
                } else if upper.contains(" INSTEAD OF ") {
                    String::from("INSTEAD OF")
                } else {
                    String::from("UNKNOWN")
                };
                let event = if upper.contains(" INSERT ") {
                    String::from("INSERT")
                } else if upper.contains(" UPDATE ") {
                    String::from("UPDATE")
                } else if upper.contains(" DELETE ") {
                    String::from("DELETE")
                } else {
                    String::from("UNKNOWN")
                };
                triggers.push(TriggerInfo {
                    name,
                    timing,
                    event,
                    statement: sql,
                    definer: String::new(),
                });
            }

            Ok(triggers)
        }
        DatabasePool::Postgres(pool) => {
            ensure_postgres_current_database(&pool, &database).await?;

            let (schema, table_name) = postgres_table_reference_parts(&table);
            if table_name.trim().is_empty() {
                return Err(String::from("Select a table to load triggers."));
            }

            let mut rows = sqlx::query(
                "SELECT t.tgname AS trigger_name, \
                CASE \
                    WHEN (t.tgtype & 64) <> 0 THEN 'INSTEAD OF' \
                    WHEN (t.tgtype & 2) <> 0 THEN 'BEFORE' \
                    ELSE 'AFTER' \
                END AS action_timing, \
                TRIM(BOTH ' ' FROM CONCAT_WS(' OR', \
                    CASE WHEN (t.tgtype & 4) <> 0 THEN 'INSERT' END, \
                    CASE WHEN (t.tgtype & 16) <> 0 THEN 'UPDATE' END, \
                    CASE WHEN (t.tgtype & 8) <> 0 THEN 'DELETE' END, \
                    CASE WHEN (t.tgtype & 32) <> 0 THEN 'TRUNCATE' END \
                )) AS event_manipulation, \
                pg_get_triggerdef(t.oid, true) AS action_statement \
                FROM pg_trigger t \
                JOIN pg_class c ON c.oid = t.tgrelid \
                JOIN pg_namespace n ON n.oid = c.relnamespace \
                WHERE NOT t.tgisinternal \
                AND n.nspname = COALESCE($1::text, current_schema()) \
                AND c.relname = $2 \
                ORDER BY t.tgname",
            )
            .bind(schema)
            .bind(table_name)
            .fetch(&pool);

            let mut triggers = Vec::new();
            while let Some(row) = rows.try_next().await.map_err(|error| error.to_string())? {
                let name: String = row.try_get(0).map_err(|error| error.to_string())?;
                let timing: String = row.try_get(1).map_err(|error| error.to_string())?;
                let event: Option<String> = row.try_get(2).map_err(|error| error.to_string())?;
                let statement: String = row.try_get(3).map_err(|error| error.to_string())?;
                triggers.push(TriggerInfo {
                    name,
                    timing,
                    event: event.unwrap_or_else(|| String::from("UNKNOWN")),
                    statement,
                    definer: String::new(),
                });
            }

            Ok(triggers)
        }
    }
}

pub(crate) async fn update_mysql_trigger(
    pool: DatabasePool,
    database: String,
    table: String,
    old_name: String,
    trigger: TriggerInfo,
) -> Result<(), String> {
    metadata_timeout(
        "Trigger update",
        update_mysql_trigger_inner(pool, database, table, old_name, trigger),
    )
    .await
}

async fn update_mysql_trigger_inner(
    pool: DatabasePool,
    database: String,
    table: String,
    old_name: String,
    trigger: TriggerInfo,
) -> Result<(), String> {
    let DatabasePool::MySql(pool) = pool else {
        return Err(String::from(
            "Trigger update is only supported for MySQL/MariaDB.",
        ));
    };
    let database = database.trim();
    let table = table.trim();
    let old_name = old_name.trim();
    let name = trigger.name.trim();
    let timing = trigger.timing.trim();
    let event = trigger.event.trim();
    let body = trigger.statement.trim();
    if database.is_empty() || table.is_empty() || old_name.is_empty() || name.is_empty() {
        return Err(String::from("Trigger, table, and database are required."));
    }
    if timing.is_empty() || event.is_empty() || body.is_empty() {
        return Err(String::from(
            "Trigger timing, event, and body are required.",
        ));
    }

    let mut conn = pool.acquire().await.map_err(|error| error.to_string())?;
    let use_stmt = format!("USE `{}`", escape_mysql_identifier(database));
    conn.execute(AssertSqlSafe(use_stmt))
        .await
        .map_err(|error| error.to_string())?;

    let drop_stmt = format!(
        "DROP TRIGGER IF EXISTS `{}`",
        escape_mysql_identifier(old_name)
    );
    conn.execute(AssertSqlSafe(drop_stmt))
        .await
        .map_err(|error| error.to_string())?;

    let mut body = body.to_string();
    if !body.ends_with(';') {
        body.push(';');
    }
    let definer = format_definer(&trigger.definer);
    let create_stmt = if let Some(definer) = definer {
        format!(
            "CREATE {} TRIGGER `{}` {} {} ON `{}` FOR EACH ROW\n{}",
            definer,
            escape_mysql_identifier(name),
            timing,
            event,
            escape_mysql_identifier(table),
            body
        )
    } else {
        format!(
            "CREATE TRIGGER `{}` {} {} ON `{}` FOR EACH ROW\n{}",
            escape_mysql_identifier(name),
            timing,
            event,
            escape_mysql_identifier(table),
            body
        )
    };
    conn.execute(AssertSqlSafe(create_stmt))
        .await
        .map_err(|error| error.to_string())?;

    Ok(())
}

pub(crate) async fn fetch_table_relations(
    pool: DatabasePool,
    database: String,
    table: String,
) -> Result<Vec<RelationInfo>, String> {
    metadata_timeout(
        "Table relation metadata load",
        fetch_table_relations_inner(pool, database, table),
    )
    .await
}

async fn fetch_table_relations_inner(
    pool: DatabasePool,
    database: String,
    table: String,
) -> Result<Vec<RelationInfo>, String> {
    match pool {
        DatabasePool::MySql(pool) => {
            if database.trim().is_empty() {
                return Err(String::from("Select a database to load relations."));
            }

            let mut rows = sqlx::query(
                "SELECT column_name, referenced_table_name, \
                referenced_column_name \
                FROM information_schema.key_column_usage \
                WHERE table_schema = ? \
                AND table_name = ? \
                AND referenced_table_name IS NOT NULL \
                ORDER BY ordinal_position",
            )
            .bind(database)
            .bind(table)
            .fetch(&pool);

            let mut relations = Vec::new();
            while let Some(row) = rows.try_next().await.map_err(|error| error.to_string())? {
                let column: String = row.try_get(0).map_err(|error| error.to_string())?;
                let referenced_table: String = row.try_get(1).map_err(|error| error.to_string())?;
                let referenced_column: String =
                    row.try_get(2).map_err(|error| error.to_string())?;
                relations.push(RelationInfo {
                    column,
                    referenced_table,
                    referenced_column,
                });
            }

            Ok(relations)
        }
        DatabasePool::Sqlite(pool) => {
            let schema = if database.trim().is_empty() {
                String::from("main")
            } else {
                database.trim().to_string()
            };
            let statement = format!(
                "PRAGMA \"{}\".foreign_key_list('{}')",
                escape_sqlite_identifier(&schema),
                escape_sqlite_string_literal(table.trim())
            );
            let mut rows = sqlx::query(AssertSqlSafe(statement)).fetch(&pool);

            let mut relations = Vec::new();
            while let Some(row) = rows.try_next().await.map_err(|error| error.to_string())? {
                let referenced_table: String = row.try_get(2).map_err(|error| error.to_string())?;
                let column: String = row.try_get(3).map_err(|error| error.to_string())?;
                let referenced_column: Option<String> =
                    row.try_get(4).map_err(|error| error.to_string())?;
                relations.push(RelationInfo {
                    column,
                    referenced_table,
                    referenced_column: referenced_column.unwrap_or_default(),
                });
            }

            Ok(relations)
        }
        DatabasePool::Postgres(pool) => {
            ensure_postgres_current_database(&pool, &database).await?;

            let (schema, table_name) = postgres_table_reference_parts(&table);
            if table_name.trim().is_empty() {
                return Err(String::from("Select a table to load relations."));
            }

            let mut rows = sqlx::query(
                "SELECT kcu.column_name, ccu.table_schema, ccu.table_name, ccu.column_name \
                FROM information_schema.table_constraints tc \
                JOIN information_schema.key_column_usage kcu \
                ON tc.constraint_name = kcu.constraint_name \
                AND tc.table_schema = kcu.table_schema \
                JOIN information_schema.constraint_column_usage ccu \
                ON tc.constraint_name = ccu.constraint_name \
                AND tc.table_schema = ccu.table_schema \
                WHERE tc.constraint_type = 'FOREIGN KEY' \
                AND tc.table_schema = COALESCE($1::text, current_schema()) \
                AND tc.table_name = $2 \
                ORDER BY kcu.ordinal_position",
            )
            .bind(schema)
            .bind(table_name)
            .fetch(&pool);

            let mut relations = Vec::new();
            while let Some(row) = rows.try_next().await.map_err(|error| error.to_string())? {
                let column: String = row.try_get(0).map_err(|error| error.to_string())?;
                let referenced_schema: String =
                    row.try_get(1).map_err(|error| error.to_string())?;
                let referenced_table_name: String =
                    row.try_get(2).map_err(|error| error.to_string())?;
                let referenced_column: String =
                    row.try_get(3).map_err(|error| error.to_string())?;

                relations.push(RelationInfo {
                    column,
                    referenced_table: format!("{referenced_schema}.{referenced_table_name}"),
                    referenced_column,
                });
            }

            Ok(relations)
        }
    }
}

pub(crate) async fn fetch_sidebar_triggers(
    pool: DatabasePool,
    database: String,
) -> Result<Vec<SidebarTriggerEntry>, String> {
    metadata_timeout(
        "Sidebar trigger metadata load",
        fetch_sidebar_triggers_inner(pool, database),
    )
    .await
}

async fn fetch_sidebar_triggers_inner(
    pool: DatabasePool,
    database: String,
) -> Result<Vec<SidebarTriggerEntry>, String> {
    match pool {
        DatabasePool::MySql(pool) => {
            if database.trim().is_empty() {
                return Err(String::from("Select a database to load triggers."));
            }

            let mut rows = sqlx::query(
                "SELECT event_object_table, trigger_name, action_timing, event_manipulation, \
                action_statement, definer \
                FROM information_schema.triggers \
                WHERE trigger_schema = ? \
                ORDER BY event_object_table, trigger_name",
            )
            .bind(database)
            .fetch(&pool);

            let mut entries = Vec::new();
            while let Some(row) = rows.try_next().await.map_err(|error| error.to_string())? {
                let table: String = row.try_get(0).map_err(|error| error.to_string())?;
                let name: String = row.try_get(1).map_err(|error| error.to_string())?;
                let timing: String = row.try_get(2).map_err(|error| error.to_string())?;
                let event: String = row.try_get(3).map_err(|error| error.to_string())?;
                let statement: String = row.try_get(4).map_err(|error| error.to_string())?;
                let definer: String = row.try_get(5).map_err(|error| error.to_string())?;

                entries.push(SidebarTriggerEntry {
                    table,
                    trigger: TriggerInfo {
                        name,
                        timing,
                        event,
                        statement,
                        definer,
                    },
                });
            }

            Ok(entries)
        }
        DatabasePool::Sqlite(pool) => {
            let schema = if database.trim().is_empty() {
                String::from("main")
            } else {
                database.trim().to_string()
            };

            let statement = format!(
                "SELECT tbl_name, name, sql \
                FROM \"{}\".sqlite_master \
                WHERE type = 'trigger' \
                ORDER BY tbl_name, name",
                escape_sqlite_identifier(&schema)
            );
            let mut rows = sqlx::query(AssertSqlSafe(statement)).fetch(&pool);

            let mut entries = Vec::new();
            while let Some(row) = rows.try_next().await.map_err(|error| error.to_string())? {
                let table: String = row.try_get(0).map_err(|error| error.to_string())?;
                let name: String = row.try_get(1).map_err(|error| error.to_string())?;
                let statement: Option<String> =
                    row.try_get(2).map_err(|error| error.to_string())?;
                let sql = statement.unwrap_or_default();
                let upper = sql.to_ascii_uppercase();
                let timing = if upper.contains(" BEFORE ") {
                    String::from("BEFORE")
                } else if upper.contains(" AFTER ") {
                    String::from("AFTER")
                } else if upper.contains(" INSTEAD OF ") {
                    String::from("INSTEAD OF")
                } else {
                    String::from("UNKNOWN")
                };
                let event = if upper.contains(" INSERT ") {
                    String::from("INSERT")
                } else if upper.contains(" UPDATE ") {
                    String::from("UPDATE")
                } else if upper.contains(" DELETE ") {
                    String::from("DELETE")
                } else {
                    String::from("UNKNOWN")
                };

                entries.push(SidebarTriggerEntry {
                    table,
                    trigger: TriggerInfo {
                        name,
                        timing,
                        event,
                        statement: sql,
                        definer: String::new(),
                    },
                });
            }

            Ok(entries)
        }
        DatabasePool::Postgres(pool) => {
            ensure_postgres_current_database(&pool, &database).await?;

            let mut rows = sqlx::query(
                "SELECT n.nspname AS schema_name, c.relname AS table_name, \
                t.tgname AS trigger_name, \
                CASE \
                    WHEN (t.tgtype & 64) <> 0 THEN 'INSTEAD OF' \
                    WHEN (t.tgtype & 2) <> 0 THEN 'BEFORE' \
                    ELSE 'AFTER' \
                END AS action_timing, \
                TRIM(BOTH ' ' FROM CONCAT_WS(' OR', \
                    CASE WHEN (t.tgtype & 4) <> 0 THEN 'INSERT' END, \
                    CASE WHEN (t.tgtype & 16) <> 0 THEN 'UPDATE' END, \
                    CASE WHEN (t.tgtype & 8) <> 0 THEN 'DELETE' END, \
                    CASE WHEN (t.tgtype & 32) <> 0 THEN 'TRUNCATE' END \
                )) AS event_manipulation, \
                pg_get_triggerdef(t.oid, true) AS action_statement \
                FROM pg_trigger t \
                JOIN pg_class c ON c.oid = t.tgrelid \
                JOIN pg_namespace n ON n.oid = c.relnamespace \
                WHERE NOT t.tgisinternal \
                AND (n.nspname = 'public' \
                    OR (n.nspname NOT LIKE 'pg_%' AND n.nspname <> 'information_schema')) \
                ORDER BY n.nspname, c.relname, t.tgname",
            )
            .fetch(&pool);

            let mut entries = Vec::new();
            while let Some(row) = rows.try_next().await.map_err(|error| error.to_string())? {
                let schema: String = row.try_get(0).map_err(|error| error.to_string())?;
                let table_name: String = row.try_get(1).map_err(|error| error.to_string())?;
                let name: String = row.try_get(2).map_err(|error| error.to_string())?;
                let timing: String = row.try_get(3).map_err(|error| error.to_string())?;
                let event: Option<String> = row.try_get(4).map_err(|error| error.to_string())?;
                let statement: String = row.try_get(5).map_err(|error| error.to_string())?;

                entries.push(SidebarTriggerEntry {
                    table: format!("{schema}.{table_name}"),
                    trigger: TriggerInfo {
                        name,
                        timing,
                        event: event.unwrap_or_else(|| String::from("UNKNOWN")),
                        statement,
                        definer: String::new(),
                    },
                });
            }

            Ok(entries)
        }
    }
}

pub(crate) async fn fetch_sidebar_relations(
    pool: DatabasePool,
    database: String,
) -> Result<Vec<SidebarRelationEntry>, String> {
    metadata_timeout(
        "Sidebar relation metadata load",
        fetch_sidebar_relations_inner(pool, database),
    )
    .await
}

async fn fetch_sidebar_relations_inner(
    pool: DatabasePool,
    database: String,
) -> Result<Vec<SidebarRelationEntry>, String> {
    match pool {
        DatabasePool::MySql(pool) => {
            if database.trim().is_empty() {
                return Err(String::from("Select a database to load relations."));
            }

            let mut rows = sqlx::query(
                "SELECT table_name, column_name, referenced_table_name, referenced_column_name, constraint_name \
                FROM information_schema.key_column_usage \
                WHERE table_schema = ? \
                AND referenced_table_name IS NOT NULL \
                ORDER BY table_name, ordinal_position",
            )
            .bind(database)
            .fetch(&pool);

            let mut entries = Vec::new();
            while let Some(row) = rows.try_next().await.map_err(|error| error.to_string())? {
                let table: String = row.try_get(0).map_err(|error| error.to_string())?;
                let column: String = row.try_get(1).map_err(|error| error.to_string())?;
                let referenced_table: String = row.try_get(2).map_err(|error| error.to_string())?;
                let referenced_column: String =
                    row.try_get(3).map_err(|error| error.to_string())?;
                let constraint_name: String = row.try_get(4).map_err(|error| error.to_string())?;

                entries.push(SidebarRelationEntry {
                    table,
                    relation: RelationInfo {
                        column,
                        referenced_table,
                        referenced_column,
                    },
                    constraint_name,
                });
            }

            Ok(entries)
        }
        DatabasePool::Sqlite(pool) => {
            let schema = if database.trim().is_empty() {
                String::from("main")
            } else {
                database.trim().to_string()
            };

            let tables_sql = format!(
                "SELECT name \
                FROM \"{}\".sqlite_master \
                WHERE type = 'table' \
                AND name NOT LIKE 'sqlite_%' \
                ORDER BY name",
                escape_sqlite_identifier(&schema)
            );
            let mut table_rows = sqlx::query(AssertSqlSafe(tables_sql)).fetch(&pool);
            let mut tables = Vec::new();
            while let Some(row) = table_rows
                .try_next()
                .await
                .map_err(|error| error.to_string())?
            {
                let table: String = row.try_get(0).map_err(|error| error.to_string())?;
                if !table.trim().is_empty() {
                    tables.push(table);
                }
            }

            let mut entries = Vec::new();
            for table in tables {
                let relation_sql = format!(
                    "PRAGMA \"{}\".foreign_key_list('{}')",
                    escape_sqlite_identifier(&schema),
                    escape_sqlite_string_literal(table.trim())
                );
                let mut rows = sqlx::query(AssertSqlSafe(relation_sql)).fetch(&pool);
                while let Some(row) = rows.try_next().await.map_err(|error| error.to_string())? {
                    let id: i64 = row.try_get(0).map_err(|error| error.to_string())?;
                    let referenced_table: String =
                        row.try_get(2).map_err(|error| error.to_string())?;
                    let column: String = row.try_get(3).map_err(|error| error.to_string())?;
                    let referenced_column: Option<String> =
                        row.try_get(4).map_err(|error| error.to_string())?;

                    entries.push(SidebarRelationEntry {
                        table: table.clone(),
                        relation: RelationInfo {
                            column,
                            referenced_table,
                            referenced_column: referenced_column.unwrap_or_default(),
                        },
                        constraint_name: format!("fk_{table}_{id}"),
                    });
                }
            }

            Ok(entries)
        }
        DatabasePool::Postgres(pool) => {
            ensure_postgres_current_database(&pool, &database).await?;

            let mut rows = sqlx::query(
                "SELECT tc.table_schema, tc.table_name, kcu.column_name, \
                referenced.table_schema, referenced.table_name, referenced.column_name, tc.constraint_name \
                FROM information_schema.table_constraints tc \
                JOIN information_schema.key_column_usage kcu \
                ON tc.constraint_catalog = kcu.constraint_catalog \
                AND tc.constraint_schema = kcu.constraint_schema \
                AND tc.constraint_name = kcu.constraint_name \
                JOIN information_schema.referential_constraints rc \
                ON tc.constraint_catalog = rc.constraint_catalog \
                AND tc.constraint_schema = rc.constraint_schema \
                AND tc.constraint_name = rc.constraint_name \
                JOIN information_schema.key_column_usage referenced \
                ON rc.unique_constraint_catalog = referenced.constraint_catalog \
                AND rc.unique_constraint_schema = referenced.constraint_schema \
                AND rc.unique_constraint_name = referenced.constraint_name \
                AND kcu.position_in_unique_constraint = referenced.ordinal_position \
                WHERE tc.constraint_type = 'FOREIGN KEY' \
                AND (tc.table_schema = 'public' \
                    OR (tc.table_schema NOT LIKE 'pg_%' \
                        AND tc.table_schema <> 'information_schema')) \
                ORDER BY tc.table_schema, tc.table_name, kcu.ordinal_position",
            )
            .fetch(&pool);

            let mut entries = Vec::new();
            while let Some(row) = rows.try_next().await.map_err(|error| error.to_string())? {
                let table_schema: String = row.try_get(0).map_err(|error| error.to_string())?;
                let table_name: String = row.try_get(1).map_err(|error| error.to_string())?;
                let column: String = row.try_get(2).map_err(|error| error.to_string())?;
                let referenced_schema: String =
                    row.try_get(3).map_err(|error| error.to_string())?;
                let referenced_table_name: String =
                    row.try_get(4).map_err(|error| error.to_string())?;
                let referenced_column: String =
                    row.try_get(5).map_err(|error| error.to_string())?;
                let constraint_name: String = row.try_get(6).map_err(|error| error.to_string())?;

                entries.push(SidebarRelationEntry {
                    table: format!("{table_schema}.{table_name}"),
                    relation: RelationInfo {
                        column,
                        referenced_table: format!("{referenced_schema}.{referenced_table_name}"),
                        referenced_column,
                    },
                    constraint_name,
                });
            }

            Ok(entries)
        }
    }
}

pub(crate) async fn fetch_schema_diagram(
    pool: DatabasePool,
    database: String,
) -> Result<SchemaDiagramData, String> {
    metadata_timeout(
        "Schema diagram metadata load",
        fetch_schema_diagram_inner(pool, database),
    )
    .await
}

async fn fetch_schema_diagram_inner(
    pool: DatabasePool,
    database: String,
) -> Result<SchemaDiagramData, String> {
    let (tables, relations) = tokio::join!(
        fetch_schema_diagram_tables(pool.clone(), database.clone()),
        fetch_sidebar_relations_inner(pool, database),
    );
    let tables = tables?;
    let relations = relations.unwrap_or_default();
    Ok(SchemaDiagramData::build(tables, relations))
}

fn push_diagram_column(
    tables: &mut Vec<(String, Vec<DiagramColumn>)>,
    table: String,
    column: DiagramColumn,
) {
    match tables.last_mut() {
        Some((name, columns)) if *name == table => columns.push(column),
        _ => tables.push((table, vec![column])),
    }
}

async fn fetch_schema_diagram_tables(
    pool: DatabasePool,
    database: String,
) -> Result<Vec<(String, Vec<DiagramColumn>)>, String> {
    match pool {
        DatabasePool::MySql(pool) => {
            if database.trim().is_empty() {
                return Err(String::from("Select a database to build the diagram."));
            }

            let mut rows = sqlx::query(
                "SELECT c.table_name, c.column_name, c.column_type, c.column_key, c.is_nullable, \
                c.column_default, c.extra, COALESCE(indexes.indexed, 0), \
                COALESCE(indexes.unique_single, 0) \
                FROM information_schema.columns c \
                JOIN information_schema.tables t \
                ON t.table_schema = c.table_schema AND t.table_name = c.table_name \
                LEFT JOIN ( \
                    SELECT s.table_schema, s.table_name, s.column_name, 1 AS indexed, \
                    MAX(CASE WHEN s.non_unique = 0 AND members.column_count = 1 THEN 1 ELSE 0 END) \
                    AS unique_single \
                    FROM information_schema.statistics s \
                    JOIN ( \
                        SELECT table_schema, table_name, index_name, COUNT(*) AS column_count \
                        FROM information_schema.statistics \
                        WHERE table_schema = ? \
                        GROUP BY table_schema, table_name, index_name \
                    ) members ON members.table_schema = s.table_schema \
                        AND members.table_name = s.table_name AND members.index_name = s.index_name \
                    WHERE s.table_schema = ? \
                    GROUP BY s.table_schema, s.table_name, s.column_name \
                ) indexes ON indexes.table_schema = c.table_schema \
                    AND indexes.table_name = c.table_name AND indexes.column_name = c.column_name \
                WHERE c.table_schema = ? AND t.table_type = 'BASE TABLE' \
                ORDER BY c.table_name, c.ordinal_position",
            )
            .bind(database.trim())
            .bind(database.trim())
            .bind(database.trim())
            .fetch(&pool);

            let mut tables = Vec::new();
            while let Some(row) = rows.try_next().await.map_err(|error| error.to_string())? {
                let table: String = row.try_get(0).map_err(|error| error.to_string())?;
                let name: String = row.try_get(1).map_err(|error| error.to_string())?;
                let data_type: String = row.try_get(2).map_err(|error| error.to_string())?;
                let key: String = row.try_get(3).map_err(|error| error.to_string())?;
                let nullable: String = row.try_get(4).map_err(|error| error.to_string())?;
                let default: Option<String> = row.try_get(5).map_err(|error| error.to_string())?;
                let attributes: String = row.try_get(6).map_err(|error| error.to_string())?;
                let indexed: bool = row.try_get(7).map_err(|error| error.to_string())?;
                let unique: bool = row.try_get(8).map_err(|error| error.to_string())?;
                push_diagram_column(
                    &mut tables,
                    table,
                    DiagramColumn {
                        name,
                        data_type,
                        primary: key.eq_ignore_ascii_case("PRI"),
                        foreign: false,
                        nullable: nullable.eq_ignore_ascii_case("YES"),
                        default,
                        indexed,
                        unique,
                        attributes: attributes
                            .replace("DEFAULT_GENERATED", "")
                            .trim()
                            .to_string(),
                    },
                );
            }

            Ok(tables)
        }
        DatabasePool::Sqlite(pool) => {
            let schema = if database.trim().is_empty() {
                String::from("main")
            } else {
                database.trim().to_string()
            };

            let tables_sql = format!(
                "SELECT name \
                FROM \"{}\".sqlite_master \
                WHERE type = 'table' \
                AND name NOT LIKE 'sqlite_%' \
                ORDER BY name",
                escape_sqlite_identifier(&schema)
            );
            let mut table_rows = sqlx::query(AssertSqlSafe(tables_sql)).fetch(&pool);
            let mut names = Vec::new();
            while let Some(row) = table_rows
                .try_next()
                .await
                .map_err(|error| error.to_string())?
            {
                let table: String = row.try_get(0).map_err(|error| error.to_string())?;
                if !table.trim().is_empty() {
                    names.push(table);
                }
            }

            let mut tables = Vec::new();
            for table in names {
                let statement = format!(
                    "PRAGMA \"{}\".table_info('{}')",
                    escape_sqlite_identifier(&schema),
                    escape_sqlite_string_literal(table.trim())
                );
                let mut rows = sqlx::query(AssertSqlSafe(statement)).fetch(&pool);
                let mut columns = Vec::new();
                while let Some(row) = rows.try_next().await.map_err(|error| error.to_string())? {
                    let name: String = row.try_get(1).map_err(|error| error.to_string())?;
                    let data_type: String = row.try_get(2).map_err(|error| error.to_string())?;
                    let not_null: i64 = row.try_get(3).map_err(|error| error.to_string())?;
                    let default: Option<String> =
                        row.try_get(4).map_err(|error| error.to_string())?;
                    let primary: i64 = row.try_get(5).map_err(|error| error.to_string())?;
                    columns.push(DiagramColumn {
                        name,
                        data_type,
                        primary: primary > 0,
                        foreign: false,
                        nullable: not_null == 0 && primary == 0,
                        default,
                        indexed: primary > 0,
                        unique: false,
                        attributes: String::new(),
                    });
                }
                drop(rows);
                let primary_columns = columns.iter().filter(|column| column.primary).count();
                if primary_columns == 1
                    && let Some(column) = columns.iter_mut().find(|column| column.primary)
                {
                    column.unique = true;
                }
                let indexes_sql = format!(
                    "PRAGMA \"{}\".index_list('{}')",
                    escape_sqlite_identifier(&schema),
                    escape_sqlite_string_literal(table.trim())
                );
                let mut indexes = sqlx::query(AssertSqlSafe(indexes_sql)).fetch(&pool);
                let mut table_indexes = Vec::new();
                while let Some(row) = indexes
                    .try_next()
                    .await
                    .map_err(|error| error.to_string())?
                {
                    let index: String = row.try_get(1).map_err(|error| error.to_string())?;
                    let unique: i64 = row.try_get(2).map_err(|error| error.to_string())?;
                    table_indexes.push((index, unique != 0));
                }
                drop(indexes);
                for (index, unique) in table_indexes {
                    let columns_sql = format!(
                        "PRAGMA \"{}\".index_info('{}')",
                        escape_sqlite_identifier(&schema),
                        escape_sqlite_string_literal(&index)
                    );
                    let mut index_rows = sqlx::query(AssertSqlSafe(columns_sql)).fetch(&pool);
                    let mut indexed_columns = Vec::new();
                    while let Some(index_row) = index_rows
                        .try_next()
                        .await
                        .map_err(|error| error.to_string())?
                    {
                        indexed_columns.push(
                            index_row
                                .try_get::<String, _>(2)
                                .map_err(|error| error.to_string())?,
                        );
                    }
                    for column in &mut columns {
                        if indexed_columns
                            .iter()
                            .any(|name| name.eq_ignore_ascii_case(&column.name))
                        {
                            column.indexed = true;
                            column.unique |= unique && indexed_columns.len() == 1;
                        }
                    }
                }
                tables.push((table, columns));
            }

            Ok(tables)
        }
        DatabasePool::Postgres(pool) => {
            ensure_postgres_current_database(&pool, &database).await?;

            let mut rows = sqlx::query(
                "SELECT c.table_schema, c.table_name, c.column_name, c.data_type, \
                c.is_nullable, c.column_default, \
                EXISTS ( \
                    SELECT 1 FROM information_schema.table_constraints tc \
                    JOIN information_schema.key_column_usage kcu \
                    ON tc.constraint_name = kcu.constraint_name \
                    AND tc.table_schema = kcu.table_schema \
                    WHERE tc.constraint_type = 'PRIMARY KEY' \
                    AND tc.table_schema = c.table_schema \
                    AND tc.table_name = c.table_name \
                    AND kcu.column_name = c.column_name \
                ), \
                EXISTS ( \
                    SELECT 1 FROM pg_catalog.pg_index i \
                    JOIN pg_catalog.pg_class indexed_table ON indexed_table.oid = i.indrelid \
                    JOIN pg_catalog.pg_namespace indexed_schema ON indexed_schema.oid = indexed_table.relnamespace \
                    JOIN pg_catalog.pg_attribute indexed_column \
                    ON indexed_column.attrelid = indexed_table.oid \
                    AND indexed_column.attnum = ANY(i.indkey) \
                    WHERE indexed_schema.nspname = c.table_schema \
                    AND indexed_table.relname = c.table_name \
                    AND indexed_column.attname = c.column_name \
                ), \
                EXISTS ( \
                    SELECT 1 FROM pg_catalog.pg_index i \
                    JOIN pg_catalog.pg_class indexed_table ON indexed_table.oid = i.indrelid \
                    JOIN pg_catalog.pg_namespace indexed_schema ON indexed_schema.oid = indexed_table.relnamespace \
                    JOIN pg_catalog.pg_attribute indexed_column \
                    ON indexed_column.attrelid = indexed_table.oid \
                    AND indexed_column.attnum = ANY(i.indkey) \
                    WHERE indexed_schema.nspname = c.table_schema \
                    AND indexed_table.relname = c.table_name \
                    AND indexed_column.attname = c.column_name \
                    AND i.indisunique AND array_length(i.indkey::smallint[], 1) = 1 \
                ) \
                FROM information_schema.columns c \
                JOIN information_schema.tables t \
                ON t.table_schema = c.table_schema AND t.table_name = c.table_name \
                WHERE t.table_type = 'BASE TABLE' \
                AND (c.table_schema = 'public' \
                    OR (c.table_schema NOT LIKE 'pg_%' \
                        AND c.table_schema <> 'information_schema')) \
                ORDER BY c.table_schema, c.table_name, c.ordinal_position",
            )
            .fetch(&pool);

            let mut tables = Vec::new();
            while let Some(row) = rows.try_next().await.map_err(|error| error.to_string())? {
                let table_schema: String = row.try_get(0).map_err(|error| error.to_string())?;
                let table_name: String = row.try_get(1).map_err(|error| error.to_string())?;
                let name: String = row.try_get(2).map_err(|error| error.to_string())?;
                let data_type: String = row.try_get(3).map_err(|error| error.to_string())?;
                let nullable: String = row.try_get(4).map_err(|error| error.to_string())?;
                let default: Option<String> = row.try_get(5).map_err(|error| error.to_string())?;
                let primary: bool = row.try_get(6).map_err(|error| error.to_string())?;
                let indexed: bool = row.try_get(7).map_err(|error| error.to_string())?;
                let unique: bool = row.try_get(8).map_err(|error| error.to_string())?;
                push_diagram_column(
                    &mut tables,
                    format!("{table_schema}.{table_name}"),
                    DiagramColumn {
                        name,
                        data_type,
                        primary,
                        foreign: false,
                        nullable: nullable.eq_ignore_ascii_case("YES"),
                        default,
                        indexed,
                        unique,
                        attributes: String::new(),
                    },
                );
            }

            Ok(tables)
        }
    }
}

pub(crate) async fn fetch_databases(pool: DatabasePool) -> Result<Vec<String>, String> {
    metadata_timeout("Database list load", fetch_databases_inner(pool)).await
}

async fn fetch_databases_inner(pool: DatabasePool) -> Result<Vec<String>, String> {
    match pool {
        DatabasePool::MySql(pool) => {
            let mut rows = sqlx::query("SHOW DATABASES").fetch(&pool);

            let mut databases = Vec::new();
            while let Some(row) = rows.try_next().await.map_err(|error| error.to_string())? {
                let name: String = row.try_get(0).map_err(|error| error.to_string())?;
                databases.push(name);
            }

            Ok(databases)
        }
        DatabasePool::Sqlite(pool) => {
            let mut rows = sqlx::query("PRAGMA database_list").fetch(&pool);

            let mut databases = Vec::new();
            while let Some(row) = rows.try_next().await.map_err(|error| error.to_string())? {
                let name: String = row.try_get(1).map_err(|error| error.to_string())?;
                if !name.trim().is_empty() {
                    databases.push(name);
                }
            }
            if databases.is_empty() {
                databases.push(String::from("main"));
            }
            databases.sort();
            databases.dedup();
            Ok(databases)
        }
        DatabasePool::Postgres(pool) => {
            let mut rows = sqlx::query(
                "SELECT datname \
                FROM pg_database \
                WHERE datallowconn = true \
                AND datistemplate = false \
                ORDER BY datname",
            )
            .fetch(&pool);

            let mut databases = Vec::new();
            while let Some(row) = rows.try_next().await.map_err(|error| error.to_string())? {
                let name: String = row.try_get(0).map_err(|error| error.to_string())?;
                if !name.trim().is_empty() {
                    databases.push(name);
                }
            }

            databases.sort();
            databases.dedup();
            Ok(databases)
        }
    }
}

pub(crate) async fn fetch_table_ddl(
    pool: DatabasePool,
    database: String,
    table: String,
) -> Result<String, String> {
    metadata_timeout(
        "Table DDL load",
        fetch_table_ddl_inner(pool, database, table),
    )
    .await
}

async fn fetch_table_ddl_inner(
    pool: DatabasePool,
    database: String,
    table: String,
) -> Result<String, String> {
    match pool {
        DatabasePool::MySql(pool) => {
            let database = database.trim().to_string();
            let mut conn = pool.acquire().await.map_err(|error| error.to_string())?;
            if !database.is_empty() {
                let use_stmt = format!("USE `{}`", escape_mysql_identifier(&database));
                (&mut *conn)
                    .execute(AssertSqlSafe(use_stmt))
                    .await
                    .map_err(|error| error.to_string())?;
            }
            let statement = format!(
                "SHOW CREATE TABLE `{}`",
                escape_mysql_identifier(table.trim())
            );
            let row = sqlx::query(AssertSqlSafe(statement))
                .fetch_one(&mut *conn)
                .await
                .map_err(|error| error.to_string())?;
            let ddl: String = row.try_get(1).map_err(|error| error.to_string())?;
            Ok(format!("{ddl};"))
        }
        DatabasePool::Sqlite(pool) => {
            let schema = if database.trim().is_empty() {
                String::from("main")
            } else {
                database.trim().to_string()
            };
            let statement = format!(
                "SELECT sql FROM \"{}\".sqlite_master WHERE name = '{}' AND sql IS NOT NULL",
                escape_sqlite_identifier(&schema),
                escape_sqlite_string_literal(table.trim())
            );
            let row = sqlx::query(AssertSqlSafe(statement))
                .fetch_optional(&pool)
                .await
                .map_err(|error| error.to_string())?
                .ok_or_else(|| String::from("This object has no stored definition."))?;
            let ddl: String = row.try_get(0).map_err(|error| error.to_string())?;
            Ok(format!("{ddl};"))
        }
        DatabasePool::Postgres(pool) => {
            ensure_postgres_current_database(&pool, &database).await?;
            let (schema, table_name) = postgres_table_reference_parts(table.trim());
            let schema = schema.unwrap_or_else(|| String::from("public"));

            let mut rows = sqlx::query(
                "SELECT column_name, data_type, is_nullable, column_default                 FROM information_schema.columns                 WHERE table_schema = $1 AND table_name = $2                 ORDER BY ordinal_position",
            )
            .bind(&schema)
            .bind(&table_name)
            .fetch(&pool);

            let mut lines = Vec::new();
            while let Some(row) = rows.try_next().await.map_err(|error| error.to_string())? {
                let name: String = row.try_get(0).map_err(|error| error.to_string())?;
                let data_type: String = row.try_get(1).map_err(|error| error.to_string())?;
                let nullable: String = row.try_get(2).map_err(|error| error.to_string())?;
                let default: Option<String> = row.try_get(3).map_err(|error| error.to_string())?;
                let mut line = format!("  \"{name}\" {data_type}");
                if let Some(default) = default {
                    line.push_str(&format!(" DEFAULT {default}"));
                }
                if !nullable.eq_ignore_ascii_case("YES") {
                    line.push_str(" NOT NULL");
                }
                lines.push(line);
            }
            drop(rows);

            if lines.is_empty() {
                return Err(String::from("This object has no stored definition."));
            }

            let mut constraints = sqlx::query(
                "SELECT tc.constraint_type, tc.constraint_name,                 string_agg(kcu.column_name, ', ' ORDER BY kcu.ordinal_position)                 FROM information_schema.table_constraints tc                 JOIN information_schema.key_column_usage kcu                 ON tc.constraint_name = kcu.constraint_name                 AND tc.table_schema = kcu.table_schema                 WHERE tc.table_schema = $1 AND tc.table_name = $2                 AND tc.constraint_type IN ('PRIMARY KEY', 'UNIQUE')                 GROUP BY tc.constraint_type, tc.constraint_name                 ORDER BY tc.constraint_type",
            )
            .bind(&schema)
            .bind(&table_name)
            .fetch(&pool);

            while let Some(row) = constraints
                .try_next()
                .await
                .map_err(|error| error.to_string())?
            {
                let kind: String = row.try_get(0).map_err(|error| error.to_string())?;
                let name: String = row.try_get(1).map_err(|error| error.to_string())?;
                let columns: String = row.try_get(2).map_err(|error| error.to_string())?;
                lines.push(format!("  CONSTRAINT \"{name}\" {kind} ({columns})"));
            }

            Ok(format!(
                "CREATE TABLE \"{schema}\".\"{table_name}\" (\n{}\n);",
                lines.join(",\n")
            ))
        }
    }
}

pub(crate) async fn fetch_table_info(
    pool: DatabasePool,
    database: String,
    table: String,
) -> Result<TableInfoDetails, String> {
    metadata_timeout(
        "Table info metadata load",
        fetch_table_info_inner(pool, database, table),
    )
    .await
}

async fn fetch_table_info_inner(
    pool: DatabasePool,
    database: String,
    table: String,
) -> Result<TableInfoDetails, String> {
    match pool {
        DatabasePool::MySql(pool) => {
            let database = database.trim().to_string();
            if database.is_empty() {
                return Err(String::from("Select a database first."));
            }
            let table = table.trim().to_string();
            if table.is_empty() {
                return Err(String::from("Select a table first."));
            }

            let mut conn = pool.acquire().await.map_err(|error| error.to_string())?;

            let row = sqlx::query(
                "SELECT engine, table_rows, data_length, index_length, data_free, \
                auto_increment, table_collation, create_time, update_time, check_time, \
                row_format, avg_row_length, table_comment \
                FROM information_schema.tables \
                WHERE table_schema = ? AND table_name = ? \
                LIMIT 1",
            )
            .bind(database)
            .bind(table)
            .fetch_optional(&mut *conn)
            .await
            .map_err(|error| error.to_string())?
            .ok_or_else(|| String::from("Table info is not available."))?;

            let engine: Option<String> = row.try_get(0).map_err(|error| error.to_string())?;
            let table_rows: Option<u64> = row.try_get(1).map_err(|error| error.to_string())?;
            let data_length: Option<u64> = row.try_get(2).map_err(|error| error.to_string())?;
            let index_length: Option<u64> = row.try_get(3).map_err(|error| error.to_string())?;
            let data_free: Option<u64> = row.try_get(4).map_err(|error| error.to_string())?;
            let auto_increment: Option<u64> = row.try_get(5).map_err(|error| error.to_string())?;
            let table_collation: Option<String> =
                row.try_get(6).map_err(|error| error.to_string())?;
            let create_time: Option<NaiveDateTime> =
                row.try_get(7).map_err(|error| error.to_string())?;
            let update_time: Option<NaiveDateTime> =
                row.try_get(8).map_err(|error| error.to_string())?;
            let check_time: Option<NaiveDateTime> =
                row.try_get(9).map_err(|error| error.to_string())?;
            let row_format: Option<String> = row.try_get(10).map_err(|error| error.to_string())?;
            let avg_row_length: Option<u64> = row.try_get(11).map_err(|error| error.to_string())?;
            let table_comment: Option<String> =
                row.try_get(12).map_err(|error| error.to_string())?;

            let charset = table_collation
                .as_deref()
                .and_then(|value| value.split('_').next())
                .filter(|value| !value.trim().is_empty())
                .map(|value| value.to_string());

            Ok(TableInfoDetails {
                engine,
                rows: table_rows,
                data_size: data_length,
                index_size: index_length,
                free_size: data_free,
                avg_row_length,
                auto_increment,
                row_format,
                collation: table_collation,
                charset,
                created_at: format_optional_datetime(create_time),
                updated_at: format_optional_datetime(update_time),
                checked_at: format_optional_datetime(check_time),
                comment: table_comment,
            })
        }
        DatabasePool::Sqlite(pool) => {
            let database = if database.trim().is_empty() {
                String::from("main")
            } else {
                database.trim().to_string()
            };
            let table = table.trim().to_string();
            if table.is_empty() {
                return Err(String::from("Select a table first."));
            }

            let mut conn = pool.acquire().await.map_err(|error| error.to_string())?;
            let table_ref = format!(
                "\"{}\".\"{}\"",
                escape_sqlite_identifier(&database),
                escape_sqlite_identifier(&table)
            );

            let count_sql = format!("SELECT COUNT(*) FROM {}", table_ref);
            let table_rows: Option<u64> = sqlx::query_scalar::<_, i64>(AssertSqlSafe(count_sql))
                .fetch_optional(&mut *conn)
                .await
                .map_err(|error| error.to_string())?
                .map(|value| value.max(0) as u64);

            let page_size_sql = format!(
                "PRAGMA \"{}\".page_size",
                escape_sqlite_identifier(&database)
            );
            let page_count_sql = format!(
                "PRAGMA \"{}\".page_count",
                escape_sqlite_identifier(&database)
            );
            let freelist_sql = format!(
                "PRAGMA \"{}\".freelist_count",
                escape_sqlite_identifier(&database)
            );

            let page_size = sqlx::query_scalar::<_, i64>(AssertSqlSafe(page_size_sql))
                .fetch_optional(&mut *conn)
                .await
                .map_err(|error| error.to_string())?
                .unwrap_or(0)
                .max(0) as u64;
            let page_count = sqlx::query_scalar::<_, i64>(AssertSqlSafe(page_count_sql))
                .fetch_optional(&mut *conn)
                .await
                .map_err(|error| error.to_string())?
                .unwrap_or(0)
                .max(0) as u64;
            let freelist_count = sqlx::query_scalar::<_, i64>(AssertSqlSafe(freelist_sql))
                .fetch_optional(&mut *conn)
                .await
                .map_err(|error| error.to_string())?
                .unwrap_or(0)
                .max(0) as u64;

            let data_size = if page_size > 0 && page_count >= freelist_count {
                Some((page_count - freelist_count) * page_size)
            } else {
                None
            };
            let free_size = if page_size > 0 {
                Some(freelist_count * page_size)
            } else {
                None
            };

            let seq_sql = format!(
                "SELECT seq FROM \"{}\".sqlite_sequence WHERE name = ? LIMIT 1",
                escape_sqlite_identifier(&database)
            );
            let auto_increment = sqlx::query_scalar::<_, i64>(AssertSqlSafe(seq_sql))
                .bind(table.clone())
                .fetch_optional(&mut *conn)
                .await
                .ok()
                .flatten()
                .map(|value| value.max(0) as u64);

            Ok(TableInfoDetails {
                engine: Some(String::from("SQLite")),
                rows: table_rows,
                data_size,
                index_size: None,
                free_size,
                avg_row_length: None,
                auto_increment,
                row_format: None,
                collation: None,
                charset: None,
                created_at: None,
                updated_at: None,
                checked_at: None,
                comment: None,
            })
        }
        DatabasePool::Postgres(pool) => {
            ensure_postgres_current_database(&pool, &database).await?;

            let (schema, table_name) = postgres_table_reference_parts(&table);
            if table_name.trim().is_empty() {
                return Err(String::from("Select a table first."));
            }

            let relation_row = sqlx::query(
                "SELECT \
                CASE c.relkind \
                    WHEN 'r' THEN 'PostgreSQL Table' \
                    WHEN 'p' THEN 'PostgreSQL Partitioned Table' \
                    WHEN 'v' THEN 'PostgreSQL View' \
                    WHEN 'm' THEN 'PostgreSQL Materialized View' \
                    WHEN 'f' THEN 'PostgreSQL Foreign Table' \
                    ELSE 'PostgreSQL Relation' \
                END AS engine, \
                c.reltuples::double precision AS estimated_rows, \
                pg_relation_size(c.oid) AS data_size, \
                pg_indexes_size(c.oid) AS index_size, \
                pg_total_relation_size(c.oid) AS total_size, \
                am.amname AS row_format, \
                obj_description(c.oid, 'pg_class') AS comment \
                FROM pg_class c \
                JOIN pg_namespace n ON n.oid = c.relnamespace \
                LEFT JOIN pg_am am ON am.oid = c.relam \
                WHERE n.nspname = COALESCE($1::text, current_schema()) \
                AND c.relname = $2 \
                AND c.relkind IN ('r', 'p', 'v', 'm', 'f') \
                LIMIT 1",
            )
            .bind(schema)
            .bind(table_name)
            .fetch_optional(&pool)
            .await
            .map_err(|error| error.to_string())?
            .ok_or_else(|| String::from("Table info is not available."))?;

            let engine: Option<String> =
                relation_row.try_get(0).map_err(|error| error.to_string())?;
            let estimated_rows: Option<f64> =
                relation_row.try_get(1).map_err(|error| error.to_string())?;
            let data_size_raw: Option<i64> =
                relation_row.try_get(2).map_err(|error| error.to_string())?;
            let index_size_raw: Option<i64> =
                relation_row.try_get(3).map_err(|error| error.to_string())?;
            let total_size_raw: Option<i64> =
                relation_row.try_get(4).map_err(|error| error.to_string())?;
            let row_format: Option<String> =
                relation_row.try_get(5).map_err(|error| error.to_string())?;
            let comment: Option<String> =
                relation_row.try_get(6).map_err(|error| error.to_string())?;

            let rows = non_negative_f64_to_u64(estimated_rows);
            let data_size = non_negative_i64_to_u64(data_size_raw);
            let index_size = non_negative_i64_to_u64(index_size_raw);
            let total_size = non_negative_i64_to_u64(total_size_raw);

            let avg_row_length = match (rows, data_size) {
                (Some(row_count), Some(data)) if row_count > 0 => Some(data / row_count),
                _ => None,
            };

            let free_size = match (total_size, data_size, index_size) {
                (Some(total), Some(data), Some(index)) if total >= data.saturating_add(index) => {
                    Some(total - data - index)
                }
                _ => None,
            };

            let db_info = sqlx::query(
                "SELECT pg_encoding_to_char(encoding), datcollate \
                FROM pg_database \
                WHERE datname = current_database() \
                LIMIT 1",
            )
            .fetch_optional(&pool)
            .await
            .map_err(|error| error.to_string())?;

            let (charset, collation) = if let Some(row) = db_info {
                let charset: Option<String> = row.try_get(0).map_err(|error| error.to_string())?;
                let collation: Option<String> =
                    row.try_get(1).map_err(|error| error.to_string())?;
                (charset, collation)
            } else {
                (None, None)
            };

            Ok(TableInfoDetails {
                engine,
                rows,
                data_size,
                index_size,
                free_size,
                avg_row_length,
                auto_increment: None,
                row_format,
                collation,
                charset,
                created_at: None,
                updated_at: None,
                checked_at: None,
                comment,
            })
        }
    }
}

pub(crate) async fn fetch_table_column_nullability(
    pool: DatabasePool,
    database: &str,
    table: &str,
) -> Result<HashMap<String, bool>, String> {
    metadata_timeout(
        "Table column metadata load",
        fetch_table_column_nullability_inner(pool, database, table),
    )
    .await
}

async fn fetch_table_column_nullability_inner(
    pool: DatabasePool,
    database: &str,
    table: &str,
) -> Result<HashMap<String, bool>, String> {
    match pool {
        DatabasePool::MySql(pool) => {
            let mut rows = sqlx::query(
                "SELECT column_name, is_nullable \
                FROM information_schema.columns \
                WHERE table_schema = ? \
                AND table_name = ? \
                ORDER BY ordinal_position",
            )
            .bind(database)
            .bind(table)
            .fetch(&pool);

            let mut nullability = HashMap::new();
            while let Some(row) = rows.try_next().await.map_err(|error| error.to_string())? {
                let name: String = row.try_get(0).map_err(|error| error.to_string())?;
                let is_nullable: String = row.try_get(1).map_err(|error| error.to_string())?;
                nullability.insert(name, is_nullable.eq_ignore_ascii_case("yes"));
            }

            Ok(nullability)
        }
        DatabasePool::Sqlite(pool) => {
            let schema = if database.trim().is_empty() {
                String::from("main")
            } else {
                database.trim().to_string()
            };
            let statement = format!(
                "PRAGMA \"{}\".table_info('{}')",
                escape_sqlite_identifier(&schema),
                escape_sqlite_string_literal(table.trim())
            );
            let mut rows = sqlx::query(AssertSqlSafe(statement)).fetch(&pool);

            let mut nullability = HashMap::new();
            while let Some(row) = rows.try_next().await.map_err(|error| error.to_string())? {
                let name: String = row.try_get(1).map_err(|error| error.to_string())?;
                let not_null: i64 = row.try_get(3).map_err(|error| error.to_string())?;
                let pk: i64 = row.try_get(5).map_err(|error| error.to_string())?;
                nullability.insert(name, not_null == 0 && pk == 0);
            }
            Ok(nullability)
        }
        DatabasePool::Postgres(pool) => {
            ensure_postgres_current_database(&pool, database).await?;

            let (schema, table_name) = postgres_table_reference_parts(table);
            if table_name.trim().is_empty() {
                return Err(String::from("Select a table to inspect columns."));
            }

            let mut rows = sqlx::query(
                "SELECT column_name, is_nullable \
                FROM information_schema.columns \
                WHERE table_schema = COALESCE($1::text, current_schema()) \
                AND table_name = $2 \
                ORDER BY ordinal_position",
            )
            .bind(schema)
            .bind(table_name)
            .fetch(&pool);

            let mut nullability = HashMap::new();
            while let Some(row) = rows.try_next().await.map_err(|error| error.to_string())? {
                let name: String = row.try_get(0).map_err(|error| error.to_string())?;
                let is_nullable: String = row.try_get(1).map_err(|error| error.to_string())?;
                nullability.insert(name, is_nullable.eq_ignore_ascii_case("yes"));
            }

            Ok(nullability)
        }
    }
}

pub(crate) async fn fetch_query_suggestion_columns(
    pool: DatabasePool,
    database: String,
    table: String,
) -> Result<Vec<String>, String> {
    metadata_timeout(
        "Query suggestion column metadata load",
        fetch_query_suggestion_columns_inner(pool, database, table),
    )
    .await
}

async fn fetch_query_suggestion_columns_inner(
    pool: DatabasePool,
    database: String,
    table: String,
) -> Result<Vec<String>, String> {
    match pool {
        DatabasePool::MySql(pool) => {
            if database.trim().is_empty() {
                return Err(String::from("Select a database first."));
            }

            let mut rows = sqlx::query(
                "SELECT column_name \
                FROM information_schema.columns \
                WHERE table_schema = ? \
                AND table_name = ? \
                ORDER BY ordinal_position",
            )
            .bind(database.trim())
            .bind(table.trim())
            .fetch(&pool);

            let mut columns = Vec::new();
            while let Some(row) = rows.try_next().await.map_err(|error| error.to_string())? {
                let name: String = row.try_get(0).map_err(|error| error.to_string())?;
                if !name.trim().is_empty() {
                    columns.push(name);
                }
            }

            Ok(columns)
        }
        DatabasePool::Sqlite(pool) => {
            let schema = if database.trim().is_empty() {
                String::from("main")
            } else {
                database.trim().to_string()
            };
            let statement = format!(
                "PRAGMA \"{}\".table_info('{}')",
                escape_sqlite_identifier(&schema),
                escape_sqlite_string_literal(table.trim())
            );
            let mut rows = sqlx::query(AssertSqlSafe(statement)).fetch(&pool);

            let mut columns = Vec::new();
            while let Some(row) = rows.try_next().await.map_err(|error| error.to_string())? {
                let name: String = row.try_get(1).map_err(|error| error.to_string())?;
                if !name.trim().is_empty() {
                    columns.push(name);
                }
            }

            Ok(columns)
        }
        DatabasePool::Postgres(pool) => {
            ensure_postgres_current_database(&pool, &database).await?;

            let (schema, table_name) = postgres_table_reference_parts(table.trim());
            if table_name.trim().is_empty() {
                return Err(String::from("Select a table first."));
            }

            let mut rows = sqlx::query(
                "SELECT column_name \
                FROM information_schema.columns \
                WHERE table_schema = COALESCE($1::text, current_schema()) \
                AND table_name = $2 \
                ORDER BY ordinal_position",
            )
            .bind(schema)
            .bind(table_name)
            .fetch(&pool);

            let mut columns = Vec::new();
            while let Some(row) = rows.try_next().await.map_err(|error| error.to_string())? {
                let name: String = row.try_get(0).map_err(|error| error.to_string())?;
                if !name.trim().is_empty() {
                    columns.push(name);
                }
            }

            Ok(columns)
        }
    }
}

pub(crate) async fn fetch_primary_keys(
    pool: DatabasePool,
    database: &str,
    table: &str,
) -> Result<Vec<String>, String> {
    metadata_timeout(
        "Primary key metadata load",
        fetch_primary_keys_inner(pool, database, table),
    )
    .await
}

async fn fetch_primary_keys_inner(
    pool: DatabasePool,
    database: &str,
    table: &str,
) -> Result<Vec<String>, String> {
    match pool {
        DatabasePool::MySql(pool) => {
            let mut rows = sqlx::query(
                "SELECT column_name \
                FROM information_schema.columns \
                WHERE table_schema = ? \
                AND table_name = ? \
                AND column_key = 'PRI' \
                ORDER BY ordinal_position",
            )
            .bind(database)
            .bind(table)
            .fetch(&pool);

            let mut keys = Vec::new();
            while let Some(row) = rows.try_next().await.map_err(|error| error.to_string())? {
                let name: String = row.try_get(0).map_err(|error| error.to_string())?;
                keys.push(name);
            }

            Ok(keys)
        }
        DatabasePool::Sqlite(pool) => {
            let schema = if database.trim().is_empty() {
                String::from("main")
            } else {
                database.trim().to_string()
            };
            let statement = format!(
                "PRAGMA \"{}\".table_info('{}')",
                escape_sqlite_identifier(&schema),
                escape_sqlite_string_literal(table.trim())
            );
            let mut rows = sqlx::query(AssertSqlSafe(statement)).fetch(&pool);

            let mut keyed: Vec<(i64, String)> = Vec::new();
            while let Some(row) = rows.try_next().await.map_err(|error| error.to_string())? {
                let name: String = row.try_get(1).map_err(|error| error.to_string())?;
                let pk: i64 = row.try_get(5).map_err(|error| error.to_string())?;
                if pk > 0 {
                    keyed.push((pk, name));
                }
            }
            keyed.sort_by_key(|(order, _)| *order);
            Ok(keyed.into_iter().map(|(_, name)| name).collect())
        }
        DatabasePool::Postgres(pool) => {
            ensure_postgres_current_database(&pool, database).await?;

            let (schema, table_name) = postgres_table_reference_parts(table);
            if table_name.trim().is_empty() {
                return Err(String::from("Select a table to inspect primary keys."));
            }

            let mut rows = sqlx::query(
                "SELECT kcu.column_name \
                FROM information_schema.table_constraints tc \
                JOIN information_schema.key_column_usage kcu \
                ON tc.constraint_name = kcu.constraint_name \
                AND tc.table_schema = kcu.table_schema \
                WHERE tc.constraint_type = 'PRIMARY KEY' \
                AND tc.table_schema = COALESCE($1::text, current_schema()) \
                AND tc.table_name = $2 \
                ORDER BY kcu.ordinal_position",
            )
            .bind(schema)
            .bind(table_name)
            .fetch(&pool);

            let mut keys = Vec::new();
            while let Some(row) = rows.try_next().await.map_err(|error| error.to_string())? {
                let name: String = row.try_get(0).map_err(|error| error.to_string())?;
                keys.push(name);
            }

            Ok(keys)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{fetch_schema_diagram, fetch_tables};
    use crate::DatabasePool;
    use std::time::Instant;

    #[test]
    fn fetch_table_ddl_sqlite_returns_the_stored_definition() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("tokio runtime");

        runtime.block_on(async {
            let temp_dir = tempfile::tempdir().expect("temp dir");
            let path = temp_dir.path().join("ddl.db");
            let pool = sqlx::sqlite::SqlitePoolOptions::new()
                .max_connections(1)
                .connect_with(
                    sqlx::sqlite::SqliteConnectOptions::new()
                        .filename(&path)
                        .create_if_missing(true),
                )
                .await
                .expect("open sqlite pool");

            sqlx::query(sqlx::AssertSqlSafe(String::from(
                "CREATE TABLE invoices (id INTEGER PRIMARY KEY, total REAL NOT NULL)",
            )))
            .execute(&pool)
            .await
            .expect("create table");

            let ddl = super::fetch_table_ddl(
                DatabasePool::Sqlite(pool.clone()),
                String::new(),
                String::from("invoices"),
            )
            .await
            .expect("ddl loads");

            assert!(ddl.starts_with("CREATE TABLE invoices"));
            assert!(ddl.contains("total REAL NOT NULL"));
            assert!(ddl.trim_end().ends_with(';'));

            let missing = super::fetch_table_ddl(
                DatabasePool::Sqlite(pool.clone()),
                String::new(),
                String::from("nope"),
            )
            .await;
            assert!(missing.is_err());

            pool.close().await;
        });
    }

    #[test]
    fn fetch_schema_diagram_sqlite_reads_columns_keys_and_relations() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("tokio runtime");

        runtime.block_on(async {
            let temp_dir = tempfile::tempdir().expect("temp dir");
            let path = temp_dir.path().join("diagram.db");
            let pool = sqlx::sqlite::SqlitePoolOptions::new()
                .max_connections(1)
                .connect_with(
                    sqlx::sqlite::SqliteConnectOptions::new()
                        .filename(&path)
                        .create_if_missing(true),
                )
                .await
                .expect("open sqlite pool");

            for statement in [
                "CREATE TABLE customers (id INTEGER PRIMARY KEY, name TEXT UNIQUE)",
                "CREATE TABLE orders (id INTEGER PRIMARY KEY, customer_id INTEGER \
                 REFERENCES customers(id), status TEXT DEFAULT 'new')",
                "CREATE INDEX orders_customer_id_idx ON orders(customer_id)",
                "CREATE TABLE profiles (id INTEGER PRIMARY KEY, customer_id INTEGER UNIQUE \
                 REFERENCES customers(id))",
            ] {
                sqlx::query(sqlx::AssertSqlSafe(statement.to_string()))
                    .execute(&pool)
                    .await
                    .expect("create table");
            }

            let diagram = fetch_schema_diagram(DatabasePool::Sqlite(pool.clone()), String::new())
                .await
                .expect("fetch_schema_diagram succeeds");

            assert_eq!(diagram.tables.len(), 3);
            let orders = diagram
                .tables
                .iter()
                .find(|table| table.name == "orders")
                .expect("orders table");
            assert!(orders.columns[0].primary);
            assert!(orders.columns[1].foreign);
            assert!(orders.columns[1].indexed);
            assert!(!orders.columns[1].unique);
            assert_eq!(orders.columns[1].data_type, "INTEGER");
            assert_eq!(orders.columns[2].default.as_deref(), Some("'new'"));
            let profiles = diagram
                .tables
                .iter()
                .find(|table| table.name == "profiles")
                .expect("profiles table");
            assert!(profiles.columns[1].unique);
            assert_eq!(diagram.edges.len(), 2);
            assert!(diagram.edges.iter().any(|edge| edge.one_to_one));
            assert!(
                diagram
                    .edges
                    .iter()
                    .all(|edge| !edge.constraint_name.is_empty())
            );

            pool.close().await;
        });
    }

    #[test]
    fn fetch_schema_diagram_mysql_reads_a_live_server() {
        let Ok(url) = std::env::var("CRYODB_TEST_MYSQL_URL") else {
            return;
        };
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("tokio runtime");

        runtime.block_on(async {
            let pool = sqlx::mysql::MySqlPoolOptions::new()
                .max_connections(1)
                .connect(&url)
                .await
                .expect("connect to the MySQL server named by CRYODB_TEST_MYSQL_URL");
            let database = sqlx::query_scalar::<_, Option<String>>("SELECT DATABASE()")
                .fetch_one(&pool)
                .await
                .expect("selected database")
                .unwrap_or_default();
            assert!(
                !database.is_empty(),
                "CRYODB_TEST_MYSQL_URL needs a database"
            );

            for statement in [
                "DROP TABLE IF EXISTS cryodb_orders",
                "DROP TABLE IF EXISTS cryodb_customers",
                "CREATE TABLE cryodb_customers (id INT PRIMARY KEY)",
                "CREATE TABLE cryodb_orders (id INT PRIMARY KEY, customer_id INT NOT NULL)",
            ] {
                sqlx::query(sqlx::AssertSqlSafe(String::from(statement)))
                    .execute(&pool)
                    .await
                    .expect("seed schema");
            }

            let applied = crate::db::edits::run_schema_script(
                DatabasePool::MySql(pool.clone()),
                database.clone(),
                vec![
                    String::from("ALTER TABLE cryodb_orders ADD COLUMN note VARCHAR(40) NULL;"),
                    String::from(
                        "ALTER TABLE cryodb_orders ADD CONSTRAINT fk_cryodb_orders_customer_id \
                         FOREIGN KEY (customer_id) REFERENCES cryodb_customers (id);",
                    ),
                ],
            )
            .await
            .expect("schema script runs");
            assert_eq!(applied, 2);

            let diagram = fetch_schema_diagram(DatabasePool::MySql(pool.clone()), database.clone())
                .await
                .expect("diagram loads");
            let orders = diagram
                .tables
                .iter()
                .find(|table| table.name == "cryodb_orders")
                .expect("the seeded table is on the diagram");
            assert!(orders.columns.iter().any(|column| column.name == "note"));
            assert!(
                orders
                    .columns
                    .iter()
                    .any(|column| column.name == "id" && column.primary)
            );
            assert!(
                diagram
                    .edges
                    .iter()
                    .any(|edge| edge.constraint_name == "fk_cryodb_orders_customer_id")
            );

            let ddl = super::fetch_table_ddl(
                DatabasePool::MySql(pool.clone()),
                database,
                String::from("cryodb_orders"),
            )
            .await
            .expect("ddl loads");
            assert!(ddl.contains("cryodb_orders"));
            assert!(ddl.contains("note"));

            for statement in [
                "DROP TABLE IF EXISTS cryodb_orders",
                "DROP TABLE IF EXISTS cryodb_customers",
            ] {
                sqlx::query(sqlx::AssertSqlSafe(String::from(statement)))
                    .execute(&pool)
                    .await
                    .expect("clean up");
            }
            pool.close().await;
        });
    }

    #[test]
    fn fetch_schema_diagram_postgres_reads_a_live_server() {
        let Ok(url) = std::env::var("CRYODB_TEST_POSTGRES_URL") else {
            return;
        };
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("tokio runtime");

        runtime.block_on(async {
            let pool = sqlx::postgres::PgPoolOptions::new()
                .max_connections(1)
                .connect(&url)
                .await
                .expect("connect to the PostgreSQL server named by CRYODB_TEST_POSTGRES_URL");
            let database: String = sqlx::query_scalar("SELECT current_database()")
                .fetch_one(&pool)
                .await
                .expect("selected database");

            for statement in [
                "DROP TABLE IF EXISTS cryodb_orders",
                "DROP TABLE IF EXISTS cryodb_customers",
                "CREATE TABLE cryodb_customers (id INT, tenant_id INT, PRIMARY KEY (id, tenant_id))",
                "CREATE TABLE cryodb_orders (id INT PRIMARY KEY, customer_id INT NOT NULL, tenant_id INT NOT NULL)",
            ] {
                sqlx::query(sqlx::AssertSqlSafe(String::from(statement)))
                    .execute(&pool)
                    .await
                    .expect("seed schema");
            }

            let applied = crate::db::edits::run_schema_script(
                DatabasePool::Postgres(pool.clone()),
                database.clone(),
                vec![
                    String::from("ALTER TABLE cryodb_orders ADD COLUMN note VARCHAR(40);"),
                    String::from(
                        "ALTER TABLE cryodb_orders ADD CONSTRAINT fk_cryodb_orders_customer_id \
                         FOREIGN KEY (customer_id, tenant_id) REFERENCES cryodb_customers (id, tenant_id);",
                    ),
                ],
            )
            .await
            .expect("schema script runs");
            assert_eq!(applied, 2);

            let diagram =
                fetch_schema_diagram(DatabasePool::Postgres(pool.clone()), database.clone())
                    .await
                    .expect("diagram loads");
            let orders = diagram
                .tables
                .iter()
                .find(|table| table.name.ends_with("cryodb_orders"))
                .expect("the seeded table is on the diagram");
            assert!(orders.columns.iter().any(|column| column.name == "note"));
            assert!(
                orders
                    .columns
                    .iter()
                    .any(|column| column.name == "id" && column.primary)
            );
            assert!(
                diagram
                    .edges
                    .iter()
                    .any(|edge| edge.constraint_name == "fk_cryodb_orders_customer_id")
            );
            assert_eq!(
                diagram
                    .edges
                    .iter()
                    .filter(|edge| edge.constraint_name == "fk_cryodb_orders_customer_id")
                    .count(),
                2
            );

            let ddl = super::fetch_table_ddl(
                DatabasePool::Postgres(pool.clone()),
                database,
                String::from("public.cryodb_orders"),
            )
            .await
            .expect("ddl loads");
            assert!(ddl.contains("cryodb_orders"));
            assert!(ddl.contains("note"));

            for statement in [
                "DROP TABLE IF EXISTS cryodb_orders",
                "DROP TABLE IF EXISTS cryodb_customers",
            ] {
                sqlx::query(sqlx::AssertSqlSafe(String::from(statement)))
                    .execute(&pool)
                    .await
                    .expect("clean up");
            }
            pool.close().await;
        });
    }

    #[test]
    fn fetch_tables_sqlite_large_table_count_stays_under_timeout() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("tokio runtime");

        runtime.block_on(async {
            let temp_dir = tempfile::tempdir().expect("temp dir");
            let path = temp_dir.path().join("stress-tables.db");
            let pool = sqlx::sqlite::SqlitePoolOptions::new()
                .max_connections(1)
                .connect_with(
                    sqlx::sqlite::SqliteConnectOptions::new()
                        .filename(&path)
                        .create_if_missing(true),
                )
                .await
                .expect("open sqlite pool");

            const TABLE_COUNT: usize = 500;
            for index in 0..TABLE_COUNT {
                let statement =
                    format!("CREATE TABLE stress_t_{index} (id INTEGER PRIMARY KEY, value TEXT)");
                sqlx::query(sqlx::AssertSqlSafe(statement))
                    .execute(&pool)
                    .await
                    .expect("create stress table");
            }

            let started = Instant::now();
            let tables = fetch_tables(DatabasePool::Sqlite(pool.clone()), String::new())
                .await
                .expect("fetch_tables succeeds");
            let elapsed = started.elapsed();

            assert_eq!(tables.len(), TABLE_COUNT);
            assert!(
                elapsed.as_secs() < crate::METADATA_TIMEOUT_SECS,
                "fetch_tables for {TABLE_COUNT} tables took {elapsed:?}, expected < {}s",
                crate::METADATA_TIMEOUT_SECS
            );

            eprintln!("fetch_tables sqlite: {TABLE_COUNT} tables loaded in {elapsed:?}");

            pool.close().await;
        });
    }
}
