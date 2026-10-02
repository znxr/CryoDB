use super::State;
use crate::model::table::PostgresObjectKind;

impl State {
    fn postgres_schema_key(schema: &str) -> String {
        schema.trim().to_ascii_lowercase()
    }
    fn postgres_schema_kind_key(schema: &str, kind: PostgresObjectKind) -> String {
        format!(
            "{}|{}",
            Self::postgres_schema_key(schema),
            kind.searchable_label()
        )
    }
    pub(crate) fn sync_postgres_schema_open_state(&mut self) {
        let mut known = std::collections::HashSet::new();
        let mut known_kinds = std::collections::HashSet::new();
        for object in &self.postgres_sidebar_objects {
            let key = Self::postgres_schema_key(&object.schema);
            if key.is_empty() {
                continue;
            }
            known.insert(key.clone());
            self.postgres_schema_open.entry(key).or_insert(true);

            let kind_key = Self::postgres_schema_kind_key(&object.schema, object.kind);
            known_kinds.insert(kind_key.clone());
            self.postgres_schema_kind_open
                .entry(kind_key)
                .or_insert(true);
        }

        self.postgres_schema_open
            .retain(|schema, _| known.contains(schema));
        self.postgres_schema_kind_open
            .retain(|key, _| known_kinds.contains(key));
    }
    pub(crate) fn is_postgres_schema_open(&self, schema: &str) -> bool {
        let key = Self::postgres_schema_key(schema);
        if key.is_empty() {
            return true;
        }
        self.postgres_schema_open.get(&key).copied().unwrap_or(true)
    }
    pub(crate) fn toggle_postgres_schema_open(&mut self, schema: &str) {
        let key = Self::postgres_schema_key(schema);
        if key.is_empty() {
            return;
        }
        let current = self.postgres_schema_open.get(&key).copied().unwrap_or(true);
        self.postgres_schema_open.insert(key, !current);
    }
    pub(crate) fn is_postgres_schema_kind_open(
        &self,
        schema: &str,
        kind: PostgresObjectKind,
    ) -> bool {
        let key = Self::postgres_schema_kind_key(schema, kind);
        if key.is_empty() {
            return true;
        }
        self.postgres_schema_kind_open
            .get(&key)
            .copied()
            .unwrap_or(true)
    }
    pub(crate) fn toggle_postgres_schema_kind_open(
        &mut self,
        schema: &str,
        kind: PostgresObjectKind,
    ) {
        let key = Self::postgres_schema_kind_key(schema, kind);
        if key.is_empty() {
            return;
        }
        let current = self
            .postgres_schema_kind_open
            .get(&key)
            .copied()
            .unwrap_or(true);
        self.postgres_schema_kind_open.insert(key, !current);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::features::workspace::explorer::{Context, Message};
    use crate::model::{
        connection::DatabaseDriver,
        settings::Settings,
        table::{FolderStore, PostgresSidebarObject},
    };

    #[test]
    fn schema_refresh_preserves_collapsed_groups_and_ignores_other_databases() {
        let mut state = State::new(FolderStore::default(), None);
        state.toggle_postgres_schema_open("sales");
        let settings = Settings::default();
        let context = || Context {
            settings: &settings,
            workspace_busy: false,
            query_running: false,
            menu_fallback: iced::Point::ORIGIN,
            theme: iced::Theme::Dark,
            folder_keys: None,
            driver: DatabaseDriver::PostgreSql,
            pool: None,
            database: Some("active".into()),
            is_connected: true,
            selected_table: None,
            query: String::new(),
        };
        let object = PostgresSidebarObject {
            schema: "sales".into(),
            name: "users".into(),
            kind: PostgresObjectKind::Table,
        };
        let _ = state.update(
            Message::PostgresSidebarObjectsLoaded {
                database: "active".into(),
                result: Ok(vec![object]),
            },
            context(),
        );
        assert!(!state.is_postgres_schema_open("sales"));
        let _ = state.update(
            Message::PostgresSidebarObjectsLoaded {
                database: "previous".into(),
                result: Err("late failure".into()),
            },
            context(),
        );
        assert_eq!(state.postgres_sidebar_objects.len(), 1);
        assert!(!state.is_postgres_schema_open("sales"));
        let _ = state.update(
            Message::PostgresSidebarObjectsLoaded {
                database: "active".into(),
                result: Ok(Vec::new()),
            },
            context(),
        );
        assert!(state.postgres_schema_open.is_empty());
        assert!(state.postgres_schema_kind_open.is_empty());
    }
}
