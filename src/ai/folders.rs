use crate::ai::client::{
    ai_retry_backoff, clean_ai_output, is_retryable_ai_error, parse_ai_error,
    parse_anthropic_content, parse_ollama_content, parse_ollama_generate_content,
    parse_openai_content, resolve_ai_endpoint,
};
use crate::ai::{
    AiEndpointKind, AiMessage, AiRequestConfig, AnthropicMessagesRequest, OllamaChatRequest,
    OllamaGenerateRequest, OpenAiChatRequest,
};
use crate::{
    AI_FOLDER_GROUPING_SYSTEM_PROMPT, AI_REQUEST_MAX_RETRIES, ANTHROPIC_API_VERSION,
    CONNECT_TIMEOUT_SECS, FOLDER_GROUPING_AI_TIMEOUT_SECS, GeneratedFolder,
};
use reqwest::Client;
use std::collections::{BTreeSet, HashMap};
use std::fmt::Write;
use std::time::Duration;

fn folder_grouping_ai_max_tokens(table_count: usize) -> u32 {
    let estimate = 80 + table_count.saturating_mul(7);
    estimate.clamp(160, 1100) as u32
}

fn parse_ai_folder_groups_response(
    response: &str,
    tables: &[String],
) -> Result<Vec<GeneratedFolder>, String> {
    let value = serde_json::from_str::<serde_json::Value>(response)
        .map_err(|error| format!("Failed to parse smart-group response: {error}"))?;

    let mut groups = Vec::<(String, Vec<String>)>::new();
    let mut assignments = Vec::<(String, String)>::new();
    match &value {
        serde_json::Value::Object(map) => {
            if let Some(assignments_value) = map.get("assignments") {
                match assignments_value {
                    serde_json::Value::Object(assign_map) => {
                        assignments.extend(assign_map.iter().filter_map(|(table, folder)| {
                            let folder = folder.as_str()?.trim();
                            if folder.is_empty() {
                                None
                            } else {
                                Some((table.trim().to_string(), folder.to_string()))
                            }
                        }));
                    }
                    serde_json::Value::Array(items) => {
                        for item in items {
                            if let Some(pair) = item.as_array()
                                && pair.len() >= 2
                                && let (Some(table), Some(folder)) =
                                    (pair[0].as_str(), pair[1].as_str())
                            {
                                let table = table.trim();
                                let folder = folder.trim();
                                if !table.is_empty() && !folder.is_empty() {
                                    assignments.push((table.to_string(), folder.to_string()));
                                }
                                continue;
                            }
                            let Some(item_map) = item.as_object() else {
                                continue;
                            };
                            let table = item_map
                                .get("table")
                                .or_else(|| item_map.get("table_name"))
                                .or_else(|| item_map.get("name"))
                                .and_then(|value| value.as_str())
                                .map(str::trim)
                                .filter(|value| !value.is_empty());
                            let folder = item_map
                                .get("folder")
                                .or_else(|| item_map.get("group"))
                                .or_else(|| item_map.get("category"))
                                .and_then(|value| value.as_str())
                                .map(str::trim)
                                .filter(|value| !value.is_empty());
                            if let (Some(table), Some(folder)) = (table, folder) {
                                assignments.push((table.to_string(), folder.to_string()));
                            }
                        }
                    }
                    _ => {}
                }
            }

            if let Some(group_value) = map.get("groups") {
                if let Some(items) = group_value.as_array() {
                    for item in items {
                        let Some(item_map) = item.as_object() else {
                            continue;
                        };
                        let folder = item_map
                            .get("folder")
                            .or_else(|| item_map.get("name"))
                            .or_else(|| item_map.get("group"))
                            .and_then(|value| value.as_str())
                            .map(str::trim)
                            .filter(|value| !value.is_empty())
                            .map(str::to_string);
                        let Some(folder) = folder else {
                            continue;
                        };
                        let table_names = item_map
                            .get("tables")
                            .or_else(|| item_map.get("table_names"))
                            .or_else(|| item_map.get("items"))
                            .and_then(|value| value.as_array())
                            .map(|values| {
                                values
                                    .iter()
                                    .filter_map(|value| value.as_str())
                                    .map(|name| name.trim().to_string())
                                    .filter(|name| !name.is_empty())
                                    .collect::<Vec<_>>()
                            })
                            .unwrap_or_default();
                        groups.push((folder, table_names));
                    }
                }
            } else {
                let has_string_values = map.values().any(|value| value.as_str().is_some());
                let has_array_values = map.values().any(|value| value.as_array().is_some());
                if has_string_values && !has_array_values {
                    assignments.extend(map.iter().filter_map(|(table, folder)| {
                        let folder = folder.as_str()?.trim();
                        if folder.is_empty() {
                            None
                        } else {
                            Some((table.trim().to_string(), folder.to_string()))
                        }
                    }));
                } else {
                    for (folder, value) in map {
                        if folder.eq_ignore_ascii_case("assignments")
                            || folder.eq_ignore_ascii_case("groups")
                        {
                            continue;
                        }
                        let folder = folder.trim().to_string();
                        if folder.is_empty() {
                            continue;
                        }
                        let table_names = value
                            .as_array()
                            .map(|values| {
                                values
                                    .iter()
                                    .filter_map(|value| value.as_str())
                                    .map(|name| name.trim().to_string())
                                    .filter(|name| !name.is_empty())
                                    .collect::<Vec<_>>()
                            })
                            .unwrap_or_default();
                        groups.push((folder, table_names));
                    }
                }
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                let Some(item_map) = item.as_object() else {
                    continue;
                };
                let table = item_map
                    .get("table")
                    .or_else(|| item_map.get("table_name"))
                    .or_else(|| item_map.get("name"))
                    .and_then(|value| value.as_str())
                    .map(str::trim)
                    .filter(|value| !value.is_empty());
                let folder = item_map
                    .get("folder")
                    .or_else(|| item_map.get("group"))
                    .or_else(|| item_map.get("category"))
                    .and_then(|value| value.as_str())
                    .map(str::trim)
                    .filter(|value| !value.is_empty());
                if let (Some(table), Some(folder)) = (table, folder) {
                    assignments.push((table.to_string(), folder.to_string()));
                    continue;
                }

                let folder = item_map
                    .get("folder")
                    .or_else(|| item_map.get("name"))
                    .or_else(|| item_map.get("group"))
                    .and_then(|value| value.as_str())
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(str::to_string);
                let Some(folder) = folder else {
                    continue;
                };
                let table_names = item_map
                    .get("tables")
                    .or_else(|| item_map.get("table_names"))
                    .or_else(|| item_map.get("items"))
                    .and_then(|value| value.as_array())
                    .map(|values| {
                        values
                            .iter()
                            .filter_map(|value| value.as_str())
                            .map(|name| name.trim().to_string())
                            .filter(|name| !name.is_empty())
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                groups.push((folder, table_names));
            }
        }
        _ => {}
    }

    let lookup = tables
        .iter()
        .map(|name| (name.to_ascii_lowercase(), name.clone()))
        .collect::<HashMap<_, _>>();
    let mut grouped = HashMap::<String, BTreeSet<String>>::new();

    for (folder, names) in groups {
        let folder = folder.trim().to_string();
        if folder.is_empty() {
            continue;
        }
        let folder_tables = grouped.entry(folder).or_default();
        for name in names {
            let normalized = name.trim().to_ascii_lowercase();
            let Some(actual) = lookup.get(&normalized) else {
                continue;
            };
            folder_tables.insert(actual.clone());
        }
    }

    for (table, folder) in assignments {
        let folder = folder.trim().to_string();
        if folder.is_empty() {
            continue;
        }
        let normalized = table.trim().to_ascii_lowercase();
        let Some(actual) = lookup.get(&normalized) else {
            continue;
        };
        grouped.entry(folder).or_default().insert(actual.clone());
    }

    let mut output = grouped
        .into_iter()
        .filter_map(|(folder, tables)| {
            if tables.is_empty() {
                None
            } else {
                Some(GeneratedFolder {
                    folder,
                    tables: tables.into_iter().collect(),
                })
            }
        })
        .collect::<Vec<_>>();

    output.sort_by(|left, right| {
        left.folder
            .to_ascii_lowercase()
            .cmp(&right.folder.to_ascii_lowercase())
    });

    if output.is_empty() {
        return Err(String::from(
            "AI response did not include any valid table groups.",
        ));
    }

    Ok(output)
}

pub(crate) async fn generate_ai_folder_groups(
    config: AiRequestConfig,
    tables: Vec<String>,
    existing_folders: Vec<String>,
) -> Result<Vec<GeneratedFolder>, String> {
    let model = config.model.trim();
    if model.is_empty() {
        return Err(String::from("AI model is empty. Set it in Settings."));
    }
    if tables.is_empty() {
        return Ok(Vec::new());
    }

    let (kind, endpoint) = resolve_ai_endpoint(&config)?;
    let mut table_lines = String::new();
    for table in &tables {
        let _ = writeln!(&mut table_lines, "{table}");
    }
    let mut existing_folder_lines = String::new();
    if !existing_folders.is_empty() {
        for folder in existing_folders.iter().take(60) {
            let _ = writeln!(&mut existing_folder_lines, "- {}", folder.trim());
        }
    }
    let user_prompt = format!(
        "Assign each unresolved MySQL table to one folder.\n\
        Return ONLY JSON in this exact shape:\n\
        {{\"assignments\":{{\"table_a\":\"Folder Name\",\"table_b\":\"Other Folder\"}}}}\n\
        Rules:\n\
        - Include every provided table exactly once.\n\
        - Use only provided table names as assignment keys.\n\
        - Reuse existing folder names when they fit.\n\
        - Keep folder names short and stable.\n\
        Existing folders:\n{}\n\
        Tables:\n{}",
        existing_folder_lines.trim_end(),
        table_lines.trim_end()
    );

    let messages = vec![
        AiMessage::system(AI_FOLDER_GROUPING_SYSTEM_PROMPT),
        AiMessage::user(user_prompt.clone()),
    ];

    let client = Client::builder()
        .connect_timeout(Duration::from_secs(CONNECT_TIMEOUT_SECS))
        .timeout(Duration::from_secs(FOLDER_GROUPING_AI_TIMEOUT_SECS))
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
                    max_tokens: Some(folder_grouping_ai_max_tokens(tables.len())),
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
                    max_tokens: folder_grouping_ai_max_tokens(tables.len()),
                    system: Some(String::from(AI_FOLDER_GROUPING_SYSTEM_PROMPT)),
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
                    system: Some(AI_FOLDER_GROUPING_SYSTEM_PROMPT.to_string()),
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

        return parse_ai_folder_groups_response(&cleaned, &tables);
    }

    Err(if last_error.is_empty() {
        String::from("AI smart-groups request failed after retries.")
    } else {
        format!("{last_error} (after retries)")
    })
}

pub(crate) fn generate_fast_folder_groups(tables: &[String]) -> Vec<GeneratedFolder> {
    if tables.len() < 2 {
        return Vec::new();
    }

    let mut grouped = HashMap::<String, Vec<String>>::new();
    for table in tables {
        let Some(key) = fast_group_key_for_table(table) else {
            continue;
        };
        grouped.entry(key).or_default().push(table.clone());
    }

    let mut output = grouped
        .into_iter()
        .filter_map(|(key, mut group_tables)| {
            if group_tables.len() < 2 {
                return None;
            }
            group_tables.sort_by_key(|left| left.to_ascii_lowercase());
            let folder = format_fast_group_folder(&key);
            if folder.is_empty() {
                None
            } else {
                Some(GeneratedFolder {
                    folder,
                    tables: group_tables,
                })
            }
        })
        .collect::<Vec<_>>();

    output.sort_by(|left, right| {
        left.folder
            .to_ascii_lowercase()
            .cmp(&right.folder.to_ascii_lowercase())
    });
    output
}

fn fast_group_key_for_table(table: &str) -> Option<String> {
    let tokens = split_table_tokens(table);
    if tokens.is_empty() {
        return None;
    }

    let mut fallback = None;
    for token in tokens.into_iter().take(3) {
        let normalized = normalize_fast_group_token(&token);
        if normalized.len() < 2 {
            continue;
        }
        if fallback.is_none() {
            fallback = Some(normalized.clone());
        }
        if !is_generic_fast_group_token(&normalized) {
            return Some(normalized);
        }
    }

    fallback
}

fn split_table_tokens(value: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut prev_lower_or_digit = false;

    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() {
            if ch.is_ascii_uppercase() && prev_lower_or_digit && !current.is_empty() {
                tokens.push(current.to_ascii_lowercase());
                current.clear();
            }
            current.push(ch);
            prev_lower_or_digit = ch.is_ascii_lowercase() || ch.is_ascii_digit();
        } else {
            if !current.is_empty() {
                tokens.push(current.to_ascii_lowercase());
                current.clear();
            }
            prev_lower_or_digit = false;
        }
    }

    if !current.is_empty() {
        tokens.push(current.to_ascii_lowercase());
    }

    tokens
}

