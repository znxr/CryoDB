use crate::ai::{
    AiEndpointKind, AiMessage, AiProvider, AiRequestConfig, AiStreamChunk,
    AnthropicMessagesRequest, OllamaChatRequest, OllamaGenerateRequest, OpenAiChatRequest,
};
use crate::{
    AI_CLI_TIMEOUT_GRACE_SECS, AI_INLINE_COMPLETION_SYSTEM_PROMPT,
    AI_INLINE_COMPLETION_TIMEOUT_SECS, AI_REQUEST_MAX_RETRIES, AI_REQUEST_TIMEOUT_SECS,
    AI_SYSTEM_PROMPT, ANTHROPIC_API_VERSION, ANTHROPIC_MAX_TOKENS, CONNECT_TIMEOUT_SECS,
    DatabaseDriver,
};
use reqwest::Client;
use std::time::Duration;

pub(crate) fn ai_retry_backoff(attempt: usize) -> Duration {
    let shift = attempt.min(4) as u32;
    Duration::from_millis(400u64.saturating_mul(1u64 << shift))
}

pub(crate) fn is_retryable_ai_error(error: &str) -> bool {
    let text = error.to_ascii_lowercase();
    text.contains("timeout")
        || text.contains("timed out")
        || text.contains("request timeout")
        || text.contains("connection reset")
        || text.contains("connection refused")
        || text.contains("connection closed")
        || text.contains("unexpected eof")
        || text.contains("temporarily unavailable")
        || text.contains("temporary")
        || text.contains("rate limit")
        || text.contains("too many requests")
        || contains_retryable_status_code(&text, "408")
        || contains_retryable_status_code(&text, "425")
        || contains_retryable_status_code(&text, "429")
        || contains_retryable_status_code(&text, "500")
        || contains_retryable_status_code(&text, "502")
        || contains_retryable_status_code(&text, "503")
        || contains_retryable_status_code(&text, "504")
}

fn contains_retryable_status_code(text: &str, code: &str) -> bool {
    text.contains(&format!(" {code}"))
        || text.contains(&format!("({code})"))
        || text.contains(&format!("status {code}"))
}

pub(crate) fn resolve_ai_endpoint(
    config: &AiRequestConfig,
) -> Result<(AiEndpointKind, String), String> {
    let endpoint = config.endpoint.trim();
    if endpoint.is_empty() {
        return Err(String::from("AI endpoint is empty. Set it in Settings."));
    }

    let normalized = endpoint.trim_end_matches('/');

    if normalized.contains("/chat/completions") {
        return Ok((AiEndpointKind::OpenAi, normalized.to_string()));
    }

    if normalized.contains("/v1/messages") {
        return Ok((AiEndpointKind::Anthropic, normalized.to_string()));
    }

    if normalized.contains("/api/chat") {
        return Ok((AiEndpointKind::OllamaChat, normalized.to_string()));
    }

    if normalized.contains("/api/generate") {
        return Ok((AiEndpointKind::OllamaGenerate, normalized.to_string()));
    }

    if normalized.ends_with("/v1") {
        return match config.provider {
            AiProvider::Anthropic => {
                Ok((AiEndpointKind::Anthropic, format!("{normalized}/messages")))
            }
            _ => Ok((
                AiEndpointKind::OpenAi,
                format!("{normalized}/chat/completions"),
            )),
        };
    }

    if normalized.ends_with("/api") {
        return match config.provider {
            AiProvider::Ollama => Ok((
                AiEndpointKind::OllamaGenerate,
                format!("{normalized}/generate"),
            )),
            _ => Ok((
                AiEndpointKind::OpenAi,
                format!("{normalized}/chat/completions"),
            )),
        };
    }

    match config.provider {
        AiProvider::Ollama => Ok((
            AiEndpointKind::OllamaGenerate,
            format!("{normalized}/api/generate"),
        )),
        AiProvider::Anthropic => Ok((
            AiEndpointKind::Anthropic,
            format!("{normalized}/v1/messages"),
        )),
        _ => Ok((
            AiEndpointKind::OpenAi,
            format!("{normalized}/v1/chat/completions"),
        )),
    }
}

