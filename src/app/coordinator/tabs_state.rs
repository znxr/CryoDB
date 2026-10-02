use crate::app::core::App;
use crate::app::features::workspace::tabs::{QueryTab, QueryTabState, TabEntry};
use crate::app::message::Message;
use crate::model::connection::StoredConnection;
use crate::ui::ids::table_search_input_id;
use iced::Task;
use std::collections::HashMap;

impl App {
    pub(crate) fn blank_query_state(&self) -> QueryTabState {
        QueryTabState {
            query: Self::new_query_editor("", &self.settings.values, self.settings.theme_choice),
            query_undo_stack: Vec::new(),
            query_redo_stack: Vec::new(),
            query_error: None,
            apply_error: None,
            apply_message: None,
            selected_table: None,
            table_triggers: Vec::new(),
            triggers_table: None,
            selected_trigger: None,
            trigger_edit_name: String::new(),
            trigger_edit_timing: String::new(),
            trigger_edit_event: String::new(),
            trigger_edit_definer_user: String::new(),
            trigger_edit_definer_host: String::new(),
            table_relations: Vec::new(),
            relations_table: None,
            table_query: None,
            editable_query: None,
            query_table: None,
            table_has_next_page: false,
            last_query_was_table: false,
            inactive_results_released: false,
            results: None,
            query_result_sets: Vec::new(),
            active_query_result: 0,
            pending_edits: HashMap::new(),
            original_row_snapshots: HashMap::new(),
            column_widths: Vec::new(),
            column_resize: None,
            results_vertical_viewport: None,
            results_horizontal_viewport: None,
            editing_cell: None,
            selected_cell: None,
            selected_rows: None,
            row_drag_anchor: None,
            row_drag_active: false,
            results_cursor: None,
            row_context_menu: None,
        }
    }

    pub(crate) fn take_query_state(&mut self) -> QueryTabState {
        let query = std::mem::replace(
            &mut self.workspace.query.editor,
            Self::new_query_editor("", &self.settings.values, self.settings.theme_choice),
        );
        let query_undo_stack = std::mem::take(&mut self.workspace.query.undo_stack);
        let query_redo_stack = std::mem::take(&mut self.workspace.query.redo_stack);
        let query_error = self.workspace.query.error.take();
        let apply_error = self.workspace.results.apply_error.take();
        let apply_message = self.workspace.results.apply_message.take();
        let selected_table = self.workspace.selected_table.take();
        let table_triggers = std::mem::take(&mut self.workspace.explorer.table_triggers);
        let triggers_table = self.workspace.explorer.triggers_table.take();
        let selected_trigger = self.workspace.explorer.selected_trigger.take();
        let trigger_edit_name = std::mem::take(&mut self.workspace.explorer.trigger_edit_name);
        let trigger_edit_timing = std::mem::take(&mut self.workspace.explorer.trigger_edit_timing);
        let trigger_edit_event = std::mem::take(&mut self.workspace.explorer.trigger_edit_event);
        let trigger_edit_definer_user =
            std::mem::take(&mut self.workspace.explorer.trigger_edit_definer_user);
        let trigger_edit_definer_host =
            std::mem::take(&mut self.workspace.explorer.trigger_edit_definer_host);
        let table_relations = std::mem::take(&mut self.workspace.explorer.table_relations);
        let relations_table = self.workspace.explorer.relations_table.take();
        let table_query = self.workspace.query.table_query.take();
        let editable_query = self.workspace.query.editable_query.take();
        let query_table = self.workspace.query.table.take();
        let table_has_next_page = self.workspace.query.table_has_next_page;
        let last_query_was_table = self.workspace.query.last_query_was_table;
        let inactive_results_released = self.workspace.results.released;
        let results = self.workspace.results.current.take();
        let query_result_sets = std::mem::take(&mut self.workspace.results.sets);
        let active_query_result = self.workspace.results.active_set;
        let pending_edits = std::mem::take(&mut self.workspace.results.pending_edits);
        let original_row_snapshots =
            std::mem::take(&mut self.workspace.results.original_row_snapshots);
        let column_widths = std::mem::take(&mut self.workspace.results.column_widths);
        let column_resize = self.workspace.results.column_resize.take();
        let results_vertical_viewport = self.workspace.results.vertical_viewport.take();
        let results_horizontal_viewport = self.workspace.results.horizontal_viewport.take();
        let editing_cell = self.workspace.results.editing_cell.take();
        let selected_cell = self.workspace.results.selected_cell.take();
        let selected_rows = self.workspace.results.selected_rows.take();
        let row_drag_anchor = self.workspace.results.row_drag_anchor.take();
        let row_drag_active = self.workspace.results.row_drag_active;
        let results_cursor = self.workspace.results.cursor.take();
        let row_context_menu = self.workspace.results.row_context_menu.take();

        self.workspace.query.table_has_next_page = false;
        self.workspace.query.last_query_was_table = false;
        self.workspace.results.active_set = 0;
        self.workspace.results.row_drag_active = false;

        QueryTabState {
            query,
            query_undo_stack,
            query_redo_stack,
            query_error,
            apply_error,
            apply_message,
            selected_table,
            table_triggers,
            triggers_table,
            selected_trigger,
            trigger_edit_name,
            trigger_edit_timing,
            trigger_edit_event,
            trigger_edit_definer_user,
            trigger_edit_definer_host,
            table_relations,
            relations_table,
            table_query,
            editable_query,
            query_table,
            table_has_next_page,
            last_query_was_table,
            inactive_results_released,
            results,
            query_result_sets,
            active_query_result,
            pending_edits,
            original_row_snapshots,
            column_widths,
            column_resize,
            results_vertical_viewport,
            results_horizontal_viewport,
            editing_cell,
            selected_cell,
            selected_rows,
            row_drag_anchor,
            row_drag_active,
            results_cursor,
            row_context_menu,
        }
    }

