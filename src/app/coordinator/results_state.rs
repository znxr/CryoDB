use crate::app::core::App;
use crate::app::message::Message;
use crate::app::types::LayoutMode;
use crate::constants::{
    BASE_RESULTS_VIEWPORT_HEIGHT, COLUMN_WIDTH_COMPACT_SCALE, COLUMN_WIDTH_MIN,
    READONLY_CELL_CHAR_LIMIT, RESULT_COLUMN_OVERSCAN, RESULT_ROW_OVERSCAN, ROW_HEADER_WIDTH,
};
use crate::db::codecs::sql_literal_from_value_for_driver;
use crate::model::connection::DatabaseDriver;
use crate::model::settings::ResultGridDensity;
use crate::model::table::{
    ColumnKind, PostgresObjectKind, PostgresSidebarObject, RelationInfo, ResultSet,
    TableCacheEntry, TableRelationFilter,
};
use crate::ui::ids::{cell_input_id, results_vertical_scroll_id};
use crate::utils::helpers::{
    escape_mysql_string_literal, escape_sqlite_string_literal, is_binary_preview_value,
    is_null_value, is_valid_json, looks_like_json, normalize_query, sql_escape_string_literal,
    sql_quote_identifier, sql_quote_identifier_path, sql_quote_table_reference,
};
use iced::Task;
use iced::widget::{scrollable, text_editor};
use std::sync::Arc;

impl App {
    pub(crate) fn clear_pending_edits_state(&mut self) {
        self.workspace.results.pending_edits.clear();
        self.workspace.results.original_row_snapshots.clear();
        self.workspace.results.released = false;
    }

    pub(crate) fn table_cache_limits(&self) -> (usize, usize) {
        let max_entries = self.settings.values.table_cache_limit_entries.max(1);
        let max_bytes = self
            .settings
            .values
            .table_cache_limit_mb
            .saturating_mul(1024 * 1024);
        (max_entries, max_bytes)
    }

    pub(crate) fn mark_table_cache_recent(&mut self, key: &(String, usize)) {
        self.workspace
            .table_cache_lru
            .retain(|existing| existing != key);
        self.workspace.table_cache_lru.push_back(key.clone());
    }

    pub(crate) fn remove_table_cache_key(&mut self, key: &(String, usize)) {
        if let Some(removed) = self.workspace.table_cache.remove(key) {
            self.workspace.table_cache_bytes_estimate = self
                .workspace
                .table_cache_bytes_estimate
                .saturating_sub(removed.results.estimated_heap_bytes());
        }
        self.workspace
            .table_cache_lru
            .retain(|existing| existing != key);
    }

    pub(crate) fn trim_table_cache_to_limits(&mut self) {
        let (max_entries, max_bytes) = self.table_cache_limits();

        while self.workspace.table_cache.len() > max_entries
            || (max_bytes > 0 && self.workspace.table_cache_bytes_estimate > max_bytes)
        {
            let Some(oldest) = self.workspace.table_cache_lru.pop_front() else {
                break;
            };
            if let Some(removed) = self.workspace.table_cache.remove(&oldest) {
                self.workspace.table_cache_bytes_estimate = self
                    .workspace
                    .table_cache_bytes_estimate
                    .saturating_sub(removed.results.estimated_heap_bytes());
            }
        }
    }

    pub(crate) fn clear_table_cache(&mut self) {
        self.workspace.table_cache.clear();
        self.workspace.table_cache_lru.clear();
        self.workspace.table_cache_bytes_estimate = 0;
        self.workspace.table_inactive_since.clear();
        self.workspace.inactive_table_tabs_released.clear();
    }

    pub(crate) fn remove_table_cache_for_table(&mut self, table: &str) {
        let keys = self
            .workspace
            .table_cache
            .keys()
            .filter(|(name, _)| name == table)
            .cloned()
            .collect::<Vec<_>>();
        for key in keys {
            self.remove_table_cache_key(&key);
        }
        self.workspace.table_inactive_since.remove(table);
        self.workspace.inactive_table_tabs_released.remove(table);
    }

    pub(crate) fn retain_table_cache_for_selected(&mut self, selected: &str) {
        let keys = self
            .workspace
            .table_cache
            .keys()
            .filter(|(table, _)| table != selected)
            .cloned()
            .collect::<Vec<_>>();
        for key in keys {
            self.remove_table_cache_key(&key);
        }
    }

    pub(crate) fn insert_table_cache_entry(
        &mut self,
        table: String,
        page: usize,
        results: Arc<ResultSet>,
        has_next_page: bool,
    ) {
        self.workspace.inactive_table_tabs_released.remove(&table);
        let key = (table, page);
        let estimated_bytes = results.estimated_heap_bytes();
        let (_, max_bytes) = self.table_cache_limits();

        if max_bytes > 0 && estimated_bytes > max_bytes {
            self.remove_table_cache_key(&key);
            return;
        }

        if let Some(previous) = self.workspace.table_cache.insert(
            key.clone(),
            TableCacheEntry {
                results,
                has_next_page,
            },
        ) {
            self.workspace.table_cache_bytes_estimate = self
                .workspace
                .table_cache_bytes_estimate
                .saturating_sub(previous.results.estimated_heap_bytes());
        }

        self.workspace.table_cache_bytes_estimate = self
            .workspace
            .table_cache_bytes_estimate
            .saturating_add(estimated_bytes);
        self.mark_table_cache_recent(&key);
        self.trim_table_cache_to_limits();
    }

