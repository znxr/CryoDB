use super::ChatConnectionState;
use super::State;
use super::types::{ChatMessage, ChatRole, ChatSession, ChatSessionOption};
use crate::ai::AiMessage;
use crate::constants::{AI_CHAT_MAX_HISTORY, AI_CHAT_MAX_SESSIONS, AI_CHAT_NOTE_MAX_CHARS};
use crate::model::connection::{ConnectionInfo, DatabaseDriver};
use iced::widget::text_editor;

impl State {
    pub(crate) fn chat_scope_prefix_for(connection: &ConnectionInfo) -> String {
        let identity = match connection.driver {
            DatabaseDriver::Sqlite => connection.sqlite_path.trim().to_string(),
            _ => format!(
                "{}@{}:{}",
                connection.username.trim(),
                connection.host.trim(),
                connection.port.trim()
            ),
        };
        format!("{}|{}|", connection.driver, identity)
    }
    pub(crate) fn chat_scope_key_for(connection: &ConnectionInfo) -> String {
        format!(
            "{}{}",
            Self::chat_scope_prefix_for(connection),
            connection.database.trim()
        )
    }
    pub(crate) fn chat_sessions_for_scope(&self, scope: &str) -> Option<&Vec<ChatSession>> {
        self.chat_sessions.get(scope)
    }
    pub(crate) fn chat_active_session(&self, scope: &str) -> Option<&ChatSession> {
        let scope = scope.to_string();
        let id = self.chat_active_session.get(&scope).copied()?;
        self.chat_sessions
            .get(&scope)?
            .iter()
            .find(|session| session.id == id)
    }
    pub(crate) fn chat_messages(&self, scope: &str) -> &[ChatMessage] {
        self.chat_active_session(scope)
            .map(|session| session.messages.as_slice())
            .unwrap_or_default()
    }
    pub(crate) fn chat_active_session_mut(&mut self, scope: &str) -> &mut ChatSession {
        let scope = scope.to_string();
        let sessions = self.chat_sessions.entry(scope.clone()).or_default();

        if sessions.is_empty() {
            let id = self.next_chat_session_id;
            self.next_chat_session_id = self.next_chat_session_id.wrapping_add(1);
            sessions.push(ChatSession {
                id,
                title: String::from("New chat"),
                messages: Vec::new(),
            });
            self.chat_active_session.insert(scope.clone(), id);
        }

        let active = self
            .chat_active_session
            .get(&scope)
            .copied()
            .filter(|id| sessions.iter().any(|session| session.id == *id))
            .unwrap_or_else(|| {
                let id = sessions[0].id;
                self.chat_active_session.insert(scope.clone(), id);
                id
            });

        sessions
            .iter_mut()
            .find(|session| session.id == active)
            .expect("active chat session")
    }
    pub(crate) fn new_chat_session(&mut self, scope: &str) {
        let scope = scope.to_string();
        let id = self.next_chat_session_id;
        self.next_chat_session_id = self.next_chat_session_id.wrapping_add(1);

        let sessions = self.chat_sessions.entry(scope.clone()).or_default();
        sessions.retain(|session| !session.messages.is_empty());
        sessions.push(ChatSession {
            id,
            title: String::from("New chat"),
            messages: Vec::new(),
        });
        if sessions.len() > AI_CHAT_MAX_SESSIONS {
            sessions.remove(0);
        }
        self.chat_active_session.insert(scope, id);
    }
    pub(crate) fn chat_session_options(&self, scope: &str) -> Vec<ChatSessionOption> {
        self.chat_sessions_for_scope(scope)
            .map(|sessions| {
                sessions
                    .iter()
                    .rev()
                    .map(|session| ChatSessionOption {
                        id: session.id,
                        label: session.title.clone(),
                    })
                    .collect()
            })
            .unwrap_or_default()
    }
    pub(crate) fn chat_last_user_prompt(&self, scope: &str) -> Option<String> {
        self.chat_messages(scope)
            .iter()
            .rev()
            .find(|message| message.role == ChatRole::User)
            .map(|message| message.content.clone())
    }
    pub(crate) fn chat_history_messages(&self, scope: &str) -> Vec<AiMessage> {
        let messages = self.chat_messages(scope);
        let mut kept = 0usize;
        let mut start = messages.len();
        for (index, message) in messages.iter().enumerate().rev() {
            if message.role != ChatRole::Note {
                if kept == AI_CHAT_MAX_HISTORY {
                    break;
                }
                kept += 1;
            }
            start = index;
        }

        messages
            .iter()
            .skip(start)
            .map(|message| match message.role {
                ChatRole::Assistant => AiMessage::assistant(message.content.clone()),
                ChatRole::Note => AiMessage::user(
                    message
                        .content
                        .chars()
                        .take(AI_CHAT_NOTE_MAX_CHARS)
                        .collect::<String>(),
                ),
                ChatRole::User => AiMessage::user(message.content.clone()),
            })
            .collect()
    }
    pub(crate) fn take_chat_connection_state(&mut self) -> ChatConnectionState {
        let state = ChatConnectionState {
            open: self.chat_open,
            input: std::mem::replace(&mut self.chat_input, text_editor::Content::with_text("")),
            sending: self.chat_sending,
            error: self.chat_error.take(),
            request_id: self.chat_request_id,
            streaming_reply: self.chat_streaming_reply.take(),
            streaming_reasoning: self.chat_streaming_reasoning.take(),
            activity: self.chat_activity.take(),
            auto_run_pending: self.chat_auto_run_pending,
            auto_retries: self.chat_auto_retries,
            result_followup: self.chat_result_followup,
            pending_sample: self.chat_pending_sample.take(),
            extra_tables: std::mem::take(&mut self.chat_extra_tables),
            schema_rounds: self.chat_schema_rounds,
            diagram_steps: self.chat_diagram_steps,
            diagram_prompt: self.chat_diagram_prompt.take(),
            diagram_last: self.chat_diagram_last.take(),
            follows_diagram_agent: self.chat_follows_diagram_agent,
            deferred_reply: self.chat_deferred_reply.take(),
        };
        self.restore_chat_connection_state(ChatConnectionState::default());
        state
    }
    pub(crate) fn restore_chat_connection_state(&mut self, state: ChatConnectionState) {
        self.chat_open = state.open;
        self.chat_input = state.input;
        self.chat_sending = state.sending;
        self.chat_error = state.error;
        self.chat_request_id = state.request_id;
        self.chat_streaming_reply = state.streaming_reply;
        self.chat_streaming_reasoning = state.streaming_reasoning;
        self.chat_activity = state.activity;
        self.chat_auto_run_pending = state.auto_run_pending;
        self.chat_auto_retries = state.auto_retries;
        self.chat_result_followup = state.result_followup;
        self.chat_pending_sample = state.pending_sample;
        self.chat_extra_tables = state.extra_tables;
        self.chat_schema_rounds = state.schema_rounds;
        self.chat_diagram_steps = state.diagram_steps;
        self.chat_diagram_prompt = state.diagram_prompt;
        self.chat_diagram_last = state.diagram_last;
        self.chat_follows_diagram_agent = state.follows_diagram_agent;
        self.chat_deferred_reply = state.deferred_reply;
    }
}

impl super::ChatConnectionState {
    pub(crate) fn apply_stream(&mut self, message: super::Message) {
        match message {
            super::Message::ChatReplyDelta {
                request_id, text, ..
            } if request_id == self.request_id => {
                self.streaming_reply
                    .get_or_insert_with(String::new)
                    .push_str(&text);
            }
            super::Message::ChatReasoningDelta {
                request_id, text, ..
            } if request_id == self.request_id => {
                self.streaming_reasoning
                    .get_or_insert_with(String::new)
                    .push_str(&text);
            }
            super::Message::ChatReplyReady {
                request_id, result, ..
            } if request_id == self.request_id => {
                self.streaming_reply = None;
                self.streaming_reasoning = None;
                self.activity = None;
                self.sending = false;
                self.deferred_reply = Some((request_id, result));
            }
            _ => {}
        }
    }
}
