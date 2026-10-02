mod chat;
mod effects;
mod fix;
mod generation;
pub(crate) mod view;
use self::types::ChatSessionOption;
use crate::app::types::ToastLevel;
use crate::db::DatabasePool;
use crate::model::connection::ConnectionInfo;
use crate::model::settings::Settings;
use iced::{Task, Theme};
use iced_code_editor::Message as CodeEditorMessage;

pub(crate) struct Context<'a> {
    pub(crate) role_modal_open: bool,
    pub(crate) scope: &'a str,
    pub(crate) settings: &'a Settings,
    pub(crate) connection: &'a ConnectionInfo,
    pub(crate) database: Option<String>,
    pub(crate) pool: Option<&'a DatabasePool>,
    pub(crate) tables: &'a [String],
    pub(crate) selected_table: Option<&'a str>,
    pub(crate) query: &'a iced_code_editor::CodeEditor,
    pub(crate) query_error: Option<&'a str>,
    pub(crate) query_running: bool,
    pub(crate) theme: Theme,
}

impl Context<'_> {
    fn chat_available(&self) -> bool {
        self.settings.ai_enabled
            && crate::ai::AiRequestConfig::from_settings(self.settings).is_usable()
    }
}

#[derive(Debug, Clone)]
pub(crate) enum Message {
    DiagramNote {
        summary: String,
        report: String,
        prompt: String,
    },
    FixQueryWithAi,
    AiQueryFixFinished {
        source_query: String,
        result: Result<crate::ai::AiSqlFixResponse, String>,
    },

    ModalBlocked,
    AiPromptAction(text_editor::Action),
    AiUseCurrentSqlContextToggled(bool),
    GenerateAiQuery,
    AiQueryGenerated(Result<String, String>),
    CloseAiModal,

    ChatModeSelected(crate::model::settings::ChatMode),
    Tooltip(Option<String>),
    ToggleChatSidebar,
    CloseChatSidebar,
    ChatInputAction(text_editor::Action),
    ChatSend,
    ChatReplyDelta {
        scope: String,
        request_id: u64,
        text: String,
    },
    ChatReasoningDelta {
        scope: String,
        request_id: u64,
        text: String,
    },
    ChatReplyReady {
        scope: String,
        request_id: u64,
        result: Result<String, String>,
    },
    ChatInsertSql(String),
    ChatCopySql(String),
    ChatCancel,
    ChatNewSession,
    ChatTitleReady {
        scope: String,
        session_id: u64,
        result: Result<String, String>,
    },
    ChatSessionSelected(ChatSessionOption),
    ChatMessageAction {
        index: usize,
        block: usize,
        action: text_editor::Action,
    },
    ChatCodeAction {
        index: usize,
        block: usize,
        action: CodeEditorMessage,
    },
    ChatDiagramFollowToggled,
    ChatExplainError,
    ChatExplainResults,
    ChatClear,
    SendNote {
        summary: Option<String>,
        prompt: String,
    },
    SampleResolved {
        table: String,
        prompt: String,
        resolved: Option<String>,
    },
}

pub(crate) enum Output {
    FixStarted,
    FixError(String),
    FixedSql {
        sql: String,
        summary: String,
    },
    GeneratedSql {
        target: self::types::AiModalTarget,
        sql: String,
    },
    ChatModeSelected(crate::model::settings::ChatMode),
    Tooltip(Option<String>),
    SidebarChanged,
    ClearDiagramAgent,
    PauseDiagramFollow,
    Toast(ToastLevel, String),
    InsertSql {
        sql: String,
        announce: bool,
    },
    OpenSqlTab(String),
    RunQuery {
        sql: String,
        ensure_tab: bool,
    },
    Diagram {
        action: DiagramAgentAction,
        prompt: String,
    },
    Sample {
        table: String,
        prompt: String,
    },
    ExplainResults,
    Inactive {
        scope: String,
        message: Message,
    },
}

impl State {
    pub(crate) fn update(
        &mut self,
        message: Message,
        context: &Context<'_>,
    ) -> (Task<Message>, Vec<Output>) {
        let mut outputs = Vec::new();
        let task = self.update_internal(message, context, &mut outputs);
        (task, outputs)
    }
}
mod parse;
mod sessions;
pub(crate) mod types;
use self::types::{AiModalTarget, ChatSession, DiagramAgentAction};
use iced::widget::text_editor;
use std::collections::HashMap;

