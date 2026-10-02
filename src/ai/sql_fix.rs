use crate::ai::client::{
    ai_retry_backoff, clean_ai_output, is_retryable_ai_error, parse_ai_error,
    parse_anthropic_content, parse_ollama_content, parse_ollama_generate_content,
    parse_openai_content, resolve_ai_endpoint,
};
use crate::ai::{
    AiEndpointKind, AiMessage, AiRequestConfig, AiSqlFixContext, AiSqlFixPromptPayload,
    AiSqlFixResponse, AiSqlFixResponseRaw, AiSqlFixSchemaColumn, AiSqlFixSchemaTable,
    AnthropicMessagesRequest, OllamaChatRequest, OllamaGenerateRequest, OpenAiChatRequest,
};
use crate::utils::sql_parse::{SqlToken, parse_single_table_select, sql_tokens};
use crate::{
    AI_REQUEST_MAX_RETRIES, AI_REQUEST_TIMEOUT_SECS, AI_SQL_FIX_MAX_COLUMNS_PER_TABLE,
    AI_SQL_FIX_MAX_TABLES, AI_SQL_FIX_SYSTEM_PROMPT, ANTHROPIC_API_VERSION, CONNECT_TIMEOUT_SECS,
    DatabaseDriver, DatabasePool, escape_sqlite_identifier, escape_sqlite_string_literal,
};
use futures_util::TryStreamExt;
use reqwest::Client;
use sqlx::{AssertSqlSafe, Row};
use std::collections::HashMap;
use std::time::Duration;

pub(crate) fn should_offer_ai_query_fix(error: &str) -> bool {
    let normalized = error.to_ascii_lowercase();
    normalized.contains("syntax error")
        || normalized.contains("error in your sql syntax")
        || normalized.contains("parse error")
        || (normalized.contains("near") && normalized.contains("syntax"))
}

fn ai_sql_fix_driver(driver: DatabaseDriver) -> &'static str {
    match driver {
        DatabaseDriver::MySql | DatabaseDriver::MariaDb => "mysql",
        DatabaseDriver::Sqlite => "sqlite",
        DatabaseDriver::PostgreSql => "postgres",
    }
}

fn ai_sql_fix_table_candidates_from_query(query: &str) -> Vec<String> {
    let tokens = sql_tokens(query);
    let mut output = Vec::new();
    let mut expect_table = false;
    let mut index = 0usize;

    while let Some(token) = tokens.get(index) {
        match token {
            SqlToken::Word(word) => {
                if expect_table {
                    if word.eq_ignore_ascii_case("select")
                        || word.eq_ignore_ascii_case("values")
                        || word.eq_ignore_ascii_case("set")
                    {
                        expect_table = false;
                        index += 1;
                        continue;
                    }

                    let mut table = word.clone();
                    if matches!(tokens.get(index + 1), Some(SqlToken::Dot))
                        && let Some(SqlToken::Word(next)) = tokens.get(index + 2)
                    {
                        table = format!("{word}.{next}");
                        index += 2;
                    }

                    if !table.trim().is_empty() {
                        output.push(table);
                    }
                    expect_table = false;
                } else if word.eq_ignore_ascii_case("from")
                    || word.eq_ignore_ascii_case("join")
                    || word.eq_ignore_ascii_case("update")
                    || word.eq_ignore_ascii_case("into")
                    || word.eq_ignore_ascii_case("table")
                {
                    expect_table = true;
                }
            }
            SqlToken::LParen if expect_table => {
                expect_table = false;
            }
            _ => {}
        }
        index += 1;
    }

    output
}

