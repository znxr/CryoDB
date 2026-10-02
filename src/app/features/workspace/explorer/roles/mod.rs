mod sql;
mod update;
pub(crate) mod view;
use super::State;
use crate::model::table::{
    PostgresRoleForm, PostgresRoleModalMode, PostgresRoleModalTab, PostgresRoleTextField,
    PostgresRoleToggleField,
};

#[derive(Debug, Clone)]
pub(crate) struct PostgresRoleModalState {
    pub(crate) mode: PostgresRoleModalMode,
    pub(crate) source_role_name: Option<String>,
    pub(crate) active_tab: PostgresRoleModalTab,
    pub(crate) original: PostgresRoleForm,
    pub(crate) draft: PostgresRoleForm,
    pub(crate) sql_editor: iced::widget::text_editor::Content,
    pub(crate) sql_dirty: bool,
}

impl State {
    pub(crate) fn open_postgres_role_create_modal(&mut self) {
        let form = PostgresRoleForm::default();
        let mut modal = PostgresRoleModalState {
            mode: PostgresRoleModalMode::Create,
            source_role_name: None,
            active_tab: PostgresRoleModalTab::General,
            original: form.clone(),
            draft: form,
            sql_editor: iced::widget::text_editor::Content::with_text(""),
            sql_dirty: false,
        };
        let preview = self.postgres_role_modal_sql_preview(&modal);
        modal.sql_editor = iced::widget::text_editor::Content::with_text(&preview);
        self.postgres_role_modal = Some(modal);
    }
    pub(crate) fn open_postgres_role_properties_modal(
        &mut self,
        source_role_name: String,
        mut form: PostgresRoleForm,
    ) {
        form.password.clear();
        let mut modal = PostgresRoleModalState {
            mode: PostgresRoleModalMode::Properties,
            source_role_name: Some(source_role_name),
            active_tab: PostgresRoleModalTab::General,
            original: form.clone(),
            draft: form,
            sql_editor: iced::widget::text_editor::Content::with_text(""),
            sql_dirty: false,
        };
        let preview = self.postgres_role_modal_sql_preview(&modal);
        modal.sql_editor = iced::widget::text_editor::Content::with_text(&preview);
        self.postgres_role_modal = Some(modal);
    }
    pub(crate) fn update_postgres_role_modal_text(
        &mut self,
        field: PostgresRoleTextField,
        value: String,
    ) {
        let Some(modal) = self.postgres_role_modal.as_mut() else {
            return;
        };
        let draft = &mut modal.draft;
        match field {
            PostgresRoleTextField::RoleName => draft.role_name = value,
            PostgresRoleTextField::ConnectionLimit => draft.connection_limit = value,
            PostgresRoleTextField::Password => draft.password = value,
            PostgresRoleTextField::ValidUntil => draft.valid_until = value,
            PostgresRoleTextField::MemberOfRoles => draft.member_of_roles = value,
            PostgresRoleTextField::Members => draft.members = value,
            PostgresRoleTextField::SearchPath => draft.search_path = value,
            PostgresRoleTextField::WorkMem => draft.work_mem = value,
            PostgresRoleTextField::MaintenanceWorkMem => draft.maintenance_work_mem = value,
            PostgresRoleTextField::StatementTimeout => draft.statement_timeout = value,
            PostgresRoleTextField::LockTimeout => draft.lock_timeout = value,
            PostgresRoleTextField::IdleInTransactionSessionTimeout => {
                draft.idle_in_transaction_session_timeout = value
            }
            PostgresRoleTextField::Comment => draft.comment = value,
        }
        self.sync_postgres_role_modal_sql_if_clean();
    }
    pub(crate) fn update_postgres_role_modal_toggle(
        &mut self,
        field: PostgresRoleToggleField,
        enabled: bool,
    ) {
        let Some(modal) = self.postgres_role_modal.as_mut() else {
            return;
        };
        let draft = &mut modal.draft;
        match field {
            PostgresRoleToggleField::CanLogin => draft.can_login = enabled,
            PostgresRoleToggleField::IsSuperuser => draft.is_superuser = enabled,
            PostgresRoleToggleField::InheritPrivileges => draft.inherit_privileges = enabled,
            PostgresRoleToggleField::CanCreateDb => draft.can_create_db = enabled,
            PostgresRoleToggleField::CanCreateRole => draft.can_create_role = enabled,
            PostgresRoleToggleField::CanReplicate => draft.can_replicate = enabled,
            PostgresRoleToggleField::BypassRls => draft.bypass_rls = enabled,
        }
        self.sync_postgres_role_modal_sql_if_clean();
    }
    pub(crate) fn reset_postgres_role_modal(&mut self) {
        let Some(modal) = self.postgres_role_modal.as_mut() else {
            return;
        };
        modal.draft = modal.original.clone();
        modal.sql_dirty = false;
        self.sync_postgres_role_modal_sql_if_clean();
    }
    pub(crate) fn sync_postgres_role_modal_sql_if_clean(&mut self) {
        let Some(modal) = self.postgres_role_modal.as_ref() else {
            return;
        };
        if modal.sql_dirty {
            return;
        }
        let preview = self.postgres_role_modal_sql_preview(modal);
        if let Some(modal) = self.postgres_role_modal.as_mut() {
            modal.sql_editor = iced::widget::text_editor::Content::with_text(&preview);
            modal.sql_dirty = false;
        }
    }
    pub(crate) fn apply_postgres_role_modal_sql_action(
        &mut self,
        action: iced::widget::text_editor::Action,
    ) {
        let Some(modal) = self.postgres_role_modal.as_mut() else {
            return;
        };
        modal.sql_editor.perform(action);
        modal.sql_dirty = true;
    }
    pub(crate) fn set_postgres_role_modal_sql_text(&mut self, sql: &str, mark_dirty: bool) {
        let Some(modal) = self.postgres_role_modal.as_mut() else {
            return;
        };
        modal.sql_editor = iced::widget::text_editor::Content::with_text(sql);
        modal.sql_dirty = mark_dirty;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::table::FolderStore;

    #[test]
    fn manual_role_sql_survives_form_edits_until_reset() {
        let mut state = State::new(FolderStore::default(), None);
        state.open_postgres_role_properties_modal(
            "reader".into(),
            PostgresRoleForm::with_role_name("reader"),
        );
        state.set_postgres_role_modal_sql_text("ALTER ROLE reader LOGIN;", true);
        state.update_postgres_role_modal_text(PostgresRoleTextField::Comment, "Read access".into());
        let modal = state.postgres_role_modal.as_ref().unwrap();
        assert!(modal.sql_dirty);
        assert_eq!(
            state.postgres_role_modal_save_statements(modal).unwrap(),
            vec!["ALTER ROLE reader LOGIN"]
        );
        state.reset_postgres_role_modal();
        let modal = state.postgres_role_modal.as_ref().unwrap();
        assert!(!modal.sql_dirty);
        assert!(modal.draft.comment.is_empty());
        assert!(!modal.sql_editor.text().contains("ALTER ROLE reader LOGIN;"));
        assert_eq!(
            modal.sql_editor.text(),
            state.postgres_role_modal_sql_preview(modal)
        );
    }
}
