use super::{Context, Message, Output, State};
use crate::ai::{
    AiRequestConfig,
    sql_fix::{generate_ai_sql_fix, should_offer_ai_query_fix},
};
use iced::Task;
impl State {
    pub(super) fn update_fix(
        &mut self,
        message: Message,
        context: &Context<'_>,
        outputs: &mut Vec<Output>,
    ) -> Task<Message> {
        match message {
            Message::FixQueryWithAi => {
                if self.is_fixing_query_with_ai || self.is_generating_ai || context.query_running {
                    return Task::none();
                }
                let Some(error_message) = context.query_error.map(str::to_string) else {
                    return Task::none();
                };
                if !should_offer_ai_query_fix(&error_message) {
                    return Task::none();
                }
                let original_query = context.query.content();
                if original_query.trim().is_empty() {
                    outputs.push(Output::FixError(String::from("Query is empty.")));
                    return Task::none();
                }
                if context.settings.ai_endpoint.trim().is_empty() {
                    outputs.push(Output::FixError(String::from(
                        "AI endpoint is empty. Set it in Settings.",
                    )));
                    return Task::none();
                }
                if context.settings.ai_model.trim().is_empty() {
                    outputs.push(Output::FixError(String::from(
                        "AI model is empty. Set it in Settings.",
                    )));
                    return Task::none();
                }

                let config = AiRequestConfig::from_settings(context.settings);
                let pool = context.pool.cloned();
                let driver = context.connection.driver;
                let database = context.database.clone();
                let known_tables = context.tables.to_vec();
                let selected_table = context.selected_table.map(str::to_string);
                self.is_fixing_query_with_ai = true;
                outputs.push(Output::FixStarted);

                Task::perform(
                    generate_ai_sql_fix(
                        config,
                        pool,
                        driver,
                        database,
                        known_tables,
                        selected_table,
                        original_query.clone(),
                        error_message,
                    ),
                    move |result| Message::AiQueryFixFinished {
                        source_query: original_query,
                        result,
                    },
                )
            }
            Message::AiQueryFixFinished {
                source_query,
                result,
            } => {
                self.is_fixing_query_with_ai = false;
                match result {
                    Ok(fix) => {
                        let current_query = context.query.content();
                        if current_query.trim() != source_query.trim() {
                            outputs.push(Output::FixError(String::from(
                                "Query changed while AI fix was running. Review and retry.",
                            )));
                            return Task::none();
                        }

                        if let Some(fixed_query) = fix
                            .fixed_query
                            .as_deref()
                            .map(str::trim)
                            .filter(|value| !value.is_empty())
                        {
                            let diff_hint = fix
                                .diff_summary
                                .first()
                                .map(|value| value.trim())
                                .filter(|value| !value.is_empty());
                            outputs.push(Output::FixedSql {
                                sql: fixed_query.to_string(),
                                summary: match diff_hint {
                                    Some(diff_hint) => format!(
                                        "AI fix applied ({}, {:.0}% confidence): {}",
                                        fix.fix_type,
                                        (fix.confidence * 100.0).clamp(0.0, 100.0),
                                        diff_hint
                                    ),
                                    None => format!(
                                        "AI fix applied ({}, {:.0}% confidence).",
                                        fix.fix_type,
                                        (fix.confidence * 100.0).clamp(0.0, 100.0)
                                    ),
                                },
                            });
                        } else {
                            let explanation = fix.explanation.trim();
                            outputs.push(Output::FixError(if explanation.is_empty() {
                                String::from("AI could not produce a safe SQL fix.")
                            } else {
                                crate::i18n::tr_with(
                                    "AI could not safely fix the query: {explanation}",
                                    &[("{explanation}", explanation)],
                                )
                            }));
                        }
                    }
                    Err(error) => {
                        outputs.push(Output::FixError(crate::i18n::tr_with(
                            "AI fix failed: {error}",
                            &[("{error}", &error)],
                        )));
                    }
                }
                Task::none()
            }
            _ => unreachable!(),
        }
    }
}
