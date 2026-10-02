use super::types::{ChatDirective, ChatRole};
use super::{Context, Message, Output, State};
use crate::app::types::ToastLevel;
use crate::constants::AI_CHAT_SAMPLE_ROWS;
use crate::constants::{
    AI_CHAT_DIRECTIVE_PREFIX, AI_CHAT_MAX_DIAGRAM_STEPS, AI_CHAT_MAX_SCHEMA_ROUNDS,
};
use crate::model::settings::ChatMode;
use crate::ui::widgets::code_snippet;
use crate::utils::helpers::sql_quote_table_reference;
use iced::{Task, widget::text_editor};

impl State {
    pub(super) fn update_internal(
        &mut self,
        message: Message,
        context: &Context<'_>,
        outputs: &mut Vec<Output>,
    ) -> Task<Message> {
        match message {
            Message::DiagramNote {
                summary,
                report,
                prompt,
            } => self.diagram_note(context, summary, report, prompt),
            Message::FixQueryWithAi | Message::AiQueryFixFinished { .. } => {
                self.update_fix(message, context, outputs)
            }
            Message::ModalBlocked => Task::none(),
            Message::AiPromptAction(_)
            | Message::AiUseCurrentSqlContextToggled(_)
            | Message::GenerateAiQuery
            | Message::AiQueryGenerated(_)
            | Message::CloseAiModal => self.update_generation(message, context, outputs),
            Message::ChatModeSelected(mode) => {
                outputs.push(Output::ChatModeSelected(mode));
                Task::none()
            }
            Message::Tooltip(text) => {
                outputs.push(Output::Tooltip(text));
                Task::none()
            }
            Message::SendNote { summary, prompt } => self.send_chat_note(context, summary, prompt),
            Message::SampleResolved {
                table,
                prompt,
                resolved,
            } => {
                let Some(resolved) = resolved else {
                    return self.send_chat_note(context, Some(format!("No table named {table}")), format!("There is no table called {table}. Use a table from the list you were given, then answer: {prompt}"));
                };
                self.chat_activity = Some(format!("Sampling rows from {resolved}"));
                self.chat_pending_sample = Some(prompt);
                outputs.push(Output::RunQuery {
                    sql: format!(
                        "SELECT * FROM {} LIMIT {AI_CHAT_SAMPLE_ROWS}",
                        sql_quote_table_reference(context.connection.driver, &resolved)
                    ),
                    ensure_tab: false,
                });
                Task::none()
            }

            Message::ToggleChatSidebar => {
                if !context.settings.ai_enabled {
                    return Task::none();
                }
                if self.chat_open {
                    self.chat_open = false;
                    outputs.push(Output::SidebarChanged);
                } else {
                    self.chat_open = true;
                    outputs.push(Output::SidebarChanged);
                }
                Task::none()
            }
            Message::ChatDiagramFollowToggled => {
                if self.chat_diagram_steps == 0 {
                    return Task::none();
                }
                self.chat_follows_diagram_agent = !self.chat_follows_diagram_agent;
                if !self.chat_follows_diagram_agent {
                    outputs.push(Output::PauseDiagramFollow);
                }
                Task::none()
            }
            Message::ChatExplainError => {
                let Some(error) = context.query_error.map(str::to_string) else {
                    return Task::none();
                };
                if self.chat_sending {
                    return Task::none();
                }
                self.chat_open = true;
                outputs.push(Output::SidebarChanged);
                self.chat_auto_retries = 0;
                self.chat_input = text_editor::Content::with_text(&format!(
                    "The query in the editor failed with: {error}\nExplain why and return a corrected query."
                ));
                self.update_internal(Message::ChatSend, context, outputs)
            }
            Message::ChatExplainResults => {
                outputs.push(Output::ExplainResults);
                Task::none()
            }
            Message::CloseChatSidebar => {
                self.chat_open = false;
                outputs.push(Output::SidebarChanged);
                Task::none()
            }
            Message::ChatInputAction(action) => {
                self.chat_input.perform(action);
                Task::none()
            }
            Message::ChatClear => {
                let session = self.chat_active_session_mut(context.scope);
                session.messages.clear();
                session.title = String::from("New chat");
                self.chat_extra_tables.clear();
                self.chat_schema_rounds = 0;
                self.clear_diagram_agent();
                outputs.push(Output::ClearDiagramAgent);
                self.chat_result_followup = false;
                self.chat_pending_sample = None;
                self.chat_error = None;
                self.chat_request_id = self.chat_request_id.wrapping_add(1);
                self.chat_streaming_reply = None;
                self.chat_streaming_reasoning = None;
                self.chat_activity = None;
                self.chat_sending = false;
                Task::none()
            }
            Message::ChatCopySql(sql) => iced::clipboard::write(sql),
            Message::ChatNewSession => {
                self.new_chat_session(context.scope);
                self.chat_error = None;
                self.chat_extra_tables.clear();
                self.chat_schema_rounds = 0;
                self.clear_diagram_agent();
                outputs.push(Output::ClearDiagramAgent);
                self.chat_auto_retries = 0;
                Self::scroll_chat_to_end()
            }
            Message::ChatTitleReady {
                scope,
                session_id,
                result,
            } => {
                let Ok(title) = result else {
                    return Task::none();
                };
                let title = Self::shorten_chat_title(&title);
                if title.is_empty() {
                    return Task::none();
                }
                if let Some(session) = self.chat_sessions.get_mut(&scope).and_then(|sessions| {
                    sessions.iter_mut().find(|session| session.id == session_id)
                }) {
                    session.title = title;
                }
                Task::none()
            }
            Message::ChatSessionSelected(option) => {
                let scope = context.scope.to_string();
                self.chat_active_session.insert(scope, option.id);
                self.chat_error = None;
                self.chat_extra_tables.clear();
                self.chat_schema_rounds = 0;
                self.clear_diagram_agent();
                outputs.push(Output::ClearDiagramAgent);
                Self::scroll_chat_to_end()
            }
            Message::ChatCancel => {
                if !self.chat_sending {
                    return Task::none();
                }
                self.chat_sending = false;
                self.chat_request_id = self.chat_request_id.wrapping_add(1);
                self.chat_streaming_reply = None;
                self.chat_streaming_reasoning = None;
                self.chat_activity = None;
                self.chat_auto_run_pending = false;
                self.chat_auto_retries = 0;
                self.chat_schema_rounds = 0;
                self.clear_diagram_agent();
                outputs.push(Output::ClearDiagramAgent);
                self.chat_error = Some(String::from("Request cancelled."));
                Task::none()
            }
            Message::ChatMessageAction {
                index,
                block,
                action,
            } => {
                if !action.is_edit()
                    && let Some(block) = self
                        .chat_active_session_mut(context.scope)
                        .messages
                        .get_mut(index)
                        .and_then(|message| message.blocks.get_mut(block))
                {
                    block.editor.perform(action);
                }
                Task::none()
            }
            Message::ChatCodeAction {
                index,
                block,
                action,
            } => {
                if !code_snippet::is_edit(&action)
                    && let Some(code) = self
                        .chat_active_session_mut(context.scope)
                        .messages
                        .get_mut(index)
                        .and_then(|message| message.blocks.get_mut(block))
                        .and_then(|block| block.code.as_mut())
                {
                    return code
                        .update(&action)
                        .map(move |action| Message::ChatCodeAction {
                            index,
                            block,
                            action,
                        });
                }
                Task::none()
            }
            Message::ChatInsertSql(sql) => {
                outputs.push(Output::InsertSql {
                    sql,
                    announce: true,
                });
                Task::none()
            }
            Message::ChatSend => {
                if self.chat_sending {
                    return Task::none();
                }
                let prompt = self.chat_input.text().trim().to_string();
                if prompt.is_empty() {
                    return Task::none();
                }
                if !context.chat_available() {
                    self.chat_error = Some(String::from(
                        "Set an AI endpoint and model in Settings > AI > Provider.",
                    ));
                    return Task::none();
                }

                let first_message =
                    self.push_chat_message(context.scope, ChatRole::User, prompt.clone());
                self.sync_code_snippets(context);
                self.chat_input = text_editor::Content::with_text("");
                self.chat_error = None;
                self.chat_extra_tables.clear();
                self.chat_schema_rounds = 0;
                self.clear_diagram_agent();
                outputs.push(Output::ClearDiagramAgent);
                self.chat_result_followup = false;

                let request = self.dispatch_chat_request(context, &prompt);
                let title = if first_message {
                    self.request_chat_title(context, &prompt)
                } else {
                    Task::none()
                };
                Task::batch([Self::scroll_chat_to_end(), title, request])
            }
            Message::ChatReasoningDelta {
                scope,
                request_id,
                text,
            } => {
                if scope != context.scope {
                    outputs.push(Output::Inactive {
                        scope: scope.clone(),
                        message: Message::ChatReasoningDelta {
                            scope,
                            request_id,
                            text,
                        },
                    });
                    return Task::none();
                }
                if request_id != self.chat_request_id {
                    return Task::none();
                }
                self.chat_streaming_reasoning
                    .get_or_insert_with(String::new)
                    .push_str(&text);
                Self::scroll_chat_to_end()
            }
            Message::ChatReplyDelta {
                scope,
                request_id,
                text,
            } => {
                if scope != context.scope {
                    outputs.push(Output::Inactive {
                        scope: scope.clone(),
                        message: Message::ChatReplyDelta {
                            scope,
                            request_id,
                            text,
                        },
                    });
                    return Task::none();
                }
                if request_id != self.chat_request_id {
                    return Task::none();
                }
                self.chat_streaming_reply
                    .get_or_insert_with(String::new)
                    .push_str(&text);
                Self::scroll_chat_to_end()
            }
            Message::ChatReplyReady {
                scope,
                request_id,
                result,
            } => {
                if scope != context.scope {
                    outputs.push(Output::Inactive {
                        scope: scope.clone(),
                        message: Message::ChatReplyReady {
                            scope,
                            request_id,
                            result,
                        },
                    });
                    return Task::none();
                }
                if request_id != self.chat_request_id {
                    return Task::none();
                }
                self.chat_streaming_reply = None;
                self.chat_streaming_reasoning = None;
                self.chat_activity = None;
                self.chat_sending = false;
                match result {
                    Ok(reply) => {
                        let (_, reply) = crate::ai::client::split_reasoning(&reply);
                        let reply = reply.trim().to_string();
                        if reply.is_empty() {
                            self.chat_error = Some(String::from("AI response was empty."));
                            return Self::scroll_chat_to_end();
                        }

                        if let Some(directive) = Self::chat_directive(&reply) {
                            let diagram_step = matches!(directive, ChatDirective::Diagram(_));
                            if diagram_step {
                                if self.chat_diagram_steps >= AI_CHAT_MAX_DIAGRAM_STEPS {
                                    self.push_chat_outcome(context.scope,
                                        "The assistant reached the limit of diagram steps for one message. Nothing failed: send another message to let it carry on.",
                                    );
                                    self.clear_diagram_agent();
                                    outputs.push(Output::ClearDiagramAgent);
                                    return Self::scroll_chat_to_end();
                                }
                            } else if self.chat_schema_rounds >= AI_CHAT_MAX_SCHEMA_ROUNDS {
                                self.chat_error = Some(String::from(
                                    "The assistant kept asking for more context. Open the table you mean, or name it in your question.",
                                ));
                                return Self::scroll_chat_to_end();
                            }
                            let Some(prompt) = self.chat_last_user_prompt(context.scope) else {
                                return Self::scroll_chat_to_end();
                            };
                            if diagram_step {
                                self.chat_diagram_steps += 1;
                            } else {
                                self.chat_schema_rounds += 1;
                            }

                            match directive {
                                ChatDirective::Diagram(action) => {
                                    outputs.push(Output::Diagram { action, prompt });
                                    return Task::none();
                                }
                                ChatDirective::Schema(tables) => {
                                    let (known, unknown): (Vec<String>, Vec<String>) =
                                        tables.into_iter().partition(|table| {
                                            context
                                                .tables
                                                .iter()
                                                .any(|known| known.eq_ignore_ascii_case(table))
                                        });
                                    if known.is_empty() {
                                        let names = unknown.join(", ");
                                        let suggestions =
                                            self.chat_table_suggestions(context, &unknown);
                                        let hint = if suggestions.is_empty() {
                                            String::from(
                                                "Ask the user which table they mean instead of guessing another name.",
                                            )
                                        } else {
                                            format!(
                                                "Closest tables that do exist: {}.",
                                                suggestions.join(", ")
                                            )
                                        };
                                        return self.send_chat_note(context,
                                            Some(crate::i18n::tr_with(
                                                "No table named {names}",
                                                &[("{names}", &names)],
                                            )),
                                            format!(
                                                "This database has no table named {names}. {hint} Then answer: {prompt}"
                                            ),
                                        );
                                    }
                                    self.chat_activity = Some(format!(
                                        "Reading the columns of {}",
                                        known.join(", ")
                                    ));
                                    self.chat_extra_tables.extend(known);
                                    return self.dispatch_chat_request(context, &prompt);
                                }
                                ChatDirective::Malformed(reason) => {
                                    return self.send_chat_note(context,
                                        Some(crate::i18n::tr_with("Assistant sent an invalid command ({reason})", &[("{reason}", &reason)])),
                                        format!(
                                            "That command was invalid: {reason}. Use exactly one line, for example `{AI_CHAT_DIRECTIVE_PREFIX}schema table_one, table_two` or `{AI_CHAT_DIRECTIVE_PREFIX}sample table_one`. Then answer: {prompt}"
                                        ),
                                    );
                                }
                                ChatDirective::OpenTab => {
                                    self.chat_schema_rounds = 0;
                                    self.clear_diagram_agent();
                                    outputs.push(Output::ClearDiagramAgent);
                                    let sql = Self::chat_sql_blocks(&reply).into_iter().next();
                                    let _ = self.push_chat_message(
                                        context.scope,
                                        ChatRole::Assistant,
                                        reply,
                                    );
                                    self.sync_code_snippets(context);
                                    match sql {
                                        Some(sql) => {
                                            outputs.push(Output::OpenSqlTab(sql));
                                            outputs.push(Output::Toast(
                                                ToastLevel::Success,
                                                String::from("SQL opened in a new query tab."),
                                            ));
                                        }
                                        None => outputs.push(Output::Toast(
                                            ToastLevel::Info,
                                            String::from("The assistant asked for a new tab but sent no SQL."),
                                        )),
                                    }
                                    return Self::scroll_chat_to_end();
                                }
                                ChatDirective::Explain(sql) => {
                                    if !Self::chat_sql_is_read_only(&sql) {
                                        return self.send_chat_note(context,
                                            Some(String::from("Refused to explain a write statement")),
                                            format!(
                                                "That statement is not read-only, so it was not explained. Answer without it: {prompt}"
                                            ),
                                        );
                                    }
                                    self.chat_activity =
                                        Some(String::from("Reading the query plan"));
                                    self.chat_pending_sample = Some(prompt);
                                    outputs.push(Output::RunQuery {
                                        sql: Self::chat_explain_sql(
                                            context.connection.driver,
                                            &sql,
                                        ),
                                        ensure_tab: false,
                                    });
                                    return Task::none();
                                }
                                ChatDirective::Search(term) => {
                                    self.chat_activity =
                                        Some(format!("Searching the schema for {term}"));
                                    self.chat_pending_sample = Some(prompt);
                                    outputs.push(Output::RunQuery {
                                        sql: Self::chat_search_schema_sql(
                                            context.connection.driver,
                                            &term,
                                        ),
                                        ensure_tab: false,
                                    });
                                    return Task::none();
                                }
                                ChatDirective::Sample(table) => {
                                    outputs.push(Output::Sample { table, prompt });
                                    return Task::none();
                                }
                            }
                        }

                        let sql = Self::chat_sql_blocks(&reply).into_iter().next();
                        if sql.is_none() {
                            self.chat_auto_retries = 0;
                        }
                        self.clear_diagram_agent();
                        outputs.push(Output::ClearDiagramAgent);
                        let _ = self.push_chat_message(context.scope, ChatRole::Assistant, reply);
                        self.sync_code_snippets(context);

                        let answering_results = std::mem::take(&mut self.chat_result_followup);

                        let Some(sql) = sql else {
                            return Self::scroll_chat_to_end();
                        };
                        if context.settings.chat_mode == ChatMode::Ask {
                            self.push_chat_outcome(
                                context.scope,
                                "Ask mode: the SQL was left in the reply, not put in the editor.",
                            );
                            return Self::scroll_chat_to_end();
                        }

                        outputs.push(Output::InsertSql {
                            sql: sql.clone(),
                            announce: false,
                        });

                        if context.settings.chat_mode != ChatMode::AutoRun {
                            self.push_chat_outcome(
                                context.scope,
                                "Drafted into the editor. It was not run.",
                            );
                        } else if answering_results {
                            self.push_chat_outcome(context.scope,
                                "Drafted into the editor. It was not run again, since this reply is about results that already ran.",
                            );
                        } else if !Self::chat_sql_is_read_only(&sql) {
                            self.push_chat_outcome(context.scope,
                                "Not read-only, so it was drafted into the editor instead of run. Review it and run it yourself.",
                            );
                        } else if context.query_running {
                            self.push_chat_outcome(context.scope,
                                "A query was already running, so this one was drafted into the editor instead.",
                            );
                        } else {
                            self.push_chat_outcome(context.scope, "Read-only, so it was run.");
                            self.chat_auto_run_pending = true;
                            outputs.push(Output::RunQuery {
                                sql,
                                ensure_tab: true,
                            });
                            return Self::scroll_chat_to_end();
                        }
                    }
                    Err(error) => self.chat_error = Some(error),
                }
                Self::scroll_chat_to_end()
            }
        }
    }
}