    pub(crate) fn apply_query_state(&mut self, state: QueryTabState) {
        self.workspace.query.editor = state.query;
        self.workspace.query.undo_stack = state.query_undo_stack;
        self.workspace.query.redo_stack = state.query_redo_stack;
        self.workspace.query.error = state.query_error;
        self.workspace.results.apply_error = state.apply_error;
        self.workspace.results.apply_message = state.apply_message;
        self.workspace.selected_table = state.selected_table;
        self.workspace.explorer.table_triggers = state.table_triggers;
        self.workspace.explorer.triggers_table = state.triggers_table;
        self.workspace.explorer.selected_trigger = state.selected_trigger;
        self.workspace.explorer.trigger_edit_name = state.trigger_edit_name;
        self.workspace.explorer.trigger_edit_timing = state.trigger_edit_timing;
        self.workspace.explorer.trigger_edit_event = state.trigger_edit_event;
        self.workspace.explorer.trigger_edit_definer_user = state.trigger_edit_definer_user;
        self.workspace.explorer.trigger_edit_definer_host = state.trigger_edit_definer_host;
        self.workspace.explorer.table_relations = state.table_relations;
        self.workspace.explorer.relations_table = state.relations_table;
        self.workspace.query.table_query = state.table_query;
        self.workspace.query.editable_query = state.editable_query;
        self.workspace.query.table = state.query_table;
        self.workspace.query.table_has_next_page = state.table_has_next_page;
        self.workspace.query.last_query_was_table = state.last_query_was_table;
        self.workspace.results.released = state.inactive_results_released;
        self.workspace.results.current = state.results;
        self.workspace.results.sets = state.query_result_sets;
        self.workspace.results.active_set = state.active_query_result;
        self.workspace.results.pending_edits = state.pending_edits;
        self.workspace.results.original_row_snapshots = state.original_row_snapshots;
        self.workspace.results.column_widths = state.column_widths;
        self.workspace.results.column_resize = state.column_resize;
        self.workspace.results.vertical_viewport = state.results_vertical_viewport;
        self.workspace.results.horizontal_viewport = state.results_horizontal_viewport;
        self.workspace.results.editing_cell = state.editing_cell;
        self.workspace.results.selected_cell = state.selected_cell;
        self.workspace.results.selected_rows = state.selected_rows;
        self.workspace.results.row_drag_anchor = state.row_drag_anchor;
        self.workspace.results.row_drag_active = state.row_drag_active;
        self.workspace.results.cursor = state.results_cursor;
        self.workspace.results.row_context_menu = state.row_context_menu;
        self.sync_query_editor_settings();
        self.close_text_modal();
        self.close_query_suggestions();
    }