pub(crate) fn parse_anthropic_content(body: &str) -> Result<String, String> {
    let value = serde_json::from_str::<serde_json::Value>(body)
        .map_err(|error| format!("Failed to parse AI response: {error}"))?;
    let blocks = value
        .pointer("/content")
        .and_then(|value| value.as_array())
        .ok_or_else(|| String::from("AI response did not include any content."))?;
    let text = blocks
        .iter()
        .filter(|block| block.get("type").and_then(|value| value.as_str()) == Some("text"))
        .filter_map(|block| block.get("text").and_then(|value| value.as_str()))
        .collect::<Vec<_>>()
        .join("");
    Ok(text)
}

pub(crate) fn parse_ai_error(body: &str) -> Option<String> {
    let value = serde_json::from_str::<serde_json::Value>(body).ok()?;
    if let Some(message) = value
        .pointer("/error/message")
        .and_then(|value| value.as_str())
    {
        return Some(message.to_string());
    }
    if let Some(message) = value.pointer("/error").and_then(|value| value.as_str()) {
        return Some(message.to_string());
    }
    if let Some(message) = value.pointer("/message").and_then(|value| value.as_str()) {
        return Some(message.to_string());
    }
    None
}

pub(crate) fn parse_openai_content(body: &str) -> Result<String, String> {
    let value = serde_json::from_str::<serde_json::Value>(body)
        .map_err(|error| format!("Failed to parse AI response: {error}"))?;
    if let Some(content) = value
        .pointer("/choices/0/message/content")
        .and_then(|value| value.as_str())
    {
        return Ok(content.to_string());
    }
    if let Some(content) = value
        .pointer("/choices/0/text")
        .and_then(|value| value.as_str())
    {
        return Ok(content.to_string());
    }
    Err(String::from("AI response did not include any content."))
}

pub(crate) fn parse_ollama_content(body: &str) -> Result<String, String> {
    let value = serde_json::from_str::<serde_json::Value>(body)
        .map_err(|error| format!("Failed to parse AI response: {error}"))?;
    if let Some(content) = value
        .pointer("/message/content")
        .and_then(|value| value.as_str())
    {
        return Ok(content.to_string());
    }
    if let Some(content) = value
        .pointer("/choices/0/message/content")
        .and_then(|value| value.as_str())
    {
        return Ok(content.to_string());
    }
    Err(String::from("AI response did not include any content."))
}

pub(crate) fn parse_ollama_generate_content(body: &str) -> Result<String, String> {
    let value = serde_json::from_str::<serde_json::Value>(body)
        .map_err(|error| format!("Failed to parse AI response: {error}"))?;
    if let Some(content) = value.pointer("/response").and_then(|value| value.as_str()) {
        return Ok(content.to_string());
    }
    if let Some(content) = value
        .pointer("/message/content")
        .and_then(|value| value.as_str())
    {
        return Ok(content.to_string());
    }
    Err(String::from("AI response did not include any content."))
}

pub(crate) fn clean_ai_output(text: &str) -> String {
    let trimmed = text.trim();
    if trimmed.starts_with("```") {
        let mut lines = trimmed.lines();
        let _ = lines.next();
        let mut body: Vec<&str> = lines.collect();
        if body
            .last()
            .is_some_and(|line| line.trim().starts_with("```"))
        {
            body.pop();
        }
        return body.join("\n").trim().to_string();
    }
    trimmed.to_string()
}

fn split_cli_command(command: &str) -> Option<(String, Vec<String>)> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    let mut quoted = false;

    for ch in command.chars() {
        match quote {
            Some(active) if ch == active => quote = None,
            Some(_) => current.push(ch),
            None if ch == '\'' || ch == '"' => {
                quote = Some(ch);
                quoted = true;
            }
            None if ch.is_whitespace() => {
                if !current.is_empty() || quoted {
                    parts.push(std::mem::take(&mut current));
                    quoted = false;
                }
            }
            None => current.push(ch),
        }
    }
    if !current.is_empty() || quoted {
        parts.push(current);
    }

    let mut parts = parts.into_iter();
    let program = parts.next()?;
    Some((program, parts.collect()))
}

