use super::{Context, Message, Output, State};
use crate::app::types::ErrorModalState;
use crate::db::edits::execute_table_command;
use crate::model::add_column::AddColumnDraft;
use crate::model::connection::DatabaseDriver;
use crate::model::table::TableModalState;
use crate::model::table::{PostgresObjectKind, TableCommand};
use crate::utils::helpers::canonical_table_key;
use iced::Task;

impl State {
    pub(crate) fn postgres_object_kind_for_table(
        &self,
        driver: DatabaseDriver,
        table: &str,
    ) -> Option<PostgresObjectKind> {
        if driver != DatabaseDriver::PostgreSql {
            return None;
        }
        let target_key = canonical_table_key(driver, table);
        if target_key.is_empty() {
            return None;
        }

        self.postgres_sidebar_objects.iter().find_map(|object| {
            let object_key = canonical_table_key(driver, &object.qualified_name());
            if object_key.eq_ignore_ascii_case(&target_key) {
                Some(object.kind)
            } else {
                None
            }
        })
    }
    pub(crate) fn table_is_row_editable(&self, driver: DatabaseDriver, table: &str) -> bool {
        if driver != DatabaseDriver::PostgreSql {
            return true;
        }

        match self.postgres_object_kind_for_table(driver, table) {
            Some(PostgresObjectKind::Table | PostgresObjectKind::ForeignTable) => true,
            Some(
                PostgresObjectKind::View
                | PostgresObjectKind::MaterializedView
                | PostgresObjectKind::Sequence
                | PostgresObjectKind::Extension
                | PostgresObjectKind::Role,
            ) => false,
            None => true,
        }
    }
    pub(crate) fn table_supports_table_actions(&self, driver: DatabaseDriver, table: &str) -> bool {
        if driver != DatabaseDriver::PostgreSql {
            return true;
        }

        match self.postgres_object_kind_for_table(driver, table) {
            Some(PostgresObjectKind::Table) => true,
            Some(
                PostgresObjectKind::ForeignTable
                | PostgresObjectKind::View
                | PostgresObjectKind::MaterializedView
                | PostgresObjectKind::Sequence
                | PostgresObjectKind::Extension
                | PostgresObjectKind::Role,
            ) => false,
            None => true,
        }
    }
    pub(crate) fn table_command_success_text(command: &TableCommand, changed: usize) -> String {
        match command {
            TableCommand::AlterTable { table, .. } => {
                crate::i18n::tr_with("Table `{table}` altered.", &[("{table}", table)])
            }
            TableCommand::AddColumn { table, .. } => {
                crate::i18n::tr_with("Column added to `{table}`.", &[("{table}", table)])
            }
            TableCommand::Truncate { table } => crate::i18n::tr_with(
                "Table `{table}` truncated ({rows} rows affected).",
                &[("{table}", table), ("{rows}", &changed.to_string())],
            ),
            TableCommand::Drop { table } => {
                crate::i18n::tr_with("Table `{table}` dropped.", &[("{table}", table)])
            }
            TableCommand::Duplicate { table, new_table } => crate::i18n::tr_with(
                "Table `{table}` duplicated to `{target}`.",
                &[("{table}", table), ("{target}", new_table)],
            ),
            TableCommand::Rename { table, new_table } => crate::i18n::tr_with(
                "Table `{table}` renamed to `{target}`.",
                &[("{table}", table), ("{target}", new_table)],
            ),
        }
    }
    pub(super) fn execute_table_command(
        &mut self,
        command: TableCommand,
        context: Context<'_>,
    ) -> (Task<Message>, Option<Output>) {
        let mut output = None;

        if self.table_action_running {
            return (Task::none(), output);
        }
        let target_table = match &command {
            TableCommand::AlterTable { table, .. }
            | TableCommand::AddColumn { table, .. }
            | TableCommand::Truncate { table }
            | TableCommand::Drop { table }
            | TableCommand::Duplicate { table, .. }
            | TableCommand::Rename { table, .. } => table.as_str(),
        };
        if !self.table_supports_table_actions(context.driver, target_table) {
            output = Some(Output::Error(Some(ErrorModalState::new(
                "Table action",
                "This PostgreSQL object does not support this action.",
                "Use table actions on base tables.",
            ))));
            return (Task::none(), output);
        }
        let Some(pool) = context.pool.cloned() else {
            output = Some(Output::Error(Some(ErrorModalState::new(
                "Table action",
                "Not connected.",
                "Connect to a database and try again.",
            ))));
            return (Task::none(), output);
        };
        let database = context.database.unwrap_or_default();
        if database.is_empty() {
            output = Some(Output::Error(Some(ErrorModalState::new(
                "Table action",
                "No database selected.",
                "Select a database and try again.",
            ))));
            return (Task::none(), output);
        }
        self.table_action_running = true;
        self.table_modal = None;
        self.table_context_menu = None;
        (
            Task::perform(
                execute_table_command(pool, database, command.clone()),
                move |result| Message::TableCommandFinished { command, result },
            ),
            output,
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TableAction {
    Alter,
    AddColumn,
    Truncate,
    Drop,
    Duplicate,
    Rename,
    MoveToFolder,
}

impl State {
    pub(super) fn open_table_action(
        &mut self,
        action: TableAction,
        driver: DatabaseDriver,
    ) -> Option<Output> {
        let table = self.table_context_menu.as_ref()?.table.clone();
        self.table_context_menu = None;
        if action != TableAction::MoveToFolder && !self.table_supports_table_actions(driver, &table)
        {
            return None;
        }
        let output = (action == TableAction::AddColumn).then(|| Output::LoadColumns(table.clone()));
        self.table_modal = Some(match action {
            TableAction::Alter => TableModalState::AlterTable {
                table,
                clause: "MODIFY COLUMN ".into(),
            },
            TableAction::AddColumn => TableModalState::AddColumn {
                table,
                draft: AddColumnDraft::new(driver),
            },
            TableAction::Truncate => TableModalState::Confirm {
                title: "Truncate table".into(),
                summary: crate::i18n::tr_with(
                    "TRUNCATE will remove all rows from `{table}`.",
                    &[("{table}", &table)],
                ),
                command: TableCommand::Truncate { table },
            },
            TableAction::Drop => TableModalState::Confirm {
                title: "Drop table".into(),
                summary: crate::i18n::tr_with(
                    "DROP will permanently remove `{table}`.",
                    &[("{table}", &table)],
                ),
                command: TableCommand::Drop { table },
            },
            TableAction::Duplicate => TableModalState::DuplicateTable {
                new_name: format!("{table}_copy"),
                table,
            },
            TableAction::Rename => TableModalState::RenameTable {
                new_name: table.clone(),
                table,
            },
            TableAction::MoveToFolder => TableModalState::MoveToFolder {
                folder: self.folder_for_table(driver, &table).unwrap_or_default(),
                table,
            },
        });
        output
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::features::workspace::explorer::TableContextMenuState;
    use crate::model::table::{FolderStore, PostgresSidebarObject};

    #[test]
    fn postgres_views_allow_folder_moves_but_reject_table_alterations() {
        let mut state = State::new(FolderStore::default(), None);
        state.postgres_sidebar_objects = vec![PostgresSidebarObject {
            schema: "public".into(),
            name: "report".into(),
            kind: PostgresObjectKind::View,
        }];
        let menu = || TableContextMenuState {
            table: "public.report".into(),
            position: iced::Point::ORIGIN,
        };
        state.table_context_menu = Some(menu());
        assert!(
            state
                .open_table_action(TableAction::Drop, DatabaseDriver::PostgreSql)
                .is_none()
        );
        assert!(state.table_modal.is_none());
        state.table_context_menu = Some(menu());
        let _ = state.open_table_action(TableAction::MoveToFolder, DatabaseDriver::PostgreSql);
        assert!(matches!(
            state.table_modal,
            Some(TableModalState::MoveToFolder { .. })
        ));
    }
}