    pub(crate) fn save_active_query_tab(&mut self) {
        let Some(index) = self.workspace.tabs.active_query_tab else {
            return;
        };
        let state = self.take_query_state();
        if let Some(tab) = self.workspace.tabs.query_tabs.get_mut(index) {
            tab.state = Some(state);
            tab.inactive_since = Some(std::time::Instant::now());
        }
        self.prune_inactive_tab_memory();
    }

    pub(crate) fn clear_tab_drag(&mut self) {
        self.workspace.tabs.clear_tab_drag()
    }

    pub(crate) fn clear_table_tabs(&mut self) {
        self.workspace
            .tabs
            .tab_strip
            .retain(|entry| matches!(entry, TabEntry::Query(_)));
        self.workspace.tabs.pinned_table_tabs.clear();
        self.workspace.table_inactive_since.clear();
        self.workspace.inactive_table_tabs_released.clear();
        self.clear_tab_drag();
    }

    pub(crate) fn database_session_key_for(&self, database: &str) -> String {
        let mut info = self.connections.current.clone();
        info.database = database.to_string();
        StoredConnection::from_info(&info)
            .normalized()
            .secret_lookup_key()
    }

    pub(crate) fn current_database_session_key(&self) -> Option<String> {
        if self.connections.connected {
            Some(self.database_session_key_for(&self.connections.current.database))
        } else {
            None
        }
    }

    pub(crate) fn take_active_database_session(
        &mut self,
    ) -> crate::app::session::DatabaseSessionState {
        let query_state = self.take_query_state();
        crate::app::session::DatabaseSessionState {
            pool: self.connections.pool.clone(),
            query_state,
            query_tabs: std::mem::take(&mut self.workspace.tabs.query_tabs),
            active_query_tab: self.workspace.tabs.active_query_tab.take(),
            tab_strip: std::mem::take(&mut self.workspace.tabs.tab_strip)
                .into_iter()
                .filter(|entry| !matches!(entry, TabEntry::Diagram(_)))
                .collect(),
            next_query_tab_number: self.workspace.tabs.next_query_tab_number,
            open_tables: std::mem::take(&mut self.workspace.tabs.open_tables),
            pinned_table_tabs: std::mem::take(&mut self.workspace.tabs.pinned_table_tabs),
            table_pages: std::mem::take(&mut self.workspace.table_pages),
            table_cache: std::mem::take(&mut self.workspace.table_cache),
            table_cache_lru: std::mem::take(&mut self.workspace.table_cache_lru),
            table_cache_bytes_estimate: std::mem::take(
                &mut self.workspace.table_cache_bytes_estimate,
            ),
            table_inactive_since: std::mem::take(&mut self.workspace.table_inactive_since),
            inactive_table_tabs_released: std::mem::take(
                &mut self.workspace.inactive_table_tabs_released,
            ),
            table_filters: std::mem::take(&mut self.workspace.table_filters),
            table_sorts: std::mem::take(&mut self.workspace.table_sorts),
            selected_postgres_object: self.workspace.explorer.selected_postgres_object.take(),
        }
    }

    pub(crate) fn save_current_database_session(&mut self) {
        let Some(key) = self.current_database_session_key() else {
            return;
        };

        let session = self.take_active_database_session();
        self.connections.database_sessions.insert(key, session);
    }