async fn run_cli_provider(
    config: &AiRequestConfig,
    messages: &[AiMessage],
) -> Result<String, String> {
    let Some((program, args)) = split_cli_command(config.cli_command.trim()) else {
        return Err(String::from(
            "No CLI command configured. Set it in Settings > AI > Provider.",
        ));
    };

    let prompt = messages
        .iter()
        .map(|message| match message.role {
            "system" => format!("[instructions]\n{}", message.content),
            "assistant" => format!("[assistant]\n{}", message.content),
            _ => format!("[user]\n{}", message.content),
        })
        .collect::<Vec<_>>()
        .join("\n\n");

    let mut child = tokio::process::Command::new(&program)
        .args(&args)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|error| format!("Could not start `{program}`: {error}"))?;

    if let Some(mut stdin) = child.stdin.take() {
        use tokio::io::AsyncWriteExt;
        stdin
            .write_all(prompt.as_bytes())
            .await
            .map_err(|error| format!("Could not send the prompt to `{program}`: {error}"))?;
        stdin
            .shutdown()
            .await
            .map_err(|error| format!("Could not close the prompt stream: {error}"))?;
    }

    let timeout =
        Duration::from_secs(AI_REQUEST_TIMEOUT_SECS.saturating_add(AI_CLI_TIMEOUT_GRACE_SECS));
    let output = match tokio::time::timeout(timeout, child.wait_with_output()).await {
        Ok(Ok(output)) => output,
        Ok(Err(error)) => return Err(format!("`{program}` failed: {error}")),
        Err(_) => return Err(format!("`{program}` timed out.")),
    };

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(if stderr.is_empty() {
            format!("`{program}` exited with {}.", output.status)
        } else {
            format!("`{program}` failed: {stderr}")
        });
    }

    let reply = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if reply.is_empty() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(if stderr.is_empty() {
            format!("`{program}` returned nothing.")
        } else {
            format!("`{program}` returned nothing: {stderr}")
        });
    }
    Ok(reply)
}

pub(crate) async fn send_ai_messages(
    config: &AiRequestConfig,
    messages: Vec<AiMessage>,
) -> Result<String, String> {
    if config.provider == AiProvider::LocalCli {
        return run_cli_provider(config, &messages).await;
    }

    let model = config.model.trim();
    if model.is_empty() {
        return Err(String::from("AI model is empty. Set it in Settings."));
    }

    let (kind, endpoint) = resolve_ai_endpoint(config)?;
    let system_prompt = messages
        .iter()
        .find(|message| message.role == "system")
        .map(|message| message.content.clone());
    let user_prompt = messages
        .iter()
        .filter(|message| message.role != "system")
        .map(|message| message.content.as_str())
        .collect::<Vec<_>>()
        .join("\n\n");

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
                    temperature: config.temperature,
                    max_tokens: None,
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
                    max_tokens: ANTHROPIC_MAX_TOKENS,
                    system: system_prompt.clone(),
                    messages: messages
                        .iter()
                        .filter(|message| message.role != "system")
                        .cloned()
                        .collect(),
                    temperature: config.temperature,
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
                    system: system_prompt.clone(),
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
        return Ok(content);
    }

    Err(if last_error.is_empty() {
        String::from("AI request failed after retries.")
    } else {
        format!("{last_error} (after retries)")
    })
}

