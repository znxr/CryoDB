use super::{Context, Message, Output, State};
use crate::app::types::ErrorModalState;
use crate::model::table::{FolderGroupsToml, TableModalState};
use iced::Task;
use rfd::AsyncFileDialog;
use std::collections::HashSet;

impl State {
    pub(super) fn folder_file_update(
        &mut self,
        message: Message,
        context: Context<'_>,
    ) -> (Task<Message>, Option<Output>) {
        match message {
            Message::ImportFolders => {
                self.sidebar_tools_open = false;
                self.table_context_menu = None;
                self.postgres_object_context_menu = None;
                self.folder_context_menu = None;
                let task = AsyncFileDialog::new()
                    .add_filter("TOML", &["toml"])
                    .pick_file();
                (
                    Task::perform(task, |file| {
                        Message::FoldersImportFilePicked(
                            file.map(|handle| handle.path().to_path_buf()),
                        )
                    }),
                    None,
                )
            }
            Message::ExportFolders => {
                self.sidebar_tools_open = false;
                self.table_context_menu = None;
                self.postgres_object_context_menu = None;
                self.folder_context_menu = None;
                let task = AsyncFileDialog::new()
                    .add_filter("TOML", &["toml"])
                    .set_file_name("folders.toml")
                    .save_file();
                (
                    Task::perform(task, |file| {
                        Message::FoldersExportFilePicked(
                            file.map(|handle| handle.path().to_path_buf()),
                        )
                    }),
                    None,
                )
            }
            Message::FoldersImportFilePicked(file) => {
                let mut output = None;
                let Some(path) = file else {
                    return (Task::none(), output);
                };

                let contents = match std::fs::read_to_string(&path) {
                    Ok(contents) => contents,
                    Err(error) => {
                        output = Some(Output::Error(Some(ErrorModalState::new(
                            "Folders",
                            "Could not read the selected TOML file.",
                            format!("{}\n\n{}", path.display(), error),
                        ))));
                        return (Task::none(), output);
                    }
                };

                let parsed = match toml::from_str::<FolderGroupsToml>(&contents) {
                    Ok(parsed) => parsed,
                    Err(error) => {
                        output = Some(Output::Error(Some(ErrorModalState::new(
                            "Folders",
                            "Could not parse the selected TOML file.",
                            format!("{}\n\n{}", path.display(), error),
                        ))));
                        return (Task::none(), output);
                    }
                };

                let generated_folder_names = self
                    .table_folders
                    .iter()
                    .filter(|folder| folder.auto_generated)
                    .map(|folder| folder.name.to_ascii_lowercase())
                    .collect::<HashSet<_>>();
                let existing_groups = generated_folder_names.len();
                let existing_tables = self
                    .table_folder_map
                    .values()
                    .filter(|folder| generated_folder_names.contains(&folder.to_ascii_lowercase()))
                    .count();

                if existing_groups > 0 {
                    output = Some(Output::Error(None));
                    self.table_modal = Some(TableModalState::ConfirmFoldersImport {
                        path,
                        groups: parsed.groups,
                        existing_groups,
                        existing_tables,
                    });
                    return (Task::none(), output);
                }

                let (imported_groups, assigned_tables) = self.import_generated_folders(
                    context.folder_keys,
                    context.driver,
                    parsed.groups,
                );
                output = Some(Output::FolderFileSuccess(crate::i18n::tr_with(
                    "Imported {groups} folder(s) and mapped {tables} table(s).",
                    &[
                        ("{groups}", &imported_groups.to_string()),
                        ("{tables}", &assigned_tables.to_string()),
                    ],
                )));
                (Task::none(), output)
            }
            Message::FoldersExportFilePicked(file) => {
                let mut output = None;
                let Some(mut path) = file else {
                    return (Task::none(), output);
                };

                if path.extension().is_none() {
                    path.set_extension("toml");
                }

                let backup = FolderGroupsToml {
                    version: 1,
                    groups: self.export_generated_folders(context.driver),
                };

                let payload = match toml::to_string_pretty(&backup) {
                    Ok(payload) => payload,
                    Err(error) => {
                        output = Some(Output::Error(Some(ErrorModalState::new(
                            "Folders",
                            "Could not prepare the export file.",
                            error.to_string(),
                        ))));
                        return (Task::none(), output);
                    }
                };

                if let Some(parent) = path.parent()
                    && let Err(error) = std::fs::create_dir_all(parent)
                {
                    output = Some(Output::Error(Some(ErrorModalState::new(
                        "Folders",
                        "Could not create the export folder.",
                        format!("{}\n\n{}", parent.display(), error),
                    ))));
                    return (Task::none(), output);
                }

                if let Err(error) = std::fs::write(&path, payload) {
                    output = Some(Output::Error(Some(ErrorModalState::new(
                        "Folders",
                        "Could not write the export file.",
                        format!("{}\n\n{}", path.display(), error),
                    ))));
                    return (Task::none(), output);
                }

                let group_count = backup.groups.len();
                output = Some(Output::FolderFileSuccess(crate::i18n::tr_with(
                    "Exported {groups} folder(s) to {path}.",
                    &[
                        ("{groups}", &group_count.to_string()),
                        ("{path}", &path.display().to_string()),
                    ],
                )));
                (Task::none(), output)
            }
            _ => unreachable!(),
        }
    }
}