pub(crate) fn resolve_ai_sql_fix_tables(
    query: &str,
    known_tables: &[String],
    selected_table: Option<&str>,
) -> Vec<String> {
    let known_lookup = known_tables
        .iter()
        .map(|table| (table.to_ascii_lowercase(), table.clone()))
        .collect::<HashMap<_, _>>();
    let mut base_counts = HashMap::<String, usize>::new();
    for table in known_tables {
        if let Some((_, base)) = table.rsplit_once('.') {
            *base_counts.entry(base.to_ascii_lowercase()).or_insert(0) += 1;
        }
    }
    let mut base_lookup = HashMap::<String, String>::new();
    for table in known_tables {
        if let Some((_, base)) = table.rsplit_once('.') {
            let key = base.to_ascii_lowercase();
            if base_counts.get(&key).copied().unwrap_or(0) == 1 {
                base_lookup.entry(key).or_insert_with(|| table.clone());
            }
        }
    }

    let mut candidates = ai_sql_fix_table_candidates_from_query(query);
    if candidates.is_empty()
        && let Some(table) = parse_single_table_select(query)
    {
        candidates.push(table);
    }
    if candidates.is_empty()
        && let Some(table) = selected_table
    {
        candidates.push(table.to_string());
    }

    let mut resolved = Vec::new();
    for candidate in candidates {
        let cleaned = candidate
            .trim()
            .trim_matches(|ch| matches!(ch, '`' | '"' | '\''))
            .to_string();
        if cleaned.is_empty() {
            continue;
        }

        let lookup_key = cleaned.to_ascii_lowercase();
        let table_name = known_lookup
            .get(&lookup_key)
            .cloned()
            .or_else(|| {
                let base = cleaned.rsplit('.').next()?.to_ascii_lowercase();
                known_lookup
                    .get(&base)
                    .cloned()
                    .or_else(|| base_lookup.get(&base).cloned())
            })
            .unwrap_or_else(|| cleaned.clone());

        if resolved
            .iter()
            .any(|existing: &String| existing.eq_ignore_ascii_case(&table_name))
        {
            continue;
        }
        resolved.push(table_name);
        if resolved.len() >= AI_SQL_FIX_MAX_TABLES {
            break;
        }
    }

    resolved
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

fn parse_ai_sql_fix_response(response: &str) -> Result<AiSqlFixResponse, String> {
    let cleaned = clean_ai_output(response);
    let raw = serde_json::from_str::<AiSqlFixResponseRaw>(&cleaned)
        .map_err(|error| format!("Failed to parse AI fix response: {error}"))?;

    let explanation = raw
        .explanation
        .unwrap_or_else(|| String::from("AI did not provide an explanation."))
        .trim()
        .to_string();

    let mut confidence = raw.confidence.unwrap_or(0.0);
    if !confidence.is_finite() {
        confidence = 0.0;
    }
    confidence = confidence.clamp(0.0, 1.0);

    let mut fix_type = raw
        .fix_type
        .unwrap_or_else(|| String::from("unknown"))
        .trim()
        .to_ascii_lowercase();
    if !matches!(
        fix_type.as_str(),
        "syntax" | "schema" | "alias" | "join" | "driver" | "unknown"
    ) {
        fix_type = String::from("unknown");
    }

    let fixed_query = raw
        .fixed_query
        .map(|query| query.trim().to_string())
        .filter(|query| !query.is_empty());

    let diff_summary = raw
        .diff_summary
        .unwrap_or_default()
        .into_iter()
        .map(|entry| entry.trim().to_string())
        .filter(|entry| !entry.is_empty())
        .take(8)
        .collect::<Vec<_>>();

    Ok(AiSqlFixResponse {
        explanation,
        confidence,
        fix_type,
        fixed_query,
        diff_summary,
    })
}

pub(crate) async fn fetch_ai_sql_fix_schema_snapshot(
    pool: DatabasePool,
    database: Option<String>,
    tables: &[String],
) -> Result<Vec<AiSqlFixSchemaTable>, String> {
    if tables.is_empty() {
        return Ok(Vec::new());
    }

    match pool {
        DatabasePool::MySql(pool) => {
            let Some(database) = database else {
                return Ok(Vec::new());
            };
            let database = database.trim().to_string();
            if database.is_empty() {
                return Ok(Vec::new());
            }

            let mut conn = pool.acquire().await.map_err(|error| error.to_string())?;
            let mut snapshot = Vec::new();

            for table in tables.iter().take(AI_SQL_FIX_MAX_TABLES) {
                let mut rows = sqlx::query(
                    "SELECT column_name, column_type, is_nullable, column_key \
                    FROM information_schema.columns \
                    WHERE table_schema = ? \
                    AND table_name = ? \
                    ORDER BY ordinal_position",
                )
                .bind(&database)
                .bind(table)
                .fetch(&mut *conn);

                let mut columns = Vec::new();
                while let Some(row) = rows.try_next().await.map_err(|error| error.to_string())? {
                    let name: String = row.try_get(0).map_err(|error| error.to_string())?;
                    let data_type: String = row.try_get(1).map_err(|error| error.to_string())?;
                    let is_nullable: String = row.try_get(2).map_err(|error| error.to_string())?;
                    let column_key: String = row.try_get(3).map_err(|error| error.to_string())?;
                    columns.push(AiSqlFixSchemaColumn {
                        name,
                        data_type,
                        nullable: is_nullable.eq_ignore_ascii_case("yes"),
                        primary_key: column_key.eq_ignore_ascii_case("pri"),
                    });
                    if columns.len() >= AI_SQL_FIX_MAX_COLUMNS_PER_TABLE {
                        break;
                    }
                }

                if columns.is_empty() {
                    continue;
                }

                snapshot.push(AiSqlFixSchemaTable {
                    table: table.clone(),
                    columns,
                });
            }

            Ok(snapshot)
        }
        DatabasePool::Sqlite(pool) => {
            let schema = database
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
                .unwrap_or_else(|| String::from("main"));
            let mut conn = pool.acquire().await.map_err(|error| error.to_string())?;
            let mut snapshot = Vec::new();

            for table in tables.iter().take(AI_SQL_FIX_MAX_TABLES) {
                let statement = format!(
                    "PRAGMA \"{}\".table_info('{}')",
                    escape_sqlite_identifier(&schema),
                    escape_sqlite_string_literal(table.trim())
                );
                let mut rows = sqlx::query(AssertSqlSafe(statement)).fetch(&mut *conn);

                let mut columns = Vec::new();
                while let Some(row) = rows.try_next().await.map_err(|error| error.to_string())? {
                    let name: String = row.try_get(1).map_err(|error| error.to_string())?;
                    let data_type: String = row.try_get(2).map_err(|error| error.to_string())?;
                    let not_null: i64 = row.try_get(3).map_err(|error| error.to_string())?;
                    let pk: i64 = row.try_get(5).map_err(|error| error.to_string())?;
                    columns.push(AiSqlFixSchemaColumn {
                        name,
                        data_type: if data_type.trim().is_empty() {
                            String::from("UNKNOWN")
                        } else {
                            data_type
                        },
                        nullable: not_null == 0 && pk == 0,
                        primary_key: pk > 0,
                    });
                    if columns.len() >= AI_SQL_FIX_MAX_COLUMNS_PER_TABLE {
                        break;
                    }
                }

                if columns.is_empty() {
                    continue;
                }

                snapshot.push(AiSqlFixSchemaTable {
                    table: table.clone(),
                    columns,
                });
            }

            Ok(snapshot)
        }
        DatabasePool::Postgres(pool) => {
            let mut conn = pool.acquire().await.map_err(|error| error.to_string())?;

            if let Some(database) = database
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
            {
                let current_database: String = sqlx::query_scalar("SELECT current_database()")
                    .fetch_one(&mut *conn)
                    .await
                    .map_err(|error| error.to_string())?;
                if current_database != database {
                    return Ok(Vec::new());
                }
            }

            let mut snapshot = Vec::new();

            for table in tables.iter().take(AI_SQL_FIX_MAX_TABLES) {
                let (schema, table_name) = postgres_table_reference_parts(table);
                if table_name.trim().is_empty() {
                    continue;
                }
                let schema_name = schema.clone();
                let table_name_value = table_name.clone();

                let mut pk_rows = sqlx::query(
                    "SELECT kcu.column_name \
                    FROM information_schema.table_constraints tc \
                    JOIN information_schema.key_column_usage kcu \
                    ON tc.constraint_name = kcu.constraint_name \
                    AND tc.table_schema = kcu.table_schema \
                    WHERE tc.constraint_type = 'PRIMARY KEY' \
                    AND tc.table_schema = COALESCE($1::text, current_schema()) \
                    AND tc.table_name = $2",
                )
                .bind(schema_name.as_deref())
                .bind(&table_name_value)
                .fetch(&mut *conn);

                let mut primary_keys = HashMap::new();
                while let Some(row) = pk_rows
                    .try_next()
                    .await
                    .map_err(|error| error.to_string())?
                {
                    let name: String = row.try_get(0).map_err(|error| error.to_string())?;
                    primary_keys.insert(name.to_ascii_lowercase(), true);
                }
                drop(pk_rows);

                let mut rows = sqlx::query(
                    "SELECT column_name, data_type, udt_name, is_nullable \
                    FROM information_schema.columns \
                    WHERE table_schema = COALESCE($1::text, current_schema()) \
                    AND table_name = $2 \
                    ORDER BY ordinal_position",
                )
                .bind(schema_name.as_deref())
                .bind(&table_name_value)
                .fetch(&mut *conn);

                let mut columns = Vec::new();
                while let Some(row) = rows.try_next().await.map_err(|error| error.to_string())? {
                    let name: String = row.try_get(0).map_err(|error| error.to_string())?;
                    let data_type: String = row.try_get(1).map_err(|error| error.to_string())?;
                    let udt_name: String = row.try_get(2).map_err(|error| error.to_string())?;
                    let is_nullable: String = row.try_get(3).map_err(|error| error.to_string())?;
                    let normalized_type = if data_type.eq_ignore_ascii_case("USER-DEFINED")
                        || data_type.eq_ignore_ascii_case("ARRAY")
                    {
                        udt_name.clone()
                    } else {
                        data_type
                    };

                    columns.push(AiSqlFixSchemaColumn {
                        name: name.clone(),
                        data_type: normalized_type,
                        nullable: is_nullable.eq_ignore_ascii_case("yes"),
                        primary_key: primary_keys.contains_key(&name.to_ascii_lowercase()),
                    });
                    if columns.len() >= AI_SQL_FIX_MAX_COLUMNS_PER_TABLE {
                        break;
                    }
                }
                drop(rows);

                if columns.is_empty() {
                    continue;
                }

                let table_label = if let Some(schema_name_ref) = schema_name.as_deref() {
                    format!("{schema_name_ref}.{}", table_name_value.as_str())
                } else {
                    table_name_value.clone()
                };

                snapshot.push(AiSqlFixSchemaTable {
                    table: table_label,
                    columns,
                });
            }

            Ok(snapshot)
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn generate_ai_sql_fix(
    config: AiRequestConfig,
    pool: Option<DatabasePool>,
    driver: DatabaseDriver,
    database: Option<String>,
    known_tables: Vec<String>,
    selected_table: Option<String>,
    original_query: String,
    error_message: String,
) -> Result<AiSqlFixResponse, String> {
    let model = config.model.trim();
    if model.is_empty() {
        return Err(String::from("AI model is empty. Set it in Settings."));
    }

    let referenced_tables =
        resolve_ai_sql_fix_tables(&original_query, &known_tables, selected_table.as_deref());

    let schema_snapshot = if let Some(pool) = pool {
        fetch_ai_sql_fix_schema_snapshot(pool, database.clone(), &referenced_tables)
            .await
            .unwrap_or_default()
    } else {
        Vec::new()
    };

    let payload = AiSqlFixPromptPayload {
        driver: ai_sql_fix_driver(driver).to_string(),
        original_query,
        error_message,
        schema_snapshot,
        context: AiSqlFixContext {
            tables_referenced: referenced_tables,
            selected_table,
            active_database: database,
        },
    };
    let user_prompt = serde_json::to_string_pretty(&payload)
        .map_err(|error| format!("Failed to serialize AI fix payload: {error}"))?;

    let (kind, endpoint) = resolve_ai_endpoint(&config)?;
    let messages = vec![
        AiMessage::system(AI_SQL_FIX_SYSTEM_PROMPT),
        AiMessage::user(user_prompt.clone()),
    ];

    let client = Client::builder()
        .connect_timeout(Duration::from_secs(CONNECT_TIMEOUT_SECS))
        .timeout(Duration::from_secs(AI_REQUEST_TIMEOUT_SECS))
        .build()
        .map_err(|error| error.to_string())?;

    let mut last_error = String::new();
    for attempt in 0..=AI_REQUEST_MAX_RETRIES {
        let response_result = match kind {
            AiEndpointKind::OpenAi => {
                let payload = OpenAiChatRequest {
                    model: model.to_string(),
                    messages: messages.clone(),
                    stream: false,
                    temperature: config.temperature.map(|_| 0.0),
                    max_tokens: Some(900),
                };
                let mut request = client.post(&endpoint).json(&payload);
                if !config.api_key.trim().is_empty() {
                    request = request.bearer_auth(config.api_key.trim());
                }
                request.send().await
            }
            AiEndpointKind::Anthropic => {
                let payload = AnthropicMessagesRequest {
                    model: model.to_string(),
                    stream: false,
                    max_tokens: 900,
                    system: Some(String::from(AI_SQL_FIX_SYSTEM_PROMPT)),
                    messages: messages
                        .iter()
                        .filter(|message| message.role != "system")
                        .cloned()
                        .collect(),
                    temperature: config.temperature.map(|_| 0.0),
                };
                let mut request = client
                    .post(&endpoint)
                    .header("anthropic-version", ANTHROPIC_API_VERSION)
                    .json(&payload);
                if !config.api_key.trim().is_empty() {
                    request = request.header("x-api-key", config.api_key.trim());
                }
                request.send().await
            }
            AiEndpointKind::OllamaChat => {
                let payload = OllamaChatRequest {
                    model: model.to_string(),
                    messages: messages.clone(),
                    stream: false,
                };
                let mut request = client.post(&endpoint).json(&payload);
                if !config.api_key.trim().is_empty() {
                    request = request.bearer_auth(config.api_key.trim());
                }
                request.send().await
            }
            AiEndpointKind::OllamaGenerate => {
                let payload = OllamaGenerateRequest {
                    model: model.to_string(),
                    prompt: user_prompt.clone(),
                    system: Some(AI_SQL_FIX_SYSTEM_PROMPT.to_string()),
                    stream: false,
                };
                let mut request = client.post(&endpoint).json(&payload);
                if !config.api_key.trim().is_empty() {
                    request = request.bearer_auth(config.api_key.trim());
                }
                request.send().await
            }
        };

        let response = match response_result {
            Ok(response) => response,
            Err(error) => {
                let error_text = error.to_string();
                if attempt < AI_REQUEST_MAX_RETRIES && is_retryable_ai_error(&error_text) {
                    last_error = error_text;
                    tokio::time::sleep(ai_retry_backoff(attempt)).await;
                    continue;
                }
                return Err(error_text);
            }
        };

        let status = response.status();
        let body = response.text().await.map_err(|error| error.to_string())?;

        if !status.is_success() {
            let message = parse_ai_error(&body).unwrap_or_else(|| body.trim().to_string());
            let error_text = if message.is_empty() {
                format!("AI request failed ({status}).")
            } else {
                format!("AI request failed ({status}): {message}")
            };
            if attempt < AI_REQUEST_MAX_RETRIES && is_retryable_ai_error(&error_text) {
                last_error = error_text;
                tokio::time::sleep(ai_retry_backoff(attempt)).await;
                continue;
            }
            return Err(error_text);
        }

        let content = match kind {
            AiEndpointKind::OpenAi => parse_openai_content(&body)?,
            AiEndpointKind::Anthropic => parse_anthropic_content(&body)?,
            AiEndpointKind::OllamaChat => parse_ollama_content(&body)?,
            AiEndpointKind::OllamaGenerate => parse_ollama_generate_content(&body)?,
        };
        let cleaned = clean_ai_output(&content);
        if cleaned.trim().is_empty() {
            return Err(String::from("AI response was empty."));
        }

        return parse_ai_sql_fix_response(&cleaned);
    }

    Err(if last_error.is_empty() {
        String::from("AI SQL fix request failed after retries.")
    } else {
        format!("{last_error} (after retries)")
    })
}