    pub(crate) fn restore_database_session(
        &mut self,
        session: crate::app::session::DatabaseSessionState,
    ) {
        if let Some(pool) = session.pool {
            self.connections.pool = Some(pool);
        }
        self.workspace.diagram.diagram_tabs.clear();
        self.workspace.diagram.active_diagram_tab = None;
        self.workspace.tabs.query_tabs = session.query_tabs;
        self.workspace.tabs.active_query_tab = session.active_query_tab;
        self.workspace.tabs.tab_strip = session.tab_strip;
        self.workspace.tabs.next_query_tab_number = session.next_query_tab_number;
        self.workspace.tabs.open_tables = session.open_tables;
        self.workspace.tabs.pinned_table_tabs = session.pinned_table_tabs;
        self.workspace.table_pages = session.table_pages;
        self.workspace.table_cache = session.table_cache;
        self.workspace.table_cache_lru = session.table_cache_lru;
        self.workspace.table_cache_bytes_estimate = session.table_cache_bytes_estimate;
        self.workspace.table_inactive_since = session.table_inactive_since;
        self.workspace.inactive_table_tabs_released = session.inactive_table_tabs_released;
        self.workspace.table_filters = session.table_filters;
        self.workspace.table_sorts = session.table_sorts;
        self.workspace.explorer.selected_postgres_object = session.selected_postgres_object;
        if let Some(index) = self.workspace.tabs.active_query_tab
            && let Some(tab) = self.workspace.tabs.query_tabs.get_mut(index)
        {
            tab.inactive_since = None;
        }
        self.workspace.explorer.sidebar_triggers.clear();
        self.workspace.explorer.table_triggers_cache.clear();
        self.workspace.explorer.sidebar_relations.clear();
        self.workspace.explorer.table_relations_cache.clear();
        self.clear_query_suggestion_columns_cache();
        self.workspace.explorer.table_info_sidebar_open = false;
        self.workspace.explorer.table_info_loading = false;
        self.workspace.explorer.table_info_error = None;
        self.workspace.explorer.table_info_table = None;
        self.workspace.explorer.table_info = None;
        self.workspace.explorer.clear_table_info_cache();
        self.apply_query_state(session.query_state);
        self.prune_inactive_tab_memory();
    }

    pub(crate) fn reset_database_workspace(&mut self) {
        self.workspace.tabs.open_tables.clear();
        self.clear_table_tabs();
        self.workspace.table_pages.clear();
        self.clear_table_cache();
        self.workspace.table_filters.clear();
        self.workspace.table_sorts.clear();
        self.workspace.tabs.pinned_table_tabs.clear();
        self.workspace.selected_table = None;
        self.workspace.explorer.selected_postgres_object = None;
        self.workspace.explorer.table_triggers.clear();
        self.workspace.explorer.triggers_table = None;
        self.workspace.explorer.selected_trigger = None;
        self.workspace.explorer.table_relations.clear();
        self.workspace.explorer.relations_table = None;
        self.workspace.explorer.clear_table_metadata_cache();
        self.clear_query_suggestion_columns_cache();
        self.workspace.query.table_query = None;
        self.workspace.query.editable_query = None;
        self.workspace.query.table = None;
        self.workspace.explorer.table_folders.clear();
        self.workspace.explorer.table_folder_map.clear();
        self.clear_pending_edits_state();
        self.workspace.results.editing_cell = None;
        self.workspace.results.selected_cell = None;
        self.clear_row_selection();
        self.close_text_modal();
        self.workspace.results.current = None;
        self.workspace.results.sets.clear();
        self.workspace.results.active_set = 0;
        self.workspace.results.sets_pending = false;
        self.workspace.results.vertical_viewport = None;
        self.workspace.results.horizontal_viewport = None;
        self.workspace.results.column_widths.clear();
        self.workspace.results.column_resize = None;
        self.workspace.query.table_has_next_page = false;
        self.workspace.query.last_query_was_table = false;
        self.workspace.explorer.table_info_sidebar_open = false;
        self.workspace.explorer.table_info_loading = false;
        self.workspace.explorer.table_info_error = None;
        self.workspace.explorer.table_info_table = None;
        self.workspace.explorer.table_info = None;
        self.workspace.explorer.clear_table_info_cache();
        self.workspace.query.error = None;
        self.workspace.results.apply_error = None;
        self.workspace.results.apply_message = None;
        self.reset_query_tabs_to_welcome();
    }

    pub(crate) fn ensure_table_tab(&mut self, table: &str) {
        if !self.settings.values.tabs_enabled {
            return;
        }
        if !self
            .workspace
            .tabs
            .open_tables
            .iter()
            .any(|tab| tab == table)
        {
            self.workspace.tabs.open_tables.push(table.to_string());
        }
        if !self
            .workspace
            .tabs
            .tab_strip
            .iter()
            .any(|entry| matches!(entry, TabEntry::Table(name) if name == table))
        {
            self.workspace
                .tabs
                .tab_strip
                .push(TabEntry::Table(table.to_string()));
        }
    }
    pub(crate) fn adjust_query_tab_indices(&mut self, removed_index: usize) {
        self.workspace.tabs.adjust_query_tab_indices(removed_index)
    }