    pub(crate) fn get_table_cache_entry(
        &mut self,
        key: &(String, usize),
    ) -> Option<TableCacheEntry> {
        let entry = self.workspace.table_cache.get(key).cloned()?;
        self.workspace.inactive_table_tabs_released.remove(&key.0);
        self.mark_table_cache_recent(key);
        Some(entry)
    }

    pub(crate) fn detach_active_results_from_cache(&mut self) {
        if !self.workspace.query.last_query_was_table {
            return;
        }
        let Some(table) = self.workspace.selected_table.clone() else {
            return;
        };
        let Some(results) = self.workspace.results.current.as_ref() else {
            return;
        };
        let page = self.workspace.table_pages.get(&table).copied().unwrap_or(0);
        let key = (table, page);
        let should_remove = self
            .workspace
            .table_cache
            .get(&key)
            .is_some_and(|entry| Arc::ptr_eq(&entry.results, results));
        if should_remove {
            self.remove_table_cache_key(&key);
        }
    }

    pub(crate) fn has_pending_edits_for_row(&self, row: usize) -> bool {
        self.workspace
            .results
            .pending_edits
            .keys()
            .any(|(pending_row, _)| *pending_row == row)
    }

    pub(crate) fn discard_pending_edits(&mut self) {
        if self.workspace.results.pending_edits.is_empty() {
            return;
        }

        if let Some(results) = &mut self.workspace.results.current {
            let results = Arc::make_mut(results);
            for &(row, column) in self.workspace.results.pending_edits.keys() {
                if let Some(original_row) = self.workspace.results.original_row_snapshots.get(&row)
                    && let Some(original_value) = original_row.get(column)
                    && let Some(current_row) = results.rows.get_mut(row)
                    && let Some(cell) = current_row.get_mut(column)
                {
                    *cell = original_value.clone();
                }
            }
        }

        self.clear_pending_edits_state();
    }

    pub(crate) fn prune_inactive_query_tab_states(&mut self) -> usize {
        if !self.settings.values.release_inactive_tab_memory {
            return 0;
        }

        let active_index = self.workspace.tabs.active_query_tab;
        let now = std::time::Instant::now();
        let idle_duration = std::time::Duration::from_secs(
            self.settings.values.inactive_tab_release_idle_secs.max(1),
        );
        let mut released_total = 0usize;

        for (index, tab) in self.workspace.tabs.query_tabs.iter_mut().enumerate() {
            if Some(index) == active_index {
                continue;
            }
            let Some(state) = tab.state.as_mut() else {
                continue;
            };
            if !state.pending_edits.is_empty() {
                continue;
            }

            let inactive_since = match tab.inactive_since {
                Some(timestamp) => timestamp,
                None => {
                    tab.inactive_since = Some(now);
                    continue;
                }
            };
            if now.duration_since(inactive_since) < idle_duration {
                continue;
            }

            released_total = released_total.saturating_add(state.release_heavy_state());
        }

        released_total
    }

    pub(crate) fn prune_inactive_table_tab_cache(&mut self) -> usize {
        if !self.settings.values.release_inactive_tab_memory {
            return 0;
        }

        let now = std::time::Instant::now();
        let idle_duration = std::time::Duration::from_secs(
            self.settings.values.inactive_tab_release_idle_secs.max(1),
        );
        let active_table = self.workspace.selected_table.clone();

        let inactive_tables = self
            .workspace
            .tabs
            .open_tables
            .iter()
            .filter(|table| active_table.as_deref() != Some(table.as_str()))
            .cloned()
            .collect::<Vec<_>>();

        if inactive_tables.is_empty() {
            return 0;
        }

        let mut released_total = 0usize;

        for table in inactive_tables {
            let inactive_since = match self.workspace.table_inactive_since.get(&table).copied() {
                Some(timestamp) => timestamp,
                None => {
                    self.workspace
                        .table_inactive_since
                        .insert(table.clone(), now);
                    continue;
                }
            };
            if now.duration_since(inactive_since) < idle_duration {
                continue;
            }

            let keys = self
                .workspace
                .table_cache
                .keys()
                .filter(|(name, _)| name == &table)
                .cloned()
                .collect::<Vec<_>>();
            if keys.is_empty() {
                continue;
            }

            let mut released_for_table = 0usize;

            for key in keys {
                if let Some(removed) = self.workspace.table_cache.remove(&key) {
                    let removed_bytes = removed.results.estimated_heap_bytes();
                    released_for_table = released_for_table.saturating_add(removed_bytes);
                    released_total = released_total.saturating_add(removed_bytes);
                    self.workspace.table_cache_bytes_estimate = self
                        .workspace
                        .table_cache_bytes_estimate
                        .saturating_sub(removed_bytes);
                }
                self.workspace
                    .table_cache_lru
                    .retain(|existing| existing != &key);
            }

            if released_for_table > 0 {
                self.workspace.inactive_table_tabs_released.insert(table);
            }
        }

        released_total
    }

    pub(crate) fn maybe_trim_process_memory(&self) {
        let _ = self;
    }

    pub(crate) fn prune_inactive_tab_memory(&mut self) {
        if !self.settings.values.release_inactive_tab_memory {
            return;
        }

        let released_query = self.prune_inactive_query_tab_states();
        let released_table = self.prune_inactive_table_tab_cache();
        if released_query > 0 || released_table > 0 {
            self.maybe_trim_process_memory();
        }
    }

