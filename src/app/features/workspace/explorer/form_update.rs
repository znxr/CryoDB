use super::Message;
use super::{Context, Output, State};
use crate::app::types::ErrorModalState;
use crate::model::table::{TableCommand, TableFolder, TableModalState};
use iced::Task;

impl State {
    pub(super) fn submit_table_modal(
        &mut self,
        context: Context<'_>,
    ) -> (Task<Message>, Option<Output>) {
        let mut output = None;

        let Some(modal) = self.table_modal.clone() else {
            return (Task::none(), output);
        };
        match modal {
            TableModalState::MakeFolder {
                name,
                open_by_default,
                pin_to_top,
                source_folder,
            } => {
                let folder = name.trim();
                let is_renaming = source_folder
                    .as_ref()
                    .is_some_and(|value| !value.trim().is_empty());
                let modal_title = if is_renaming {
                    "Rename Folder"
                } else {
                    "Make a Folder"
                };
                if folder.is_empty() {
                    output = Some(Output::Error(Some(ErrorModalState::new(
                        modal_title,
                        "Folder name is required.",
                        "Enter a folder name before saving.",
                    ))));
                    return (Task::none(), output);
                }
                if let Some(source) = source_folder.as_deref().map(str::trim) {
                    let conflict = self.table_folders.iter().any(|entry| {
                        entry.name.eq_ignore_ascii_case(folder)
                            && !entry.name.eq_ignore_ascii_case(source)
                    });
                    if conflict {
                        output = Some(Output::Error(Some(ErrorModalState::new(
                            "Rename Folder",
                            "Folder already exists.",
                            crate::i18n::tr_with(
                                "A folder named `{folder}` already exists.",
                                &[("{folder}", folder)],
                            ),
                        ))));
                        return (Task::none(), output);
                    }
                    let Some(index) = self
                        .table_folders
                        .iter()
                        .position(|entry| entry.name.eq_ignore_ascii_case(source))
                    else {
                        output = Some(Output::Error(Some(ErrorModalState::new(
                            "Rename Folder",
                            "Folder no longer exists.",
                            "Refresh tables and try again.",
                        ))));
                        return (Task::none(), output);
                    };

                    let previous_name = self.table_folders[index].name.clone();
                    let name_changed = previous_name != folder;
                    self.table_folders[index].name = folder.to_string();
                    self.table_folders[index].is_open = open_by_default;
                    self.table_folders[index].pin_to_top = pin_to_top;

                    if name_changed {
                        let target = folder.to_string();
                        for assigned in self.table_folder_map.values_mut() {
                            if assigned.eq_ignore_ascii_case(previous_name.as_str()) {
                                *assigned = target.clone();
                            }
                        }
                    }

                    self.sort_folders();
                    self.persist_folder_store(context.folder_keys);
                    self.table_modal = None;
                    output = Some(Output::ApplyStatus {
                        error: None,
                        message: Some(if name_changed {
                            crate::i18n::tr_with(
                                "Folder `{from}` renamed to `{to}`.",
                                &[("{from}", &previous_name), ("{to}", folder)],
                            )
                        } else {
                            crate::i18n::tr_with(
                                "Folder `{folder}` updated.",
                                &[("{folder}", folder)],
                            )
                        }),
                    });
                    return (Task::none(), output);
                }

                if self.folder_exists(folder) {
                    output = Some(Output::Error(Some(ErrorModalState::new(
                        "Make a Folder",
                        "Folder already exists.",
                        crate::i18n::tr_with(
                            "A folder named `{folder}` already exists.",
                            &[("{folder}", folder)],
                        ),
                    ))));
                    return (Task::none(), output);
                }
                self.table_folders.push(TableFolder {
                    name: folder.to_string(),
                    is_open: open_by_default,
                    pin_to_top,
                    auto_generated: false,
                });
                self.sort_folders();
                self.persist_folder_store(context.folder_keys);
                self.table_modal = None;
                output = Some(Output::ApplyStatus {
                    error: None,
                    message: Some(crate::i18n::tr_with(
                        "Folder `{folder}` created.",
                        &[("{folder}", folder)],
                    )),
                });
                (Task::none(), output)
            }
            TableModalState::AlterTable { table, clause } => {
                let clause = clause.trim().trim_end_matches(';').trim();
                if clause.is_empty() {
                    output = Some(Output::Error(Some(ErrorModalState::new(
                        "Alter Table",
                        "ALTER clause is required.",
                        "Enter the ALTER TABLE clause to run.",
                    ))));
                    return (Task::none(), output);
                }
                self.execute_table_command(
                    TableCommand::AlterTable {
                        table,
                        clause: clause.to_string(),
                    },
                    context,
                )
            }
            TableModalState::AddColumn { table, draft } => {
                let definition = match draft.definition(context.driver) {
                    Ok(definition) => definition,
                    Err(reason) => {
                        output = Some(Output::Error(Some(ErrorModalState::new(
                            "Add Column",
                            "The column is not ready yet.",
                            reason,
                        ))));
                        return (Task::none(), output);
                    }
                };
                self.execute_table_command(TableCommand::AddColumn { table, definition }, context)
            }
            TableModalState::DuplicateTable { table, new_name } => {
                let new_name = new_name.trim();
                if new_name.is_empty() {
                    output = Some(Output::Error(Some(ErrorModalState::new(
                        "Duplicate Table",
                        "New table name is required.",
                        "Enter the destination table name.",
                    ))));
                    return (Task::none(), output);
                }
                self.execute_table_command(
                    TableCommand::Duplicate {
                        table,
                        new_table: new_name.to_string(),
                    },
                    context,
                )
            }
            TableModalState::RenameTable { table, new_name } => {
                let new_name = new_name.trim();
                if new_name.is_empty() {
                    output = Some(Output::Error(Some(ErrorModalState::new(
                        "Rename Table",
                        "New table name is required.",
                        "Enter the new table name.",
                    ))));
                    return (Task::none(), output);
                }
                self.execute_table_command(
                    TableCommand::Rename {
                        table,
                        new_table: new_name.to_string(),
                    },
                    context,
                )
            }
            TableModalState::RenameQueryTab { index, name } => {
                self.table_modal = None;
                (Task::none(), Some(Output::RenameQueryTab { index, name }))
            }
            TableModalState::MoveToFolder { table, folder } => {
                self.assign_table_to_folder(
                    context.driver,
                    context.folder_keys,
                    &table,
                    if folder.trim().is_empty() {
                        None
                    } else {
                        Some(folder)
                    },
                );
                self.table_modal = None;
                (Task::none(), output)
            }
            TableModalState::ConfirmFoldersImport { path, groups, .. } => {
                let (imported_groups, assigned_tables) =
                    self.import_generated_folders(context.folder_keys, context.driver, groups);
                self.table_modal = None;
                output = Some(Output::FolderFileSuccess(crate::i18n::tr_with(
                    "Imported {groups} folder(s) and mapped {tables} table(s) from {path}.",
                    &[
                        ("{groups}", &imported_groups.to_string()),
                        ("{tables}", &assigned_tables.to_string()),
                        ("{path}", &path.display().to_string()),
                    ],
                )));
                (Task::none(), output)
            }
            TableModalState::Confirm { command, .. } => {
                self.execute_table_command(command, context)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{connection::DatabaseDriver, settings::Settings, table::FolderStore};

    #[test]
    fn editing_an_alter_form_validates_before_requesting_execution() {
        let mut state = State::new(FolderStore::default(), None);
        let settings = Settings::default();
        let context = || Context {
            settings: &settings,
            workspace_busy: false,
            query_running: false,
            menu_fallback: iced::Point::ORIGIN,
            theme: iced::Theme::Dark,
            folder_keys: None,
            driver: DatabaseDriver::MySql,
            pool: None,
            database: Some("app".into()),
            is_connected: true,
            selected_table: None,
            query: String::new(),
        };
        state.table_modal = Some(TableModalState::AlterTable {
            table: "users".into(),
            clause: " ".into(),
        });
        let (_, output) = state.update(Message::SubmitTableModal, context());
        assert!(matches!(output, Some(Output::Error(Some(_)))));
        assert!(state.table_modal.is_some());
        let _ = state.update(
            Message::TableModalPrimaryChanged(" ADD COLUMN age INT; ".into()),
            context(),
        );
        let (_, output) = state.update(Message::SubmitTableModal, context());
        let Some(Output::Error(Some(error))) = output else {
            panic!("expected connection validation");
        };
        assert_eq!(error.summary, "Not connected.");
    }
}