    pub(crate) fn activate_tab_entry(&mut self, entry: TabEntry) -> Task<Message> {
        match entry {
            TabEntry::Diagram(index) => self.activate_diagram_tab(index),
            TabEntry::Query(index) => {
                if self.workspace.query.running || self.workspace.results.applying_changes {
                    Task::none()
                } else {
                    Task::batch(vec![
                        self.switch_to_query_tab(index),
                        self.scroll_tab_entry_into_view(&TabEntry::Query(index)),
                    ])
                }
            }
            TabEntry::Table(table) => {
                if !self.settings.values.tabs_enabled {
                    return Task::none();
                }
                let page = self.workspace.table_pages.get(&table).copied().unwrap_or(0);
                Task::batch(vec![
                    self.select_table(table.clone(), page, false),
                    self.scroll_tab_entry_into_view(&TabEntry::Table(table)),
                ])
            }
        }
    }

    pub(crate) fn scroll_tab_entry_into_view(&self, active: &TabEntry) -> Task<Message> {
        self.tabs_view()
            .scroll_tab_entry_into_view(active)
            .map(|value| Message::Workspace(crate::app::features::workspace::Message::Tabs(value)))
    }

    pub(crate) fn open_new_query_tab(&mut self, state: QueryTabState) {
        self.close_text_modal();
        self.close_query_suggestions();
        self.save_active_query_tab();
        let number = self.workspace.tabs.next_query_tab_number;
        self.workspace.tabs.next_query_tab_number =
            self.workspace.tabs.next_query_tab_number.saturating_add(1);
        self.workspace.tabs.query_tabs.push(QueryTab {
            title: format!("Query #{}", number),
            renamed: false,
            pinned: false,
            inactive_since: None,
            state: None,
        });
        let query_index = self.workspace.tabs.query_tabs.len() - 1;
        self.workspace
            .tabs
            .tab_strip
            .push(TabEntry::Query(query_index));
        self.workspace.tabs.active_query_tab = Some(query_index);
        self.apply_query_state(state);
        self.prune_inactive_tab_memory();
    }

    pub(crate) fn reset_query_tabs_to_welcome(&mut self) {
        self.workspace.diagram.diagram_tabs.clear();
        self.workspace.diagram.active_diagram_tab = None;
        self.workspace.diagram.next_diagram_tab_number = 1;
        self.workspace.tabs.query_tabs.clear();
        self.workspace.tabs.query_tabs.push(QueryTab {
            title: String::from("Welcome"),
            renamed: false,
            pinned: false,
            inactive_since: None,
            state: None,
        });
        self.workspace.tabs.active_query_tab = Some(0);
        self.workspace.tabs.tab_strip.clear();
        self.workspace.tabs.tab_strip.push(TabEntry::Query(0));
        self.workspace.tabs.next_query_tab_number = 2;
        self.set_query_text(self.connections.current.driver.default_query());
    }

    pub(crate) fn ensure_query_tab_for_run(&mut self) {
        if self.workspace.tabs.active_query_tab.is_some() {
            return;
        }
        self.workspace.selected_table = None;
        let number = self.workspace.tabs.next_query_tab_number;
        self.workspace.tabs.next_query_tab_number =
            self.workspace.tabs.next_query_tab_number.saturating_add(1);
        self.workspace.tabs.query_tabs.push(QueryTab {
            title: format!("Query #{}", number),
            renamed: false,
            pinned: false,
            inactive_since: None,
            state: None,
        });
        let query_index = self.workspace.tabs.query_tabs.len() - 1;
        self.workspace
            .tabs
            .tab_strip
            .push(TabEntry::Query(query_index));
        self.workspace.tabs.active_query_tab = Some(query_index);
    }
    pub(crate) fn switch_to_query_tab(&mut self, index: usize) -> Task<Message> {
        if self.workspace.query.running || self.workspace.results.applying_changes {
            return Task::none();
        }
        self.deactivate_diagram();
        if index >= self.workspace.tabs.query_tabs.len() {
            return Task::none();
        }
        if self.workspace.tabs.active_query_tab == Some(index) {
            return Task::none();
        }
        self.close_text_modal();
        self.close_query_suggestions();
        self.save_active_query_tab();
        let Some(tab) = self.workspace.tabs.query_tabs.get_mut(index) else {
            return Task::none();
        };
        tab.inactive_since = None;
        let Some(state) = tab.state.take() else {
            return Task::none();
        };
        self.apply_query_state(state);
        self.workspace.tabs.active_query_tab = Some(index);
        self.prune_inactive_tab_memory();
        if self.workspace.explorer.table_info_sidebar_open {
            if let Some(table) = self.current_table_for_info() {
                return self.request_table_info(table);
            }
            self.workspace.explorer.table_info_loading = false;
            self.workspace.explorer.table_info_table = None;
            self.workspace.explorer.table_info = None;
            self.workspace.explorer.table_info_error = Some(String::from("Select a table first."));
        }
        Task::none()
    }