    pub(crate) fn query_tab_has_inactive_results_badge(&self, query_index: usize) -> bool {
        if !self.settings.values.release_inactive_tab_memory {
            return false;
        }

        if self.workspace.tabs.active_query_tab == Some(query_index) {
            return self.workspace.results.released;
        }

        self.workspace
            .tabs
            .query_tabs
            .get(query_index)
            .and_then(|tab| tab.state.as_ref())
            .is_some_and(|state| state.inactive_results_released)
    }

    pub(crate) fn table_tab_has_inactive_results_badge(&self, table: &str) -> bool {
        if !self.settings.values.release_inactive_tab_memory {
            return false;
        }
        if self.workspace.selected_table.as_deref() == Some(table) {
            return false;
        }

        self.workspace.inactive_table_tabs_released.contains(table)
    }

    pub(crate) fn is_structure_view_active(&self) -> bool {
        let Some(table) = self.workspace.selected_table.as_deref() else {
            return false;
        };
        let expected = self.structure_query_for(table);
        normalize_query(&self.workspace.query.editor.content()) == normalize_query(&expected)
    }

    pub(crate) fn is_relations_view_active(&self) -> bool {
        let Some(table) = self.workspace.selected_table.as_deref() else {
            return false;
        };
        let expected = self.relations_query_for(table);
        normalize_query(&self.workspace.query.editor.content()) == normalize_query(&expected)
    }

    pub(crate) fn is_table_query_active(&self) -> bool {
        self.workspace
            .query
            .table_query
            .as_deref()
            .is_some_and(|query| query == self.workspace.query.editor.content().trim())
            && self.workspace.selected_table.is_some()
    }

    pub(crate) fn is_editable_query_active(&self) -> bool {
        self.workspace
            .query
            .editable_query
            .as_deref()
            .is_some_and(|query| query == self.workspace.query.editor.content().trim())
            && self.workspace.query.table.is_some()
    }

    pub(crate) fn relation_for_column(&self, column: &str) -> Option<&RelationInfo> {
        if !self.is_table_query_active() {
            return None;
        }
        let selected = self.workspace.selected_table.as_deref()?;
        if self.workspace.explorer.relations_table.as_deref() != Some(selected) {
            return None;
        }
        self.workspace
            .explorer
            .table_relations
            .iter()
            .find(|relation| relation.column == column)
    }

    pub(crate) fn current_table_page(&self) -> usize {
        self.workspace
            .selected_table
            .as_ref()
            .and_then(|table| self.workspace.table_pages.get(table).copied())
            .unwrap_or(0)
    }

    pub(crate) fn table_queries(&self, table: &str, page: usize) -> (String, String) {
        let driver = self.connections.current.driver;
        let safe_table = sql_quote_table_reference(driver, table);
        let offset = page.saturating_mul(self.settings.values.table_query_limit);
        let filter_clause = self
            .workspace
            .table_filters
            .get(table)
            .map(|filter| self.table_filter_clause(filter))
            .unwrap_or_default();
        let order_clause = self
            .workspace
            .table_sorts
            .get(table)
            .map(|sort| {
                format!(
                    " ORDER BY {} {}",
                    sql_quote_identifier(driver, &sort.column),
                    sort.direction.as_sql()
                )
            })
            .unwrap_or_default();
        let display = if offset == 0 {
            format!(
                "SELECT * FROM {}{}{} LIMIT {};",
                safe_table, filter_clause, order_clause, self.settings.values.table_query_limit
            )
        } else {
            format!(
                "SELECT * FROM {}{}{} LIMIT {} OFFSET {};",
                safe_table,
                filter_clause,
                order_clause,
                self.settings.values.table_query_limit,
                offset
            )
        };

        let fetch_limit = self.settings.values.table_query_limit + 1;
        let fetch = if offset == 0 {
            format!(
                "SELECT * FROM {}{}{} LIMIT {};",
                safe_table, filter_clause, order_clause, fetch_limit
            )
        } else {
            format!(
                "SELECT * FROM {}{}{} LIMIT {} OFFSET {};",
                safe_table, filter_clause, order_clause, fetch_limit, offset
            )
        };

        (display, fetch)
    }

    pub(crate) fn last_page_for_row_count(&self, total: u64) -> usize {
        let limit = self.settings.values.table_query_limit.max(1) as u64;
        (total.saturating_sub(1) / limit) as usize
    }

    pub(crate) fn table_count_query(&self, table: &str) -> String {
        let safe_table = sql_quote_table_reference(self.connections.current.driver, table);
        let filter_clause = self
            .workspace
            .table_filters
            .get(table)
            .map(|filter| self.table_filter_clause(filter))
            .unwrap_or_default();
        format!("SELECT COUNT(*) FROM {}{};", safe_table, filter_clause)
    }

    pub(crate) fn table_filter_clause(&self, filter: &TableRelationFilter) -> String {
        let driver = self.connections.current.driver;
        let column = sql_quote_identifier(driver, &filter.column);
        if is_null_value(&filter.value) {
            format!(" WHERE {} IS NULL", column)
        } else {
            let escaped_value = sql_escape_string_literal(driver, &filter.value);
            format!(" WHERE {} = '{}'", column, escaped_value)
        }
    }

