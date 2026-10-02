use crate::app::features::ai;
use crate::app::features::workspace::tabs::{QueryTab, QueryTabState, TabEntry};
use crate::db::DatabasePool;
use crate::model::connection::ConnectionInfo;
use crate::model::table::{
    PostgresSidebarObject, RelationInfo, SidebarRelationEntry, SidebarTriggerEntry,
    TableCacheEntry, TableFolder, TableRelationFilter, TableSortState, TriggerInfo,
};
use std::collections::{HashMap, HashSet, VecDeque};

pub(crate) struct DatabaseSessionState {
    pub(crate) pool: Option<DatabasePool>,
    pub(crate) query_state: QueryTabState,
    pub(crate) query_tabs: Vec<QueryTab>,
    pub(crate) active_query_tab: Option<usize>,
    pub(crate) tab_strip: Vec<TabEntry>,
    pub(crate) next_query_tab_number: usize,
    pub(crate) open_tables: Vec<String>,
    pub(crate) pinned_table_tabs: HashSet<String>,
    pub(crate) table_pages: HashMap<String, usize>,
    pub(crate) table_cache: HashMap<(String, usize), TableCacheEntry>,
    pub(crate) table_cache_lru: VecDeque<(String, usize)>,
    pub(crate) table_cache_bytes_estimate: usize,
    pub(crate) table_inactive_since: HashMap<String, std::time::Instant>,
    pub(crate) inactive_table_tabs_released: HashSet<String>,
    pub(crate) table_filters: HashMap<String, TableRelationFilter>,
    pub(crate) table_sorts: HashMap<String, TableSortState>,
    pub(crate) selected_postgres_object: Option<PostgresSidebarObject>,
}

pub(crate) struct ConnectionTab {
    pub(crate) id: u64,
    pub(crate) label: String,
    pub(crate) connection: ConnectionInfo,
    pub(crate) snapshot: Option<ConnectionSnapshot>,
}

pub(crate) struct ConnectionSnapshot {
    pub(crate) connection: ConnectionInfo,
    pub(crate) active_session: DatabaseSessionState,
    pub(crate) database_sessions: HashMap<String, DatabaseSessionState>,
    pub(crate) databases: Vec<String>,
    pub(crate) tables: Vec<String>,
    pub(crate) postgres_sidebar_objects: Vec<PostgresSidebarObject>,
    pub(crate) postgres_schema_open: HashMap<String, bool>,
    pub(crate) postgres_schema_kind_open: HashMap<String, bool>,
    pub(crate) sidebar_triggers: Vec<SidebarTriggerEntry>,
    pub(crate) table_triggers_cache: HashMap<String, Vec<TriggerInfo>>,
    pub(crate) sidebar_relations: Vec<SidebarRelationEntry>,
    pub(crate) table_relations_cache: HashMap<String, Vec<RelationInfo>>,
    pub(crate) table_folders: Vec<TableFolder>,
    pub(crate) table_folder_map: HashMap<String, String>,
    pub(crate) table_search: String,
    pub(crate) database_error: Option<String>,
    pub(crate) table_error: Option<String>,
    pub(crate) chat: ai::ChatConnectionState,
}
