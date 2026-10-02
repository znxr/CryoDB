use super::types::{ChatMessage, ChatRole};
use super::{Context, Message, State};
use crate::ai::{AiMessage, AiRequestConfig, AiStreamChunk};
use crate::constants::AI_CHAT_DIAGRAM_NOTE_MARK;
use crate::constants::{
    AI_CHAT_EDITOR_QUERY_MAX_CHARS, AI_CHAT_MAX_CONTEXT_TABLES, AI_CHAT_SMALL_SCHEMA_TABLES,
    AI_CHAT_TITLE_PROMPT,
};
use crate::model::connection::DatabaseDriver;
use crate::ui::ids::chat_scroll_id;
use iced::Task;

fn stream_ai_messages(
    config: AiRequestConfig,
    messages: Vec<AiMessage>,
) -> impl futures_util::Stream<Item = AiStreamChunk> {
    iced::stream::channel(64, async move |mut sender| {
        let mut send = async |chunk| {
            use futures_util::SinkExt;
            let _ = sender.send(chunk).await;
        };

        if !config.supports_streaming() {
            send(AiStreamChunk::Done(
                crate::ai::client::send_ai_messages(&config, messages).await,
            ))
            .await;
            return;
        }

        let (kind, response) = match crate::ai::client::streaming_request(&config, &messages).await
        {
            Ok(opened) => opened,
            Err(_) => {
                send(AiStreamChunk::Done(
                    crate::ai::client::send_ai_messages(&config, messages).await,
                ))
                .await;
                return;
            }
        };

        use futures_util::StreamExt;
        let mut body = response.bytes_stream();
        let mut buffer = String::new();
        let mut answer = String::new();
        let mut splitter = crate::ai::client::ReasoningSplitter::default();
        while let Some(chunk) = body.next().await {
            let chunk = match chunk {
                Ok(chunk) => chunk,
                Err(error) => {
                    if answer.is_empty() {
                        send(AiStreamChunk::Done(
                            crate::ai::client::send_ai_messages(&config, messages).await,
                        ))
                        .await;
                    } else {
                        send(AiStreamChunk::Done(Err(error.to_string()))).await;
                    }
                    return;
                }
            };
            let text = String::from_utf8_lossy(&chunk).into_owned();
            for (reasoning, delta) in crate::ai::client::stream_lines(kind, &mut buffer, &text) {
                if reasoning {
                    send(AiStreamChunk::Reasoning(delta)).await;
                    continue;
                }
                for chunk in splitter.push(&delta) {
                    if let AiStreamChunk::Delta(text) = &chunk {
                        answer.push_str(text);
                    }
                    send(chunk).await;
                }
            }
        }
        for chunk in splitter.flush() {
            if let AiStreamChunk::Delta(text) = &chunk {
                answer.push_str(text);
            }
            send(chunk).await;
        }

        if answer.is_empty() {
            send(AiStreamChunk::Done(
                crate::ai::client::send_ai_messages(&config, messages).await,
            ))
            .await;
            return;
        }
        send(AiStreamChunk::Done(Ok(answer))).await;
    })
}