    pub(crate) fn structure_query_for(&self, table: &str) -> String {
        match self.connections.current.driver {
            DatabaseDriver::Sqlite => format!(
                "PRAGMA table_info('{}');",
                escape_sqlite_string_literal(table)
            ),
            DatabaseDriver::PostgreSql => {
                let (schema, table_name) = match table.split_once('.') {
                    Some((schema, table_name)) => {
                        let schema = schema.trim();
                        let table_name = table_name.trim();
                        let schema = if schema.is_empty() {
                            None
                        } else {
                            Some(schema)
                        };
                        (schema, table_name)
                    }
                    None => (None, table.trim()),
                };

                if let Some(kind) = self.postgres_object_kind_for_table(table)
                    && matches!(
                        kind,
                        PostgresObjectKind::View | PostgresObjectKind::MaterializedView
                    )
                {
                    let object = PostgresSidebarObject {
                        schema: schema.unwrap_or("public").to_string(),
                        name: table_name.to_string(),
                        kind,
                    };
                    if let Some(definition_query) = self.postgres_object_definition_query(&object) {
                        return definition_query;
                    }
                }

                let table_name = sql_escape_string_literal(DatabaseDriver::PostgreSql, table_name);
                let schema_clause = if let Some(schema) = schema {
                    format!(
                        "'{}'",
                        sql_escape_string_literal(DatabaseDriver::PostgreSql, schema)
                    )
                } else {
                    String::from("current_schema()")
                };
                format!(
                    "SELECT column_name, data_type, is_nullable, column_default \
                    FROM information_schema.columns \
                    WHERE table_schema = {} \
                    AND table_name = '{}' \
                    ORDER BY ordinal_position;",
                    schema_clause, table_name
                )
            }
            _ => format!(
                "DESC {};",
                sql_quote_identifier(self.connections.current.driver, table)
            ),
        }
    }

    pub(crate) fn relations_query_for(&self, table: &str) -> String {
        match self.connections.current.driver {
            DatabaseDriver::Sqlite => format!(
                "PRAGMA foreign_key_list('{}');",
                escape_sqlite_string_literal(table)
            ),
            DatabaseDriver::PostgreSql => {
                let (schema, table_name) = match table.split_once('.') {
                    Some((schema, table_name)) => {
                        let schema = schema.trim();
                        let table_name = table_name.trim();
                        let schema = if schema.is_empty() {
                            None
                        } else {
                            Some(schema)
                        };
                        (schema, table_name)
                    }
                    None => (None, table.trim()),
                };
                let table_name = sql_escape_string_literal(DatabaseDriver::PostgreSql, table_name);
                let schema_clause = if let Some(schema) = schema {
                    format!(
                        "'{}'",
                        sql_escape_string_literal(DatabaseDriver::PostgreSql, schema)
                    )
                } else {
                    String::from("current_schema()")
                };
                format!(
                    "SELECT tc.constraint_name, kcu.column_name, ccu.table_name AS referenced_table_name, \
                    ccu.column_name AS referenced_column_name \
                    FROM information_schema.table_constraints tc \
                    JOIN information_schema.key_column_usage kcu \
                    ON tc.constraint_name = kcu.constraint_name \
                    AND tc.table_schema = kcu.table_schema \
                    JOIN information_schema.constraint_column_usage ccu \
                    ON tc.constraint_name = ccu.constraint_name \
                    AND tc.table_schema = ccu.table_schema \
                    WHERE tc.constraint_type = 'FOREIGN KEY' \
                    AND tc.table_schema = {} \
                    AND tc.table_name = '{}' \
                    ORDER BY kcu.ordinal_position;",
                    schema_clause, table_name
                )
            }
            _ => format!(
                "SELECT constraint_name, column_name, referenced_table_name, \
                referenced_column_name \
                FROM information_schema.key_column_usage \
                WHERE table_schema = DATABASE() \
                AND table_name = '{}' \
                AND referenced_table_name IS NOT NULL \
                ORDER BY ordinal_position;",
                escape_mysql_string_literal(table)
            ),
        }
    }

    pub(crate) fn result_row_height(&self) -> f32 {
        let font_scale = self.font_scale();
        let density_extra = match self.settings.values.result_grid_density {
            ResultGridDensity::Comfortable => 4.0 * font_scale,
            ResultGridDensity::Compact => 0.0,
        };
        if self.modern() {
            return self.scale_f32(34.0) + density_extra;
        }
        self.button_text_size() as f32 + (10.0 * font_scale) + density_extra
    }

    pub(crate) fn result_header_height(&self) -> f32 {
        let font_scale = self.font_scale();
        let density_extra = match self.settings.values.result_grid_density {
            ResultGridDensity::Comfortable => 4.0 * font_scale,
            ResultGridDensity::Compact => 0.0,
        };
        if self.modern() {
            return self.scale_f32(40.0) + density_extra;
        }
        self.button_text_size() as f32 + (10.0 * font_scale) + density_extra
    }

    pub(crate) fn default_results_viewport_height(&self) -> f32 {
        self.scale_f32(BASE_RESULTS_VIEWPORT_HEIGHT)
    }

    pub(crate) fn default_results_viewport_width(&self) -> f32 {
        if self.shell.window_size.width > 0.0 {
            self.shell.window_size.width
        } else {
            self.scale_f32(1100.0)
        }
    }

    pub(crate) fn base_column_width(&self, index: usize) -> f32 {
        self.workspace
            .results
            .column_widths
            .get(index)
            .copied()
            .unwrap_or(self.workspace.results.column_width)
    }

