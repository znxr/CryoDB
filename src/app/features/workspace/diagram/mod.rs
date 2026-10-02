mod agent;
pub(crate) mod canvas;
mod effects;
pub(crate) mod modal;
pub(crate) mod model;
mod persistence;
pub(crate) mod sql;
mod tabs;
mod toolbar;
mod update;
pub(crate) mod view;
pub(crate) use model::*;
pub(crate) use update::Message;
mod state;
use crate::model::diagram::DiagramStore;

pub(crate) struct State {
    pub(crate) diagram_tabs: Vec<DiagramTab>,
    pub(crate) active_diagram_tab: Option<usize>,
    pub(crate) next_diagram_tab_number: usize,
    pub(crate) next_diagram_tab_id: u64,
    pub(crate) diagram_store: DiagramStore,
    pub(crate) diagram_store_error: Option<String>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            diagram_tabs: Vec::new(),
            active_diagram_tab: None,
            next_diagram_tab_number: 1,
            next_diagram_tab_id: 1,
            diagram_store: crate::storage::load_diagram_store(),
            diagram_store_error: None,
        }
    }
}

pub(crate) struct Context {
    pub(crate) theme: iced::Theme,
    pub(crate) font_family: String,
    pub(crate) pool: Option<crate::db::DatabasePool>,
    pub(crate) database: Option<String>,
    pub(crate) connection_scope: String,
    pub(crate) driver: crate::model::connection::DatabaseDriver,
    pub(crate) keys: Option<(String, String)>,
}

pub(crate) enum Output {
    AgentStep { summary: String, prompt: String },
    ChatNote(Option<String>, String),
    Tooltip(Option<String>),
    HideTools,
    TabAdded(usize),
    TabActivated(usize),
    TabRemoved(usize),
    EnsureTab,
    OpenTable(String),
    Toast(crate::app::types::ToastLevel, String),
    Loaded,
}
