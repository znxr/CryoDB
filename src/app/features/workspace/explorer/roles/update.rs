use super::super::{Context, Message, Output};
use super::State;
use crate::app::types::{ErrorModalState, ToastLevel};
use crate::db::{DatabasePool, metadata::fetch_postgres_role_form, query::run_query};
use crate::model::connection::DatabaseDriver;
use crate::model::table::{
    PostgresRoleForm, PostgresRoleModalMode, PostgresRoleModalTab, PostgresRoleTextField,
    PostgresRoleToggleField,
};
use crate::utils::helpers::sql_quote_identifier;
use iced::{Task, widget::text_editor};

impl State {
    pub(crate) fn postgres_role_context_create(
        &mut self,
        _context: Context<'_>,
    ) -> (Task<Message>, Option<Output>) {
        let output = None;

        self.postgres_object_context_menu = None;
        self.open_postgres_role_create_modal();
        (Task::none(), output)
    }
    pub(crate) fn postgres_role_context_drop(
        &mut self,
        context: Context<'_>,
    ) -> (Task<Message>, Option<Output>) {
        let mut output = None;

        if self.postgres_role_action_running || context.workspace_busy || self.table_action_running
        {
            return (Task::none(), output);
        }
        let Some(role_name) = self
            .postgres_object_context_menu
            .as_ref()
            .map(|menu| menu.object.name.clone())
        else {
            return (Task::none(), output);
        };
        let Some(pool) = context.pool.cloned() else {
            output = Some(Output::Error(Some(ErrorModalState::new(
                "Drop role",
                "Not connected.",
                "Connect to a PostgreSQL database and try again.",
            ))));
            return (Task::none(), output);
        };
        let Some(database) = context.database else {
            output = Some(Output::Error(Some(ErrorModalState::new(
                "Drop role",
                "No database selected.",
                "Select a database first.",
            ))));
            return (Task::none(), output);
        };

        self.postgres_object_context_menu = None;
        let statement = format!(
            "DROP ROLE IF EXISTS {};",
            sql_quote_identifier(DatabaseDriver::PostgreSql, &role_name)
        );
        output = Some(Output::RoleSqlStarted(statement.clone()));
        self.postgres_role_action_running = true;

        (
            Task::perform(
                execute_sql_statements(pool, Some(database), vec![statement]),
                move |result| Message::PostgresRoleDropFinished {
                    role: role_name,
                    result,
                },
            ),
            output,
        )
    }
    pub(crate) fn postgres_role_context_create_script(
        &mut self,
        context: Context<'_>,
    ) -> (Task<Message>, Option<Output>) {
        let mut output = None;

        let Some(role_name) = self
            .postgres_object_context_menu
            .as_ref()
            .map(|menu| menu.object.name.clone())
        else {
            return (Task::none(), output);
        };
        let Some(pool) = context.pool.cloned() else {
            output = Some(Output::Error(Some(ErrorModalState::new(
                "Create role script",
                "Not connected.",
                "Connect to a PostgreSQL database and try again.",
            ))));
            return (Task::none(), output);
        };
        let Some(database) = context.database else {
            output = Some(Output::Error(Some(ErrorModalState::new(
                "Create role script",
                "No database selected.",
                "Select a database first.",
            ))));
            return (Task::none(), output);
        };

        self.postgres_object_context_menu = None;
        (
            Task::perform(
                fetch_postgres_role_form(pool, database, role_name.clone()),
                move |result| Message::PostgresRoleScriptLoaded {
                    role: role_name,
                    result,
                },
            ),
            output,
        )
    }
    pub(crate) fn postgres_role_context_properties(
        &mut self,
        context: Context<'_>,
    ) -> (Task<Message>, Option<Output>) {
        let mut output = None;

        let Some(role_name) = self
            .postgres_object_context_menu
            .as_ref()
            .map(|menu| menu.object.name.clone())
        else {
            return (Task::none(), output);
        };
        let Some(pool) = context.pool.cloned() else {
            output = Some(Output::Error(Some(ErrorModalState::new(
                "Role properties",
                "Not connected.",
                "Connect to a PostgreSQL database and try again.",
            ))));
            return (Task::none(), output);
        };
        let Some(database) = context.database else {
            output = Some(Output::Error(Some(ErrorModalState::new(
                "Role properties",
                "No database selected.",
                "Select a database first.",
            ))));
            return (Task::none(), output);
        };

        self.postgres_object_context_menu = None;
        (
            Task::perform(
                fetch_postgres_role_form(pool, database, role_name.clone()),
                move |result| Message::PostgresRolePropertiesLoaded {
                    role: role_name,
                    result,
                },
            ),
            output,
        )
    }
    pub(crate) fn postgres_role_properties_loaded(
        &mut self,
        role: String,
        result: Result<PostgresRoleForm, String>,
        _context: Context<'_>,
    ) -> (Task<Message>, Option<Output>) {
        let mut output = None;

        match result {
            Ok(mut form) => {
                if form.role_name.trim().is_empty() {
                    form.role_name = role.clone();
                }
                self.open_postgres_role_properties_modal(role, form);
            }
            Err(error) => {
                output = Some(Output::Error(Some(ErrorModalState::new(
                    "Role properties",
                    "Could not load role details.",
                    error,
                ))));
            }
        }
        (Task::none(), output)
    }
    pub(crate) fn postgres_role_script_loaded(
        &mut self,
        role: String,
        result: Result<PostgresRoleForm, String>,
        _context: Context<'_>,
    ) -> (Task<Message>, Option<Output>) {
        let (form, notice) = match result {
            Ok(mut form) => {
                if form.role_name.trim().is_empty() {
                    form.role_name = role.clone();
                }
                (form, None)
            }
            Err(error) => (
                PostgresRoleForm::with_role_name(role.clone()),
                Some(format!(
                    "Using default role script for `{role}` because details could not be loaded: {error}"
                )),
            ),
        };
        (
            Task::none(),
            Some(Output::RoleScript {
                notice,
                result: self.postgres_role_create_script(&form),
            }),
        )
    }
    pub(crate) fn postgres_role_drop_finished(
        &mut self,
        role: String,
        result: Result<(), String>,
        _context: Context<'_>,
    ) -> (Task<Message>, Option<Output>) {
        self.postgres_role_action_running = false;
        match result {
            Ok(()) => {
                self.postgres_role_modal = None;
                self.selected_postgres_object = None;
                (
                    Task::none(),
                    Some(Output::RoleChanged(crate::i18n::tr_with(
                        "Role `{role}` dropped.",
                        &[("{role}", &role)],
                    ))),
                )
            }
            Err(error) => {
                let output = Some(Output::Error(Some(ErrorModalState::new(
                    "Drop role",
                    "Could not drop role.",
                    error,
                ))));
                (Task::none(), output)
            }
        }
    }
    pub(crate) fn postgres_role_modal_tab_selected(
        &mut self,
        tab: PostgresRoleModalTab,
        _context: Context<'_>,
    ) -> (Task<Message>, Option<Output>) {
        let output = None;

        if let Some(modal) = self.postgres_role_modal.as_mut() {
            modal.active_tab = tab;
        }
        (Task::none(), output)
    }
    pub(crate) fn postgres_role_modal_text_changed(
        &mut self,
        field: PostgresRoleTextField,
        value: String,
        _context: Context<'_>,
    ) -> (Task<Message>, Option<Output>) {
        let output = None;

        self.update_postgres_role_modal_text(field, value);
        (Task::none(), output)
    }
    pub(crate) fn postgres_role_modal_toggle_changed(
        &mut self,
        field: PostgresRoleToggleField,
        enabled: bool,
        _context: Context<'_>,
    ) -> (Task<Message>, Option<Output>) {
        let output = None;

        self.update_postgres_role_modal_toggle(field, enabled);
        (Task::none(), output)
    }
    pub(crate) fn postgres_role_modal_sql_action(
        &mut self,
        action: text_editor::Action,
        _context: Context<'_>,
    ) -> (Task<Message>, Option<Output>) {
        let output = None;

        self.apply_postgres_role_modal_sql_action(action);
        (Task::none(), output)
    }
    pub(crate) fn postgres_role_modal_reset(
        &mut self,
        _context: Context<'_>,
    ) -> (Task<Message>, Option<Output>) {
        let output = None;

        self.reset_postgres_role_modal();
        (Task::none(), output)
    }
    pub(crate) fn postgres_role_modal_generate_sql_with_ai(
        &mut self,
        _context: Context<'_>,
    ) -> (Task<Message>, Option<Output>) {
        let Some(modal) = self.postgres_role_modal.as_ref() else {
            return (Task::none(), None);
        };
        (
            Task::none(),
            Some(Output::RoleAi {
                prompt: self.postgres_role_modal_ai_prompt(modal),
                sql: modal.sql_editor.text(),
            }),
        )
    }
    pub(crate) fn postgres_role_modal_save(
        &mut self,
        context: Context<'_>,
    ) -> (Task<Message>, Option<Output>) {
        let mut output = None;

        if self.postgres_role_action_running || context.workspace_busy || self.table_action_running
        {
            return (Task::none(), output);
        }
        let Some(modal) = self.postgres_role_modal.as_ref() else {
            return (Task::none(), output);
        };
        let statements = match self.postgres_role_modal_save_statements(modal) {
            Ok(statements) => statements,
            Err(error) => {
                output = Some(Output::Error(Some(ErrorModalState::new(
                    "Role editor",
                    "Could not generate role SQL.",
                    error,
                ))));
                return (Task::none(), output);
            }
        };

        if statements.is_empty() {
            output = Some(Output::Toast(
                ToastLevel::Info,
                "No role changes to save.".into(),
            ));
            return (Task::none(), output);
        }

        let Some(pool) = context.pool.cloned() else {
            output = Some(Output::Error(Some(ErrorModalState::new(
                "Role editor",
                "Not connected.",
                "Connect to a PostgreSQL database and try again.",
            ))));
            return (Task::none(), output);
        };
        let Some(database) = context.database else {
            output = Some(Output::Error(Some(ErrorModalState::new(
                "Role editor",
                "No database selected.",
                "Select a database first.",
            ))));
            return (Task::none(), output);
        };

        let mut preview = modal.sql_editor.text();
        if preview.trim().is_empty() {
            preview = statements.join("\n\n");
        }
        output = Some(Output::RoleSqlStarted(preview));
        self.postgres_role_action_running = true;

        (
            Task::perform(
                execute_sql_statements(pool, Some(database), statements),
                Message::PostgresRoleModalSaved,
            ),
            output,
        )
    }
    pub(crate) fn postgres_role_modal_saved(
        &mut self,
        result: Result<(), String>,
        _context: Context<'_>,
    ) -> (Task<Message>, Option<Output>) {
        self.postgres_role_action_running = false;

        match result {
            Ok(()) => {
                let (mode, role_name) = self
                    .postgres_role_modal
                    .as_ref()
                    .map(|modal| (modal.mode, modal.draft.role_name.clone()))
                    .unwrap_or((PostgresRoleModalMode::Properties, String::new()));
                self.postgres_role_modal = None;
                self.selected_postgres_object = None;
                let message = if mode == PostgresRoleModalMode::Create {
                    crate::i18n::tr_with("Role `{role}` created.", &[("{role}", &role_name)])
                } else {
                    crate::i18n::tr_with("Role `{role}` updated.", &[("{role}", &role_name)])
                };
                (Task::none(), Some(Output::RoleChanged(message)))
            }
            Err(error) => {
                let output = Some(Output::Error(Some(ErrorModalState::new(
                    "Role editor",
                    "Could not save role changes.",
                    error,
                ))));
                (Task::none(), output)
            }
        }
    }
    pub(crate) fn close_postgres_role_modal(
        &mut self,
        _context: Context<'_>,
    ) -> (Task<Message>, Option<Output>) {
        let output = None;

        self.postgres_role_modal = None;
        (Task::none(), output)
    }
}

async fn execute_sql_statements(
    pool: DatabasePool,
    database: Option<String>,
    statements: Vec<String>,
) -> Result<(), String> {
    for statement in statements {
        if statement.trim().is_empty() {
            continue;
        }
        let _ = run_query(pool.clone(), database.clone(), statement).await?;
    }
    Ok(())
}