    pub(crate) fn initial_column_widths(&self, columns: &[String]) -> Vec<f32> {
        columns
            .iter()
            .map(|name| {
                self.settings
                    .values
                    .column_width_overrides
                    .get(name)
                    .copied()
                    .unwrap_or(self.workspace.results.column_width)
            })
            .collect()
    }

    pub(crate) fn results_dimensions(&self) -> Option<(usize, usize)> {
        let results = self.workspace.results.current.as_ref()?;
        let rows = results.rows.len();
        let columns = results.columns.len();
        if rows == 0 || columns == 0 {
            None
        } else {
            Some((rows, columns))
        }
    }

    pub(crate) fn row_is_selected(&self, row: usize) -> bool {
        let Some((start, end)) = self.workspace.results.selected_rows else {
            return false;
        };
        let (min_row, max_row) = if start <= end {
            (start, end)
        } else {
            (end, start)
        };
        row >= min_row && row <= max_row
    }

    pub(crate) fn selected_rows_range(&self, row_count: usize) -> Option<(usize, usize)> {
        if row_count == 0 {
            return None;
        }

        let (mut start, mut end) = if let Some(range) = self.workspace.results.selected_rows {
            range
        } else if let Some((row, _)) = self.workspace.results.selected_cell {
            (row, row)
        } else {
            return None;
        };

        if start > end {
            std::mem::swap(&mut start, &mut end);
        }

        if start >= row_count {
            return None;
        }

        end = end.min(row_count.saturating_sub(1));
        Some((start, end))
    }

    pub(crate) fn selected_rows_text(&self, include_headers: bool) -> Option<String> {
        let results = self.workspace.results.current.as_ref()?;
        let (start, end) = self.selected_rows_range(results.rows.len())?;
        if results.columns.is_empty() {
            return None;
        }

        let column_count = results.columns.len();
        let mut output = String::new();

        if include_headers {
            output.push_str(&results.columns.join("\t"));
            output.push('\n');
        }

        for row_index in start..=end {
            let Some(row) = results.rows.get(row_index) else {
                break;
            };
            for column_index in 0..column_count {
                if column_index > 0 {
                    output.push('\t');
                }
                let value = row.get(column_index).map(String::as_str).unwrap_or("");
                output.push_str(value);
            }
            if row_index != end {
                output.push('\n');
            }
        }

        Some(output)
    }

    pub(crate) fn selected_rows_sql_insert(&self) -> Result<String, String> {
        if !self.is_editable_query_active() && !self.workspace.query.last_query_was_table {
            return Err(String::from(
                "Copy as SQL Insert is only available for table selections.",
            ));
        }

        let results = self
            .workspace
            .results
            .current
            .as_ref()
            .ok_or_else(|| String::from("No results to copy."))?;
        if results.columns.is_empty() {
            return Err(String::from("No columns to copy."));
        }

        let (start, end) = self
            .selected_rows_range(results.rows.len())
            .ok_or_else(|| String::from("Select rows to copy."))?;

        let table = self
            .workspace
            .query
            .table
            .as_ref()
            .ok_or_else(|| String::from("Select a table to copy rows."))?;
        let driver = self.connections.current.driver;
        let database = self.connections.current.database.trim();

        let table_name = match driver {
            DatabaseDriver::MySql | DatabaseDriver::MariaDb | DatabaseDriver::Sqlite => {
                if database.is_empty() {
                    return Err(String::from("Select a database to copy rows."));
                }
                sql_quote_identifier_path(driver, &[database, table])
            }
            DatabaseDriver::PostgreSql => {
                if let Some((schema, table_name)) = table.split_once('.') {
                    let schema = schema.trim();
                    let table_name = table_name.trim();
                    if schema.is_empty() || table_name.is_empty() {
                        sql_quote_identifier(driver, table.trim())
                    } else {
                        sql_quote_identifier_path(driver, &[schema, table_name])
                    }
                } else {
                    sql_quote_identifier(driver, table.trim())
                }
            }
        };

        let columns_list = results
            .columns
            .iter()
            .map(|name| sql_quote_identifier(driver, name))
            .collect::<Vec<_>>()
            .join(", ");

        let column_count = results.columns.len();
        let mut output = String::new();
        output.push_str("INSERT INTO ");
        output.push_str(&table_name);
        output.push_str(" (");
        output.push_str(&columns_list);
        output.push_str(") VALUES ");

        for row_index in start..=end {
            let Some(row) = results.rows.get(row_index) else {
                break;
            };
            output.push('(');
            for column_index in 0..column_count {
                if column_index > 0 {
                    output.push_str(", ");
                }
                let value = row.get(column_index).map(String::as_str).unwrap_or("NULL");
                let kind = results
                    .column_kinds
                    .get(column_index)
                    .copied()
                    .unwrap_or(ColumnKind::Unknown);
                output.push_str(&sql_literal_from_value_for_driver(value, kind, driver));
            }
            output.push(')');
            if row_index != end {
                output.push_str(",\n");
            } else {
                output.push(';');
            }
        }

        Ok(output)
    }

    pub(crate) fn clear_row_selection(&mut self) {
        self.workspace.results.selected_rows = None;
        self.workspace.results.row_drag_anchor = None;
        self.workspace.results.row_drag_active = false;
        self.workspace.results.row_context_menu = None;
        self.workspace.results.cursor = None;
    }