    pub(crate) fn close_query_tab(&mut self, index: usize) -> Task<Message> {
        if self.workspace.query.running || self.workspace.results.applying_changes {
            return Task::none();
        }
        if index >= self.workspace.tabs.query_tabs.len() {
            return Task::none();
        }
        if self
            .workspace
            .tabs
            .query_tabs
            .get(index)
            .is_some_and(|tab| tab.pinned)
        {
            return Task::none();
        }
        self.clear_tab_drag();
        self.workspace.tabs.tab_context_menu = None;
        let strip_index =
            self.workspace.tabs.tab_strip.iter().position(
                |entry| matches!(entry, TabEntry::Query(tab_index) if *tab_index == index),
            );
        let was_active = self.workspace.tabs.active_query_tab == Some(index);
        if was_active {
            self.close_text_modal();
            self.close_query_suggestions();
            self.workspace.tabs.active_query_tab = None;
        }
        self.workspace.tabs.query_tabs.remove(index);
        if self.workspace.tabs.query_tabs.is_empty() {
            self.workspace.tabs.next_query_tab_number = 1;
        }
        if let Some(active) = self.workspace.tabs.active_query_tab
            && index < active
        {
            self.workspace.tabs.active_query_tab = Some(active - 1);
        }
        if let Some(strip_index) = strip_index
            && strip_index < self.workspace.tabs.tab_strip.len()
        {
            self.workspace.tabs.tab_strip.remove(strip_index);
        }
        self.adjust_query_tab_indices(index);
        if was_active {
            if let Some(strip_index) = strip_index {
                let next_entry = self
                    .workspace
                    .tabs
                    .tab_strip
                    .get(strip_index)
                    .cloned()
                    .or_else(|| {
                        if strip_index > 0 {
                            self.workspace.tabs.tab_strip.get(strip_index - 1).cloned()
                        } else {
                            None
                        }
                    });
                if let Some(entry) = next_entry {
                    self.maybe_trim_process_memory();
                    return self.activate_tab_entry(entry);
                }
            }
            let state = self.blank_query_state();
            self.apply_query_state(state);
        }
        self.maybe_trim_process_memory();
        self.ensure_workspace_tab()
    }

    pub(crate) fn is_tab_entry_pinned(&self, entry: &TabEntry) -> bool {
        match entry {
            TabEntry::Query(index) => self
                .workspace
                .tabs
                .query_tabs
                .get(*index)
                .is_some_and(|tab| tab.pinned),
            TabEntry::Table(table) => self.workspace.tabs.pinned_table_tabs.contains(table),
            TabEntry::Diagram(index) => self
                .workspace
                .diagram
                .diagram_tabs
                .get(*index)
                .is_some_and(|tab| tab.pinned),
        }
    }

    pub(crate) fn set_tab_entry_pinned(&mut self, entry: &TabEntry, pinned: bool) {
        match entry {
            TabEntry::Query(index) => {
                if let Some(tab) = self.workspace.tabs.query_tabs.get_mut(*index) {
                    tab.pinned = pinned;
                }
                if !pinned {
                    self.prune_inactive_tab_memory();
                }
            }
            TabEntry::Table(table) => {
                if pinned {
                    self.workspace.tabs.pinned_table_tabs.insert(table.clone());
                } else {
                    self.workspace.tabs.pinned_table_tabs.remove(table);
                }
            }
            TabEntry::Diagram(index) => {
                if let Some(tab) = self.workspace.diagram.diagram_tabs.get_mut(*index) {
                    tab.pinned = pinned;
                }
            }
        }
    }

