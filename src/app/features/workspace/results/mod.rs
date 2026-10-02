use super::types::RowContextMenuState;
use crate::app::types::ResultsExportFormat;
use crate::model::table::{ColumnResize, ResultSet};
use iced::widget::{scrollable, text_editor};
use iced::{Point, mouse};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub(crate) enum Message {
    TabsScrolled(scrollable::Viewport),
    CursorMoved(Point),
    CellFocused {
        row: usize,
        column: usize,
    },
    CellDoubleClicked {
        row: usize,
        column: usize,
    },
    RowHeaderPressed {
        row: usize,
    },
    SelectAllRows,
    RowSelectionDragged {
        row: usize,
    },
    RowContextMenuRequested {
        row: usize,
    },
    CloseRowContextMenu,
    FollowRelation {
        table: String,
        column: String,
        value: String,
    },
    QueryResultTabSelected(usize),
    ResultCellEdited {
        row: usize,
        column: usize,
        value: String,
    },
    ApplyChanges,
    DiscardChanges,
    ChangesApplied(Result<usize, String>),
    CopyRows,
    CopyRowsWithHeaders,
    CopyRowsAsInsert,
    ExportResultsCsv,
    ExportResultsJson,
    ExportResultsXlsx,
    ResultsExportPathPicked {
        format: ResultsExportFormat,
        path: Option<PathBuf>,
    },
    ResultsExportFinished(Result<String, String>),
    DeleteRows,
    RowsDeleted {
        table: String,
        page: usize,
        result: Result<usize, String>,
    },
    ColumnResizeStart(usize),
    ColumnResizeMove(f32),
    ColumnResizeEnd,
    ResultHeaderPressed {
        column: usize,
    },
    ResultTabsWheelScrolled(mouse::ScrollDelta),
    ResultsVerticalScrolled(scrollable::Viewport),
    ResultsHorizontalScrolled(scrollable::Viewport),
    TextModalAction(text_editor::Action),
    TextModalSave,
    CloseTextModal,
}

pub(crate) struct State {
    pub(crate) applying_changes: bool,
    pub(crate) preserve_viewport: bool,
    pub(crate) editing_cell: Option<(usize, usize)>,
    pub(crate) selected_cell: Option<(usize, usize)>,
    pub(crate) selected_rows: Option<(usize, usize)>,
    pub(crate) row_drag_anchor: Option<usize>,
    pub(crate) row_drag_active: bool,
    pub(crate) cursor: Option<Point>,
    pub(crate) row_context_menu: Option<RowContextMenuState>,
    pub(crate) text_modal_open: bool,
    pub(crate) text_modal_cell: Option<(usize, usize)>,
    pub(crate) text_modal_content: text_editor::Content,
    pub(crate) text_modal_original: String,
    pub(crate) text_modal_original_was_null: bool,
    pub(crate) text_modal_is_json: bool,
    pub(crate) apply_error: Option<String>,
    pub(crate) apply_message: Option<String>,
    pub(crate) tabs_horizontal_viewport: Option<scrollable::Viewport>,
    pub(crate) pending_edits: HashMap<(usize, usize), String>,
    pub(crate) original_row_snapshots: HashMap<usize, Vec<String>>,
    pub(crate) released: bool,
    pub(crate) column_width: f32,
    pub(crate) column_widths: Vec<f32>,
    pub(crate) column_resize: Option<ColumnResize>,
    pub(crate) current: Option<Arc<ResultSet>>,
    pub(crate) sets: Vec<Arc<ResultSet>>,
    pub(crate) active_set: usize,
    pub(crate) sets_pending: bool,
    pub(crate) vertical_viewport: Option<scrollable::Viewport>,
    pub(crate) horizontal_viewport: Option<scrollable::Viewport>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            applying_changes: false,
            preserve_viewport: false,
            editing_cell: None,
            selected_cell: None,
            selected_rows: None,
            row_drag_anchor: None,
            row_drag_active: false,
            cursor: None,
            row_context_menu: None,
            text_modal_open: false,
            text_modal_cell: None,
            text_modal_content: text_editor::Content::with_text(""),
            text_modal_original: String::new(),
            text_modal_original_was_null: false,
            text_modal_is_json: false,
            apply_error: None,
            apply_message: None,
            tabs_horizontal_viewport: None,
            pending_edits: HashMap::new(),
            original_row_snapshots: HashMap::new(),
            released: false,
            column_width: 160.0,
            column_widths: Vec::new(),
            column_resize: None,
            current: None,
            sets: Vec::new(),
            active_set: 0,
            sets_pending: false,
            vertical_viewport: None,
            horizontal_viewport: None,
        }
    }
}