pub(crate) async fn generate_ai_completion(
    config: AiRequestConfig,
    prompt: String,
    driver: DatabaseDriver,
) -> Result<String, String> {
    let dialect = match driver {
        DatabaseDriver::MySql | DatabaseDriver::MariaDb => "MySQL",
        DatabaseDriver::Sqlite => "SQLite",
        DatabaseDriver::PostgreSql => "PostgreSQL",
    };
    let system_prompt = format!("{AI_INLINE_COMPLETION_SYSTEM_PROMPT} Dialect: {dialect}.");

    let request = send_ai_messages(
        &config,
        vec![AiMessage::system(&system_prompt), AiMessage::user(prompt)],
    );
    let content = match tokio::time::timeout(
        Duration::from_secs(AI_INLINE_COMPLETION_TIMEOUT_SECS),
        request,
    )
    .await
    {
        Ok(result) => result?,
        Err(_) => String::new(),
    };

    if content.trim_start().starts_with("```") {
        return Ok(clean_ai_output(&content));
    }
    Ok(content)
}

pub(crate) async fn generate_ai_sql(
    config: AiRequestConfig,
    prompt: String,
    driver: DatabaseDriver,
) -> Result<String, String> {
    let system_prompt = match driver {
        DatabaseDriver::MySql | DatabaseDriver::MariaDb => AI_SYSTEM_PROMPT.to_string(),
        DatabaseDriver::Sqlite => String::from(
            "You are a SQL query generator for SQLite. Respond ONLY with SQL queries compatible with SQLite. Do not include explanations, markdown, or code fences.",
        ),
        DatabaseDriver::PostgreSql => String::from(
            "You are a SQL query generator for PostgreSQL. Respond ONLY with SQL queries compatible with PostgreSQL. Do not include explanations, markdown, or code fences.",
        ),
    };

    let content = send_ai_messages(
        &config,
        vec![AiMessage::system(&system_prompt), AiMessage::user(prompt)],
    )
    .await?;

    let cleaned = clean_ai_output(&content);
    if cleaned.trim().is_empty() {
        return Err(String::from("AI response was empty."));
    }
    Ok(cleaned)
}

pub(crate) async fn streaming_request(
    config: &AiRequestConfig,
    messages: &[AiMessage],
) -> Result<(AiEndpointKind, reqwest::Response), String> {
    let model = config.model.trim();
    if model.is_empty() {
        return Err(String::from("AI model is empty. Set it in Settings."));
    }

    let (kind, endpoint) = resolve_ai_endpoint(config)?;
    let system_prompt = messages
        .iter()
        .find(|message| message.role == "system")
        .map(|message| message.content.clone());
    let user_prompt = messages
        .iter()
        .filter(|message| message.role != "system")
        .map(|message| message.content.as_str())
        .collect::<Vec<_>>()
        .join("\n\n");

    let client = Client::builder()
        .connect_timeout(Duration::from_secs(CONNECT_TIMEOUT_SECS))
        .timeout(Duration::from_secs(AI_REQUEST_TIMEOUT_SECS))
        .build()
        .map_err(|error| error.to_string())?;

    let request = match kind {
        AiEndpointKind::OpenAi => client
            .post(&endpoint)
            .json(&OpenAiChatRequest {
                model: model.to_string(),
                messages: messages.to_vec(),
                stream: true,
                temperature: config.temperature,
                max_tokens: None,
            })
            .bearer_auth(config.api_key.trim()),
        AiEndpointKind::Anthropic => client
            .post(&endpoint)
            .header("anthropic-version", ANTHROPIC_API_VERSION)
            .header("x-api-key", config.api_key.trim())
            .json(&AnthropicMessagesRequest {
                model: model.to_string(),
                stream: true,
                max_tokens: ANTHROPIC_MAX_TOKENS,
                system: system_prompt,
                messages: messages
                    .iter()
                    .filter(|message| message.role != "system")
                    .cloned()
                    .collect(),
                temperature: config.temperature,
            }),
        AiEndpointKind::OllamaChat => client.post(&endpoint).json(&OllamaChatRequest {
            model: model.to_string(),
            messages: messages.to_vec(),
            stream: true,
        }),
        AiEndpointKind::OllamaGenerate => client.post(&endpoint).json(&OllamaGenerateRequest {
            model: model.to_string(),
            prompt: user_prompt,
            system: system_prompt,
            stream: true,
        }),
    };

    let response = request.send().await.map_err(|error| error.to_string())?;
    let status = response.status();
    if status.is_success() {
        return Ok((kind, response));
    }

    let body = response.text().await.unwrap_or_default();
    let message = parse_ai_error(&body).unwrap_or_else(|| body.trim().to_string());
    Err(if message.is_empty() {
        format!("AI request failed ({status}).")
    } else {
        format!("AI request failed ({status}): {message}")
    })
}