fn normalize_fast_group_token(token: &str) -> String {
    let trimmed = token.trim_matches(|ch: char| !ch.is_ascii_alphanumeric());
    if trimmed.is_empty() {
        return String::new();
    }
    let trimmed = trimmed.trim_matches(|ch: char| ch.is_ascii_digit());
    if trimmed.is_empty() {
        return String::new();
    }
    singularize_fast_group_token(&trimmed.to_ascii_lowercase())
}

fn singularize_fast_group_token(token: &str) -> String {
    if token.len() > 6 && token.ends_with("series") {
        return token.to_string();
    }
    if token.len() > 4 && token.ends_with("ies") {
        let mut singular = token[..token.len() - 3].to_string();
        singular.push('y');
        return singular;
    }
    if token.len() > 4 && token.ends_with("sses") {
        return token[..token.len() - 2].to_string();
    }
    if token.len() > 4 && token.ends_with("ses") {
        let stem = &token[..token.len() - 2];
        if stem.ends_with("us") || stem.ends_with("is") {
            return token.to_string();
        }
    }
    if token.len() > 3
        && token.ends_with('s')
        && !token.ends_with("ss")
        && !token.ends_with("us")
        && !token.ends_with("is")
    {
        return token[..token.len() - 1].to_string();
    }
    token.to_string()
}

fn is_generic_fast_group_token(token: &str) -> bool {
    matches!(
        token,
        "tbl"
            | "table"
            | "tables"
            | "app"
            | "apps"
            | "core"
            | "data"
            | "db"
            | "dbo"
            | "mysql"
            | "public"
            | "schema"
            | "main"
            | "prod"
            | "dev"
            | "test"
            | "tmp"
            | "temp"
    )
}

fn format_fast_group_folder(key: &str) -> String {
    let key = key.trim();
    if key.is_empty() {
        return String::new();
    }
    let mut chars = key.chars();
    let Some(first) = chars.next() else {
        return String::new();
    };
    let mut folder = String::with_capacity(key.len());
    folder.push(first.to_ascii_uppercase());
    folder.push_str(chars.as_str());
    folder
}

pub(crate) fn fallback_generated_folder_for_table(table: &str) -> String {
    if let Some(key) = fast_group_key_for_table(table) {
        let folder = format_fast_group_folder(&key);
        if !folder.is_empty() {
            return folder;
        }
    }
    String::from("Misc")
}
