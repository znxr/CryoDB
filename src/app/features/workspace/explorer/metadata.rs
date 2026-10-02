use super::Message;
use super::State;
use crate::db::{
    DatabasePool,
    metadata::{fetch_table_info, fetch_table_relations, fetch_table_triggers},
};
use crate::model::table::{RelationInfo, SidebarRelationEntry, SidebarTriggerEntry, TriggerInfo};
use iced::Task;

impl State {
    fn sort_sidebar_trigger_entries(entries: &mut [SidebarTriggerEntry]) {
        entries.sort_by(|left, right| {
            left.table
                .to_ascii_lowercase()
                .cmp(&right.table.to_ascii_lowercase())
                .then_with(|| {
                    left.trigger
                        .name
                        .to_ascii_lowercase()
                        .cmp(&right.trigger.name.to_ascii_lowercase())
                })
                .then_with(|| left.table.cmp(&right.table))
                .then_with(|| left.trigger.name.cmp(&right.trigger.name))
        });
    }
    fn sort_sidebar_relation_entries(entries: &mut [SidebarRelationEntry]) {
        entries.sort_by(|left, right| {
            left.table
                .to_ascii_lowercase()
                .cmp(&right.table.to_ascii_lowercase())
                .then_with(|| {
                    left.relation
                        .column
                        .to_ascii_lowercase()
                        .cmp(&right.relation.column.to_ascii_lowercase())
                })
                .then_with(|| {
                    left.relation
                        .referenced_table
                        .to_ascii_lowercase()
                        .cmp(&right.relation.referenced_table.to_ascii_lowercase())
                })
                .then_with(|| {
                    left.relation
                        .referenced_column
                        .to_ascii_lowercase()
                        .cmp(&right.relation.referenced_column.to_ascii_lowercase())
                })
                .then_with(|| left.table.cmp(&right.table))
                .then_with(|| left.relation.column.cmp(&right.relation.column))
                .then_with(|| {
                    left.relation
                        .referenced_table
                        .cmp(&right.relation.referenced_table)
                })
                .then_with(|| {
                    left.relation
                        .referenced_column
                        .cmp(&right.relation.referenced_column)
                })
        });
    }
    pub(crate) fn set_sidebar_trigger_entries(
        &mut self,
        selected_table: Option<&str>,
        mut entries: Vec<SidebarTriggerEntry>,
    ) {
        Self::sort_sidebar_trigger_entries(&mut entries);
        self.sidebar_triggers = entries.clone();

        self.table_triggers_cache.clear();
        for entry in entries {
            self.table_triggers_cache
                .entry(entry.table)
                .or_default()
                .push(entry.trigger);
        }

        if let Some(selected_table) = selected_table
            && self.triggers_table.as_deref() == Some(selected_table)
        {
            self.table_triggers = self
                .table_triggers_cache
                .get(selected_table)
                .cloned()
                .unwrap_or_default();
        }
    }
    pub(crate) fn set_sidebar_relation_entries(
        &mut self,
        selected_table: Option<&str>,
        mut entries: Vec<SidebarRelationEntry>,
    ) {
        Self::sort_sidebar_relation_entries(&mut entries);
        self.sidebar_relations = entries.clone();

        self.table_relations_cache.clear();
        for entry in entries {
            self.table_relations_cache
                .entry(entry.table)
                .or_default()
                .push(entry.relation);
        }

        if let Some(selected_table) = selected_table
            && self.relations_table.as_deref() == Some(selected_table)
        {
            self.table_relations = self
                .table_relations_cache
                .get(selected_table)
                .cloned()
                .unwrap_or_default();
        }
    }
    pub(crate) fn update_sidebar_triggers_for_table(
        &mut self,
        table: &str,
        triggers: Vec<TriggerInfo>,
    ) {
        self.sidebar_triggers.retain(|entry| entry.table != table);
        self.sidebar_triggers
            .extend(triggers.into_iter().map(|trigger| SidebarTriggerEntry {
                table: table.to_string(),
                trigger,
            }));
        Self::sort_sidebar_trigger_entries(&mut self.sidebar_triggers);
    }
    pub(crate) fn update_sidebar_relations_for_table(
        &mut self,
        table: &str,
        relations: Vec<RelationInfo>,
    ) {
        self.sidebar_relations.retain(|entry| entry.table != table);
        self.sidebar_relations
            .extend(relations.into_iter().map(|relation| SidebarRelationEntry {
                table: table.to_string(),
                relation,
                constraint_name: String::new(),
            }));
        Self::sort_sidebar_relation_entries(&mut self.sidebar_relations);
    }
}