fn sse_payload(line: &str) -> Option<&str> {
    let payload = line.strip_prefix("data:")?.trim();
    (!payload.is_empty() && payload != "[DONE]").then_some(payload)
}

fn stream_delta(kind: AiEndpointKind, payload: &str) -> Option<(bool, String)> {
    let value = serde_json::from_str::<serde_json::Value>(payload).ok()?;
    let reasoning = |pointer: &str| {
        value
            .pointer(pointer)
            .and_then(|value| value.as_str())
            .filter(|text| !text.is_empty())
            .map(|text| (true, text.to_string()))
    };

    let pointer = match kind {
        AiEndpointKind::OpenAi => "/choices/0/delta/content",
        AiEndpointKind::Anthropic => {
            if value.get("type").and_then(|value| value.as_str()) != Some("content_block_delta") {
                return None;
            }
            if value
                .pointer("/delta/type")
                .and_then(|value| value.as_str())
                == Some("thinking_delta")
            {
                return reasoning("/delta/thinking");
            }
            "/delta/text"
        }
        AiEndpointKind::OllamaChat => {
            if let Some(thinking) = reasoning("/message/thinking") {
                return Some(thinking);
            }
            "/message/content"
        }
        AiEndpointKind::OllamaGenerate => {
            if let Some(thinking) = reasoning("/thinking") {
                return Some(thinking);
            }
            "/response"
        }
    };
    let text = value.pointer(pointer)?.as_str()?;
    (!text.is_empty()).then(|| (false, text.to_string()))
}

pub(crate) fn stream_lines(
    kind: AiEndpointKind,
    buffer: &mut String,
    chunk: &str,
) -> Vec<(bool, String)> {
    buffer.push_str(chunk);
    let mut deltas = Vec::new();
    while let Some(newline) = buffer.find('\n') {
        let line: String = buffer.drain(..=newline).collect();
        let line = line.trim_end();
        let payload = match kind {
            AiEndpointKind::OllamaChat | AiEndpointKind::OllamaGenerate => {
                (!line.is_empty()).then_some(line)
            }
            _ => sse_payload(line),
        };
        if let Some(delta) = payload.and_then(|payload| stream_delta(kind, payload)) {
            deltas.push(delta);
        }
    }
    deltas
}

#[cfg(test)]
mod tests {
    use super::{
        ReasoningSplitter, ai_retry_backoff, is_retryable_ai_error, split_cli_command,
        split_reasoning, stream_lines,
    };
    use crate::ai::{AiEndpointKind, AiStreamChunk};
    use std::time::Duration;

    #[test]
    fn openai_stream_lines_yield_text_and_ignore_framing() {
        let mut buffer = String::new();
        let deltas = stream_lines(
            AiEndpointKind::OpenAi,
            &mut buffer,
            "data: {\"choices\":[{\"delta\":{\"content\":\"SELECT \"}}]}\n\ndata: {\"choices\":[{\"delta\":{\"role\":\"assistant\"}}]}\ndata: [DONE]\n",
        );
        assert_eq!(deltas, vec![(false, String::from("SELECT "))]);
    }

    #[test]
    fn anthropic_stream_lines_take_only_content_deltas() {
        let mut buffer = String::new();
        let deltas = stream_lines(
            AiEndpointKind::Anthropic,
            &mut buffer,
            "event: message_start\ndata: {\"type\":\"message_start\"}\n\nevent: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"delta\":{\"type\":\"text_delta\",\"text\":\"1\"}}\n",
        );
        assert_eq!(deltas, vec![(false, String::from("1"))]);
    }