    pub(crate) fn visible_row_range(&self, row_count: usize) -> (usize, usize, f32, f32) {
        if row_count == 0 {
            return (0, 0, 0.0, 0.0);
        }

        let (offset_y, viewport_height) =
            if let Some(viewport) = self.workspace.results.vertical_viewport {
                (
                    viewport.absolute_offset().y,
                    viewport.bounds().height.max(1.0),
                )
            } else {
                (0.0, self.default_results_viewport_height())
            };

        let row_height = self.result_row_height();
        let mut first_visible = (offset_y / row_height).floor() as usize;
        if first_visible >= row_count {
            first_visible = row_count.saturating_sub(1);
        }

        let visible_rows = ((viewport_height / row_height).ceil() as usize).max(1);
        let start = first_visible.saturating_sub(RESULT_ROW_OVERSCAN);
        let end = (first_visible + visible_rows + RESULT_ROW_OVERSCAN).min(row_count);
        let top_spacer = row_height * start as f32;
        let bottom_spacer = row_height * (row_count - end) as f32;

        (start, end, top_spacer, bottom_spacer)
    }

    pub(crate) fn visible_column_range(
        &self,
        layout: LayoutMode,
        column_count: usize,
    ) -> (usize, usize, f32, f32) {
        if column_count == 0 {
            return (0, 0, 0.0, 0.0);
        }

        let (offset_x, viewport_width) =
            if let Some(viewport) = self.workspace.results.horizontal_viewport {
                (
                    viewport.absolute_offset().x.max(0.0),
                    viewport.bounds().width.max(1.0),
                )
            } else {
                (0.0, self.default_results_viewport_width())
            };

        let mut data_offset = (offset_x - ROW_HEADER_WIDTH).max(0.0);
        let data_viewport_width = viewport_width.max(1.0);

        let mut first_visible = 0usize;
        while first_visible < column_count {
            let width = self.display_column_width(layout, first_visible);
            if data_offset < width {
                break;
            }
            data_offset -= width;
            first_visible += 1;
        }

        if first_visible >= column_count {
            first_visible = column_count.saturating_sub(1);
        }

        let mut visible_span = 0.0;
        let mut last_visible_exclusive = first_visible;
        while last_visible_exclusive < column_count && visible_span < data_viewport_width {
            visible_span += self.display_column_width(layout, last_visible_exclusive);
            last_visible_exclusive += 1;
        }

        let start = first_visible.saturating_sub(RESULT_COLUMN_OVERSCAN);
        let end = (last_visible_exclusive + RESULT_COLUMN_OVERSCAN).min(column_count);

        let mut left_spacer = 0.0;
        for index in 0..start {
            left_spacer += self.display_column_width(layout, index);
        }

        let mut visible_width = 0.0;
        for index in start..end {
            visible_width += self.display_column_width(layout, index);
        }

        let mut total_width = 0.0;
        for index in 0..column_count {
            total_width += self.display_column_width(layout, index);
        }

        let right_spacer = (total_width - left_spacer - visible_width).max(0.0);

        (start, end, left_spacer, right_spacer)
    }

    pub(crate) fn column_bounds_x(&self, layout: LayoutMode, column: usize) -> Option<(f32, f32)> {
        let results = self.workspace.results.current.as_ref()?;
        if column >= results.columns.len() {
            return None;
        }

        let mut start_x = ROW_HEADER_WIDTH;
        for index in 0..column {
            start_x += self.display_column_width(layout, index);
        }
        let width = self.display_column_width(layout, column);

        Some((start_x, start_x + width))
    }

    pub(crate) fn scroll_selected_cell_into_view(&self) -> Task<Message> {
        let Some((row, column)) = self.workspace.results.selected_cell else {
            return Task::none();
        };

        let mut tasks = Vec::new();
        let layout = self.shell.layout_mode;

        if let (Some(viewport), Some((cell_start, cell_end))) = (
            self.workspace.results.horizontal_viewport,
            self.column_bounds_x(layout, column),
        ) {
            let offset_x = viewport.absolute_offset().x;
            let viewport_width = viewport.bounds().width;
            let margin = 12.0_f32;

            let target_x = if cell_start < offset_x + margin {
                Some((cell_start - margin).max(0.0))
            } else if cell_end > offset_x + viewport_width - margin {
                Some((cell_end + margin - viewport_width).max(0.0))
            } else {
                None
            };

            if let Some(target_x) = target_x {
                let max_offset = (viewport.content_bounds().width - viewport_width).max(0.0);
                let clamped = target_x.min(max_offset);
                tasks.push(iced::widget::operation::scroll_to(
                    results_vertical_scroll_id(),
                    scrollable::AbsoluteOffset {
                        x: Some(clamped),
                        y: None,
                    },
                ));
            }
        }

        if let Some(viewport) = self.workspace.results.vertical_viewport {
            let offset_y = viewport.absolute_offset().y;
            let viewport_height = viewport.bounds().height;
            let row_height = self.result_row_height();
            let row_start = row as f32 * row_height;
            let row_end = row_start + row_height;
            let margin = row_height;

            let target_y = if row_start < offset_y + margin {
                Some((row_start - margin).max(0.0))
            } else if row_end > offset_y + viewport_height - margin {
                Some((row_end + margin - viewport_height).max(0.0))
            } else {
                None
            };

            if let Some(target_y) = target_y {
                let max_offset = (viewport.content_bounds().height - viewport_height).max(0.0);
                let clamped = target_y.min(max_offset);
                tasks.push(iced::widget::operation::scroll_to(
                    results_vertical_scroll_id(),
                    scrollable::AbsoluteOffset {
                        x: None,
                        y: Some(clamped),
                    },
                ));
            }
        }

        if tasks.is_empty() {
            Task::none()
        } else {
            Task::batch(tasks)
        }
    }