pub(crate) struct State {
    pub(crate) ai_modal_open: bool,
    pub(crate) ai_modal_target: AiModalTarget,
    pub(crate) ai_modal_use_current_sql_context: bool,
    pub(crate) ai_modal_sql_context: String,
    pub(crate) ai_prompt_content: text_editor::Content,
    pub(crate) ai_prompt_error: Option<String>,
    pub(crate) is_generating_ai: bool,
    pub(crate) is_fixing_query_with_ai: bool,
    pub(crate) query_inline_suggestion_error: Option<String>,
    pub(crate) chat_open: bool,
    pub(crate) chat_sessions: HashMap<String, Vec<ChatSession>>,
    pub(crate) chat_active_session: HashMap<String, u64>,
    pub(crate) next_chat_session_id: u64,
    pub(crate) chat_input: text_editor::Content,
    pub(crate) chat_sending: bool,
    pub(crate) chat_error: Option<String>,
    pub(crate) chat_request_id: u64,
    pub(crate) next_chat_request_id: u64,
    pub(crate) chat_deferred_reply: Option<(u64, Result<String, String>)>,
    pub(crate) chat_streaming_reply: Option<String>,
    pub(crate) chat_streaming_reasoning: Option<String>,
    pub(crate) chat_activity: Option<String>,
    pub(crate) chat_auto_run_pending: bool,
    pub(crate) chat_auto_retries: u8,
    pub(crate) chat_result_followup: bool,
    pub(crate) chat_pending_sample: Option<String>,
    pub(crate) chat_extra_tables: Vec<String>,
    pub(crate) chat_schema_rounds: u8,
    pub(crate) chat_diagram_steps: u8,
    pub(crate) chat_diagram_prompt: Option<String>,
    pub(crate) chat_diagram_last: Option<DiagramAgentAction>,
    pub(crate) chat_follows_diagram_agent: bool,
    pub(crate) ai_pulse_progress: f32,
}

impl Default for State {
    fn default() -> Self {
        Self {
            ai_modal_open: false,
            ai_modal_target: AiModalTarget::QueryEditor,
            ai_modal_use_current_sql_context: false,
            ai_modal_sql_context: String::new(),
            ai_prompt_content: text_editor::Content::with_text(""),
            ai_prompt_error: None,
            is_generating_ai: false,
            is_fixing_query_with_ai: false,
            query_inline_suggestion_error: None,
            chat_open: false,
            chat_sessions: HashMap::new(),
            chat_active_session: HashMap::new(),
            next_chat_session_id: 1,
            chat_input: text_editor::Content::with_text(""),
            chat_sending: false,
            chat_error: None,
            chat_request_id: 0,
            next_chat_request_id: 0,
            chat_deferred_reply: None,
            chat_streaming_reply: None,
            chat_streaming_reasoning: None,
            chat_activity: None,
            chat_auto_run_pending: false,
            chat_auto_retries: 0,
            chat_result_followup: false,
            chat_pending_sample: None,
            chat_extra_tables: Vec::new(),
            chat_schema_rounds: 0,
            chat_diagram_steps: 0,
            chat_diagram_prompt: None,
            chat_diagram_last: None,
            chat_follows_diagram_agent: true,
            ai_pulse_progress: 0.0,
        }
    }
}

pub(crate) struct ChatConnectionState {
    pub(crate) open: bool,
    pub(crate) input: text_editor::Content,
    pub(crate) sending: bool,
    pub(crate) error: Option<String>,
    pub(crate) request_id: u64,
    pub(crate) streaming_reply: Option<String>,
    pub(crate) streaming_reasoning: Option<String>,
    pub(crate) activity: Option<String>,
    pub(crate) auto_run_pending: bool,
    pub(crate) auto_retries: u8,
    pub(crate) result_followup: bool,
    pub(crate) pending_sample: Option<String>,
    pub(crate) extra_tables: Vec<String>,
    pub(crate) schema_rounds: u8,
    pub(crate) diagram_steps: u8,
    pub(crate) diagram_prompt: Option<String>,
    pub(crate) diagram_last: Option<DiagramAgentAction>,
    pub(crate) follows_diagram_agent: bool,
    pub(crate) deferred_reply: Option<(u64, Result<String, String>)>,
}

impl Default for ChatConnectionState {
    fn default() -> Self {
        Self {
            open: false,
            input: text_editor::Content::with_text(""),
            sending: false,
            error: None,
            request_id: 0,
            streaming_reply: None,
            streaming_reasoning: None,
            activity: None,
            auto_run_pending: false,
            auto_retries: 0,
            result_followup: false,
            pending_sample: None,
            extra_tables: Vec::new(),
            schema_rounds: 0,
            diagram_steps: 0,
            diagram_prompt: None,
            diagram_last: None,
            follows_diagram_agent: true,
            deferred_reply: None,
        }
    }
}