    #[test]
    fn ollama_stream_lines_read_bare_json_documents() {
        let mut buffer = String::new();
        let chat = stream_lines(
            AiEndpointKind::OllamaChat,
            &mut buffer,
            "{\"message\":{\"content\":\"a\"},\"done\":false}\n{\"message\":{\"content\":\"b\"},\"done\":true}\n",
        );
        assert_eq!(
            chat,
            vec![(false, String::from("a")), (false, String::from("b"))]
        );

        let mut buffer = String::new();
        let generate = stream_lines(
            AiEndpointKind::OllamaGenerate,
            &mut buffer,
            "{\"response\":\"x\",\"done\":false}\n",
        );
        assert_eq!(generate, vec![(false, String::from("x"))]);
    }

    #[test]
    fn a_delta_split_across_chunks_is_held_until_its_line_completes() {
        let mut buffer = String::new();
        assert!(
            stream_lines(
                AiEndpointKind::OpenAi,
                &mut buffer,
                "data: {\"choices\":[{\"delta\":{\"cont",
            )
            .is_empty()
        );
        assert_eq!(
            stream_lines(AiEndpointKind::OpenAi, &mut buffer, "ent\":\"ok\"}}]}\n"),
            vec![(false, String::from("ok"))]
        );
    }

    #[test]
    fn reasoning_is_recognised_where_each_provider_puts_it() {
        let mut buffer = String::new();
        assert_eq!(
            stream_lines(
                AiEndpointKind::Anthropic,
                &mut buffer,
                "data: {\"type\":\"content_block_delta\",\"delta\":{\"type\":\"thinking_delta\",\"thinking\":\"weighing\"}}\n",
            ),
            vec![(true, String::from("weighing"))]
        );

        let mut buffer = String::new();
        assert_eq!(
            stream_lines(
                AiEndpointKind::OllamaChat,
                &mut buffer,
                "{\"message\":{\"thinking\":\"hmm\",\"content\":\"\"},\"done\":false}\n",
            ),
            vec![(true, String::from("hmm"))]
        );
    }

    #[test]
    fn inline_think_tags_are_split_out_of_the_answer() {
        let (reasoning, answer) = split_reasoning("<think>weighing</think>SELECT 1");
        assert_eq!(reasoning, "weighing");
        assert_eq!(answer, "SELECT 1");

        let (reasoning, answer) = split_reasoning("<think>still going");
        assert_eq!(reasoning, "still going");
        assert!(answer.is_empty());

        let (reasoning, answer) = split_reasoning("SELECT 1");
        assert!(reasoning.is_empty());
        assert_eq!(answer, "SELECT 1");
    }

    #[test]
    fn a_think_tag_split_across_chunks_is_still_routed_to_reasoning() {
        let mut splitter = ReasoningSplitter::default();
        let held = splitter.push("SELECT <thi");
        assert_eq!(held.len(), 1, "only the text before the partial tag is out");

        let opened = splitter.push("nk>weighing</think>1");
        let kinds: Vec<_> = opened
            .iter()
            .map(|chunk| match chunk {
                AiStreamChunk::Reasoning(text) => (true, text.clone()),
                AiStreamChunk::Delta(text) => (false, text.clone()),
                AiStreamChunk::Done(_) => unreachable!(),
            })
            .collect();
        assert_eq!(
            kinds,
            vec![(true, String::from("weighing")), (false, String::from("1"))]
        );
    }

    #[test]
    fn cli_command_splits_on_whitespace_and_honors_quotes() {
        assert_eq!(
            split_cli_command("claude -p"),
            Some((String::from("claude"), vec![String::from("-p")]))
        );
        assert_eq!(
            split_cli_command("  opencode   run  "),
            Some((String::from("opencode"), vec![String::from("run")]))
        );
        assert_eq!(
            split_cli_command("\"/opt/my tools/llm\" --json"),
            Some((
                String::from("/opt/my tools/llm"),
                vec![String::from("--json")]
            ))
        );
        assert_eq!(
            split_cli_command("gemini -p \"\""),
            Some((
                String::from("gemini"),
                vec![String::from("-p"), String::new()]
            ))
        );
        assert_eq!(
            split_cli_command("tool '' --flag"),
            Some((
                String::from("tool"),
                vec![String::new(), String::from("--flag")]
            ))
        );
        assert_eq!(split_cli_command("   "), None);
    }