    pub(crate) fn move_selected_cell(&mut self, delta_row: i32, delta_col: i32) -> bool {
        let Some((rows, columns)) = self.results_dimensions() else {
            return false;
        };

        let Some((row, column)) = self.workspace.results.selected_cell else {
            return false;
        };
        let max_row = rows.saturating_sub(1) as i32;
        let max_column = columns.saturating_sub(1) as i32;

        let new_row = (row as i32 + delta_row).clamp(0, max_row) as usize;
        let new_column = (column as i32 + delta_col).clamp(0, max_column) as usize;

        if self.workspace.results.selected_cell == Some((new_row, new_column)) {
            return false;
        }

        self.workspace.results.selected_cell = Some((new_row, new_column));
        self.workspace.results.selected_rows = Some((new_row, new_row));
        self.workspace.results.row_drag_anchor = Some(new_row);
        self.workspace.results.row_drag_active = false;
        self.workspace.results.row_context_menu = None;
        true
    }

    pub(crate) fn begin_cell_edit(&mut self, row: usize, column: usize) -> Task<Message> {
        if self.workspace.query.running || !self.is_editable_query_active() {
            return Task::none();
        }

        let Some(results) = &self.workspace.results.current else {
            return Task::none();
        };

        if row >= results.rows.len() || column >= results.columns.len() {
            return Task::none();
        }

        let kind = results
            .column_kinds
            .get(column)
            .copied()
            .unwrap_or(ColumnKind::Unknown);
        let value = results
            .rows
            .get(row)
            .and_then(|row| row.get(column))
            .map(String::as_str)
            .unwrap_or("");
        if !self.cell_is_editable(kind, value) {
            return Task::none();
        }

        self.workspace.results.editing_cell = Some((row, column));
        self.workspace.results.selected_cell = Some((row, column));
        self.workspace.results.selected_rows = Some((row, row));
        self.workspace.results.row_drag_anchor = Some(row);
        self.workspace.results.row_drag_active = false;
        self.workspace.results.row_context_menu = None;
        self.workspace.results.apply_error = None;
        self.workspace.results.apply_message = None;

        iced::widget::operation::focus(cell_input_id(row, column))
    }

    pub(crate) fn clear_selected_cell_if_nullable(&mut self) {
        if !self.is_editable_query_active() {
            return;
        }

        let Some((row, column)) = self.workspace.results.selected_cell else {
            return;
        };

        let Some(results) = &self.workspace.results.current else {
            return;
        };

        if row >= results.rows.len() || column >= results.columns.len() {
            return;
        }

        let is_nullable = results
            .column_nullable
            .get(column)
            .copied()
            .unwrap_or(false);
        if !is_nullable {
            return;
        }

        self.apply_cell_edit(row, column, String::from("NULL"));
    }

    pub(crate) fn apply_cell_edit(&mut self, row: usize, column: usize, value: String) {
        let original_value =
            if let Some(original_row) = self.workspace.results.original_row_snapshots.get(&row) {
                original_row.get(column).cloned()
            } else {
                self.workspace.results.current.as_ref().and_then(|results| {
                    results
                        .rows
                        .get(row)
                        .and_then(|row| row.get(column))
                        .cloned()
                })
            };
        let Some(original_value) = original_value else {
            return;
        };

        if !self
            .workspace
            .results
            .original_row_snapshots
            .contains_key(&row)
            && let Some(row_snapshot) = self
                .workspace
                .results
                .current
                .as_ref()
                .and_then(|results| results.rows.get(row).cloned())
        {
            self.workspace
                .results
                .original_row_snapshots
                .insert(row, row_snapshot);
        }

        self.detach_active_results_from_cache();

        if let Some(results) = &mut self.workspace.results.current {
            let results = Arc::make_mut(results);
            if let Some(row_values) = results.rows.get_mut(row)
                && let Some(cell) = row_values.get_mut(column)
            {
                *cell = value.clone();
                if value == original_value {
                    self.workspace.results.pending_edits.remove(&(row, column));
                    if !self.has_pending_edits_for_row(row) {
                        self.workspace.results.original_row_snapshots.remove(&row);
                    }
                } else {
                    self.workspace
                        .results
                        .pending_edits
                        .insert((row, column), value);
                }
                self.workspace.results.apply_message = None;
                self.workspace.results.apply_error = None;
            }
        }
    }

    pub(crate) fn should_open_text_modal(&self, row: usize, column: usize) -> bool {
        let Some(results) = &self.workspace.results.current else {
            return false;
        };
        let value = results
            .rows
            .get(row)
            .and_then(|row| row.get(column))
            .map(String::as_str)
            .unwrap_or("");
        let kind = results
            .column_kinds
            .get(column)
            .copied()
            .unwrap_or(ColumnKind::Unknown);
        self.should_open_text_modal_value(value, kind)
    }

