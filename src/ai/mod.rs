use crate::{AI_DEFAULT_TEMPERATURE, Settings};
use serde::{Deserialize, Serialize};

pub(crate) mod chat;
pub(crate) mod client;
pub(crate) mod folders;
pub(crate) mod sql_fix;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum AiProvider {
    OpenAI,
    Anthropic,
    Ollama,
    LocalCli,
    Custom,
}

impl std::fmt::Display for AiProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AiProvider::OpenAI => f.write_str(&crate::i18n::tr("OpenAI")),
            AiProvider::Anthropic => f.write_str(&crate::i18n::tr("Anthropic")),
            AiProvider::Ollama => f.write_str(&crate::i18n::tr("Ollama")),
            AiProvider::LocalCli => f.write_str(&crate::i18n::tr("Local CLI")),
            AiProvider::Custom => f.write_str(&crate::i18n::tr("Custom")),
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct AiRequestConfig {
    pub(crate) provider: AiProvider,
    pub(crate) endpoint: String,
    pub(crate) model: String,
    pub(crate) api_key: String,
    pub(crate) cli_command: String,
    pub(crate) temperature: Option<f32>,
}

impl AiRequestConfig {
    pub(crate) fn from_settings(settings: &Settings) -> Self {
        if !settings.ai_enabled {
            return Self {
                provider: settings.ai_provider,
                endpoint: String::new(),
                model: String::new(),
                api_key: String::new(),
                cli_command: String::new(),
                temperature: None,
            };
        }
        Self {
            provider: settings.ai_provider,
            endpoint: settings.ai_endpoint.clone(),
            model: settings.ai_model.clone(),
            api_key: settings.ai_api_key.clone(),
            cli_command: settings.ai_cli_command.clone(),
            temperature: settings
                .ai_send_temperature
                .then_some(AI_DEFAULT_TEMPERATURE),
        }
    }

    pub(crate) fn supports_autocomplete(&self) -> bool {
        self.provider != AiProvider::LocalCli
    }

    pub(crate) fn supports_streaming(&self) -> bool {
        self.provider != AiProvider::LocalCli
    }

    pub(crate) fn for_autocomplete(settings: &Settings) -> Self {
        let mut config = Self::from_settings(settings);
        if settings.ai_enabled && !settings.ai_autocomplete_use_main_provider {
            config.provider = settings.ai_autocomplete_provider;
            config.endpoint = settings.ai_autocomplete_endpoint.clone();
            config.model = settings.ai_autocomplete_model.clone();
            config.api_key = settings.ai_autocomplete_api_key.clone();
        }
        config
    }

    pub(crate) fn is_usable(&self) -> bool {
        if self.provider == AiProvider::LocalCli {
            return !self.cli_command.trim().is_empty();
        }
        !self.model.trim().is_empty() && !self.endpoint.trim().is_empty()
    }
}

#[derive(Debug, Clone)]
pub(crate) enum AiStreamChunk {
    Delta(String),
    Reasoning(String),
    Done(Result<String, String>),
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum AiEndpointKind {
    OpenAi,
    Anthropic,
    OllamaChat,
    OllamaGenerate,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct AiMessage {
    pub(crate) role: &'static str,
    pub(crate) content: String,
}

impl AiMessage {
    pub(crate) fn system(content: &str) -> Self {
        Self {
            role: "system",
            content: content.to_string(),
        }
    }

    pub(crate) fn user(content: String) -> Self {
        Self {
            role: "user",
            content,
        }
    }

    pub(crate) fn assistant(content: String) -> Self {
        Self {
            role: "assistant",
            content,
        }
    }
}

#[derive(Debug, Serialize)]
pub(crate) struct OpenAiChatRequest {
    pub(crate) model: String,
    pub(crate) messages: Vec<AiMessage>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub(crate) stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) max_tokens: Option<u32>,
}

#[derive(Debug, Serialize)]
pub(crate) struct AnthropicMessagesRequest {
    pub(crate) model: String,
    pub(crate) max_tokens: u32,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub(crate) stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) system: Option<String>,
    pub(crate) messages: Vec<AiMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) temperature: Option<f32>,
}

#[derive(Debug, Serialize)]
pub(crate) struct OllamaChatRequest {
    pub(crate) model: String,
    pub(crate) messages: Vec<AiMessage>,
    pub(crate) stream: bool,
}

#[derive(Debug, Serialize)]
pub(crate) struct OllamaGenerateRequest {
    pub(crate) model: String,
    pub(crate) prompt: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) system: Option<String>,
    pub(crate) stream: bool,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct AiSqlFixSchemaColumn {
    pub(crate) name: String,
    pub(crate) data_type: String,
    pub(crate) nullable: bool,
    pub(crate) primary_key: bool,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct AiSqlFixSchemaTable {
    pub(crate) table: String,
    pub(crate) columns: Vec<AiSqlFixSchemaColumn>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct AiSqlFixContext {
    pub(crate) tables_referenced: Vec<String>,
    pub(crate) selected_table: Option<String>,
    pub(crate) active_database: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct AiSqlFixPromptPayload {
    pub(crate) driver: String,
    pub(crate) original_query: String,
    pub(crate) error_message: String,
    pub(crate) schema_snapshot: Vec<AiSqlFixSchemaTable>,
    pub(crate) context: AiSqlFixContext,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct AiSqlFixResponseRaw {
    pub(crate) explanation: Option<String>,
    pub(crate) confidence: Option<f32>,
    pub(crate) fix_type: Option<String>,
    pub(crate) fixed_query: Option<String>,
    pub(crate) diff_summary: Option<Vec<String>>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct AiSqlFixResponse {
    pub(crate) explanation: String,
    pub(crate) confidence: f32,
    pub(crate) fix_type: String,
    pub(crate) fixed_query: Option<String>,
    pub(crate) diff_summary: Vec<String>,
}