impl State {
    pub(crate) fn load_triggers_for_table(
        &mut self,
        table: String,
        pool: Option<DatabasePool>,
        database: Option<String>,
    ) -> Task<Message> {
        self.selected_trigger = None;

        if let Some(cached) = self.table_triggers_cache.get(&table).cloned() {
            self.table_triggers = cached;
            self.triggers_table = Some(table);
            return Task::none();
        }

        self.table_triggers.clear();
        self.triggers_table = Some(table.clone());
        let Some(pool) = pool else {
            return Task::none();
        };
        let database = database.unwrap_or_default();
        if database.is_empty() {
            return Task::none();
        }

        Task::perform(
            fetch_table_triggers(pool, database, table.clone()),
            move |result| Message::TableTriggersLoaded { table, result },
        )
    }
    pub(crate) fn load_relations_for_table(
        &mut self,
        table: String,
        pool: Option<DatabasePool>,
        database: Option<String>,
    ) -> Task<Message> {
        if let Some(cached) = self.table_relations_cache.get(&table).cloned() {
            self.table_relations = cached;
            self.relations_table = Some(table);
            return Task::none();
        }

        self.table_relations.clear();
        self.relations_table = Some(table.clone());
        let Some(pool) = pool else {
            return Task::none();
        };
        let database = database.unwrap_or_default();
        if database.is_empty() {
            return Task::none();
        }

        Task::perform(
            fetch_table_relations(pool, database, table.clone()),
            move |result| Message::TableRelationsLoaded { table, result },
        )
    }
}

impl State {
    pub(crate) fn request_table_info(
        &mut self,
        table: String,
        pool: Option<DatabasePool>,
        database: Option<String>,
    ) -> Task<Message> {
        self.table_info_sidebar_open = true;
        self.table_info_table = Some(table.clone());
        self.table_info_error = None;

        if let Some(cached) = self.get_table_info_cache_entry(&table) {
            self.table_info = Some(cached);
            self.table_info_loading = false;
            return Task::none();
        }

        let Some(pool) = pool else {
            self.table_info_loading = false;
            self.table_info = None;
            self.table_info_error = Some(String::from("Not connected."));
            return Task::none();
        };
        let Some(database) = database else {
            self.table_info_loading = false;
            self.table_info = None;
            self.table_info_error = Some(String::from("Select a database first."));
            return Task::none();
        };

        self.table_info_loading = true;
        self.table_info = None;
        Task::perform(
            fetch_table_info(pool, database, table.clone()),
            move |result| Message::TableInfoLoaded { table, result },
        )
    }
}

impl State {
    pub(crate) fn open_table_ddl(
        &mut self,
        table: String,
        pool: DatabasePool,
        database: String,
    ) -> Task<Message> {
        self.table_ddl = Some(super::TableDdlState::loading(table.clone()));
        Task::perform(
            crate::db::metadata::fetch_table_ddl(pool, database, table.clone()),
            move |result| Message::TableDdlLoaded { table, result },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::features::workspace::explorer::{Context, Message};
    use crate::model::{connection::DatabaseDriver, table::FolderStore};

    #[test]
    fn relation_refresh_preserves_other_table_details_and_ignores_disconnected_results() {
        let mut state = State::new(FolderStore::default(), None);
        let relation = RelationInfo {
            column: "owner_id".into(),
            referenced_table: "users".into(),
            referenced_column: "id".into(),
        };
        state.relations_table = Some("projects".into());
        state.table_relations = vec![relation.clone()];
        let settings = crate::model::settings::Settings::default();
        let context = |is_connected| Context {
            settings: &settings,
            workspace_busy: false,
            query_running: false,
            menu_fallback: iced::Point::ORIGIN,
            theme: iced::Theme::Dark,
            folder_keys: None,
            driver: DatabaseDriver::Sqlite,
            pool: None,
            database: None,
            is_connected,
            selected_table: Some("tasks"),
            query: String::new(),
        };
        let _ = state.update(
            Message::SidebarRelationsLoaded(Ok(vec![SidebarRelationEntry {
                table: "tasks".into(),
                relation,
                constraint_name: "tasks_owner".into(),
            }])),
            context(true),
        );
        assert_eq!(state.table_relations.len(), 1);
        assert_eq!(state.relations_table.as_deref(), Some("projects"));
        assert_eq!(state.table_relations_cache["tasks"].len(), 1);
        let _ = state.update(
            Message::SidebarRelationsLoaded(Err("offline".into())),
            context(false),
        );
        assert_eq!(state.sidebar_relations.len(), 1);
        assert_eq!(state.table_relations_cache["tasks"].len(), 1);
        state.relations_table = Some("tasks".into());
        let _ = state.update(
            Message::SidebarRelationsLoaded(Ok(Vec::new())),
            context(true),
        );
        assert!(state.table_relations.is_empty());
    }
}