    pub(crate) fn should_open_text_modal_value(&self, value: &str, kind: ColumnKind) -> bool {
        if is_null_value(value) {
            return false;
        }
        let text_like = match kind {
            ColumnKind::Text => true,
            ColumnKind::Binary => !is_binary_preview_value(value),
            ColumnKind::Unknown => true,
            _ => false,
        };
        if !text_like {
            return false;
        }
        let is_long =
            value.contains('\n') || value.contains('\r') || value.len() > READONLY_CELL_CHAR_LIMIT;
        is_long || looks_like_json(value)
    }

    pub(crate) fn open_text_modal(&mut self, row: usize, column: usize) -> Task<Message> {
        let Some(results) = &self.workspace.results.current else {
            return Task::none();
        };
        if row >= results.rows.len() || column >= results.columns.len() {
            return Task::none();
        }
        let value = results
            .rows
            .get(row)
            .and_then(|row| row.get(column))
            .map(String::as_str)
            .unwrap_or("");
        let kind = results
            .column_kinds
            .get(column)
            .copied()
            .unwrap_or(ColumnKind::Unknown);
        if !self.should_open_text_modal_value(value, kind) {
            return Task::none();
        }
        let was_null = is_null_value(value);
        let content_value = if was_null { "" } else { value };
        self.workspace.results.text_modal_open = true;
        self.workspace.results.text_modal_cell = Some((row, column));
        self.workspace.results.text_modal_content = text_editor::Content::with_text(content_value);
        self.workspace.results.text_modal_original = content_value.to_string();
        self.workspace.results.text_modal_original_was_null = was_null;
        self.workspace.results.text_modal_is_json = is_valid_json(content_value);
        self.workspace.results.editing_cell = None;
        self.workspace.results.selected_cell = Some((row, column));
        self.workspace.results.apply_error = None;
        self.workspace.results.apply_message = None;
        Task::none()
    }

    pub(crate) fn text_modal_can_save(&self) -> bool {
        if !self.is_editable_query_active() {
            return false;
        }
        let Some(results) = &self.workspace.results.current else {
            return false;
        };
        let Some((row, column)) = self.workspace.results.text_modal_cell else {
            return false;
        };
        let value = results
            .rows
            .get(row)
            .and_then(|row| row.get(column))
            .map(String::as_str)
            .unwrap_or("");
        let kind = results
            .column_kinds
            .get(column)
            .copied()
            .unwrap_or(ColumnKind::Unknown);
        self.cell_is_editable(kind, value)
    }

    pub(crate) fn save_text_modal(&mut self) {
        if !self.text_modal_can_save() {
            return;
        }
        let Some((row, column)) = self.workspace.results.text_modal_cell else {
            return;
        };
        let content = self.workspace.results.text_modal_content.text();
        if content == self.workspace.results.text_modal_original {
            self.close_text_modal();
            return;
        }
        let mut value = content.clone();
        if value.trim().is_empty() && self.workspace.results.text_modal_original_was_null {
            value = String::from("NULL");
        }
        self.apply_cell_edit(row, column, value);
        self.close_text_modal();
    }

    pub(crate) fn close_text_modal(&mut self) {
        self.workspace.results.text_modal_open = false;
        self.workspace.results.text_modal_cell = None;
        self.workspace.results.text_modal_content = text_editor::Content::with_text("");
        self.workspace.results.text_modal_original.clear();
        self.workspace.results.text_modal_original_was_null = false;
        self.workspace.results.text_modal_is_json = false;
    }

    pub(crate) fn cell_is_editable(&self, kind: ColumnKind, value: &str) -> bool {
        match kind {
            ColumnKind::Binary => !is_binary_preview_value(value),
            _ => kind.is_editable(),
        }
    }

    pub(crate) fn display_column_width(&self, layout: LayoutMode, index: usize) -> f32 {
        let raw = self.raw_column_width(layout, index);

        let Some(results) = self.workspace.results.current.as_ref() else {
            return raw;
        };
        let column_count = results.columns.len();
        if column_count == 0 {
            return raw;
        }

        let viewport_width = if let Some(viewport) = self.workspace.results.horizontal_viewport {
            viewport.bounds().width.max(1.0)
        } else {
            self.default_results_viewport_width()
        };

        let row_header = ROW_HEADER_WIDTH;
        let available = (viewport_width - row_header).max(1.0);

        let total: f32 = (0..column_count)
            .map(|i| self.raw_column_width(layout, i))
            .sum();
        if total > 0.0 && total < available {
            raw * (available / total)
        } else {
            raw
        }
    }

    pub(crate) fn raw_column_width(&self, layout: LayoutMode, index: usize) -> f32 {
        let base = self.base_column_width(index);

        let width = match layout {
            LayoutMode::Wide => base,
            LayoutMode::Compact => (base * COLUMN_WIDTH_COMPACT_SCALE).max(COLUMN_WIDTH_MIN),
        };
        let label_width = self
            .workspace
            .results
            .current
            .as_ref()
            .and_then(|results| results.columns.get(index))
            .map(|name| name.chars().count() as f32 * 8.0)
            .unwrap_or_default();
        let type_width = if self.modern()
            || !matches!(
                self.settings.values.result_grid_density,
                crate::ResultGridDensity::Comfortable
            ) {
            0.0
        } else {
            self.scale_f32(86.0)
        };

        width.max((label_width + self.scale_f32(14.0) + type_width).max(COLUMN_WIDTH_MIN))
    }
}