    pub(crate) fn close_table_tab(&mut self, table: String) -> Task<Message> {
        if !self.settings.values.tabs_enabled {
            return Task::none();
        }
        if self.workspace.query.running
            && self
                .workspace
                .selected_table
                .as_deref()
                .is_some_and(|active| active == table.as_str())
        {
            return Task::none();
        }
        if self.workspace.tabs.pinned_table_tabs.contains(&table) {
            return Task::none();
        }
        self.clear_tab_drag();
        self.workspace.tabs.tab_context_menu = None;
        let strip_index = self
            .workspace
            .tabs
            .tab_strip
            .iter()
            .position(|entry| matches!(entry, TabEntry::Table(name) if name == &table));

        let Some(index) = self
            .workspace
            .tabs
            .open_tables
            .iter()
            .position(|t| t == &table)
        else {
            return Task::none();
        };

        self.workspace.tabs.open_tables.remove(index);
        self.workspace.table_pages.remove(&table);
        self.remove_table_cache_for_table(&table);
        self.workspace.table_filters.remove(&table);
        self.workspace.table_sorts.remove(&table);
        self.workspace.tabs.pinned_table_tabs.remove(&table);
        self.workspace
            .explorer
            .remove_table_info_cache_entry(&table);
        if let Some(active) = self.workspace.explorer.table_info_table.as_deref()
            && active == table
        {
            self.workspace.explorer.table_info_table = None;
            self.workspace.explorer.table_info = None;
            self.workspace.explorer.table_info_error = None;
            self.workspace.explorer.table_info_loading = false;
            self.workspace.explorer.table_info_sidebar_open = false;
        }
        if let Some(strip_index) = strip_index
            && strip_index < self.workspace.tabs.tab_strip.len()
        {
            self.workspace.tabs.tab_strip.remove(strip_index);
        }

        if self
            .workspace
            .selected_table
            .as_deref()
            .is_some_and(|active| active == table.as_str())
        {
            self.workspace.selected_table = None;
            self.workspace.explorer.table_triggers.clear();
            self.workspace.explorer.triggers_table = None;
            self.workspace.explorer.selected_trigger = None;
            self.workspace.explorer.table_relations.clear();
            self.workspace.explorer.relations_table = None;
            self.workspace.query.table_query = None;
            self.workspace.query.editable_query = None;
            self.workspace.query.table = None;
            self.workspace.query.table_has_next_page = false;
            self.workspace.query.last_query_was_table = false;
            self.workspace.query.error = None;
            self.workspace.results.apply_error = None;
            self.workspace.results.apply_message = None;
            self.clear_pending_edits_state();
            self.workspace.results.editing_cell = None;
            self.workspace.results.selected_cell = None;
            self.clear_row_selection();
            self.workspace.results.current = None;
            self.workspace.results.column_widths.clear();
            self.workspace.results.column_resize = None;

            if let Some(strip_index) = strip_index {
                let next_entry = self
                    .workspace
                    .tabs
                    .tab_strip
                    .get(strip_index)
                    .cloned()
                    .or_else(|| {
                        if strip_index > 0 {
                            self.workspace.tabs.tab_strip.get(strip_index - 1).cloned()
                        } else {
                            None
                        }
                    });
                if let Some(entry) = next_entry {
                    self.maybe_trim_process_memory();
                    return self.activate_tab_entry(entry);
                }
            }
        }

        self.maybe_trim_process_memory();
        self.ensure_workspace_tab()
    }

    pub(crate) fn close_tabs_from_strip_indices(
        &mut self,
        mut indices: Vec<usize>,
    ) -> Task<Message> {
        if indices.is_empty() {
            return Task::none();
        }
        indices.sort_unstable();
        indices.dedup();
        let mut tasks = Vec::new();
        for strip_index in indices.into_iter().rev() {
            let Some(entry) = self.workspace.tabs.tab_strip.get(strip_index).cloned() else {
                continue;
            };
            if self.is_tab_entry_pinned(&entry) {
                continue;
            }
            let task = match entry {
                TabEntry::Query(index) => self.close_query_tab(index),
                TabEntry::Table(table) => self.close_table_tab(table),
                TabEntry::Diagram(index) => self.close_schema_diagram(index),
            };
            tasks.push(task);
        }
        if tasks.is_empty() {
            Task::none()
        } else {
            Task::batch(tasks)
        }
    }

