use super::{Context, Message, Output, State};
use crate::ai::{
    AiRequestConfig,
    folders::{generate_ai_folder_groups, generate_fast_folder_groups},
};
use crate::app::types::ErrorModalState;
use crate::constants::FOLDER_GROUPING_FAST_MIN_TABLES;
use crate::model::table::GeneratedFolder;
use iced::Task;
use std::collections::HashSet;

impl State {
    pub(super) fn generate_folders(
        &mut self,
        context: Context<'_>,
    ) -> (Task<Message>, Option<Output>) {
        let mut output = None;

        if self.folder_generation_running {
            return (Task::none(), output);
        }
        if self.tables.is_empty() {
            output = Some(Output::Error(Some(ErrorModalState::new(
                "Folders",
                "No tables loaded.",
                "Load a database with tables and try again.",
            ))));
            return (Task::none(), output);
        }
        self.sidebar_tools_open = false;
        self.folder_context_menu = None;
        self.pending_generated_folders.clear();
        let fast_groups = generate_fast_folder_groups(&self.tables);
        let mut seed_groups = Vec::new();
        let mut seeded_tables = HashSet::new();
        for group in &fast_groups {
            if group.tables.len() >= FOLDER_GROUPING_FAST_MIN_TABLES {
                seeded_tables.extend(group.tables.iter().cloned());
                seed_groups.push(group.clone());
            }
        }
        let unresolved_tables = self
            .tables
            .iter()
            .filter(|table| !seeded_tables.contains(*table))
            .cloned()
            .collect::<Vec<_>>();
        if unresolved_tables.is_empty() {
            let groups = if seed_groups.is_empty() {
                fast_groups
            } else {
                seed_groups
            };
            let assigned =
                self.apply_generated_folders(context.folder_keys, context.driver, groups);
            output = Some(Output::ApplyStatus {
                error: None,
                message: Some(crate::i18n::tr_with(
                    "Grouped {tables} table(s) into folders.",
                    &[("{tables}", &assigned.to_string())],
                )),
            });
            return (Task::none(), output);
        }
        if context.settings.ai_endpoint.trim().is_empty() {
            let fallback = if seed_groups.is_empty() {
                fast_groups
            } else {
                seed_groups
            };
            if fallback.is_empty() {
                output = Some(Output::Error(Some(ErrorModalState::new(
                    "Folders",
                    "AI endpoint is empty.",
                    "Set an AI endpoint in Settings to run deep grouping.",
                ))));
            } else {
                let assigned =
                    self.apply_generated_folders(context.folder_keys, context.driver, fallback);
                output = Some(Output::ApplyStatus {
                    error: Some(String::from("AI endpoint is empty.")),
                    message: Some(crate::i18n::tr_with(
                        "Applied fast grouping to {tables} table(s).",
                        &[("{tables}", &assigned.to_string())],
                    )),
                });
            }
            return (Task::none(), output);
        }
        if context.settings.ai_model.trim().is_empty() {
            let fallback = if seed_groups.is_empty() {
                fast_groups
            } else {
                seed_groups
            };
            if fallback.is_empty() {
                output = Some(Output::Error(Some(ErrorModalState::new(
                    "Folders",
                    "AI model is empty.",
                    "Set an AI model in Settings first.",
                ))));
            } else {
                let assigned =
                    self.apply_generated_folders(context.folder_keys, context.driver, fallback);
                output = Some(Output::ApplyStatus {
                    error: Some(String::from("AI model is empty.")),
                    message: Some(crate::i18n::tr_with(
                        "Applied fast grouping to {tables} table(s).",
                        &[("{tables}", &assigned.to_string())],
                    )),
                });
            }
            return (Task::none(), output);
        }
        self.pending_generated_folders = seed_groups;
        self.folder_generation_running = true;
        let config = AiRequestConfig::from_settings(context.settings);
        let existing_folders = self
            .pending_generated_folders
            .iter()
            .map(|group| group.folder.clone())
            .collect::<Vec<_>>();
        (
            Task::perform(
                generate_ai_folder_groups(config, unresolved_tables, existing_folders),
                Message::FolderGenerationFinished,
            ),
            output,
        )
    }
    pub(super) fn folder_generation_finished(
        &mut self,
        result: Result<Vec<GeneratedFolder>, String>,
        context: Context<'_>,
    ) -> (Task<Message>, Option<Output>) {
        let mut output = None;

        self.folder_generation_running = false;
        if !context.is_connected {
            self.pending_generated_folders.clear();
            return (Task::none(), output);
        }
        match result {
            Ok(mut groups) => {
                let mut combined = std::mem::take(&mut self.pending_generated_folders);
                combined.append(&mut groups);
                let assigned =
                    self.apply_generated_folders(context.folder_keys, context.driver, combined);
                output = Some(Output::ApplyStatus {
                    error: None,
                    message: Some(crate::i18n::tr_with(
                        "Grouped {tables} table(s) into folders.",
                        &[("{tables}", &assigned.to_string())],
                    )),
                });
            }
            Err(error) => {
                let fallback = std::mem::take(&mut self.pending_generated_folders);
                if fallback.is_empty() {
                    output = Some(Output::Error(Some(ErrorModalState::new(
                        "Folders",
                        "Failed to group tables.",
                        error,
                    ))));
                } else {
                    let assigned =
                        self.apply_generated_folders(context.folder_keys, context.driver, fallback);
                    output = Some(Output::ApplyStatus {
                        error: Some(crate::i18n::tr_with(
                            "AI grouping error: {error}",
                            &[("{error}", &error)],
                        )),
                        message: Some(crate::i18n::tr_with(
                            "Applied fast grouping to {tables} table(s).",
                            &[("{tables}", &assigned.to_string())],
                        )),
                    });
                }
            }
        }
        (Task::none(), output)
    }
}
