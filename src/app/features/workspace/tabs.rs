use super::types::RowContextMenuState;
use crate::constants::{TAB_DRAG_HOLD_MS, TAB_DRAG_HOLD_THRESHOLD, TAB_DRAG_THRESHOLD};
use crate::model::table::{ColumnResize, RelationInfo, ResultSet, TriggerInfo};
use crate::ui::ids::query_tabs_scroll_id;
use iced::Point;
use iced::widget::scrollable;
use iced::{Task, mouse};
use iced_code_editor::CodeEditor;
use std::collections::HashMap;
use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;
use std::time::Instant;

pub(crate) struct QueryTabState {
    pub(crate) query: CodeEditor,
    pub(crate) query_undo_stack: Vec<String>,
    pub(crate) query_redo_stack: Vec<String>,
    pub(crate) query_error: Option<String>,
    pub(crate) apply_error: Option<String>,
    pub(crate) apply_message: Option<String>,
    pub(crate) selected_table: Option<String>,
    pub(crate) table_triggers: Vec<TriggerInfo>,
    pub(crate) triggers_table: Option<String>,
    pub(crate) selected_trigger: Option<String>,
    pub(crate) trigger_edit_name: String,
    pub(crate) trigger_edit_timing: String,
    pub(crate) trigger_edit_event: String,
    pub(crate) trigger_edit_definer_user: String,
    pub(crate) trigger_edit_definer_host: String,
    pub(crate) table_relations: Vec<RelationInfo>,
    pub(crate) relations_table: Option<String>,
    pub(crate) table_query: Option<String>,
    pub(crate) editable_query: Option<String>,
    pub(crate) query_table: Option<String>,
    pub(crate) table_has_next_page: bool,
    pub(crate) last_query_was_table: bool,
    pub(crate) inactive_results_released: bool,
    pub(crate) results: Option<Arc<ResultSet>>,
    pub(crate) query_result_sets: Vec<Arc<ResultSet>>,
    pub(crate) active_query_result: usize,
    pub(crate) pending_edits: HashMap<(usize, usize), String>,
    pub(crate) original_row_snapshots: HashMap<usize, Vec<String>>,
    pub(crate) column_widths: Vec<f32>,
    pub(crate) column_resize: Option<ColumnResize>,
    pub(crate) results_vertical_viewport: Option<scrollable::Viewport>,
    pub(crate) results_horizontal_viewport: Option<scrollable::Viewport>,
    pub(crate) editing_cell: Option<(usize, usize)>,
    pub(crate) selected_cell: Option<(usize, usize)>,
    pub(crate) selected_rows: Option<(usize, usize)>,
    pub(crate) row_drag_anchor: Option<usize>,
    pub(crate) row_drag_active: bool,
    pub(crate) results_cursor: Option<Point>,
    pub(crate) row_context_menu: Option<RowContextMenuState>,
}

impl QueryTabState {
    pub(crate) fn estimated_heavy_bytes(&self) -> usize {
        let mut total = 0usize;

        if self.query_result_sets.is_empty() {
            if let Some(results) = &self.results {
                total = total.saturating_add(results.estimated_heap_bytes());
            }
        } else {
            total = total.saturating_add(
                self.query_result_sets
                    .iter()
                    .map(|results| results.estimated_heap_bytes())
                    .sum::<usize>(),
            );
        }

        total = total.saturating_add(
            self.original_row_snapshots
                .values()
                .map(|row| {
                    row.capacity() * std::mem::size_of::<String>()
                        + row.iter().map(String::capacity).sum::<usize>()
                })
                .sum::<usize>(),
        );

        total
    }

    pub(crate) fn release_heavy_state(&mut self) -> usize {
        let released = self.estimated_heavy_bytes();
        if released == 0 {
            return 0;
        }

        self.results = None;
        self.query_result_sets.clear();
        self.active_query_result = 0;
        self.pending_edits.clear();
        self.original_row_snapshots.clear();
        self.column_widths.clear();
        self.column_resize = None;
        self.results_vertical_viewport = None;
        self.results_horizontal_viewport = None;
        self.editing_cell = None;
        self.selected_cell = None;
        self.selected_rows = None;
        self.row_drag_anchor = None;
        self.row_drag_active = false;
        self.results_cursor = None;
        self.row_context_menu = None;
        self.apply_error = None;
        self.apply_message = Some(String::from(
            "Released inactive results to reduce memory. Run query again if needed.",
        ));
        self.inactive_results_released = true;

        released
    }
}

