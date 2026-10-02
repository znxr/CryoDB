use super::PostgresRoleModalState;
use super::State;
use crate::model::connection::DatabaseDriver;
use crate::model::table::{PostgresRoleForm, PostgresRoleModalMode, PostgresRoleModalTab};
use crate::utils::helpers::{sql_escape_string_literal, sql_quote_identifier};
use std::collections::HashSet;

impl State {
    fn split_sql_script_statements(script: &str) -> Vec<String> {
        let filtered = script
            .lines()
            .filter(|line| !line.trim_start().starts_with("--"))
            .collect::<Vec<_>>()
            .join("\n");

        let mut statements = Vec::new();
        let mut current = String::new();
        let mut in_single_quote = false;
        let mut in_double_quote = false;
        let mut chars = filtered.chars().peekable();

        while let Some(ch) = chars.next() {
            if ch == '\'' && !in_double_quote {
                current.push(ch);
                if in_single_quote {
                    if chars.peek().is_some_and(|next| *next == '\'') {
                        current.push(chars.next().unwrap_or('\''));
                    } else {
                        in_single_quote = false;
                    }
                } else {
                    in_single_quote = true;
                }
                continue;
            }

            if ch == '"' && !in_single_quote {
                current.push(ch);
                if in_double_quote {
                    if chars.peek().is_some_and(|next| *next == '"') {
                        current.push(chars.next().unwrap_or('"'));
                    } else {
                        in_double_quote = false;
                    }
                } else {
                    in_double_quote = true;
                }
                continue;
            }

            if ch == ';' && !in_single_quote && !in_double_quote {
                let trimmed = current.trim();
                if !trimmed.is_empty() {
                    statements.push(trimmed.to_string());
                }
                current.clear();
            } else {
                current.push(ch);
            }
        }

        let trailing = current.trim();
        if !trailing.is_empty() {
            statements.push(trailing.to_string());
        }

        statements
    }
    pub(crate) fn postgres_role_modal_save_statements(
        &self,
        modal: &PostgresRoleModalState,
    ) -> Result<Vec<String>, String> {
        let scripted = Self::split_sql_script_statements(&modal.sql_editor.text());
        if !scripted.is_empty() {
            return Ok(scripted);
        }
        self.build_postgres_role_save_statements(modal)
    }
    pub(crate) fn postgres_role_modal_ai_prompt(&self, modal: &PostgresRoleModalState) -> String {
        let draft = &modal.draft;
        let mode = match modal.mode {
            PostgresRoleModalMode::Create => "create",
            PostgresRoleModalMode::Properties => "update",
        };
        format!(
            "Generate PostgreSQL SQL statements for a role {mode} workflow. Return only SQL.\n\nRole fields:\n- role_name: {}\n- can_login: {}\n- is_superuser: {}\n- inherit_privileges: {}\n- can_create_db: {}\n- can_create_role: {}\n- can_replicate: {}\n- bypass_rls: {}\n- connection_limit: {}\n- valid_until: {}\n- member_of_roles: {}\n- members: {}\n- search_path: {}\n- work_mem: {}\n- maintenance_work_mem: {}\n- statement_timeout: {}\n- lock_timeout: {}\n- idle_in_transaction_session_timeout: {}\n- comment: {}\n\nRespect PostgreSQL role syntax and return executable statements only.",
            draft.role_name.trim(),
            draft.can_login,
            draft.is_superuser,
            draft.inherit_privileges,
            draft.can_create_db,
            draft.can_create_role,
            draft.can_replicate,
            draft.bypass_rls,
            draft.connection_limit.trim(),
            draft.valid_until.trim(),
            draft.member_of_roles.trim(),
            draft.members.trim(),
            draft.search_path.trim(),
            draft.work_mem.trim(),
            draft.maintenance_work_mem.trim(),
            draft.statement_timeout.trim(),
            draft.lock_timeout.trim(),
            draft.idle_in_transaction_session_timeout.trim(),
            draft.comment.trim()
        )
    }
    fn parse_postgres_role_list(raw: &str) -> Vec<String> {
        let mut seen = HashSet::new();
        let mut values = Vec::new();

        for token in raw.split([',', '\n', ';']) {
            let trimmed = token.trim().trim_matches('"').trim_matches('`');
            if trimmed.is_empty() {
                continue;
            }
            let key = trimmed.to_ascii_lowercase();
            if seen.insert(key) {
                values.push(trimmed.to_string());
            }
        }

        values
    }
    fn postgres_role_list_difference(left: &[String], right: &[String]) -> Vec<String> {
        let right_lookup = right
            .iter()
            .map(|value| value.to_ascii_lowercase())
            .collect::<HashSet<_>>();

        left.iter()
            .filter(|value| !right_lookup.contains(&value.to_ascii_lowercase()))
            .cloned()
            .collect()
    }
    fn postgres_role_parameters(form: &PostgresRoleForm) -> [(&'static str, &str); 6] {
        [
            ("search_path", form.search_path.as_str()),
            ("work_mem", form.work_mem.as_str()),
            ("maintenance_work_mem", form.maintenance_work_mem.as_str()),
            ("statement_timeout", form.statement_timeout.as_str()),
            ("lock_timeout", form.lock_timeout.as_str()),
            (
                "idle_in_transaction_session_timeout",
                form.idle_in_transaction_session_timeout.as_str(),
            ),
        ]
    }
    pub(crate) fn build_postgres_role_save_statements(
        &self,
        modal: &PostgresRoleModalState,
    ) -> Result<Vec<String>, String> {
        let draft = &modal.draft;
        let role_name = draft.role_name.trim();
        if role_name.is_empty() {
            return Err(String::from("Role name is required."));
        }

        let connection_limit = draft.connection_limit.trim().parse::<i32>().map_err(|_| {
            String::from("Connection limit must be a whole number. Use -1 for unlimited.")
        })?;

        let quoted_role = sql_quote_identifier(DatabaseDriver::PostgreSql, role_name);
        let mut statements = Vec::new();

        match modal.mode {
            PostgresRoleModalMode::Create => {
                let mut with_options = vec![
                    if draft.can_login {
                        String::from("LOGIN")
                    } else {
                        String::from("NOLOGIN")
                    },
                    if draft.is_superuser {
                        String::from("SUPERUSER")
                    } else {
                        String::from("NOSUPERUSER")
                    },
                    if draft.inherit_privileges {
                        String::from("INHERIT")
                    } else {
                        String::from("NOINHERIT")
                    },
                    if draft.can_create_db {
                        String::from("CREATEDB")
                    } else {
                        String::from("NOCREATEDB")
                    },
                    if draft.can_create_role {
                        String::from("CREATEROLE")
                    } else {
                        String::from("NOCREATEROLE")
                    },
                    if draft.can_replicate {
                        String::from("REPLICATION")
                    } else {
                        String::from("NOREPLICATION")
                    },
                    if draft.bypass_rls {
                        String::from("BYPASSRLS")
                    } else {
                        String::from("NOBYPASSRLS")
                    },
                ];

                if connection_limit != -1 {
                    with_options.push(format!("CONNECTION LIMIT {connection_limit}"));
                }

                let password = draft.password.trim();
                if !password.is_empty() {
                    with_options.push(format!(
                        "PASSWORD '{}'",
                        sql_escape_string_literal(DatabaseDriver::PostgreSql, password)
                    ));
                }

                let valid_until = draft.valid_until.trim();
                if !valid_until.is_empty() {
                    with_options.push(format!(
                        "VALID UNTIL '{}'",
                        sql_escape_string_literal(DatabaseDriver::PostgreSql, valid_until)
                    ));
                }

                statements.push(format!(
                    "CREATE ROLE {quoted_role} WITH\n  {};",
                    with_options.join("\n  ")
                ));

                let member_of_roles = Self::parse_postgres_role_list(&draft.member_of_roles);
                if !member_of_roles.is_empty() {
                    let parent_roles = member_of_roles
                        .iter()
                        .map(|name| sql_quote_identifier(DatabaseDriver::PostgreSql, name))
                        .collect::<Vec<_>>()
                        .join(", ");
                    statements.push(format!("GRANT {parent_roles} TO {quoted_role};"));
                }

                let members = Self::parse_postgres_role_list(&draft.members);
                if !members.is_empty() {
                    let child_roles = members
                        .iter()
                        .map(|name| sql_quote_identifier(DatabaseDriver::PostgreSql, name))
                        .collect::<Vec<_>>()
                        .join(", ");
                    statements.push(format!("GRANT {quoted_role} TO {child_roles};"));
                }

                for (parameter, value) in Self::postgres_role_parameters(draft) {
                    let value = value.trim();
                    if value.is_empty() {
                        continue;
                    }
                    statements.push(format!(
                        "ALTER ROLE {quoted_role} SET {parameter} = '{}';",
                        sql_escape_string_literal(DatabaseDriver::PostgreSql, value)
                    ));
                }

                let comment = draft.comment.trim();
                if !comment.is_empty() {
                    statements.push(format!(
                        "COMMENT ON ROLE {quoted_role} IS '{}';",
                        sql_escape_string_literal(DatabaseDriver::PostgreSql, comment)
                    ));
                }
            }
            PostgresRoleModalMode::Properties => {
                let source_role_name = modal
                    .source_role_name
                    .as_deref()
                    .unwrap_or(modal.original.role_name.as_str())
                    .trim();
                if source_role_name.is_empty() {
                    return Err(String::from("Role source is not available."));
                }

                let source_quoted =
                    sql_quote_identifier(DatabaseDriver::PostgreSql, source_role_name);
                let mut active_role = source_quoted.clone();

                if !source_role_name.eq_ignore_ascii_case(role_name) {
                    statements.push(format!(
                        "ALTER ROLE {source_quoted} RENAME TO {quoted_role};"
                    ));
                    active_role = quoted_role.clone();
                }

                let mut with_options = vec![
                    if draft.can_login {
                        String::from("LOGIN")
                    } else {
                        String::from("NOLOGIN")
                    },
                    if draft.is_superuser {
                        String::from("SUPERUSER")
                    } else {
                        String::from("NOSUPERUSER")
                    },
                    if draft.inherit_privileges {
                        String::from("INHERIT")
                    } else {
                        String::from("NOINHERIT")
                    },
                    if draft.can_create_db {
                        String::from("CREATEDB")
                    } else {
                        String::from("NOCREATEDB")
                    },
                    if draft.can_create_role {
                        String::from("CREATEROLE")
                    } else {
                        String::from("NOCREATEROLE")
                    },
                    if draft.can_replicate {
                        String::from("REPLICATION")
                    } else {
                        String::from("NOREPLICATION")
                    },
                    if draft.bypass_rls {
                        String::from("BYPASSRLS")
                    } else {
                        String::from("NOBYPASSRLS")
                    },
                    format!("CONNECTION LIMIT {connection_limit}"),
                ];

                let password = draft.password.trim();
                if !password.is_empty() {
                    with_options.push(format!(
                        "PASSWORD '{}'",
                        sql_escape_string_literal(DatabaseDriver::PostgreSql, password)
                    ));
                }

                let original_valid_until = modal.original.valid_until.trim();
                let draft_valid_until = draft.valid_until.trim();
                if !draft_valid_until.is_empty() {
                    with_options.push(format!(
                        "VALID UNTIL '{}'",
                        sql_escape_string_literal(DatabaseDriver::PostgreSql, draft_valid_until)
                    ));
                } else if !original_valid_until.is_empty() {
                    with_options.push(String::from("VALID UNTIL 'infinity'"));
                }

                statements.push(format!(
                    "ALTER ROLE {active_role} WITH\n  {};",
                    with_options.join("\n  ")
                ));

                let original_member_of =
                    Self::parse_postgres_role_list(&modal.original.member_of_roles);
                let draft_member_of = Self::parse_postgres_role_list(&draft.member_of_roles);

                let revoked_member_of =
                    Self::postgres_role_list_difference(&original_member_of, &draft_member_of);
                if !revoked_member_of.is_empty() {
                    let roles = revoked_member_of
                        .iter()
                        .map(|name| sql_quote_identifier(DatabaseDriver::PostgreSql, name))
                        .collect::<Vec<_>>()
                        .join(", ");
                    statements.push(format!("REVOKE {roles} FROM {active_role};"));
                }

                let granted_member_of =
                    Self::postgres_role_list_difference(&draft_member_of, &original_member_of);
                if !granted_member_of.is_empty() {
                    let roles = granted_member_of
                        .iter()
                        .map(|name| sql_quote_identifier(DatabaseDriver::PostgreSql, name))
                        .collect::<Vec<_>>()
                        .join(", ");
                    statements.push(format!("GRANT {roles} TO {active_role};"));
                }

                let original_members = Self::parse_postgres_role_list(&modal.original.members);
                let draft_members = Self::parse_postgres_role_list(&draft.members);

                let revoked_members =
                    Self::postgres_role_list_difference(&original_members, &draft_members);
                if !revoked_members.is_empty() {
                    let roles = revoked_members
                        .iter()
                        .map(|name| sql_quote_identifier(DatabaseDriver::PostgreSql, name))
                        .collect::<Vec<_>>()
                        .join(", ");
                    statements.push(format!("REVOKE {active_role} FROM {roles};"));
                }

                let granted_members =
                    Self::postgres_role_list_difference(&draft_members, &original_members);
                if !granted_members.is_empty() {
                    let roles = granted_members
                        .iter()
                        .map(|name| sql_quote_identifier(DatabaseDriver::PostgreSql, name))
                        .collect::<Vec<_>>()
                        .join(", ");
                    statements.push(format!("GRANT {active_role} TO {roles};"));
                }

                for (parameter, draft_value_raw) in Self::postgres_role_parameters(draft) {
                    let draft_value = draft_value_raw.trim();
                    let original_value = Self::postgres_role_parameters(&modal.original)
                        .iter()
                        .find(|(name, _)| *name == parameter)
                        .map(|(_, value)| value.trim())
                        .unwrap_or_default();

                    if draft_value == original_value {
                        continue;
                    }

                    if draft_value.is_empty() {
                        if !original_value.is_empty() {
                            statements.push(format!("ALTER ROLE {active_role} RESET {parameter};"));
                        }
                    } else {
                        statements.push(format!(
                            "ALTER ROLE {active_role} SET {parameter} = '{}';",
                            sql_escape_string_literal(DatabaseDriver::PostgreSql, draft_value)
                        ));
                    }
                }

                let original_comment = modal.original.comment.trim();
                let draft_comment = draft.comment.trim();
                if draft_comment != original_comment {
                    if draft_comment.is_empty() {
                        statements.push(format!("COMMENT ON ROLE {active_role} IS NULL;"));
                    } else {
                        statements.push(format!(
                            "COMMENT ON ROLE {active_role} IS '{}';",
                            sql_escape_string_literal(DatabaseDriver::PostgreSql, draft_comment)
                        ));
                    }
                }
            }
        }

        Ok(statements)
    }
    pub(crate) fn postgres_role_modal_sql_preview(&self, modal: &PostgresRoleModalState) -> String {
        match self.build_postgres_role_save_statements(modal) {
            Ok(statements) => {
                if statements.is_empty() {
                    String::from("-- No SQL changes to apply.")
                } else {
                    statements.join("\n\n")
                }
            }
            Err(error) => format!("-- {error}"),
        }
    }
    pub(crate) fn postgres_role_create_script(
        &self,
        form: &PostgresRoleForm,
    ) -> Result<String, String> {
        let role_name = form.role_name.trim();
        if role_name.is_empty() {
            return Err(String::from("Role name is required."));
        }

        let modal = PostgresRoleModalState {
            mode: PostgresRoleModalMode::Create,
            source_role_name: None,
            active_tab: PostgresRoleModalTab::Sql,
            original: PostgresRoleForm::with_role_name(role_name.to_string()),
            draft: form.clone(),
            sql_editor: iced::widget::text_editor::Content::with_text(""),
            sql_dirty: false,
        };

        let statements = self.build_postgres_role_save_statements(&modal)?;
        let role_quoted = sql_quote_identifier(DatabaseDriver::PostgreSql, role_name);
        let mut script = format!(
            "-- Role: {role_name}\n-- DROP ROLE IF EXISTS {role_quoted};\n\n{}",
            statements.join("\n\n")
        );
        if !script.ends_with('\n') {
            script.push('\n');
        }
        Ok(script)
    }
}
