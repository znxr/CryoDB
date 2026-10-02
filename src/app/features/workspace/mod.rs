use crate::model::table::{TableCacheEntry, TableRelationFilter, TableSortState};
use std::collections::{HashMap, HashSet, VecDeque};

pub(crate) mod diagram;
pub(crate) mod tabs;
pub(crate) mod types;

pub(crate) mod explorer;
pub(crate) mod query;
pub(crate) mod results;

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone)]
pub(crate) enum Message {
    Explorer(explorer::Message),
    Query(query::Message),
    Results(results::Message),
    Tabs(tabs::Message),
    Diagram(diagram::Message),
    OpenTableDdl,
    OpenTableInfoSidebar,
    ToggleActiveTabPin,
    InactiveTabPruneTick,
}

pub(crate) struct State {
    pub(crate) explorer: explorer::State,
    pub(crate) tabs: tabs::State,
    pub(crate) diagram: diagram::State,
    pub(crate) query: query::State,
    pub(crate) results: results::State,
    pub(crate) table_pages: HashMap<String, usize>,
    pub(crate) table_cache: HashMap<(String, usize), TableCacheEntry>,
    pub(crate) table_cache_lru: VecDeque<(String, usize)>,
    pub(crate) table_cache_bytes_estimate: usize,
    pub(crate) table_inactive_since: HashMap<String, std::time::Instant>,
    pub(crate) inactive_table_tabs_released: HashSet<String>,
    pub(crate) table_filters: HashMap<String, TableRelationFilter>,
    pub(crate) table_sorts: HashMap<String, TableSortState>,
    pub(crate) selected_table: Option<String>,
}

pub(crate) mod tabs_view;
impl State {
    pub(crate) fn new(
        folder_store: crate::model::table::FolderStore,
        folder_store_error: Option<String>,
        query: iced_code_editor::CodeEditor,
    ) -> Self {
        Self {
            explorer: explorer::State::new(folder_store, folder_store_error),
            tabs: tabs::State::default(),
            diagram: diagram::State::default(),
            query: query::State::new(query),
            results: results::State::default(),
            table_pages: HashMap::new(),
            table_cache: HashMap::new(),
            table_cache_lru: VecDeque::new(),
            table_cache_bytes_estimate: 0,
            table_inactive_since: HashMap::new(),
            inactive_table_tabs_released: HashSet::new(),
            table_filters: HashMap::new(),
            table_sorts: HashMap::new(),
            selected_table: None,
        }
    }
}
