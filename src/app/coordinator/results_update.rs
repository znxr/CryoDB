use crate::app::core::App;
use crate::app::features::workspace::results::Message as ResultsMessage;
use crate::app::features::workspace::types::RowContextMenuState;
use crate::app::message::Message;
use crate::app::types::{ErrorModalState, ResultsExportFormat, ToastLevel};
use crate::app::update::export_results_file;
use crate::constants::COLUMN_WIDTH_MIN;
use crate::db::edits::{apply_table_changes, delete_table_rows};
use crate::model::table::{ColumnResize, SortDirection, TableRelationFilter, TableSortState};
use crate::ui::ids::{
    result_tabs_scroll_id, results_horizontal_scroll_id, results_vertical_scroll_id,
};
use crate::utils::helpers::is_valid_json;
use iced::widget::scrollable;
use iced::{Point, Task, mouse};
use rfd::AsyncFileDialog;
use std::collections::HashMap;

impl App {
    pub(crate) fn update_results(&mut self, message: ResultsMessage) -> Task<Message> {
        match message {
            ResultsMessage::TabsScrolled(viewport) => {
                self.workspace.results.tabs_horizontal_viewport = Some(viewport);
                Task::none()
            }
            ResultsMessage::CursorMoved(position) => {
                self.workspace.results.cursor = Some(position);
                Task::none()
            }
            ResultsMessage::CellFocused { row, column } => {
                let Some((rows, columns)) = self.results_dimensions() else {
                    return Task::none();
                };
                if row >= rows || column >= columns {
                    return Task::none();
                }
                self.workspace.results.selected_cell = Some((row, column));
                self.workspace.results.selected_rows = Some((row, row));
                self.workspace.results.row_drag_anchor = Some(row);
                self.workspace.results.row_drag_active = true;
                self.workspace.results.row_context_menu = None;
                if self.workspace.results.editing_cell != Some((row, column)) {
                    self.workspace.results.editing_cell = None;
                }
                self.scroll_selected_cell_into_view()
            }
            ResultsMessage::CellDoubleClicked { row, column } => {
                if self.should_open_text_modal(row, column) {
                    return self.open_text_modal(row, column);
                }
                let focus_task = self.begin_cell_edit(row, column);
                let scroll_task = self.scroll_selected_cell_into_view();
                Task::batch(vec![focus_task, scroll_task])
            }
            ResultsMessage::RowHeaderPressed { row } => {
                let Some((rows, columns)) = self.results_dimensions() else {
                    return Task::none();
                };
                if row >= rows {
                    return Task::none();
                }
                self.workspace.results.selected_rows = Some((row, row));
                self.workspace.results.row_drag_anchor = Some(row);
                self.workspace.results.row_drag_active = true;
                self.workspace.results.row_context_menu = None;
                self.workspace.results.editing_cell = None;
                let column = self
                    .workspace
                    .results
                    .selected_cell
                    .map(|(_, col)| col)
                    .unwrap_or(0);
                let column = column.min(columns.saturating_sub(1));
                self.workspace.results.selected_cell = Some((row, column));
                Task::none()
            }
            ResultsMessage::SelectAllRows => {
                let Some((rows, _)) = self.results_dimensions() else {
                    return Task::none();
                };
                if rows == 0 {
                    return Task::none();
                }
                self.workspace.results.selected_rows = Some((0, rows - 1));
                self.workspace.results.row_drag_active = false;
                self.workspace.results.row_drag_anchor = None;
                self.workspace.results.row_context_menu = None;
                self.workspace.results.editing_cell = None;
                Task::none()
            }
            ResultsMessage::RowSelectionDragged { row } => {
                if !self.workspace.results.row_drag_active {
                    return Task::none();
                }
                let Some((rows, _)) = self.results_dimensions() else {
                    return Task::none();
                };
                if rows == 0 {
                    return Task::none();
                }
                let clamped_row = row.min(rows.saturating_sub(1));
                let anchor = self
                    .workspace
                    .results
                    .row_drag_anchor
                    .unwrap_or(clamped_row)
                    .min(rows.saturating_sub(1));
                let (start, end) = if anchor <= clamped_row {
                    (anchor, clamped_row)
                } else {
                    (clamped_row, anchor)
                };
                if self.workspace.results.selected_rows != Some((start, end)) {
                    self.workspace.results.selected_rows = Some((start, end));
                    self.workspace.results.row_context_menu = None;
                }
                Task::none()
            }
            ResultsMessage::RowContextMenuRequested { row } => {
                let Some((rows, columns)) = self.results_dimensions() else {
                    return Task::none();
                };
                if row >= rows {
                    return Task::none();
                }

                let position = self
                    .workspace
                    .results
                    .cursor
                    .unwrap_or(Point::new(self.scale_f32(8.0), self.scale_f32(8.0)));

                if !self.row_is_selected(row) {
                    self.workspace.results.selected_rows = Some((row, row));
                }
                self.workspace.results.row_drag_active = false;
                self.workspace.results.row_drag_anchor = None;
                self.workspace.results.editing_cell = None;
                let column = self
                    .workspace
                    .results
                    .selected_cell
                    .map(|(_, col)| col)
                    .unwrap_or(0);
                let column = column.min(columns.saturating_sub(1));
                self.workspace.results.selected_cell = Some((row, column));
                self.workspace.tabs.tab_context_menu = None;
                self.workspace.results.row_context_menu = Some(RowContextMenuState { position });
                Task::none()
            }
            ResultsMessage::CloseRowContextMenu => {
                self.workspace.results.row_context_menu = None;
                Task::none()
            }
            ResultsMessage::FollowRelation {
                table,
                column,
                value,
            } => {
                if self.workspace.query.running {
                    return Task::none();
                }
                self.workspace
                    .table_filters
                    .insert(table.clone(), TableRelationFilter { column, value });
                self.workspace.table_pages.insert(table.clone(), 0);
                self.remove_table_cache_for_table(&table);
                self.select_table(table, 0, true)
            }
            ResultsMessage::QueryResultTabSelected(index) => {
                let Some(results) = self.workspace.results.sets.get(index).cloned() else {
                    return Task::none();
                };
                self.workspace.results.active_set = index;
                self.workspace.results.column_widths = self.initial_column_widths(&results.columns);
                self.workspace.results.current = Some(results);
                self.workspace.results.vertical_viewport = None;
                self.workspace.results.horizontal_viewport = None;
                self.workspace.results.column_resize = None;
                self.workspace.results.editing_cell = None;
                self.workspace.results.selected_cell = None;
                self.clear_row_selection();
                Task::none()
            }
            ResultsMessage::ResultCellEdited { row, column, value } => {
                if self.workspace.results.editing_cell != Some((row, column)) {
                    return Task::none();
                }
                self.apply_cell_edit(row, column, value);
                Task::none()
            }
            ResultsMessage::ApplyChanges => {
                if self.workspace.results.applying_changes {
                    return Task::none();
                }
                if self.workspace.results.pending_edits.is_empty() {
                    return Task::none();
                }
                let Some(pool) = self.connections.pool.clone() else {
                    self.workspace.results.apply_error = Some(String::from("Not connected."));
                    return Task::none();
                };
                let Some(table) = self.workspace.query.table.clone() else {
                    self.workspace.results.apply_error =
                        Some(String::from("Select a table to apply edits."));
                    return Task::none();
                };
                if !self.table_is_row_editable(&table) {
                    self.workspace.results.apply_error = Some(String::from(
                        "This PostgreSQL object is read-only in CryoDB. Edit rows on base tables instead.",
                    ));
                    return Task::none();
                }
                let Some(results) = self.workspace.results.current.as_ref() else {
                    self.workspace.results.apply_error =
                        Some(String::from("No original data to apply changes."));
                    return Task::none();
                };
                if self.workspace.results.original_row_snapshots.is_empty() {
                    self.workspace.results.apply_error =
                        Some(String::from("No original data to apply changes."));
                    return Task::none();
                }

                if !self.is_editable_query_active() {
                    self.workspace.results.apply_error = Some(String::from(
                        "Apply is only available for table selections.",
                    ));
                    return Task::none();
                }

                let database = self.connections.current.database.trim().to_string();
                if database.is_empty() {
                    self.workspace.results.apply_error =
                        Some(String::from("Select a database to apply changes."));
                    return Task::none();
                }

                self.workspace.results.applying_changes = true;
                self.workspace.results.apply_error = None;
                self.workspace.results.apply_message = None;

                let edits: HashMap<(usize, usize), String> =
                    self.workspace.results.pending_edits.clone();
                let columns = results.columns.clone();
                let column_kinds = results.column_kinds.clone();
                let rows = self.workspace.results.original_row_snapshots.clone();

                Task::perform(
                    apply_table_changes(pool, database, table, columns, column_kinds, rows, edits),
                    |value| {
                        Message::Workspace(crate::app::features::workspace::Message::Results(
                            crate::app::features::workspace::results::Message::ChangesApplied(
                                value,
                            ),
                        ))
                    },
                )
            }
            ResultsMessage::DiscardChanges => {
                if !self.workspace.results.pending_edits.is_empty() {
                    self.discard_pending_edits();
                    self.workspace.results.apply_error = None;
                    self.workspace.results.apply_message = Some(String::from("Changes discarded."));
                    if self.is_table_query_active()
                        && let (Some(table), Some(results)) = (
                            self.workspace.selected_table.clone(),
                            self.workspace.results.current.clone(),
                        )
                    {
                        let page = self.workspace.table_pages.get(&table).copied().unwrap_or(0);
                        self.insert_table_cache_entry(
                            table,
                            page,
                            results,
                            self.workspace.query.table_has_next_page,
                        );
                    }
                }
                Task::none()
            }
            ResultsMessage::ChangesApplied(result) => {
                self.workspace.results.applying_changes = false;
                match result {
                    Ok(count) => {
                        self.workspace.results.apply_message = Some(crate::i18n::tr_with(
                            "Applied {count} change(s).",
                            &[("{count}", &count.to_string())],
                        ));
                        self.workspace.results.apply_error = None;
                        self.clear_pending_edits_state();
                        if let Some(table) = self.workspace.query.table.clone() {
                            if self.is_table_query_active() {
                                let page =
                                    self.workspace.table_pages.get(&table).copied().unwrap_or(0);
                                self.remove_table_cache_for_table(&table);
                                self.workspace.results.preserve_viewport = true;
                                return self.start_table_query(table, page, false);
                            }
                            let query = self.workspace.query.editor.content();
                            self.workspace.results.preserve_viewport = true;
                            return self.run_query_text(query);
                        }
                    }
                    Err(error) => {
                        self.workspace.results.apply_error = Some(error);
                    }
                }
                Task::none()
            }
            ResultsMessage::CopyRows => {
                self.workspace.results.row_context_menu = None;
                let Some(text) = self.selected_rows_text(false) else {
                    return Task::none();
                };
                iced::clipboard::write(text)
            }
            ResultsMessage::CopyRowsWithHeaders => {
                self.workspace.results.row_context_menu = None;
                let Some(text) = self.selected_rows_text(true) else {
                    return Task::none();
                };
                iced::clipboard::write(text)
            }
            ResultsMessage::CopyRowsAsInsert => {
                self.workspace.results.row_context_menu = None;
                match self.selected_rows_sql_insert() {
                    Ok(text) => iced::clipboard::write(text),
                    Err(error) => {
                        self.shell.error_modal = Some(ErrorModalState::new(
                            "Copy as SQL Insert",
                            "Unable to build SQL insert.",
                            error,
                        ));
                        Task::none()
                    }
                }
            }
            ResultsMessage::ExportResultsCsv => {
                self.workspace.results.row_context_menu = None;
                if self.workspace.results.current.is_none() {
                    return Task::none();
                }
                let task = AsyncFileDialog::new()
                    .set_file_name(ResultsExportFormat::Csv.default_file_name())
                    .add_filter(
                        ResultsExportFormat::Csv.label(),
                        &[ResultsExportFormat::Csv.extension()],
                    )
                    .save_file();
                Task::perform(task, |file| {
                    Message::Workspace(crate::app::features::workspace::Message::Results(crate::app::features::workspace::results::Message::ResultsExportPathPicked{
                    format: ResultsExportFormat::Csv,
                    path: file.map(|handle| handle.path().to_path_buf()),
                }))
                })
            }
            ResultsMessage::ExportResultsJson => {
                self.workspace.results.row_context_menu = None;
                if self.workspace.results.current.is_none() {
                    return Task::none();
                }
                let task = AsyncFileDialog::new()
                    .set_file_name(ResultsExportFormat::Json.default_file_name())
                    .add_filter(
                        ResultsExportFormat::Json.label(),
                        &[ResultsExportFormat::Json.extension()],
                    )
                    .save_file();
                Task::perform(task, |file| {
                    Message::Workspace(crate::app::features::workspace::Message::Results(crate::app::features::workspace::results::Message::ResultsExportPathPicked{
                    format: ResultsExportFormat::Json,
                    path: file.map(|handle| handle.path().to_path_buf()),
                }))
                })
            }
            ResultsMessage::ExportResultsXlsx => {
                self.workspace.results.row_context_menu = None;
                if self.workspace.results.current.is_none() {
                    return Task::none();
                }
                let task = AsyncFileDialog::new()
                    .set_file_name(ResultsExportFormat::Xlsx.default_file_name())
                    .add_filter(
                        ResultsExportFormat::Xlsx.label(),
                        &[ResultsExportFormat::Xlsx.extension()],
                    )
                    .save_file();
                Task::perform(task, |file| {
                    Message::Workspace(crate::app::features::workspace::Message::Results(crate::app::features::workspace::results::Message::ResultsExportPathPicked{
                    format: ResultsExportFormat::Xlsx,
                    path: file.map(|handle| handle.path().to_path_buf()),
                }))
                })
            }
            ResultsMessage::ResultsExportPathPicked { format, path } => {
                let Some(path) = path else {
                    return Task::none();
                };
                let Some(results) = self.workspace.results.current.as_ref().cloned() else {
                    return Task::none();
                };
                Task::perform(export_results_file(results, format, path), |value| {
                    Message::Workspace(crate::app::features::workspace::Message::Results(
                        crate::app::features::workspace::results::Message::ResultsExportFinished(
                            value,
                        ),
                    ))
                })
            }
            ResultsMessage::ResultsExportFinished(result) => {
                match result {
                    Ok(path) => {
                        self.push_toast(
                            ToastLevel::Success,
                            crate::i18n::tr_with(
                                "Exported results to `{path}`.",
                                &[("{path}", &path)],
                            ),
                        );
                    }
                    Err(error) => {
                        self.shell.error_modal = Some(ErrorModalState::new(
                            "Export results",
                            "Unable to export results.",
                            error,
                        ));
                    }
                }
                Task::none()
            }
            ResultsMessage::DeleteRows => {
                self.workspace.results.row_context_menu = None;
                if self.workspace.query.running || self.workspace.results.applying_changes {
                    return Task::none();
                }
                if !self.workspace.results.pending_edits.is_empty() {
                    self.shell.error_modal = Some(ErrorModalState::new(
                        "Delete rows",
                        "Pending edits detected.",
                        "Apply or discard pending edits before deleting rows.",
                    ));
                    return Task::none();
                }
                let Some(pool) = self.connections.pool.clone() else {
                    self.shell.error_modal = Some(ErrorModalState::new(
                        "Delete rows",
                        "Not connected.",
                        "Connect to a database to delete rows.",
                    ));
                    return Task::none();
                };
                let Some(results) = self.workspace.results.current.as_ref() else {
                    return Task::none();
                };
                if !self.is_editable_query_active() && !self.workspace.query.last_query_was_table {
                    self.shell.error_modal = Some(ErrorModalState::new(
                        "Delete rows",
                        "Delete is only available for table selections.",
                        "Run a table query to delete rows.",
                    ));
                    return Task::none();
                }
                let Some(table) = self.workspace.query.table.clone() else {
                    self.shell.error_modal = Some(ErrorModalState::new(
                        "Delete rows",
                        "Select a table to delete rows.",
                        "Open a table to delete rows.",
                    ));
                    return Task::none();
                };
                if !self.table_is_row_editable(&table) {
                    self.shell.error_modal = Some(ErrorModalState::new(
                        "Delete rows",
                        "This PostgreSQL object is read-only.",
                        "Delete rows on base tables instead.",
                    ));
                    return Task::none();
                }
                let database = self.connections.current.database.trim().to_string();
                if database.is_empty() {
                    self.shell.error_modal = Some(ErrorModalState::new(
                        "Delete rows",
                        "Select a database to delete rows.",
                        "Choose a database and try again.",
                    ));
                    return Task::none();
                }
                let Some((start, end)) = self.selected_rows_range(results.rows.len()) else {
                    return Task::none();
                };
                let columns = results.columns.clone();
                let rows = results.rows[start..=end].to_vec();
                let page = self.workspace.table_pages.get(&table).copied().unwrap_or(0);

                self.workspace.results.applying_changes = true;
                self.workspace.results.apply_error = None;
                self.workspace.results.apply_message = None;

                Task::perform(
                    delete_table_rows(pool, database, table.clone(), columns, rows),
                    move |result| {
                        Message::Workspace(crate::app::features::workspace::Message::Results(
                            crate::app::features::workspace::results::Message::RowsDeleted {
                                table,
                                page,
                                result,
                            },
                        ))
                    },
                )
            }
            ResultsMessage::RowsDeleted {
                table,
                page,
                result,
            } => {
                self.workspace.results.applying_changes = false;
                self.workspace.results.row_context_menu = None;
                match result {
                    Ok(count) => {
                        self.workspace.results.apply_message = Some(crate::i18n::tr_with(
                            "Deleted {count} row(s).",
                            &[("{count}", &count.to_string())],
                        ));
                        self.workspace.results.apply_error = None;
                        self.clear_row_selection();
                        self.remove_table_cache_for_table(&table);
                        if self.workspace.selected_table.as_deref() == Some(table.as_str()) {
                            return self.start_table_query(table, page, false);
                        }
                    }
                    Err(error) => {
                        self.workspace.results.apply_error = Some(error);
                    }
                }
                Task::none()
            }
            ResultsMessage::ColumnResizeStart(column) => {
                if self.workspace.results.column_widths.len() <= column {
                    self.workspace
                        .results
                        .column_widths
                        .resize(column + 1, self.workspace.results.column_width);
                }

                let start_width = self.base_column_width(column);
                self.workspace.results.column_resize = Some(ColumnResize {
                    column,
                    start_x: None,
                    start_width,
                });
                Task::none()
            }
            ResultsMessage::ColumnResizeMove(x) => {
                let Some(mut resize) = self.workspace.results.column_resize.take() else {
                    return Task::none();
                };

                if resize.start_x.is_none() {
                    resize.start_x = Some(x);
                    resize.start_width = self.base_column_width(resize.column);
                    self.workspace.results.column_resize = Some(resize);
                    return Task::none();
                }

                let start_x = resize.start_x.unwrap_or(x);
                let new_width = (resize.start_width + (x - start_x)).max(COLUMN_WIDTH_MIN);

                if let Some(width) = self.workspace.results.column_widths.get_mut(resize.column) {
                    *width = new_width;
                }
                self.workspace.results.column_resize = Some(resize);
                Task::none()
            }
            ResultsMessage::ColumnResizeEnd => {
                if let Some(resize) = self.workspace.results.column_resize.take()
                    && let Some(results) = &self.workspace.results.current
                    && let Some(name) = results.columns.get(resize.column).cloned()
                    && let Some(&width) = self.workspace.results.column_widths.get(resize.column)
                {
                    if (width - self.workspace.results.column_width).abs() < f32::EPSILON {
                        self.settings.values.column_width_overrides.remove(&name);
                    } else {
                        self.settings
                            .values
                            .column_width_overrides
                            .insert(name, width);
                    }
                    self.persist_settings_store();
                }
                self.workspace.results.row_drag_active = false;
                self.workspace.results.row_drag_anchor = None;
                Task::none()
            }
            ResultsMessage::ResultHeaderPressed { column } => {
                if self.workspace.query.running || self.workspace.results.applying_changes {
                    return Task::none();
                }
                if !self.workspace.results.pending_edits.is_empty() {
                    return Task::none();
                }
                if !self.workspace.query.last_query_was_table {
                    return Task::none();
                }
                let Some(table) = self.workspace.selected_table.clone() else {
                    return Task::none();
                };
                let Some(results) = self.workspace.results.current.as_ref() else {
                    return Task::none();
                };
                let Some(column_name) = results.columns.get(column).cloned() else {
                    return Task::none();
                };
                let direction = match self.workspace.table_sorts.get(&table) {
                    Some(sort) if sort.column == column_name => sort.direction.toggled(),
                    _ => SortDirection::Asc,
                };
                self.workspace.table_sorts.insert(
                    table.clone(),
                    TableSortState {
                        column: column_name,
                        direction,
                    },
                );
                self.remove_table_cache_for_table(&table);
                self.start_table_query(table, 0, false)
            }
            ResultsMessage::ResultTabsWheelScrolled(delta) => {
                let Some(viewport) = self.workspace.results.tabs_horizontal_viewport else {
                    return Task::none();
                };

                let step = self.scale_f32(56.0);
                let movement = match delta {
                    mouse::ScrollDelta::Lines { x, y } => (x * step) - (y * step),
                    mouse::ScrollDelta::Pixels { x, y } => x - y,
                };
                let offset_x = viewport.absolute_offset().x;
                let max_offset =
                    (viewport.content_bounds().width - viewport.bounds().width).max(0.0);
                let next_offset = (offset_x + movement).clamp(0.0, max_offset);
                if movement.abs() < f32::EPSILON
                    || max_offset <= 0.0
                    || (next_offset - offset_x).abs() < f32::EPSILON
                {
                    return Task::none();
                }

                iced::widget::operation::scroll_to(
                    result_tabs_scroll_id(),
                    scrollable::AbsoluteOffset {
                        x: Some(next_offset),
                        y: None,
                    },
                )
            }
            ResultsMessage::ResultsVerticalScrolled(viewport) => {
                let previous_x = self
                    .workspace
                    .results
                    .horizontal_viewport
                    .map(|known| known.absolute_offset().x)
                    .unwrap_or(0.0);
                let next_x = viewport.absolute_offset().x;
                self.workspace.results.vertical_viewport = Some(viewport);
                if self.workspace.results.horizontal_viewport.is_none() {
                    self.workspace.results.horizontal_viewport = Some(viewport);
                }
                if let (Some(results), Some((row, _))) = (
                    self.workspace.results.current.as_ref(),
                    self.workspace.results.editing_cell,
                ) {
                    let (start, end, _, _) = self.visible_row_range(results.rows.len());
                    if row < start || row >= end {
                        self.workspace.results.editing_cell = None;
                    }
                }
                if (next_x - previous_x).abs() > 0.5 {
                    iced::widget::operation::scroll_to(
                        results_horizontal_scroll_id(),
                        scrollable::AbsoluteOffset {
                            x: Some(next_x),
                            y: None,
                        },
                    )
                } else {
                    Task::none()
                }
            }
            ResultsMessage::ResultsHorizontalScrolled(viewport) => {
                let current_x = self
                    .workspace
                    .results
                    .horizontal_viewport
                    .map(|known| known.absolute_offset().x)
                    .unwrap_or(0.0);
                let next_x = viewport.absolute_offset().x;
                self.workspace.results.horizontal_viewport = Some(viewport);
                if (next_x - current_x).abs() > 0.5 {
                    iced::widget::operation::scroll_to(
                        results_vertical_scroll_id(),
                        scrollable::AbsoluteOffset {
                            x: Some(next_x),
                            y: None,
                        },
                    )
                } else {
                    Task::none()
                }
            }
            ResultsMessage::TextModalAction(action) => {
                self.workspace.results.text_modal_content.perform(action);
                let content = self.workspace.results.text_modal_content.text();
                self.workspace.results.text_modal_is_json = is_valid_json(&content);
                Task::none()
            }
            ResultsMessage::TextModalSave => {
                self.save_text_modal();
                Task::none()
            }
            ResultsMessage::CloseTextModal => {
                self.close_text_modal();
                Task::none()
            }
        }
    }
}
