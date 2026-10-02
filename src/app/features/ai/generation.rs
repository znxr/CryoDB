use super::types::AiModalTarget;
use super::{Context, Message, Output, State};
use crate::ai::{AiRequestConfig, client::generate_ai_sql};
use crate::model::connection::DatabaseDriver;
use iced::{Task, widget::text_editor};
impl State {
    pub(super) fn update_generation(
        &mut self,
        message: Message,
        context: &Context<'_>,
        outputs: &mut Vec<Output>,
    ) -> Task<Message> {
        match message {
            Message::AiPromptAction(action) => {
                self.ai_prompt_content.perform(action);
                self.ai_prompt_error = None;
                Task::none()
            }
            Message::AiUseCurrentSqlContextToggled(enabled) => {
                self.ai_modal_use_current_sql_context = enabled;
                Task::none()
            }
            Message::GenerateAiQuery => {
                if self.is_generating_ai || self.is_fixing_query_with_ai {
                    return Task::none();
                }
                if self.ai_modal_target == AiModalTarget::PostgresRoleSql
                    && !context.role_modal_open
                {
                    self.ai_prompt_error = Some(String::from(
                        "The role modal is closed. Reopen role SQL and try again.",
                    ));
                    return Task::none();
                }
                let prompt_text = self.ai_prompt_content.text();
                if prompt_text.trim().is_empty() {
                    self.ai_prompt_error = Some(String::from("Prompt is empty."));
                    return Task::none();
                }
                if context.settings.ai_endpoint.trim().is_empty() {
                    self.ai_prompt_error =
                        Some(String::from("AI endpoint is empty. Set it in Settings."));
                    return Task::none();
                }
                if context.settings.ai_model.trim().is_empty() {
                    self.ai_prompt_error =
                        Some(String::from("AI model is empty. Set it in Settings."));
                    return Task::none();
                }

                let sql_context = self.ai_modal_sql_context.trim();
                let prompt = if self.ai_modal_use_current_sql_context && !sql_context.is_empty() {
                    format!(
                        "{}\n\nCurrent SQL context:\n{}",
                        prompt_text.trim(),
                        sql_context
                    )
                } else {
                    prompt_text
                };

                let config = AiRequestConfig::from_settings(context.settings);
                let driver = if self.ai_modal_target == AiModalTarget::PostgresRoleSql {
                    DatabaseDriver::PostgreSql
                } else {
                    context.connection.driver
                };
                self.is_generating_ai = true;
                self.ai_prompt_error = None;
                Task::perform(
                    generate_ai_sql(config, prompt, driver),
                    Message::AiQueryGenerated,
                )
            }
            Message::AiQueryGenerated(result) => {
                self.is_generating_ai = false;
                match result {
                    Ok(sql) => {
                        outputs.push(Output::GeneratedSql {
                            target: self.ai_modal_target,
                            sql,
                        });
                        self.ai_modal_open = false;
                        self.ai_modal_target = AiModalTarget::QueryEditor;
                        self.ai_modal_use_current_sql_context = false;
                        self.ai_modal_sql_context.clear();
                        self.ai_prompt_content = text_editor::Content::with_text("");
                        self.ai_prompt_error = None;
                    }
                    Err(error) => {
                        self.ai_prompt_error = Some(error);
                    }
                }
                Task::none()
            }
            Message::CloseAiModal => {
                self.ai_modal_open = false;
                self.ai_modal_target = AiModalTarget::QueryEditor;
                self.ai_modal_use_current_sql_context = false;
                self.ai_modal_sql_context.clear();
                self.ai_prompt_error = None;
                Task::none()
            }
            _ => unreachable!(),
        }
    }
}