    #[test]
    fn retryable_ai_error_catches_common_transient_failures() {
        assert!(is_retryable_ai_error(
            "AI request failed (500): internal server error"
        ));
        assert!(is_retryable_ai_error(
            "AI request failed (408): request timeout"
        ));
        assert!(is_retryable_ai_error(
            "rate limit exceeded by upstream provider"
        ));
        assert!(is_retryable_ai_error(
            "connection closed before message completed"
        ));
        assert!(!is_retryable_ai_error("invalid api key"));
        assert!(!is_retryable_ai_error("malformed request payload"));
    }

    #[test]
    fn ai_retry_backoff_grows_exponentially_and_caps() {
        assert_eq!(ai_retry_backoff(0), Duration::from_millis(400));
        assert_eq!(ai_retry_backoff(1), Duration::from_millis(800));
        assert_eq!(ai_retry_backoff(2), Duration::from_millis(1600));
        assert_eq!(ai_retry_backoff(4), Duration::from_millis(6400));
        assert_eq!(ai_retry_backoff(8), Duration::from_millis(6400));
    }
}

const THINK_OPEN: &str = "<think>";
const THINK_CLOSE: &str = "</think>";

pub(crate) fn split_reasoning(text: &str) -> (String, String) {
    let mut reasoning = String::new();
    let mut answer = String::new();
    let mut rest = text;
    while let Some(open) = rest.find(THINK_OPEN) {
        answer.push_str(&rest[..open]);
        rest = &rest[open + THINK_OPEN.len()..];
        match rest.find(THINK_CLOSE) {
            Some(close) => {
                reasoning.push_str(&rest[..close]);
                rest = &rest[close + THINK_CLOSE.len()..];
            }
            None => {
                reasoning.push_str(rest);
                return (reasoning, answer);
            }
        }
    }
    answer.push_str(rest);
    (reasoning, answer)
}

fn partial_tag_len(text: &str) -> usize {
    let max = THINK_CLOSE.len().min(text.len());
    (1..=max)
        .rev()
        .find(|len| {
            let tail = &text[text.len() - len..];
            THINK_OPEN.starts_with(tail) || THINK_CLOSE.starts_with(tail)
        })
        .unwrap_or(0)
}

#[derive(Default)]
pub(crate) struct ReasoningSplitter {
    inside: bool,
    pending: String,
}

impl ReasoningSplitter {
    pub(crate) fn push(&mut self, text: &str) -> Vec<AiStreamChunk> {
        self.pending.push_str(text);
        let hold = partial_tag_len(&self.pending);
        let ready: String = self.pending.drain(..self.pending.len() - hold).collect();

        let mut chunks = Vec::new();
        let mut rest = ready.as_str();
        loop {
            let (tag, len) = if self.inside {
                (THINK_CLOSE, THINK_CLOSE.len())
            } else {
                (THINK_OPEN, THINK_OPEN.len())
            };
            let Some(index) = rest.find(tag) else {
                break;
            };
            if index > 0 {
                chunks.push(self.emit(&rest[..index]));
            }
            self.inside = !self.inside;
            rest = &rest[index + len..];
        }
        if !rest.is_empty() {
            chunks.push(self.emit(rest));
        }
        chunks
    }

    pub(crate) fn flush(&mut self) -> Vec<AiStreamChunk> {
        if self.pending.is_empty() {
            return Vec::new();
        }
        let rest = std::mem::take(&mut self.pending);
        vec![self.emit(&rest)]
    }

    fn emit(&self, text: &str) -> AiStreamChunk {
        if self.inside {
            AiStreamChunk::Reasoning(text.to_string())
        } else {
            AiStreamChunk::Delta(text.to_string())
        }
    }
}