pub(crate) struct QueryTab {
    pub(crate) title: String,
    pub(crate) renamed: bool,
    pub(crate) pinned: bool,
    pub(crate) inactive_since: Option<Instant>,
    pub(crate) state: Option<QueryTabState>,
}

impl QueryTab {
    pub(crate) fn display_title(&self) -> &str {
        &self.title
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum TabEntry {
    Query(usize),
    Table(String),
    Diagram(usize),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TabContentKind {
    Query,
    Table,
    Diagram,
}

impl TabEntry {
    pub(crate) fn content_kind(&self) -> TabContentKind {
        match self {
            Self::Query(_) => TabContentKind::Query,
            Self::Table(_) => TabContentKind::Table,
            Self::Diagram(_) => TabContentKind::Diagram,
        }
    }
}

pub(crate) struct DragTab {
    pub(crate) from_index: usize,
    pub(crate) width: f32,
}

pub(crate) struct PendingTabDrag {
    pub(crate) index: usize,
    pub(crate) width: f32,
    pub(crate) origin_x: Option<f32>,
    pub(crate) started_at: Instant,
}

pub(crate) struct State {
    pub(crate) tab_context_menu: Option<TabContextMenuState>,
    pub(crate) tabs_cursor: Option<Point>,
    pub(crate) query_tabs: Vec<QueryTab>,
    pub(crate) active_query_tab: Option<usize>,
    pub(crate) tab_strip: Vec<TabEntry>,
    pub(crate) pinned_table_tabs: HashSet<String>,
    pub(crate) open_tables: Vec<String>,
    pub(crate) drag_tab: Option<DragTab>,
    pub(crate) drag_target_index: Option<usize>,
    pub(crate) pending_tab_drag: Option<PendingTabDrag>,
    pub(crate) tabs_horizontal_viewport: Option<scrollable::Viewport>,
    pub(crate) next_query_tab_number: usize,
    pub(crate) tabs_hidden: bool,
}
impl Default for State {
    fn default() -> Self {
        Self {
            tab_context_menu: None,
            tabs_cursor: None,
            query_tabs: vec![QueryTab {
                title: String::from("Welcome"),
                renamed: false,
                pinned: false,
                inactive_since: None,
                state: None,
            }],
            active_query_tab: Some(0),
            tab_strip: vec![TabEntry::Query(0)],
            pinned_table_tabs: HashSet::new(),
            open_tables: Vec::new(),
            drag_tab: None,
            drag_target_index: None,
            pending_tab_drag: None,
            tabs_horizontal_viewport: None,
            next_query_tab_number: 2,
            tabs_hidden: false,
        }
    }
}

impl State {
    pub(crate) fn clear_tab_drag(&mut self) {
        self.drag_tab = None;
        self.drag_target_index = None;
        self.pending_tab_drag = None;
    }
    pub(crate) fn adjust_query_tab_indices(&mut self, removed_index: usize) {
        for entry in &mut self.tab_strip {
            if let TabEntry::Query(index) = entry
                && *index > removed_index
            {
                *index -= 1;
            }
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct TabContextMenuState {
    pub(crate) position: Point,
    pub(crate) strip_index: usize,
}

#[derive(Debug, Clone)]
pub(crate) enum Message {
    CloseEntry(TabEntry),
    Tooltip(Option<String>),
    TabPressed { index: usize, width: f32 },
    TabDragMoved { index: usize, x: f32, width: f32 },
    TabDragReleased,
    TabContextMenuRequested { index: usize },
    TabContextClose,
    TabContextCloseAll,
    TabContextCloseRight,
    TabContextTogglePin,
    TabContextRename,
    CloseTabContextMenu,
    TabsCursorMoved(Point),
    TabsCursorLeft,
    TabsScrolled(scrollable::Viewport),
    TabsWheelScrolled(mouse::ScrollDelta),
}
pub(crate) enum Output {
    CloseEntry(TabEntry),
    Tooltip(Option<String>),
    Activate(TabEntry),
    ContextOpened,
    Close(Vec<usize>),
    TogglePin(TabEntry),
    Rename { index: usize, name: String },
}
impl State {
    pub(crate) fn update(
        &mut self,
        message: Message,
        scale: f32,
        global_cursor: Option<Point>,
    ) -> (Task<Message>, Option<Output>) {
        let mut output = None;
        let task = self.update_internal(message, scale, global_cursor, &mut output);
        (task, output)
    }
    fn update_internal(
        &mut self,
        message: Message,
        scale: f32,
        global_cursor: Option<Point>,
        output: &mut Option<Output>,
    ) -> Task<Message> {
        match message {
            Message::CloseEntry(entry) => {
                *output = Some(Output::CloseEntry(entry));
                Task::none()
            }
            Message::Tooltip(text) => {
                *output = Some(Output::Tooltip(text));
                Task::none()
            }
            Message::TabPressed { index, width } => {
                if index >= self.tab_strip.len() {
                    return Task::none();
                }
                self.tab_context_menu = None;
                self.pending_tab_drag = Some(PendingTabDrag {
                    index,
                    width,
                    origin_x: None,
                    started_at: Instant::now(),
                });
                Task::none()
            }
            Message::TabDragMoved { index, x, width } => {
                if index >= self.tab_strip.len() {
                    return Task::none();
                }
                let threshold = scale * TAB_DRAG_THRESHOLD;
                let hold_threshold = scale * TAB_DRAG_HOLD_THRESHOLD;
                if let Some(pending) = self.pending_tab_drag.as_mut() {
                    let crossed = pending.index != index;
                    let origin = *pending.origin_x.get_or_insert(x);
                    let delta = (x - origin).abs();
                    let held =
                        pending.started_at.elapsed() >= Duration::from_millis(TAB_DRAG_HOLD_MS);
                    if !crossed && delta < threshold && (!held || delta < hold_threshold) {
                        return Task::none();
                    }
                    let pending = self.pending_tab_drag.take().unwrap();
                    self.drag_tab = Some(DragTab {
                        from_index: pending.index,
                        width: pending.width,
                    });
                }
                if self.drag_tab.is_none() {
                    return Task::none();
                }
                let target = if x < width * 0.5 { index } else { index + 1 };
                self.drag_target_index = Some(target);
                Task::none()
            }
            Message::TabDragReleased => {
                self.tab_context_menu = None;
                if let Some(drag) = self.drag_tab.take() {
                    let Some(mut target) = self.drag_target_index else {
                        return Task::none();
                    };
                    self.drag_target_index = None;
                    if drag.from_index >= self.tab_strip.len() {
                        return Task::none();
                    }
                    if drag.from_index < target {
                        target = target.saturating_sub(1);
                    }
                    if target > self.tab_strip.len() {
                        target = self.tab_strip.len();
                    }
                    if target == drag.from_index {
                        return Task::none();
                    }
                    let entry = self.tab_strip.remove(drag.from_index);
                    self.tab_strip.insert(target, entry);
                    return Task::none();
                }
                if let Some(pending) = self.pending_tab_drag.take() {
                    self.drag_target_index = None;
                    if let Some(entry) = self.tab_strip.get(pending.index).cloned() {
                        *output = Some(Output::Activate(entry));
                        return Task::none();
                    }
                }
                self.drag_target_index = None;
                Task::none()
            }
            Message::TabContextMenuRequested { index } => {
                if index >= self.tab_strip.len() {
                    return Task::none();
                }
                self.clear_tab_drag();
                *output = Some(Output::ContextOpened);
                let position = global_cursor
                    .or(self.tabs_cursor)
                    .unwrap_or(Point::new(scale * 12.0, scale * 12.0));
                self.tab_context_menu = Some(TabContextMenuState {
                    position,
                    strip_index: index,
                });
                Task::none()
            }
            Message::TabContextClose => {
                if let Some(menu) = self.tab_context_menu.take() {
                    *output = Some(Output::Close(vec![menu.strip_index]));
                }
                Task::none()
            }
            Message::TabContextCloseAll => {
                self.tab_context_menu = None;
                let indices = (0..self.tab_strip.len()).collect::<Vec<_>>();
                {
                    *output = Some(Output::Close(indices));
                    Task::none()
                }
            }
            Message::TabContextCloseRight => {
                let Some(menu) = self.tab_context_menu.take() else {
                    return Task::none();
                };
                if menu.strip_index + 1 >= self.tab_strip.len() {
                    return Task::none();
                }
                let indices = ((menu.strip_index + 1)..self.tab_strip.len()).collect();
                {
                    *output = Some(Output::Close(indices));
                    Task::none()
                }
            }
            Message::TabContextTogglePin => {
                let Some(menu) = self.tab_context_menu.take() else {
                    return Task::none();
                };
                let Some(entry) = self.tab_strip.get(menu.strip_index).cloned() else {
                    return Task::none();
                };
                *output = Some(Output::TogglePin(entry));
                Task::none()
            }
            Message::TabContextRename => {
                let Some(menu) = self.tab_context_menu.take() else {
                    return Task::none();
                };
                let Some(TabEntry::Query(index)) = self.tab_strip.get(menu.strip_index).cloned()
                else {
                    return Task::none();
                };
                let Some(tab) = self.query_tabs.get(index) else {
                    return Task::none();
                };
                *output = Some(Output::Rename {
                    index,
                    name: tab.title.clone(),
                });
                Task::none()
            }
            Message::CloseTabContextMenu => {
                self.tab_context_menu = None;
                Task::none()
            }
            Message::TabsCursorMoved(position) => {
                self.tabs_cursor = Some(position);
                Task::none()
            }
            Message::TabsCursorLeft => {
                self.tabs_cursor = None;
                Task::none()
            }
            Message::TabsScrolled(viewport) => {
                self.tabs_horizontal_viewport = Some(viewport);
                Task::none()
            }
            Message::TabsWheelScrolled(delta) => {
                let Some(viewport) = self.tabs_horizontal_viewport else {
                    return Task::none();
                };

                let step = scale * 56.0;
                let movement = match delta {
                    mouse::ScrollDelta::Lines { x, y } => (x * step) - (y * step),
                    mouse::ScrollDelta::Pixels { x, y } => x - y,
                };

                if movement.abs() < f32::EPSILON {
                    return Task::none();
                }

                let offset_x = viewport.absolute_offset().x;
                let viewport_width = viewport.bounds().width;
                let max_offset = (viewport.content_bounds().width - viewport_width).max(0.0);
                if max_offset <= 0.0 {
                    return Task::none();
                }

                let next_offset = (offset_x + movement).clamp(0.0, max_offset);
                if (next_offset - offset_x).abs() < f32::EPSILON {
                    return Task::none();
                }

                iced::widget::operation::scroll_to(
                    query_tabs_scroll_id(),
                    scrollable::AbsoluteOffset {
                        x: Some(next_offset),
                        y: None,
                    },
                )
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn releasing_a_click_requests_activation_without_reordering_tabs() {
        let mut state = State::default();
        state
            .tab_strip
            .push(TabEntry::Table(String::from("orders")));
        let original = state.tab_strip.clone();

        let _ = state.update(
            Message::TabPressed {
                index: 1,
                width: 120.0,
            },
            1.0,
            None,
        );
        let (_, output) = state.update(Message::TabDragReleased, 1.0, None);

        assert!(
            matches!(output, Some(Output::Activate(TabEntry::Table(table))) if table == "orders")
        );
        assert_eq!(state.tab_strip, original);
        assert!(state.pending_tab_drag.is_none());
        assert!(state.drag_tab.is_none());
    }

    #[test]
    fn crossing_another_tab_reorders_immediately_without_activating_it() {
        let mut state = State::default();
        state.tab_strip.extend([
            TabEntry::Table(String::from("orders")),
            TabEntry::Diagram(0),
        ]);

        let _ = state.update(
            Message::TabPressed {
                index: 0,
                width: 120.0,
            },
            1.0,
            None,
        );
        let _ = state.update(
            Message::TabDragMoved {
                index: 1,
                x: 90.0,
                width: 120.0,
            },
            1.0,
            None,
        );
        let (_, output) = state.update(Message::TabDragReleased, 1.0, None);

        assert!(output.is_none());
        assert_eq!(
            state.tab_strip,
            [
                TabEntry::Table(String::from("orders")),
                TabEntry::Query(0),
                TabEntry::Diagram(0)
            ]
        );
        assert_eq!(state.active_query_tab, Some(0));
        assert!(state.drag_tab.is_none());
        assert!(state.pending_tab_drag.is_none());
    }
}
