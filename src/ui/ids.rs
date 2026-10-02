use iced::widget::Id;

pub(crate) fn cell_input_id(row: usize, column: usize) -> Id {
    Id::from(format!("cell-{}-{}", row, column))
}

pub(crate) fn text_field_id(name: &str) -> Id {
    Id::from(format!("field:{name}"))
}

pub(crate) fn table_search_input_id() -> Id {
    Id::from("table-search-input")
}

pub(crate) fn omni_bar_input_id() -> Id {
    Id::from("omni-bar-input")
}

pub(crate) fn omni_bar_results_scroll_id() -> Id {
    Id::from("omni-bar-results-scroll")
}

pub(crate) fn sidebar_tables_scroll_id() -> Id {
    Id::from("sidebar-tables-scroll")
}

pub(crate) fn results_vertical_scroll_id() -> Id {
    Id::from("results-vertical-scroll")
}

pub(crate) fn results_horizontal_scroll_id() -> Id {
    Id::from("results-horizontal-scroll")
}

pub(crate) fn chat_scroll_id() -> Id {
    Id::from("chat-scroll")
}

pub(crate) fn query_tabs_scroll_id() -> Id {
    Id::from("query-tabs-scroll")
}

pub(crate) fn result_tabs_scroll_id() -> Id {
    Id::from("result-tabs-scroll")
}

pub(crate) fn connection_tabs_scroll_id() -> Id {
    Id::from("connection-tabs-scroll")
}

pub(crate) fn query_editor_id() -> Id {
    Id::from("query-editor")
}

pub(crate) fn database_switcher_scroll_id() -> Id {
    Id::from("database-switcher-scroll")
}