impl State {
    pub(super) fn diagram_note(
        &mut self,
        context: &Context<'_>,
        summary: String,
        report: String,
        prompt: String,
    ) -> Task<Message> {
        let note = format!(
            "{AI_CHAT_DIAGRAM_NOTE_MARK} {summary}. {} Then answer: {prompt}",
            report
        );

        for message in &mut self.chat_active_session_mut(context.scope).messages {
            if let Some(headline) = message
                .content
                .starts_with(AI_CHAT_DIAGRAM_NOTE_MARK)
                .then(|| message.content.split_once(". ").map(|(head, _)| head))
                .flatten()
            {
                message.content = format!("{headline}.");
            }
        }
        self.chat_activity = Some(summary.clone());
        self.send_chat_note(context, Some(summary), note)
    }
    pub(crate) fn push_chat_message(
        &mut self,
        scope: &str,
        role: ChatRole,
        content: String,
    ) -> bool {
        let session = self.chat_active_session_mut(scope);
        let first_user_message = session.messages.is_empty() && role == ChatRole::User;
        if first_user_message {
            session.title = Self::shorten_chat_title(&content);
        }
        session.messages.push(ChatMessage::new(role, content));
        first_user_message
    }
    pub(crate) fn request_chat_title(
        &mut self,
        context: &Context<'_>,
        prompt: &str,
    ) -> Task<Message> {
        let Some(session) = self.chat_active_session(context.scope) else {
            return Task::none();
        };
        let session_id = session.id;
        let scope = context.scope.to_string();
        let config = AiRequestConfig::from_settings(context.settings);
        if !config.is_usable() {
            return Task::none();
        }

        let request = format!("{AI_CHAT_TITLE_PROMPT}{prompt}");
        Task::perform(
            async move {
                crate::ai::client::send_ai_messages(&config, vec![AiMessage::user(request)]).await
            },
            move |result| Message::ChatTitleReady {
                scope: scope.clone(),
                session_id,
                result,
            },
        )
    }
    pub(crate) fn send_chat_note(
        &mut self,
        context: &Context<'_>,
        summary: Option<String>,
        prompt: String,
    ) -> Task<Message> {
        if self.chat_sending || !context.chat_available() {
            return Task::none();
        }
        self.chat_result_followup = true;
        self.chat_error = None;

        let session = self.chat_active_session_mut(context.scope);
        match summary {
            Some(summary) => session
                .messages
                .push(ChatMessage::note(summary, prompt.clone())),
            None => {
                if session.messages.is_empty() {
                    session.title = Self::shorten_chat_title(&prompt);
                }
                session
                    .messages
                    .push(ChatMessage::new(ChatRole::User, prompt.clone()));
            }
        }
        self.sync_code_snippets(context);

        let request = self.dispatch_chat_request(context, &prompt);
        Task::batch([Self::scroll_chat_to_end(), request])
    }
    pub(crate) fn push_chat_outcome(&mut self, scope: &str, summary: &str) {
        let summary = crate::i18n::tr(summary);
        self.chat_active_session_mut(scope)
            .messages
            .push(ChatMessage::note(summary.clone(), summary));
    }
    pub(crate) fn chat_detail_tables(&self, context: &Context<'_>, prompt: &str) -> Vec<String> {
        let mut tables: Vec<String> = Vec::new();

        if let Some(table) = context.selected_table.map(str::to_string) {
            tables.push(table);
        }

        let words = prompt
            .to_ascii_lowercase()
            .split(|ch: char| !ch.is_ascii_alphanumeric() && ch != '_')
            .filter(|word| word.len() >= 3)
            .map(str::to_string)
            .collect::<Vec<_>>();

        for table in context.tables {
            let base = table
                .rsplit('.')
                .next()
                .unwrap_or(table)
                .to_ascii_lowercase();
            if base.len() < 3 || tables.iter().any(|entry| entry.eq_ignore_ascii_case(table)) {
                continue;
            }
            let matches = words.iter().any(|word| {
                word == &base
                    || word.trim_end_matches('s') == base.trim_end_matches('s')
                    || base.split('_').any(|part| part.len() >= 4 && part == word)
            });
            if matches {
                tables.push(table.clone());
            }
        }

        let mut referenced = crate::ai::sql_fix::resolve_ai_sql_fix_tables(
            prompt,
            context.tables,
            context.selected_table,
        );
        referenced.extend(crate::ai::sql_fix::resolve_ai_sql_fix_tables(
            &context.query.content(),
            context.tables,
            None,
        ));
        for table in referenced {
            if context
                .tables
                .iter()
                .any(|known| known.eq_ignore_ascii_case(&table))
                && !tables
                    .iter()
                    .any(|entry| entry.eq_ignore_ascii_case(&table))
            {
                tables.push(table);
            }
        }

        for table in &self.chat_extra_tables {
            if context
                .tables
                .iter()
                .any(|known| known.eq_ignore_ascii_case(table))
                && !tables.iter().any(|entry| entry.eq_ignore_ascii_case(table))
            {
                tables.push(table.clone());
            }
        }

        if context.tables.len() <= AI_CHAT_SMALL_SCHEMA_TABLES {
            for table in context.tables {
                if !tables.iter().any(|entry| entry.eq_ignore_ascii_case(table)) {
                    tables.push(table.clone());
                }
            }
            return tables;
        }

        tables.truncate(AI_CHAT_MAX_CONTEXT_TABLES);
        tables
    }
    pub(crate) fn chat_table_suggestions(
        &self,
        context: &Context<'_>,
        missing: &[String],
    ) -> Vec<String> {
        let needles = missing
            .iter()
            .flat_map(|name| {
                name.to_ascii_lowercase()
                    .split(|ch: char| !ch.is_ascii_alphanumeric())
                    .filter(|part| part.len() >= 4)
                    .map(str::to_string)
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();

        context
            .tables
            .iter()
            .filter(|table| {
                let table = table.to_ascii_lowercase();
                needles.iter().any(|needle| table.contains(needle))
            })
            .take(AI_CHAT_MAX_CONTEXT_TABLES)
            .cloned()
            .collect()
    }
    pub(crate) fn dispatch_chat_request(
        &mut self,
        context: &Context<'_>,
        prompt: &str,
    ) -> Task<Message> {
        self.chat_sending = true;
        self.chat_deferred_reply = None;
        self.next_chat_request_id = self.next_chat_request_id.wrapping_add(1);
        self.chat_request_id = self.next_chat_request_id;
        let request_id = self.chat_request_id;
        let scope = context.scope.to_string();

        let request = crate::ai::chat::ChatRequest {
            config: AiRequestConfig::from_settings(context.settings),
            pool: context.pool.cloned(),
            database: context.database.clone(),
            driver: self.chat_driver_label(context),
            tables: context.tables.to_vec(),
            detail_tables: self.chat_detail_tables(context, prompt),
            selected_table: context.selected_table.map(str::to_string),
            editor_query: if context.settings.ai_send_query_context {
                let query = context.query.content();
                let query = query.trim();
                if query.is_empty() {
                    None
                } else {
                    Some(
                        query
                            .chars()
                            .take(AI_CHAT_EDITOR_QUERY_MAX_CHARS)
                            .collect::<String>(),
                    )
                }
            } else {
                None
            },
            last_query: context
                .query_error
                .as_ref()
                .map(|_| context.query.content())
                .filter(|query| !query.trim().is_empty()),
            last_error: context.query_error.map(str::to_string),
            history: self.chat_history_messages(context.scope),
        };

        self.chat_streaming_reply = None;
        self.chat_streaming_reasoning = None;
        Task::future(crate::ai::chat::chat_prompt(request)).then(move |(config, messages)| {
            let scope = scope.clone();
            Task::run(
                stream_ai_messages(config, messages),
                move |chunk| match chunk {
                    crate::ai::AiStreamChunk::Delta(text) => Message::ChatReplyDelta {
                        scope: scope.clone(),
                        request_id,
                        text,
                    },
                    crate::ai::AiStreamChunk::Reasoning(text) => Message::ChatReasoningDelta {
                        scope: scope.clone(),
                        request_id,
                        text,
                    },
                    crate::ai::AiStreamChunk::Done(result) => Message::ChatReplyReady {
                        scope: scope.clone(),
                        request_id,
                        result,
                    },
                },
            )
        })
    }
    pub(crate) fn chat_driver_label(&self, context: &Context<'_>) -> &'static str {
        match context.connection.driver {
            DatabaseDriver::MySql => "MySQL",
            DatabaseDriver::MariaDb => "MariaDB",
            DatabaseDriver::Sqlite => "SQLite",
            DatabaseDriver::PostgreSql => "PostgreSQL",
        }
    }
    pub(crate) fn chat_explain_sql(driver: DatabaseDriver, sql: &str) -> String {
        match driver {
            DatabaseDriver::Sqlite => format!("EXPLAIN QUERY PLAN {sql}"),
            _ => format!("EXPLAIN {sql}"),
        }
    }
    pub(crate) fn chat_search_schema_sql(driver: DatabaseDriver, term: &str) -> String {
        let needle = term.replace('\'', "''").to_ascii_lowercase();
        match driver {
            DatabaseDriver::Sqlite => format!(
                "SELECT m.name AS table_name, p.name AS column_name, p.type AS data_type \
                 FROM sqlite_master m JOIN pragma_table_info(m.name) p \
                 WHERE m.type = 'table' AND (lower(p.name) LIKE '%{needle}%' OR lower(m.name) LIKE '%{needle}%') \
                 LIMIT 40"
            ),
            DatabaseDriver::PostgreSql => format!(
                "SELECT table_schema || '.' || table_name AS table_name, column_name, data_type \
                 FROM information_schema.columns \
                 WHERE table_schema NOT IN ('pg_catalog', 'information_schema') \
                 AND (lower(column_name) LIKE '%{needle}%' OR lower(table_name) LIKE '%{needle}%') \
                 ORDER BY table_name, column_name LIMIT 40"
            ),
            _ => format!(
                "SELECT table_name, column_name, column_type AS data_type \
                 FROM information_schema.columns \
                 WHERE table_schema = DATABASE() \
                 AND (lower(column_name) LIKE '%{needle}%' OR lower(table_name) LIKE '%{needle}%') \
                 ORDER BY table_name, column_name LIMIT 40"
            ),
        }
    }
    pub(crate) fn scroll_chat_to_end() -> Task<Message> {
        iced::widget::operation::snap_to_end(chat_scroll_id())
    }
    pub(super) fn sync_code_snippets(&mut self, context: &Context<'_>) {
        self.restyle_snippets(
            &context.theme,
            crate::ui::presentation::font_for_choice(&context.settings.editor_font),
            context.settings.font_size as f32,
        );
    }
    pub(crate) fn restyle_snippets(&mut self, theme: &iced::Theme, font: iced::Font, size: f32) {
        for editor in self
            .chat_sessions
            .values_mut()
            .flatten()
            .flat_map(|session| session.messages.iter_mut())
            .flat_map(|message| message.blocks.iter_mut())
            .filter_map(|block| block.code.as_mut())
        {
            crate::ui::widgets::code_snippet::restyle(editor, theme, font, size);
        }
    }
    pub(super) fn clear_diagram_agent(&mut self) {
        self.chat_diagram_steps = 0;
        self.chat_diagram_prompt = None;
        self.chat_diagram_last = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn chat_explain_and_search_sql_match_the_driver() {
        let driver = DatabaseDriver::Sqlite;
        assert!(State::chat_explain_sql(driver, "SELECT 1").starts_with("EXPLAIN QUERY PLAN"));
        assert!(State::chat_search_schema_sql(driver, "mail").contains("pragma_table_info"));

        let driver = DatabaseDriver::PostgreSql;
        assert_eq!(
            State::chat_explain_sql(driver, "SELECT 1"),
            "EXPLAIN SELECT 1"
        );
        assert!(
            State::chat_search_schema_sql(driver, "mail").contains("information_schema.columns")
        );

        let driver = DatabaseDriver::MySql;
        assert!(State::chat_search_schema_sql(driver, "mail").contains("DATABASE()"));
        assert!(State::chat_search_schema_sql(driver, "o'brien").contains("o''brien"));
    }
}
