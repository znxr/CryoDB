use crate::app::core::App;
use crate::app::features::workspace::explorer::SidebarFilterKind;
use crate::app::features::workspace::tabs::TabEntry;
use crate::app::message::Message;
use crate::app::types::LayoutMode;
use crate::constants::{
    ADHOC_QUERY_MAX_ROWS, ICON_POSTGRES_EXTENSION, ICON_POSTGRES_FOREIGN_TABLE,
    ICON_POSTGRES_LOGIN_GROUP_ROLE, ICON_POSTGRES_MATERIALIZED_VIEW, ICON_POSTGRES_SEQUENCE,
    ICON_POSTGRES_TABLE, ICON_POSTGRES_VIEW, MYSQL_DEFAULT_PORT, SIDEBAR_LIST_RIGHT_PADDING,
    SIDEBAR_SCROLLBAR_GUTTER, SIDEBAR_SCROLLBAR_WIDTH, SIDEBAR_TRIGGER_INDENT,
};
use crate::db::query::run_query_with_control;
use crate::model::connection::DatabaseDriver;
use crate::model::table::{PostgresObjectKind, PostgresSidebarObject, TableCommand};
use crate::ui::ids::sidebar_tables_scroll_id;
use crate::utils::helpers::{
    canonical_table_key, sql_escape_string_literal, sql_quote_identifier_path,
};
use crate::utils::text::max_chars_for_width;
use iced::Task;
use iced::widget::scrollable;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

impl App {
    pub(crate) fn current_folder_connection_key(&self) -> Option<String> {
        if self.connections.current.driver == DatabaseDriver::Sqlite {
            let path = self.connections.current.sqlite_path.trim();
            if path.is_empty() {
                return None;
            }
            return Some(format!("sqlite|{path}"));
        }
        let host = self.connections.current.host.trim();
        let port = if self.connections.current.port.trim().is_empty() {
            MYSQL_DEFAULT_PORT
        } else {
            self.connections.current.port.trim()
        };
        let username = self.connections.current.username.trim();
        if host.is_empty() || username.is_empty() {
            return None;
        }
        if self.connections.current.driver == DatabaseDriver::MySql {
            Some(format!("{host}|{port}|{username}"))
        } else {
            Some(format!(
                "{}|{host}|{port}|{username}",
                self.connections.current.driver
            ))
        }
    }

    pub(crate) fn table_key(&self, table: &str) -> String {
        canonical_table_key(self.connections.current.driver, table)
    }

    pub(crate) fn folder_for_table(&self, table: &str) -> Option<String> {
        self.workspace
            .explorer
            .folder_for_table(self.connections.current.driver, table)
    }

    pub(crate) fn table_assigned_to_folder(&self, table: &str, folder: &str) -> bool {
        self.folder_for_table(table)
            .is_some_and(|assigned| assigned == folder)
    }

    pub(crate) fn table_has_folder_assignment(&self, table: &str) -> bool {
        self.workspace
            .explorer
            .table_has_folder_assignment(self.connections.current.driver, table)
    }