    pub(crate) fn visible_switchable_tabs(&self) -> Vec<TabEntry> {
        self.workspace
            .tabs
            .tab_strip
            .iter()
            .filter(|entry| {
                matches!(entry, TabEntry::Query(_) | TabEntry::Diagram(_))
                    || (self.settings.values.tabs_enabled && matches!(entry, TabEntry::Table(_)))
            })
            .cloned()
            .collect()
    }

    pub(crate) fn active_tab_entry(&self) -> Option<TabEntry> {
        if let Some(index) = self.workspace.diagram.active_diagram_tab {
            return Some(TabEntry::Diagram(index));
        }
        if let Some(index) = self.workspace.tabs.active_query_tab {
            return Some(TabEntry::Query(index));
        }
        self.workspace.selected_table.clone().map(TabEntry::Table)
    }

    pub(crate) fn cycle_visible_tab(&mut self, reverse: bool) -> Task<Message> {
        let tabs = self.visible_switchable_tabs();
        if tabs.len() < 2 {
            return Task::none();
        }

        let current_index = self
            .active_tab_entry()
            .and_then(|active| tabs.iter().position(|entry| entry == &active))
            .unwrap_or(0);
        let next_index = if reverse {
            (current_index + tabs.len() - 1) % tabs.len()
        } else {
            (current_index + 1) % tabs.len()
        };

        self.activate_tab_entry(tabs[next_index].clone())
    }

    pub(crate) fn close_active_tab(&mut self) -> Task<Message> {
        if let Some(index) = self.workspace.diagram.active_diagram_tab {
            return self.close_schema_diagram(index);
        }
        if let Some(index) = self.workspace.tabs.active_query_tab {
            self.close_query_tab(index)
        } else if let Some(table) = self.workspace.selected_table.clone() {
            self.close_table_tab(table)
        } else {
            Task::none()
        }
    }

    pub(crate) fn toggle_active_tab_pin(&mut self) {
        let Some(entry) = self.active_tab_entry() else {
            return;
        };
        let is_pinned = self.is_tab_entry_pinned(&entry);
        self.set_tab_entry_pinned(&entry, !is_pinned);
    }

    pub(crate) fn tab_entry_title(&self, entry: &TabEntry) -> String {
        match entry {
            TabEntry::Query(index) => self
                .workspace
                .tabs
                .query_tabs
                .get(*index)
                .map(|tab| tab.display_title().to_string())
                .unwrap_or_else(|| format!("Query #{}", index.saturating_add(1))),
            TabEntry::Table(name) => name.clone(),
            TabEntry::Diagram(index) => self.diagram_tab_title(*index),
        }
    }
    pub(crate) fn focus_query_editor_input(&mut self) -> Task<Message> {
        if !self.connections.connected {
            return Task::none();
        }
        self.shell.query_editor_hidden = false;
        let clamped_editor_ratio = self.clamp_editor_ratio(self.shell.editor_ratio);
        self.shell.editor_ratio = clamped_editor_ratio;
        self.shell
            .panes
            .resize(self.shell.editor_split, clamped_editor_ratio);
        Task::done(Message::Workspace(
            crate::app::features::workspace::Message::Query(
                crate::app::features::workspace::query::Message::FocusQueryEditorNow,
            ),
        ))
    }

    pub(crate) fn focus_table_results(&mut self) -> Task<Message> {
        self.workspace.results.editing_cell = None;
        self.workspace.results.row_context_menu = None;
        let Some((rows, columns)) = self.results_dimensions() else {
            return Task::none();
        };
        if rows == 0 || columns == 0 {
            return Task::none();
        }
        self.workspace.results.selected_cell = Some((0, 0));
        self.workspace.results.selected_rows = Some((0, 0));
        self.workspace.results.row_drag_anchor = Some(0);
        self.workspace.results.row_drag_active = false;
        self.scroll_selected_cell_into_view()
    }

    pub(crate) fn focus_sidebar_input(&mut self) -> Task<Message> {
        if !self.connections.connected || self.is_sidebar_hidden() {
            return Task::none();
        }
        iced::widget::operation::focus(table_search_input_id())
    }
}