    pub(crate) fn sidebar_filter_kinds(&self) -> &'static [SidebarFilterKind] {
        crate::app::features::workspace::explorer::State::sidebar_filter_kinds_for_driver(
            self.connections.current.driver,
        )
    }

    pub(crate) fn sidebar_object_filter_active(&self) -> bool {
        crate::app::features::workspace::explorer::State::sidebar_object_filter_kinds_for_driver(
            self.connections.current.driver,
        )
        .iter()
        .copied()
        .any(|filter| !self.workspace.explorer.sidebar_filter_enabled(filter))
    }

    pub(crate) fn sidebar_filter_active(&self) -> bool {
        self.sidebar_filter_kinds()
            .iter()
            .copied()
            .any(|filter| !self.workspace.explorer.sidebar_filter_enabled(filter))
    }

    pub(crate) fn sidebar_table_visible(&self, table: &str) -> bool {
        if !self.settings.values.show_hidden_tables {
            let lower = table.to_ascii_lowercase();
            if lower.starts_with("pg_")
                || lower.starts_with("sql_")
                || lower == "information_schema"
            {
                return false;
            }
        }
        if self.connections.current.driver != DatabaseDriver::PostgreSql {
            return self.workspace.explorer.sidebar_filter_tables;
        }

        if let Some(kind) = self.postgres_object_kind_for_table(table) {
            self.workspace.explorer.sidebar_postgres_kind_visible(kind)
        } else {
            self.workspace.explorer.sidebar_filter_tables
        }
    }

    pub(crate) fn postgres_object_kind_for_table(&self, table: &str) -> Option<PostgresObjectKind> {
        self.workspace
            .explorer
            .postgres_object_kind_for_table(self.connections.current.driver, table)
    }

    pub(crate) fn table_is_row_editable(&self, table: &str) -> bool {
        self.workspace
            .explorer
            .table_is_row_editable(self.connections.current.driver, table)
    }

    pub(crate) fn postgres_object_is_selected(&self, object: &PostgresSidebarObject) -> bool {
        self.workspace
            .explorer
            .selected_postgres_object
            .as_ref()
            .is_some_and(|selected| {
                selected.kind == object.kind
                    && selected.schema.eq_ignore_ascii_case(&object.schema)
                    && selected.name.eq_ignore_ascii_case(&object.name)
            })
    }

    pub(crate) fn postgres_object_icon(kind: PostgresObjectKind) -> char {
        match kind {
            PostgresObjectKind::Table => ICON_POSTGRES_TABLE,
            PostgresObjectKind::View => ICON_POSTGRES_VIEW,
            PostgresObjectKind::MaterializedView => ICON_POSTGRES_MATERIALIZED_VIEW,
            PostgresObjectKind::ForeignTable => ICON_POSTGRES_FOREIGN_TABLE,
            PostgresObjectKind::Sequence => ICON_POSTGRES_SEQUENCE,
            PostgresObjectKind::Extension => ICON_POSTGRES_EXTENSION,
            PostgresObjectKind::Role => ICON_POSTGRES_LOGIN_GROUP_ROLE,
        }
    }

    pub(crate) fn postgres_object_label(kind: PostgresObjectKind) -> &'static str {
        match kind {
            PostgresObjectKind::Table => "Table",
            PostgresObjectKind::View => "View",
            PostgresObjectKind::MaterializedView => "Materialized View",
            PostgresObjectKind::ForeignTable => "Foreign Table",
            PostgresObjectKind::Sequence => "Sequence",
            PostgresObjectKind::Extension => "Extension",
            PostgresObjectKind::Role => "Login / Group Role",
        }
    }

    pub(crate) fn postgres_table_icon_for(&self, table: &str) -> Option<char> {
        let kind = self.postgres_object_kind_for_table(table)?;
        Some(Self::postgres_object_icon(kind))
    }

    pub(crate) fn postgres_object_definition_query(
        &self,
        object: &PostgresSidebarObject,
    ) -> Option<String> {
        let qualified = sql_quote_identifier_path(
            DatabaseDriver::PostgreSql,
            &[object.schema.as_str(), object.name.as_str()],
        );
        let qualified_literal = sql_escape_string_literal(DatabaseDriver::PostgreSql, &qualified);

        match object.kind {
            PostgresObjectKind::View | PostgresObjectKind::MaterializedView => Some(format!(
                "SELECT pg_get_viewdef('{}'::regclass, true) AS definition;",
                qualified_literal
            )),
            _ => None,
        }
    }

    pub(crate) fn postgres_object_metadata_query(&self, object: &PostgresSidebarObject) -> String {
        let schema_literal =
            sql_escape_string_literal(DatabaseDriver::PostgreSql, object.schema.as_str());
        let name_literal =
            sql_escape_string_literal(DatabaseDriver::PostgreSql, object.name.as_str());

        match object.kind {
            PostgresObjectKind::Sequence => format!(
                "SELECT schemaname, sequencename, data_type, start_value, min_value, max_value, \
                increment_by, cycle, cache_size, last_value \
                FROM pg_sequences \
                WHERE schemaname = '{}' AND sequencename = '{}';",
                schema_literal, name_literal
            ),
            PostgresObjectKind::Extension => format!(
                "SELECT e.extname AS extension_name, e.extversion AS version, \
                n.nspname AS schema_name, pg_get_userbyid(e.extowner) AS owner, \
                obj_description(e.oid, 'pg_extension') AS comment \
                FROM pg_extension e \
                JOIN pg_namespace n ON n.oid = e.extnamespace \
                WHERE e.extname = '{}';",
                name_literal
            ),
            PostgresObjectKind::Role => format!(
                "SELECT r.rolname AS role_name, \
                CASE WHEN r.rolcanlogin THEN 'LOGIN' ELSE 'GROUP' END AS role_type, \
                r.rolsuper AS is_superuser, \
                r.rolinherit AS inherit_privileges, \
                r.rolcreaterole AS can_create_roles, \
                r.rolcreatedb AS can_create_databases, \
                r.rolreplication AS can_replicate, \
                r.rolbypassrls AS bypass_row_level_security, \
                r.rolconnlimit AS connection_limit, \
                r.rolvaliduntil AS valid_until, \
                COALESCE(ARRAY_TO_STRING(ARRAY( \
                    SELECT parent.rolname \
                    FROM pg_auth_members member_of \
                    JOIN pg_roles parent ON parent.oid = member_of.roleid \
                    WHERE member_of.member = r.oid \
                    ORDER BY parent.rolname \
                ), ', '), '') AS member_of_roles \
                FROM pg_roles r \
                WHERE r.rolname = '{}';",
                name_literal
            ),
            _ => self
                .postgres_object_definition_query(object)
                .unwrap_or_else(|| self.structure_query_for(&object.qualified_name())),
        }
    }

    pub(crate) fn open_postgres_sidebar_object(
        &mut self,
        object: PostgresSidebarObject,
    ) -> Task<Message> {
        if self.connections.current.driver != DatabaseDriver::PostgreSql {
            return Task::none();
        }
        if self.workspace.query.running {
            return Task::none();
        }

        let Some(pool) = self.connections.pool.clone() else {
            self.workspace.query.error = Some(String::from("Not connected."));
            return Task::none();
        };
        let Some(database) = self.current_database() else {
            self.workspace.query.error = Some(String::from("Select a database first."));
            return Task::none();
        };

        self.leave_query_tab_context();
        self.workspace.explorer.selected_postgres_object = Some(object.clone());
        self.workspace.selected_table = None;
        self.workspace.explorer.selected_trigger = None;
        self.workspace.explorer.table_triggers.clear();
        self.workspace.explorer.triggers_table = None;
        self.workspace.explorer.table_relations.clear();
        self.workspace.explorer.relations_table = None;
        self.workspace.query.table_query = None;
        self.workspace.query.editable_query = None;
        self.workspace.query.table = None;
        self.workspace.query.error = None;
        self.workspace.results.apply_error = None;
        self.workspace.results.apply_message = None;
        self.clear_pending_edits_state();
        self.workspace.results.editing_cell = None;
        self.workspace.results.selected_cell = None;
        self.clear_row_selection();
        self.close_text_modal();
        self.close_query_suggestions();
        self.workspace.results.current = None;
        self.workspace.results.column_widths.clear();
        self.workspace.results.column_resize = None;
        self.workspace.query.table_has_next_page = false;
        self.workspace.query.last_query_was_table = false;

        let query = self.postgres_object_metadata_query(&object);
        self.set_query_text(&query);
        self.push_history(query.clone());

        self.workspace.query.running = true;
        let cancel_flag = Arc::new(AtomicBool::new(false));
        self.workspace.query.cancel_flag = Some(cancel_flag.clone());
        let timeout_secs = self.settings.values.query_timeout_secs;
        Task::perform(
            async move {
                let query_future = run_query_with_control(
                    pool,
                    Some(database),
                    query,
                    Some(cancel_flag),
                    Some(ADHOC_QUERY_MAX_ROWS),
                );
                if timeout_secs > 0 {
                    match tokio::time::timeout(
                        std::time::Duration::from_secs(timeout_secs),
                        query_future,
                    )
                    .await
                    {
                        Ok(result) => result,
                        Err(_) => Err(crate::i18n::tr_with(
                            "Query timed out after {seconds} seconds.",
                            &[("{seconds}", &timeout_secs.to_string())],
                        )),
                    }
                } else {
                    query_future.await
                }
            },
            |value| {
                Message::Workspace(crate::app::features::workspace::Message::Query(
                    crate::app::features::workspace::query::Message::QueryFinished(value),
                ))
            },
        )
    }

    pub(crate) fn table_alias_lookup(&self) -> HashMap<String, String> {
        self.workspace
            .explorer
            .table_alias_lookup(self.connections.current.driver)
    }

    pub(crate) fn resolve_table_reference(&self, table: &str) -> Option<String> {
        let trimmed = table.trim();
        if trimmed.is_empty() {
            return None;
        }

        let lookup = self.table_alias_lookup();
        if let Some(table) = lookup.get(&trimmed.to_ascii_lowercase()).cloned() {
            return Some(table);
        }

        if self.connections.current.driver != DatabaseDriver::PostgreSql
            && let Some(base) = trimmed.rsplit('.').next()
        {
            let base_key = base
                .trim_matches('"')
                .trim_matches('`')
                .to_ascii_lowercase();
            if let Some(table) = lookup.get(&base_key).cloned() {
                return Some(table);
            }
        }

        let canonical = self.table_key(trimmed);
        if canonical.is_empty() {
            None
        } else {
            lookup.get(&canonical.to_ascii_lowercase()).cloned()
        }
    }

    pub(crate) fn table_search_terms(&self, table: &str) -> String {
        let mut terms = Vec::new();

        let raw = table.trim();
        if !raw.is_empty() {
            terms.push(raw.to_string());
        }

        let canonical = self.table_key(table);
        if !canonical.is_empty() && !terms.iter().any(|term| term == &canonical) {
            terms.push(canonical.clone());
        }

        if self.connections.current.driver == DatabaseDriver::PostgreSql
            && let Some((schema, base)) = canonical.rsplit_once('.')
        {
            if !base.is_empty() && !terms.iter().any(|term| term == base) {
                terms.push(base.to_string());
            }
            if !schema.is_empty() && !terms.iter().any(|term| term == schema) {
                terms.push(schema.to_string());
            }
        }

        terms.join(" ")
    }

    pub(crate) fn folder_store_keys(&self) -> Option<(String, String)> {
        Some((
            self.current_folder_connection_key()?,
            self.current_database()?,
        ))
    }

    pub(crate) fn persist_folder_store(&mut self) {
        let keys = self.folder_store_keys();
        self.workspace.explorer.persist_folder_store(keys)
    }

    pub(crate) fn load_folder_state_for_current_database(&mut self) {
        let keys = self.folder_store_keys();
        self.workspace
            .explorer
            .load_folder_state_for_current_database(self.connections.current.driver, keys)
    }

    pub(crate) fn prune_folder_store_databases(&mut self) {
        let key = self.current_folder_connection_key();
        self.workspace
            .explorer
            .prune_folder_store_databases(key, &self.connections.databases);
    }
    pub(crate) fn request_table_info(&mut self, table: String) -> Task<Message> {
        let database = self.current_database();
        self.workspace
            .explorer
            .request_table_info(table, self.connections.pool.clone(), database)
            .map(|value| {
                Message::Workspace(crate::app::features::workspace::Message::Explorer(value))
            })
    }

    pub(crate) fn dissolve_folder(&mut self, folder_name: &str) -> bool {
        let keys = self.folder_store_keys();
        if !self.workspace.explorer.dissolve_folder(folder_name, keys) {
            return false;
        }
        self.workspace.results.apply_error = None;
        self.workspace.results.apply_message = Some(crate::i18n::tr_with(
            "Folder `{folder}` removed.",
            &[("{folder}", folder_name)],
        ));
        true
    }

    pub(crate) fn open_folder_for_table(&mut self, table: &str) {
        let keys = self.folder_store_keys();
        self.workspace
            .explorer
            .open_folder_for_table(self.connections.current.driver, keys, table)
    }

    pub(crate) fn clear_generated_folders(&mut self) -> usize {
        let keys = self.folder_store_keys();
        self.workspace.explorer.clear_generated_folders(keys)
    }

    pub(crate) fn set_generated_folders_open(&mut self, is_open: bool) -> usize {
        let keys = self.folder_store_keys();
        self.workspace
            .explorer
            .set_generated_folders_open(keys, is_open)
    }

    pub(crate) fn load_triggers_for_table(&mut self, table: String) -> Task<Message> {
        let database = self.current_database();
        self.workspace
            .explorer
            .load_triggers_for_table(table, self.connections.pool.clone(), database)
            .map(|value| {
                Message::Workspace(crate::app::features::workspace::Message::Explorer(value))
            })
    }

    pub(crate) fn load_relations_for_table(&mut self, table: String) -> Task<Message> {
        let database = self.current_database();
        self.workspace
            .explorer
            .load_relations_for_table(table, self.connections.pool.clone(), database)
            .map(|value| {
                Message::Workspace(crate::app::features::workspace::Message::Explorer(value))
            })
    }

    pub(crate) fn apply_table_command_side_effects(&mut self, command: &TableCommand) {
        let mut folder_state_changed = false;
        match command {
            TableCommand::Drop { table } => {
                let table_key = self.table_key(table);
                if !table_key.is_empty()
                    && self
                        .workspace
                        .explorer
                        .table_folder_map
                        .remove(&table_key)
                        .is_some()
                {
                    folder_state_changed = true;
                }
                self.remove_table_state(table);
            }
            TableCommand::Rename { table, new_table } => {
                let old_key = self.table_key(table);
                let new_key = self.table_key(new_table);
                if !old_key.is_empty()
                    && !new_key.is_empty()
                    && let Some(folder) = self.workspace.explorer.table_folder_map.remove(&old_key)
                {
                    self.workspace
                        .explorer
                        .table_folder_map
                        .insert(new_key, folder);
                    folder_state_changed = true;
                }
                self.rename_table_state(table, new_table);
            }
            TableCommand::Duplicate { table, new_table } => {
                let source_key = self.table_key(table);
                let target_key = self.table_key(new_table);
                if !source_key.is_empty()
                    && !target_key.is_empty()
                    && let Some(folder) = self
                        .workspace
                        .explorer
                        .table_folder_map
                        .get(&source_key)
                        .cloned()
                {
                    let previous = self
                        .workspace
                        .explorer
                        .table_folder_map
                        .insert(target_key, folder.clone());
                    if previous.as_deref() != Some(folder.as_str()) {
                        folder_state_changed = true;
                    }
                }
                self.workspace
                    .explorer
                    .remove_table_metadata_cache(new_table);
            }
            TableCommand::AlterTable { table, .. }
            | TableCommand::AddColumn { table, .. }
            | TableCommand::Truncate { table } => {
                self.workspace.explorer.remove_table_metadata_cache(table);
                if self.workspace.selected_table.as_deref() == Some(table.as_str()) {
                    self.workspace.explorer.table_triggers.clear();
                    self.workspace.explorer.triggers_table = None;
                    self.workspace.explorer.selected_trigger = None;
                    self.workspace.explorer.table_relations.clear();
                    self.workspace.explorer.relations_table = None;
                }
            }
        }
        if folder_state_changed {
            self.persist_folder_store();
        }
    }

    pub(crate) fn remove_table_state(&mut self, table: &str) {
        self.workspace.tabs.open_tables.retain(|name| name != table);
        self.workspace
            .tabs
            .tab_strip
            .retain(|entry| !matches!(entry, TabEntry::Table(name) if name == table));
        self.workspace.tabs.pinned_table_tabs.remove(table);
        self.workspace.table_pages.remove(table);
        self.remove_table_cache_for_table(table);
        self.workspace.explorer.remove_table_metadata_cache(table);
        self.workspace.table_filters.remove(table);
        self.workspace.table_sorts.remove(table);
        self.workspace.explorer.remove_table_info_cache_entry(table);

        if self.workspace.explorer.triggers_table.as_deref() == Some(table) {
            self.workspace.explorer.table_triggers.clear();
            self.workspace.explorer.triggers_table = None;
            self.workspace.explorer.selected_trigger = None;
        }
        if self.workspace.explorer.relations_table.as_deref() == Some(table) {
            self.workspace.explorer.table_relations.clear();
            self.workspace.explorer.relations_table = None;
        }
        if self.workspace.query.table.as_deref() == Some(table) {
            self.workspace.query.table = None;
            self.workspace.query.editable_query = None;
            self.workspace.query.table_query = None;
        }
        if self.workspace.selected_table.as_deref() == Some(table) {
            self.workspace.selected_table = None;
            self.workspace.query.table_has_next_page = false;
            self.workspace.query.last_query_was_table = false;
            self.clear_pending_edits_state();
            self.workspace.results.editing_cell = None;
            self.workspace.results.selected_cell = None;
            self.clear_row_selection();
            self.workspace.results.current = None;
        }
        if self.workspace.explorer.table_info_table.as_deref() == Some(table) {
            self.workspace.explorer.table_info_table = None;
            self.workspace.explorer.table_info = None;
            self.workspace.explorer.table_info_error = None;
            self.workspace.explorer.table_info_loading = false;
            self.workspace.explorer.table_info_sidebar_open = false;
        }
    }

    pub(crate) fn rename_table_state(&mut self, old: &str, new: &str) {
        for name in &mut self.workspace.tabs.open_tables {
            if name == old {
                *name = new.to_string();
            }
        }
        for entry in &mut self.workspace.tabs.tab_strip {
            if let TabEntry::Table(name) = entry
                && name == old
            {
                *name = new.to_string();
            }
        }
        if self.workspace.tabs.pinned_table_tabs.remove(old) {
            self.workspace
                .tabs
                .pinned_table_tabs
                .insert(new.to_string());
        }
        if let Some(page) = self.workspace.table_pages.remove(old) {
            self.workspace.table_pages.insert(new.to_string(), page);
        }
        if let Some(filter) = self.workspace.table_filters.remove(old) {
            self.workspace.table_filters.insert(new.to_string(), filter);
        }
        if let Some(sort) = self.workspace.table_sorts.remove(old) {
            self.workspace.table_sorts.insert(new.to_string(), sort);
        }
        self.workspace
            .explorer
            .rename_table_info_cache_entry(old, new);
        if let Some(triggers) = self.workspace.explorer.table_triggers_cache.remove(old) {
            self.workspace
                .explorer
                .table_triggers_cache
                .insert(new.to_string(), triggers);
        }
        if let Some(relations) = self.workspace.explorer.table_relations_cache.remove(old) {
            self.workspace
                .explorer
                .table_relations_cache
                .insert(new.to_string(), relations);
        }

        let moved_cache = self
            .workspace
            .table_cache
            .iter()
            .filter_map(|((name, page), entry)| {
                if name == old {
                    Some((*page, entry.clone()))
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        self.remove_table_cache_for_table(old);
        for (page, entry) in moved_cache {
            self.insert_table_cache_entry(
                new.to_string(),
                page,
                Arc::clone(&entry.results),
                entry.has_next_page,
            );
        }

        if self.workspace.selected_table.as_deref() == Some(old) {
            self.workspace.selected_table = Some(new.to_string());
        }
        if self.workspace.query.table.as_deref() == Some(old) {
            self.workspace.query.table = Some(new.to_string());
        }
        if self.workspace.explorer.triggers_table.as_deref() == Some(old) {
            self.workspace.explorer.triggers_table = Some(new.to_string());
        }
        if self.workspace.explorer.relations_table.as_deref() == Some(old) {
            self.workspace.explorer.relations_table = Some(new.to_string());
        }
        if self.workspace.explorer.table_info_table.as_deref() == Some(old) {
            self.workspace.explorer.table_info_table = Some(new.to_string());
        }
    }

    pub(crate) fn compact_row_count(&self, table: &str) -> Option<String> {
        let count = *self.workspace.explorer.table_row_counts.get(table)?;
        Some(if count >= 1_000_000 {
            format!("{}m", count / 1_000_000)
        } else if count >= 1_000 {
            format!("{}k", count / 1_000)
        } else {
            count.to_string()
        })
    }

    pub(crate) fn scroll_sidebar_to_table(&self, table: &str) -> Task<Message> {
        let filter = self
            .workspace
            .explorer
            .table_search
            .trim()
            .to_ascii_lowercase();
        let filtered_tables: Vec<&String> = if filter.is_empty() {
            self.workspace.explorer.tables.iter().collect()
        } else {
            self.workspace
                .explorer
                .tables
                .iter()
                .filter(|name| name.to_ascii_lowercase().contains(&filter))
                .collect()
        };

        if filtered_tables.is_empty() {
            return Task::none();
        }

        let row_height = self.button_text_size() as f32 + self.button_padding()[0] * 2.0;
        let folder_row_height =
            self.label_text_size() as f32 + self.button_padding_tight()[0] * 2.0;
        let spacing = self.scale_u16(3);
        let banner_height = self.label_text_size() as f32 + self.scale_u16(6) * 2.0;
        let mut offset_y = 0.0_f32;

        if self.workspace.explorer.folder_generation_running {
            offset_y += banner_height + spacing;
        }

        for folder in &self.workspace.explorer.table_folders {
            let folder_tables = filtered_tables
                .iter()
                .copied()
                .filter(|table_name| self.table_assigned_to_folder(table_name, &folder.name))
                .collect::<Vec<_>>();

            let folder_matches =
                !filter.is_empty() && folder.name.to_ascii_lowercase().contains(&filter);
            if folder_tables.is_empty() && !folder_matches && !filter.is_empty() {
                continue;
            }

            offset_y += folder_row_height + spacing;

            if folder.is_open {
                for table_name in folder_tables {
                    if table_name.as_str() == table {
                        let target_y = (offset_y - row_height).max(0.0);
                        return iced::widget::operation::scroll_to(
                            sidebar_tables_scroll_id(),
                            scrollable::AbsoluteOffset {
                                x: None,
                                y: Some(target_y),
                            },
                        );
                    }
                    offset_y += row_height + spacing;
                }
            }
        }

        let root_tables = filtered_tables
            .iter()
            .copied()
            .filter(|table_name| !self.table_has_folder_assignment(table_name))
            .collect::<Vec<_>>();

        for table_name in root_tables {
            if table_name.as_str() == table {
                let target_y = (offset_y - row_height).max(0.0);
                return iced::widget::operation::scroll_to(
                    sidebar_tables_scroll_id(),
                    scrollable::AbsoluteOffset {
                        x: None,
                        y: Some(target_y),
                    },
                );
            }
            offset_y += row_height + spacing;
        }

        Task::none()
    }

    pub(crate) fn sidebar_content_width(&self, layout: LayoutMode) -> Option<f32> {
        let window_width = self.shell.window_size.width;
        if window_width <= 0.0 {
            return None;
        }

        let outer_padding = self.scale_u16(6) * 2.0;
        let content_width = (window_width - outer_padding).max(0.0);
        let panel_width = match layout {
            LayoutMode::Wide => content_width * self.shell.sidebar_ratio,
            LayoutMode::Compact => content_width,
        };
        let panel_padding = if self.modern() {
            0.0
        } else {
            self.scale_u16(6) * 2.0
        };
        let inner_width = (panel_width - panel_padding).max(0.0);
        Some(inner_width)
    }

    pub(crate) fn sidebar_table_max_chars(&self, layout: LayoutMode) -> usize {
        let Some(inner_width) = self.sidebar_content_width(layout) else {
            return if matches!(layout, LayoutMode::Wide) {
                36
            } else {
                26
            };
        };

        let right_padding = self.scale_f32(SIDEBAR_LIST_RIGHT_PADDING);
        let scrollbar_width = self.scale_f32(SIDEBAR_SCROLLBAR_WIDTH);
        let scrollbar_spacing = self.scale_f32(SIDEBAR_SCROLLBAR_GUTTER);
        let button_padding = self.button_padding()[1] * 2.0;
        let modern_extras = if self.modern() {
            self.sidebar_icon_size() + self.scale_f32(38.0) + self.scale_u16(6) * 2.0
        } else {
            0.0
        };
        let reserved =
            right_padding + scrollbar_width + scrollbar_spacing + button_padding + modern_extras;
        let available = (inner_width - reserved).max(40.0);
        let adjusted = available / self.font_scale();
        max_chars_for_width(adjusted)
    }

    pub(crate) fn sidebar_child_max_chars(&self, layout: LayoutMode) -> usize {
        let Some(inner_width) = self.sidebar_content_width(layout) else {
            return if matches!(layout, LayoutMode::Wide) {
                32
            } else {
                22
            };
        };

        let right_padding = self.scale_f32(SIDEBAR_LIST_RIGHT_PADDING);
        let scrollbar_width = self.scale_f32(SIDEBAR_SCROLLBAR_WIDTH);
        let scrollbar_spacing = self.scale_f32(SIDEBAR_SCROLLBAR_GUTTER);
        let button_padding = self.button_padding_tight()[1] * 2.0;
        let indent = self.scale_f32(SIDEBAR_TRIGGER_INDENT);
        let reserved =
            right_padding + scrollbar_width + scrollbar_spacing + button_padding + indent;
        let available = (inner_width - reserved).max(40.0);
        let adjusted = available / self.font_scale();
        max_chars_for_width(adjusted)
    }
}
