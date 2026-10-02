use crate::ai::sql_fix::should_offer_ai_query_fix;
use crate::app::core::App;
use crate::app::message::Message;
use crate::app::types::LayoutMode;
use crate::app::view::history_input;
use crate::constants::{
    COLUMN_RESIZE_HANDLE_HEIGHT, COLUMN_RESIZE_HANDLE_WIDTH, ICON_AI, ICON_ARROW_LEFT_DOUBLE_LINE,
    ICON_ARROW_LEFT_S_LINE, ICON_ARROW_RIGHT_DOUBLE_LINE, ICON_ARROW_RIGHT_S_LINE,
    ICON_ARROW_RIGHT_UP_LINE, ICON_BOTTOM_CHAT, ICON_BOTTOM_SIDEBAR, ICON_CHAT_AI, ICON_CHECK_FILL,
    ICON_CLIPBOARD_LINE, ICON_CLOSE_FILL, ICON_CLOSE_LINE, ICON_POSTGRES_VECTOR,
    ICON_RESIZE_VERTICAL, READONLY_CELL_CHAR_LIMIT, RESULTS_SCROLLBAR_GUTTER, ROW_HEADER_WIDTH,
};
use crate::model::connection::DatabaseDriver;
use crate::model::settings::{ResultGridDensity, ShortcutBinding};
use crate::model::table::{ColumnKind, RelationInfo};
use crate::ui::ids::{
    cell_input_id, result_tabs_scroll_id, results_horizontal_scroll_id, results_vertical_scroll_id,
};
use crate::ui::styles::{
    ai_fix_button_style, alert_style, compact_button_style, compact_button_style_with_text_color,
    compact_primary_button_style, compact_secondary_button_style, compact_tab_active_button_style,
    modal_backdrop_style, panel_border_style, panel_style, resize_handle_style, table_cell_style,
    table_header_style, table_input_style, table_row_header_style,
};
use crate::ui::theme::ui_radius;
use crate::utils::helpers::{is_null_value, is_postgres_vector_type_name};
use crate::utils::text::{compact_inline_preview, max_chars_for_width, truncate_with_ellipsis};
use iced::border::Radius;
use iced::widget::{
    Row, button, container, mouse_area, row, scrollable, space, stack, text, text_editor,
};
use iced::{Background, Border, Center, Element, Fill, Length, Padding, alignment, mouse};

impl App {
    pub(crate) fn results_panel(&self, layout: LayoutMode) -> Element<'_, Message> {
        let spacing = match layout {
            LayoutMode::Wide => self.scale_u16(8),
            LayoutMode::Compact => self.scale_u16(5),
        };

        let mut handle_line_color = self.theme().extended_palette().background.strong.color;
        handle_line_color.a = 0.65;
        let mut handle_icon_color = self.theme().palette().text;
        handle_icon_color.a = 0.62;
        let resize_handle: Element<'_, Message> =
            if matches!(layout, LayoutMode::Wide) && !self.is_query_editor_hidden() {
                let divider_line = container(space::horizontal())
                    .height(Length::Fixed(1.0))
                    .width(Fill)
                    .style(move |_| container::Style {
                        background: Some(handle_line_color.into()),
                        ..container::Style::default()
                    });
                let divider_icon = text(ICON_RESIZE_VERTICAL.to_string())
                    .font(self.icon_font())
                    .size(self.scale_f32(12.0))
                    .color(handle_icon_color)
                    .wrapping(text::Wrapping::None);

                mouse_area(
                    container(stack![
                        container(divider_line)
                            .width(Fill)
                            .height(Fill)
                            .align_y(alignment::Vertical::Center),
                        container(divider_icon)
                            .padding(Padding {
                                top: 0.0,
                                right: self.scale_f32(8.0),
                                bottom: 0.0,
                                left: self.scale_f32(8.0),
                            })
                            .width(Fill)
                            .height(Fill)
                            .center(Fill),
                    ])
                    .width(Fill)
                    .height(Length::Fixed(self.scale_f32(14.0))),
                )
                .on_press(Message::Workspace(crate::app::features::workspace::Message::Query(
                    crate::app::features::workspace::query::Message::QueryEditorResizeDragStarted,
                )))
                .interaction(mouse::Interaction::ResizingVertically)
                .into()
            } else {
                container(space::horizontal().height(Length::Fixed(0.0)))
                    .width(Fill)
                    .into()
            };

        let results = self.results_view(layout);
        let result_tabs: Option<Element<'_, Message>> = self
            .workspace
            .results
            .current
            .as_ref()
            .filter(|results| {
                self.workspace.results.sets.len() > 1
                    && self
                        .workspace
                        .results
                        .sets
                        .get(self.workspace.results.active_set)
                        .is_some_and(|active| std::sync::Arc::ptr_eq(active, results))
            })
            .map(|_| {
                let mut tabs = Row::new().spacing(self.scale_u16(2));
                for index in 0..self.workspace.results.sets.len() {
                    let active = index == self.workspace.results.active_set;
                    tabs = tabs.push(
                        button(self.label_text(crate::i18n::tr_with(
                            "Result {number}",
                            &[("{number}", &(index + 1).to_string())],
                        )))
                        .padding(self.button_padding_tight())
                        .style(move |theme, status| {
                            if active {
                                compact_tab_active_button_style(theme, status)
                            } else {
                                compact_button_style(theme, status)
                            }
                        })
                        .on_press(Message::Workspace(crate::app::features::workspace::Message::Results(crate::app::features::workspace::results::Message::QueryResultTabSelected(index)))),
                    );
                }
                scrollable(tabs)
                    .id(result_tabs_scroll_id())
                    .on_scroll(|viewport| {
                        Message::Workspace(crate::app::features::workspace::Message::Results(
                            crate::app::features::workspace::results::Message::TabsScrolled(
                                viewport,
                            ),
                        ))
                    })
                    .direction(iced::widget::scrollable::Direction::Horizontal(
                        iced::widget::scrollable::Scrollbar::new()
                            .width(0.0)
                            .scroller_width(0.0)
                            .spacing(0.0),
                    ))
                    .width(Fill)
                    .height(Length::Shrink)
                    .into()
            });

        let mut body = iced::widget::column![].height(Fill).width(Fill);
        if let Some(result_tabs) = result_tabs {
            body = body
                .push(mouse_area(result_tabs).on_scroll(|value| {
                    Message::Workspace(crate::app::features::workspace::Message::Results(
                        crate::app::features::workspace::results::Message::ResultTabsWheelScrolled(
                            value,
                        ),
                    ))
                }))
                .spacing(spacing);
        }
        let body = body.push(results).spacing(spacing).height(Fill).width(Fill);

        iced::widget::column![resize_handle, body]
            .spacing(0)
            .height(Fill)
            .into()
    }

    pub(crate) fn pagination_controls(&self) -> Element<'_, Message> {
        if !self.is_table_query_active()
            || (!self.workspace.query.table_has_next_page && self.current_table_page() == 0)
        {
            return row![].into();
        }

        let status_icon_size = self.label_text_size();
        let page = self.current_table_page();
        let can_prev = page > 0 && !self.workspace.query.running;
        let can_next = self.workspace.query.table_has_next_page && !self.workspace.query.running;

        let page_button = |icon: char, enabled: bool, label, shortcut, message| {
            let content = self.icon_text(icon).size(status_icon_size);
            if enabled {
                self.with_shortcut_tooltip(
                    button(content)
                        .padding(self.scale_padding([2, 4]))
                        .style(compact_button_style)
                        .on_press(message),
                    label,
                    shortcut,
                )
            } else {
                container(content)
                    .padding(self.scale_padding([2, 4]))
                    .into()
            }
        };

        row![
            page_button(
                ICON_ARROW_LEFT_DOUBLE_LINE,
                can_prev,
                "First results page",
                &self.settings.values.first_results_page_shortcut,
                Message::Workspace(crate::app::features::workspace::Message::Query(
                    crate::app::features::workspace::query::Message::TablePageFirst
                )),
            ),
            page_button(
                ICON_ARROW_LEFT_S_LINE,
                can_prev,
                "Previous results page",
                &self.settings.values.previous_results_page_shortcut,
                Message::Workspace(crate::app::features::workspace::Message::Query(
                    crate::app::features::workspace::query::Message::TablePagePrev
                )),
            ),
            self.label_text(format!("{} {}", crate::i18n::tr("Page"), page + 1)),
            page_button(
                ICON_ARROW_RIGHT_S_LINE,
                can_next,
                "Next results page",
                &self.settings.values.next_results_page_shortcut,
                Message::Workspace(crate::app::features::workspace::Message::Query(
                    crate::app::features::workspace::query::Message::TablePageNext
                )),
            ),
            page_button(
                ICON_ARROW_RIGHT_DOUBLE_LINE,
                can_next,
                "Last results page",
                &self.settings.values.last_results_page_shortcut,
                Message::Workspace(crate::app::features::workspace::Message::Query(
                    crate::app::features::workspace::query::Message::TablePageLast
                )),
            ),
        ]
        .spacing(self.scale_u16(3))
        .align_y(Center)
        .into()
    }

    pub(crate) fn results_view(&self, layout: LayoutMode) -> Element<'_, Message> {
        if let Some(error) = &self.workspace.query.error {
            let copy_query_error_button: Element<'_, Message> = {
                let button = button(self.icon_text(ICON_CLIPBOARD_LINE))
                    .padding(self.button_padding_icon())
                    .style(compact_button_style)
                    .on_press(Message::Workspace(
                        crate::app::features::workspace::Message::Query(
                            crate::app::features::workspace::query::Message::CopyQueryError,
                        ),
                    ));
                self.with_tooltip(button, "Copy error")
            };

            let fix_action: Element<'_, Message> =
                if self.settings.values.ai_enabled && should_offer_ai_query_fix(error) {
                    let label = if self.ai.is_fixing_query_with_ai {
                        "Fixing..."
                    } else {
                        "Fix with AI"
                    };
                    let icon = if self.ai.is_fixing_query_with_ai {
                        self.loading_spinner_icon()
                    } else {
                        ICON_AI
                    };
                    let button = button(self.action_label(label, icon, layout))
                        .padding(self.button_padding_tight())
                        .style(ai_fix_button_style);
                    if self.ai.is_fixing_query_with_ai
                        || self.ai.is_generating_ai
                        || self.workspace.query.running
                    {
                        button.into()
                    } else {
                        button
                            .on_press(Message::Ai(
                                crate::app::features::ai::Message::FixQueryWithAi,
                            ))
                            .into()
                    }
                } else {
                    container(space::horizontal()).width(Length::Shrink).into()
                };

            let ask_chat_button: Element<'_, Message> = if !self.settings.values.ai_enabled {
                container(space::horizontal()).width(Length::Shrink).into()
            } else {
                let button = button(self.icon_text(ICON_CHAT_AI))
                    .padding(self.button_padding_icon())
                    .style(compact_button_style);
                if self.ai.chat_sending {
                    button.into()
                } else {
                    self.with_tooltip(
                        button.on_press(Message::Ai(
                            crate::app::features::ai::Message::ChatExplainError,
                        )),
                        "Ask AI chat",
                    )
                }
            };

            let actions = row![copy_query_error_button, ask_chat_button, fix_action]
                .spacing(self.scale_u16(6))
                .align_y(Center);

            let error_row = row![
                self.input_text(format!("{}: {error}", crate::i18n::tr("Error")))
                    .width(Fill),
                actions
            ]
            .spacing(self.scale_u16(6))
            .align_y(Center);

            let error_panel = container(error_row)
                .padding(self.scale_u16(8))
                .width(Fill)
                .style(alert_style);

            return container(error_panel)
                .padding(self.scale_u16(6))
                .height(Fill)
                .style(panel_style)
                .into();
        }

        if self.workspace.query.running && self.workspace.results.current.is_none() {
            return container(self.label_text("Running query..."))
                .center(Fill)
                .into();
        }

        let Some(results) = &self.workspace.results.current else {
            return container(self.label_text("Run a query to see results here."))
                .center(Fill)
                .into();
        };

        if results.columns.is_empty() {
            return container(self.label_text("No columns returned."))
                .center(Fill)
                .into();
        }

        let (start_column, end_column, left_spacer, right_spacer) =
            self.visible_column_range(layout, results.columns.len());

        let header_row = self.results_header(
            &results.columns,
            &results.column_kinds,
            layout,
            start_column,
            end_column,
            left_spacer,
            right_spacer,
        );

        let header = scrollable(header_row)
            .id(results_horizontal_scroll_id())
            .on_scroll(|value| {
                Message::Workspace(crate::app::features::workspace::Message::Results(
                    crate::app::features::workspace::results::Message::ResultsHorizontalScrolled(
                        value,
                    ),
                ))
            })
            .direction(iced::widget::scrollable::Direction::Horizontal(
                iced::widget::scrollable::Scrollbar::hidden(),
            ))
            .width(Fill)
            .height(Length::Shrink);

        let row_count = results.rows.len();
        let (start_row, end_row, top_spacer, bottom_spacer) = self.visible_row_range(row_count);

        let mut rows = iced::widget::column![].spacing(if self.modern() { 1 } else { 0 });

        if top_spacer > 0.0 {
            rows = rows.push(space::vertical().height(Length::Fixed(top_spacer)));
        }

        if start_row < end_row {
            for (offset, row_values) in results.rows[start_row..end_row].iter().enumerate() {
                let row_index = start_row + offset;
                rows = rows.push(self.results_row(
                    row_index,
                    row_values,
                    row_count,
                    &results.columns,
                    &results.column_kinds,
                    layout,
                    start_column,
                    end_column,
                    left_spacer,
                    right_spacer,
                ));
            }
        }

        if bottom_spacer > 0.0 {
            rows = rows.push(space::vertical().height(Length::Fixed(bottom_spacer)));
        }

        let modern = self.modern();
        let body_content = container(rows)
            .width(Fill)
            .padding(iced::padding::bottom(
                self.scale_f32(RESULTS_SCROLLBAR_GUTTER),
            ))
            .style(move |theme| container::Style {
                background: modern
                    .then(|| Background::Color(crate::ui::theme::tokens(theme).line_weak)),
                ..container::Style::default()
            });

        let body = scrollable(body_content)
            .id(results_vertical_scroll_id())
            .on_scroll(|value| {
                Message::Workspace(crate::app::features::workspace::Message::Results(
                    crate::app::features::workspace::results::Message::ResultsVerticalScrolled(
                        value,
                    ),
                ))
            })
            .direction(iced::widget::scrollable::Direction::Both {
                vertical: iced::widget::scrollable::Scrollbar::default(),
                horizontal: iced::widget::scrollable::Scrollbar::default(),
            })
            .width(Fill)
            .height(Fill);

        let table = iced::widget::column![header, body]
            .spacing(0)
            .width(Fill)
            .height(Fill);

        let table_area = mouse_area(table)
            .on_move(|position| {
                Message::Workspace(crate::app::features::workspace::Message::Results(
                    crate::app::features::workspace::results::Message::ColumnResizeMove(position.x),
                ))
            })
            .on_release(Message::Workspace(
                crate::app::features::workspace::Message::Results(
                    crate::app::features::workspace::results::Message::ColumnResizeEnd,
                ),
            ))
            .on_exit(Message::Workspace(
                crate::app::features::workspace::Message::Results(
                    crate::app::features::workspace::results::Message::ColumnResizeEnd,
                ),
            ));

        let table_area = mouse_area(table_area).on_move(|position| {
            Message::Workspace(crate::app::features::workspace::Message::Results(
                crate::app::features::workspace::results::Message::CursorMoved(position),
            ))
        });

        let table_area: Element<'_, Message> = if self.workspace.results.row_context_menu.is_some()
        {
            stack![table_area, self.row_context_menu_overlay()].into()
        } else {
            table_area.into()
        };

        container(table_area).width(Fill).height(Fill).into()
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn results_header(
        &self,
        columns: &[String],
        column_kinds: &[ColumnKind],
        layout: LayoutMode,
        start_column: usize,
        end_column: usize,
        left_spacer: f32,
        right_spacer: f32,
    ) -> Element<'_, Message> {
        let mut header_row = row![].spacing(0);

        header_row = header_row.push(self.header_cell(
            String::from("#"),
            ROW_HEADER_WIDTH,
            None,
            None,
            true,
            false,
        ));

        if left_spacer > 0.0 {
            header_row = header_row.push(space::horizontal().width(Length::Fixed(left_spacer)));
        }

        for index in start_column..end_column {
            let Some(column_name) = columns.get(index) else {
                continue;
            };
            let width = self.display_column_width(layout, index);
            let kind = column_kinds.get(index).copied();
            header_row = header_row.push(self.header_cell(
                column_name.clone(),
                width,
                Some(index),
                kind,
                false,
                right_spacer == 0.0 && index + 1 == columns.len(),
            ));
        }

        if right_spacer > 0.0 {
            header_row = header_row.push(space::horizontal().width(Length::Fixed(right_spacer)));
        }

        header_row.into()
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn results_row(
        &self,
        row_index: usize,
        values: &[String],
        row_count: usize,
        columns: &[String],
        column_kinds: &[ColumnKind],
        layout: LayoutMode,
        start_column: usize,
        end_column: usize,
        left_spacer: f32,
        right_spacer: f32,
    ) -> Element<'_, Message> {
        let mut row_cells = row![].spacing(0);

        let is_row_selected = self.row_is_selected(row_index);
        let is_last_row = row_index + 1 == row_count;

        row_cells = row_cells.push(self.row_header_cell(
            row_index,
            (row_index + 1).to_string(),
            is_row_selected,
            is_last_row,
        ));

        if left_spacer > 0.0 {
            row_cells = row_cells.push(space::horizontal().width(Length::Fixed(left_spacer)));
        }

        for column_index in start_column..end_column {
            let value = values.get(column_index).map(String::as_str).unwrap_or("");
            let width = self.display_column_width(layout, column_index);
            let kind = column_kinds
                .get(column_index)
                .copied()
                .unwrap_or(ColumnKind::Unknown);
            let can_edit = self.is_editable_query_active() && self.cell_is_editable(kind, value);
            let can_open_modal = self.should_open_text_modal_value(value, kind);
            let can_inline_edit = can_edit && !can_open_modal;
            let is_active = self.workspace.results.editing_cell == Some((row_index, column_index));
            let is_selected =
                self.workspace.results.selected_cell == Some((row_index, column_index));
            let relation = columns
                .get(column_index)
                .and_then(|name| self.relation_for_column(name));
            row_cells = row_cells.push(self.data_cell(
                row_index,
                column_index,
                value,
                width,
                can_inline_edit,
                can_open_modal,
                is_active,
                is_selected,
                is_row_selected,
                is_last_row && right_spacer == 0.0 && column_index + 1 == columns.len(),
                relation,
            ));
        }

        if right_spacer > 0.0 {
            row_cells = row_cells.push(space::horizontal().width(Length::Fixed(right_spacer)));
        }

        row_cells.into()
    }

    pub(crate) fn row_context_menu_overlay(&self) -> Element<'_, Message> {
        let Some(state) = &self.workspace.results.row_context_menu else {
            return container(space::horizontal()).into();
        };

        let can_delete = (self.is_editable_query_active()
            || self.workspace.query.last_query_was_table)
            && self.workspace.query.table.is_some()
            && !self.workspace.query.running
            && !self.workspace.results.applying_changes
            && self.workspace.results.pending_edits.is_empty();

        let copy_button = button(self.label_text("Copy"))
            .padding(self.button_padding_tight())
            .width(Fill)
            .style(compact_button_style)
            .on_press(Message::Workspace(
                crate::app::features::workspace::Message::Results(
                    crate::app::features::workspace::results::Message::CopyRows,
                ),
            ));

        let copy_headers_button = button(self.label_text("Copy with column names"))
            .padding(self.button_padding_tight())
            .width(Fill)
            .style(compact_button_style)
            .on_press(Message::Workspace(
                crate::app::features::workspace::Message::Results(
                    crate::app::features::workspace::results::Message::CopyRowsWithHeaders,
                ),
            ));

        let copy_sql_button = button(self.label_text("Copy as SQL Insert"))
            .padding(self.button_padding_tight())
            .width(Fill)
            .style(compact_button_style)
            .on_press(Message::Workspace(
                crate::app::features::workspace::Message::Results(
                    crate::app::features::workspace::results::Message::CopyRowsAsInsert,
                ),
            ));

        let export_csv_button = button(self.label_text("Export as CSV"))
            .padding(self.button_padding_tight())
            .width(Fill)
            .style(compact_button_style)
            .on_press(Message::Workspace(
                crate::app::features::workspace::Message::Results(
                    crate::app::features::workspace::results::Message::ExportResultsCsv,
                ),
            ));

        let export_json_button = button(self.label_text("Export as JSON"))
            .padding(self.button_padding_tight())
            .width(Fill)
            .style(compact_button_style)
            .on_press(Message::Workspace(
                crate::app::features::workspace::Message::Results(
                    crate::app::features::workspace::results::Message::ExportResultsJson,
                ),
            ));

        let export_xlsx_button = button(self.label_text("Export as XLSX"))
            .padding(self.button_padding_tight())
            .width(Fill)
            .style(compact_button_style)
            .on_press(Message::Workspace(
                crate::app::features::workspace::Message::Results(
                    crate::app::features::workspace::results::Message::ExportResultsXlsx,
                ),
            ));

        let delete_button: Element<'_, Message> = {
            let button = button(self.label_text("Delete Row"))
                .padding(self.button_padding_tight())
                .width(Fill)
                .style(compact_button_style);
            if can_delete {
                button
                    .on_press(Message::Workspace(
                        crate::app::features::workspace::Message::Results(
                            crate::app::features::workspace::results::Message::DeleteRows,
                        ),
                    ))
                    .into()
            } else {
                button.into()
            }
        };

        let menu = iced::widget::column![
            copy_button,
            copy_headers_button,
            copy_sql_button,
            export_csv_button,
            export_json_button,
            export_xlsx_button,
            delete_button
        ]
        .spacing(self.scale_u16(2))
        .width(Length::Fixed(self.scale_f32(220.0)));

        let menu = if self.chat_available() && self.workspace.results.current.is_some() {
            menu.push(
                button(
                    row![
                        self.icon_text(ICON_CHAT_AI).size(self.label_text_size()),
                        self.label_text("Explain these results"),
                    ]
                    .spacing(self.scale_u16(6))
                    .align_y(Center),
                )
                .padding(self.button_padding_tight())
                .width(Fill)
                .style(compact_button_style)
                .on_press(Message::Ai(
                    crate::app::features::ai::Message::ChatExplainResults,
                )),
            )
        } else {
            menu
        };

        let panel = container(menu)
            .padding(self.scale_u16(4))
            .style(panel_border_style);
        let offset_x = self.scale_f32(4.0);
        let offset_y = self.scale_f32(12.0);

        let padding = Padding {
            top: state.position.y + offset_y,
            left: state.position.x + offset_x,
            right: 0.0,
            bottom: 0.0,
        };

        let menu_layer = container(panel)
            .padding(padding)
            .width(Fill)
            .height(Fill)
            .align_x(alignment::Horizontal::Left)
            .align_y(alignment::Vertical::Top);

        let backdrop = mouse_area(container(space::horizontal()).width(Fill).height(Fill))
            .on_press(Message::Workspace(
                crate::app::features::workspace::Message::Results(
                    crate::app::features::workspace::results::Message::CloseRowContextMenu,
                ),
            ))
            .on_right_press(Message::Workspace(
                crate::app::features::workspace::Message::Results(
                    crate::app::features::workspace::results::Message::CloseRowContextMenu,
                ),
            ));

        stack![backdrop, menu_layer].width(Fill).height(Fill).into()
    }

    pub(crate) fn results_status_bar(&self, layout: LayoutMode) -> Element<'_, Message> {
        if let Some(state) = self.active_diagram() {
            return self.diagram_status_bar(state);
        }
        let status_button_padding = self.scale_padding([2, 4]);
        let status_icon_size = self.label_text_size();
        let pending = self.workspace.results.pending_edits.len();
        let mut status_parts = Vec::new();
        let table_name = self
            .workspace
            .query
            .table
            .as_ref()
            .or(self.workspace.selected_table.as_ref());
        if self.modern() {
            if let (Some(table), Some(results), Some((row, column))) = (
                table_name,
                self.workspace.results.current.as_ref(),
                self.workspace.results.selected_cell,
            ) {
                let column_name = results
                    .columns
                    .get(column)
                    .cloned()
                    .unwrap_or_else(|| column.to_string());
                status_parts.push(format!("{table}.{column_name} · row {}", row + 1));
                if let Some(kind) = results.column_kinds.get(column) {
                    status_parts.push(kind.display_label().to_ascii_lowercase());
                }
            } else if let Some(table) = table_name {
                status_parts.push(table.clone());
            }
            if let Some(results) = self.workspace.results.current.as_ref() {
                status_parts.push(format!(
                    "{} {}",
                    crate::i18n::tr("rows"),
                    results.rows.len()
                ));
            }
        }
        let separator = if self.modern() { "   " } else { " | " };
        let status_text = status_parts.join(separator);

        let pending_badge: Element<'_, Message> = if pending > 0 {
            let accent = crate::ui::theme::appearance_color(
                self.settings.values.accent_color,
                self.theme().palette().primary,
            );
            let mut badge_bg = accent;
            badge_bg.a = 0.20;
            container(
                text(format!("{} {}", crate::i18n::tr("pending"), pending))
                    .font(self.ui_font())
                    .size(self.label_text_size().saturating_sub(2))
                    .color(accent),
            )
            .padding([self.scale_u16(1), self.scale_u16(6)])
            .style(move |_| container::Style {
                background: Some(Background::Color(badge_bg)),
                border: Border {
                    color: accent,
                    width: 1.0,
                    radius: Radius::from(10.0),
                },
                ..container::Style::default()
            })
            .into()
        } else {
            space::horizontal().into()
        };

        let has_table_results = self.workspace.results.current.is_some()
            && (self.workspace.query.table.is_some() || self.workspace.query.last_query_was_table);

        let can_apply = has_table_results
            && pending > 0
            && !self.workspace.results.applying_changes
            && self.is_editable_query_active();

        let can_discard =
            has_table_results && pending > 0 && !self.workspace.results.applying_changes;

        let actions: Element<'_, Message> = if can_discard || can_apply {
            let mut actions = row![].spacing(self.scale_u16(6)).align_y(Center);
            if can_discard {
                actions = actions.push(
                    button(self.icon_text(ICON_CLOSE_FILL).size(status_icon_size))
                        .padding(status_button_padding)
                        .style(compact_secondary_button_style)
                        .on_press(Message::Workspace(
                            crate::app::features::workspace::Message::Results(
                                crate::app::features::workspace::results::Message::DiscardChanges,
                            ),
                        )),
                );
            }
            if can_apply {
                actions = actions.push(
                    button(self.icon_text(ICON_CHECK_FILL).size(status_icon_size))
                        .padding(status_button_padding)
                        .style(compact_primary_button_style)
                        .on_press(Message::Workspace(
                            crate::app::features::workspace::Message::Results(
                                crate::app::features::workspace::results::Message::ApplyChanges,
                            ),
                        )),
                );
            }
            actions.into()
        } else {
            space::horizontal().width(Length::Shrink).into()
        };
        let pager = self.pagination_controls();
        let controls = row![pager, actions]
            .spacing(self.scale_u16(6))
            .align_y(Center);

        let content: Element<'_, Message> = match layout {
            LayoutMode::Wide => {
                let status_label: Element<'_, Message> = if status_text.is_empty() {
                    space::horizontal().into()
                } else {
                    container(self.label_text(status_text.clone()))
                        .padding(Padding {
                            top: 0.0,
                            right: 0.0,
                            bottom: 0.0,
                            left: self.scale_f32(2.0),
                        })
                        .into()
                };
                let status_word: Element<'_, Message> = if self.modern() {
                    let (word, color) = self.modern_status_word();
                    text(self.tracked_caption(word))
                        .font(self.ui_font())
                        .size(self.label_text_size())
                        .color(color)
                        .into()
                } else {
                    space::horizontal().width(Length::Shrink).into()
                };
                row![
                    status_label,
                    pending_badge,
                    space::horizontal(),
                    controls,
                    status_word,
                ]
                .spacing(self.scale_u16(6))
                .align_y(Center)
                .into()
            }
            LayoutMode::Compact => {
                if status_text.is_empty() {
                    controls.into()
                } else {
                    let status_label = container(self.label_text(status_text)).padding(Padding {
                        top: 0.0,
                        right: 0.0,
                        bottom: 0.0,
                        left: self.scale_f32(2.0),
                    });
                    let status_row = row![status_label, pending_badge]
                        .spacing(self.scale_u16(6))
                        .align_y(Center);
                    iced::widget::column![status_row, controls,]
                        .spacing(self.scale_u16(3))
                        .into()
                }
            }
        };

        content
    }

    pub(crate) fn bottom_bar_toggle(
        &self,
        icon: char,
        active: bool,
        tooltip: &'static str,
        shortcut: &ShortcutBinding,
        message: Message,
    ) -> Element<'_, Message> {
        let modern = self.modern();
        let icon: Element<'_, Message> = if modern {
            container(self.icon_text(icon).size(self.label_text_size()))
                .height(Fill)
                .align_x(iced::alignment::Horizontal::Center)
                .align_y(iced::alignment::Vertical::Center)
                .into()
        } else {
            self.icon_text(icon).size(self.label_text_size()).into()
        };
        let button =
            button(icon)
                .padding(self.scale_padding([2, 5]))
                .style(move |theme, status| {
                    let mut color = theme.palette().text;
                    color.a = if active { 0.9 } else { 0.42 };
                    if modern {
                        compact_button_style_with_text_color(theme, status, color)
                    } else {
                        iced::widget::button::Style {
                            text_color: color,
                            ..iced::widget::button::Style::default()
                        }
                    }
                });
        let button = if modern { button.height(Fill) } else { button }.on_press(message);
        self.with_shortcut_tooltip(button, tooltip, shortcut)
    }

    pub(crate) fn classic_footer(&self, layout: LayoutMode) -> Element<'_, Message> {
        let sidebar = self.bottom_bar_toggle(
            ICON_BOTTOM_SIDEBAR,
            !self.shell.sidebar_hidden,
            "Toggle sidebar",
            &self.settings.values.toggle_sidebar_shortcut,
            Message::Shell(crate::app::shell::Message::ToggleSidebar),
        );
        let chat: Element<'_, Message> = if self.settings.values.ai_enabled {
            self.bottom_bar_toggle(
                ICON_BOTTOM_CHAT,
                self.ai.chat_open,
                "Toggle AI chat sidebar",
                &self.settings.values.toggle_chat_sidebar_shortcut,
                Message::Ai(crate::app::features::ai::Message::ToggleChatSidebar),
            )
        } else {
            space::horizontal().width(Length::Shrink).into()
        };

        container(
            row![
                sidebar,
                container(self.results_status_bar(layout)).width(Fill),
                chat
            ]
            .spacing(self.scale_u16(6))
            .align_y(Center),
        )
        .padding(self.scale_padding([2, 6]))
        .width(Fill)
        .style(panel_style)
        .into()
    }

    pub(crate) fn header_cell(
        &self,
        label: String,
        width: f32,
        column_index: Option<usize>,
        column_kind: Option<ColumnKind>,
        round_top_left: bool,
        round_top_right: bool,
    ) -> Element<'_, Message> {
        let show_column_kind = (self.modern()
            || matches!(
                self.settings.values.result_grid_density,
                ResultGridDensity::Comfortable
            ))
            && column_index.is_some();
        let resize_handle_width = self.scale_f32(COLUMN_RESIZE_HANDLE_WIDTH);
        let type_gap = self.scale_f32(8.0);
        let type_tag_width = if show_column_kind && !self.modern() {
            self.scale_f32(78.0)
        } else {
            0.0
        };
        let left_padding = self.scale_f32(6.0);
        let right_padding = if column_index.is_some() {
            0.0
        } else {
            self.scale_f32(6.0)
        };
        let padding_width = left_padding + right_padding;
        let available_width = if column_index.is_some() {
            (width - resize_handle_width - padding_width - type_tag_width - type_gap).max(40.0)
        } else {
            (width - padding_width).max(40.0)
        };

        let max_chars = max_chars_for_width(available_width);
        let (display, truncated) = truncate_with_ellipsis(&label, max_chars);

        let label_text = self
            .header_label_text(display)
            .wrapping(text::Wrapping::None);

        let mut kind_color = self.theme().palette().text;
        kind_color.a = 0.55;
        let kind_text = column_kind.map(|kind| {
            text(kind.display_label())
                .font(self.ui_font())
                .size(self.scale_f32(10.0))
                .color(kind_color)
                .wrapping(text::Wrapping::None)
        });

        let content: Element<'_, Message> =
            if let Some(index) = column_index {
                let handle_line = container(space::horizontal())
                    .width(Length::Fixed(2.0))
                    .height(if self.modern() {
                        Fill
                    } else {
                        Length::Fixed(self.scale_f32(COLUMN_RESIZE_HANDLE_HEIGHT))
                    })
                    .style(resize_handle_style);

                let handle = mouse_area(
                    container(handle_line)
                        .width(Length::Fixed(resize_handle_width))
                        .height(Fill)
                        .align_x(iced::alignment::Horizontal::Right),
                )
                .interaction(mouse::Interaction::ResizingColumn)
                .on_press(Message::Workspace(
                    crate::app::features::workspace::Message::Results(
                        crate::app::features::workspace::results::Message::ColumnResizeStart(index),
                    ),
                ));
                let sortable_header: Element<'_, Message> = if show_column_kind {
                    if let Some(kind_text) = kind_text {
                        if self.modern() {
                            iced::widget::column![
                                container(label_text).width(Fill).clip(true),
                                container(kind_text).width(Fill).clip(true),
                            ]
                            .spacing(self.scale_f32(2.0))
                            .into()
                        } else {
                            row![
                                container(label_text).width(Fill),
                                container(kind_text)
                                    .width(Length::Fixed(type_tag_width))
                                    .align_x(iced::alignment::Horizontal::Right)
                                    .clip(true),
                            ]
                            .spacing(type_gap)
                            .align_y(Center)
                            .into()
                        }
                    } else {
                        container(label_text).width(Fill).into()
                    }
                } else {
                    container(label_text).width(Fill).into()
                };
                let label = mouse_area(container(sortable_header).width(Fill))
                .interaction(mouse::Interaction::Pointer)
                .on_press(Message::Workspace(crate::app::features::workspace::Message::Results(
                    crate::app::features::workspace::results::Message::ResultHeaderPressed {
                        column: index,
                    },
                )));

                row![label, handle].spacing(0).align_y(Center).into()
            } else {
                let label = mouse_area(container(label_text).width(Fill))
                    .interaction(mouse::Interaction::Pointer)
                    .on_press(Message::Workspace(
                        crate::app::features::workspace::Message::Results(
                            crate::app::features::workspace::results::Message::SelectAllRows,
                        ),
                    ));
                row![label].align_y(Center).into()
            };

        let mut radius = iced::border::Radius::default();
        if round_top_left {
            radius = radius.top_left(ui_radius());
        }
        if round_top_right {
            radius = radius.top_right(ui_radius());
        }

        let cell = container(content)
            .padding(Padding {
                top: self.scale_u16(4),
                right: right_padding,
                bottom: self.scale_u16(4),
                left: left_padding,
            })
            .width(Length::Fixed(width))
            .height(Length::Fixed(self.result_header_height()))
            .style(move |theme| table_header_style(theme, radius))
            .clip(true);

        if truncated {
            self.with_tooltip(cell, label)
        } else {
            cell.into()
        }
    }

    pub(crate) fn row_header_cell(
        &self,
        row: usize,
        label: String,
        is_selected: bool,
        round_bottom_left: bool,
    ) -> Element<'_, Message> {
        let radius = if round_bottom_left {
            iced::border::Radius::default().bottom_left(ui_radius())
        } else {
            iced::border::Radius::default()
        };

        let cell = container(self.label_text(label))
            .padding([self.scale_u16(4), self.scale_u16(6)])
            .width(Length::Fixed(ROW_HEADER_WIDTH))
            .height(Length::Fixed(self.result_row_height()))
            .style(move |theme| table_row_header_style(theme, is_selected, radius))
            .clip(true);

        mouse_area(cell)
            .on_press(Message::Workspace(
                crate::app::features::workspace::Message::Results(
                    crate::app::features::workspace::results::Message::RowHeaderPressed { row },
                ),
            ))
            .on_right_press(Message::Workspace(
                crate::app::features::workspace::Message::Results(
                    crate::app::features::workspace::results::Message::RowContextMenuRequested {
                        row,
                    },
                ),
            ))
            .on_move(move |_| {
                Message::Workspace(crate::app::features::workspace::Message::Results(
                    crate::app::features::workspace::results::Message::RowSelectionDragged { row },
                ))
            })
            .into()
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn data_cell(
        &self,
        row: usize,
        column: usize,
        value: &str,
        width: f32,
        can_inline_edit: bool,
        can_open_modal: bool,
        is_active: bool,
        is_selected: bool,
        row_selected: bool,
        round_bottom_right: bool,
        relation: Option<&RelationInfo>,
    ) -> Element<'_, Message> {
        let is_null = is_null_value(value);
        let is_focused = is_selected || is_active;
        let radius = if round_bottom_right {
            iced::border::Radius::default().bottom_right(ui_radius())
        } else {
            iced::border::Radius::default()
        };
        let is_pending = self
            .workspace
            .results
            .pending_edits
            .contains_key(&(row, column));
        let accent = crate::ui::theme::appearance_color(
            self.settings.values.accent_color,
            self.theme().palette().primary,
        );
        let pulse = self.ai_pulse_amount();

        if can_inline_edit && is_active {
            let placeholder = if is_null { "NULL" } else { "" };
            let display_value = if is_null { "" } else { value };

            let input = history_input(
                cell_input_id(row, column),
                placeholder,
                display_value,
                move |value| {
                    Message::Workspace(crate::app::features::workspace::Message::Results(
                        crate::app::features::workspace::results::Message::ResultCellEdited {
                            row,
                            column,
                            value,
                        },
                    ))
                },
            )
            .padding([self.scale_u16(3), self.scale_u16(5)])
            .size(self.input_text_size())
            .font(self.ui_font())
            .width(Fill)
            .style(table_input_style);

            return container(input)
                .width(Length::Fixed(width))
                .height(Length::Fixed(self.result_row_height()))
                .style(move |theme| {
                    let mut style = table_cell_style(theme, is_focused, row_selected, radius);
                    if is_pending {
                        style.border.width = 2.0;
                        let mut c = accent;
                        c.a = 0.35 + 0.45 * pulse;
                        style.border.color = c;
                    }
                    style
                })
                .clip(true)
                .into();
        }

        let display_value = if is_null { "NULL" } else { value };
        let spacing = self.scale_u16(2);
        let mut available_width = (width - 12.0).max(40.0);
        let icon_horizontal_padding = self.button_padding_icon()[1];
        if relation.is_some() {
            let icon_width = self.icon_text_size() as f32 + icon_horizontal_padding * 2.0 + spacing;
            available_width = (available_width - icon_width).max(24.0);
        }
        let max_chars = max_chars_for_width(available_width)
            .saturating_sub(1)
            .clamp(1, READONLY_CELL_CHAR_LIMIT);
        let (display, _) = compact_inline_preview(display_value, max_chars);

        let show_vector_indicator = if self.connections.current.driver == DatabaseDriver::PostgreSql
            && self.is_structure_view_active()
        {
            self.workspace
                .results
                .current
                .as_ref()
                .is_some_and(|results| {
                    results
                        .columns
                        .get(column)
                        .is_some_and(|name| name.eq_ignore_ascii_case("data_type"))
                        && is_postgres_vector_type_name(display_value)
                })
        } else {
            false
        };

        let null_text_color = if is_null {
            let mut c = self.theme().palette().text;
            c.a = 0.55;
            Some(c)
        } else {
            None
        };

        let label: Element<'_, Message> = if show_vector_indicator {
            let icon = self
                .icon_text(ICON_POSTGRES_VECTOR)
                .size(self.input_text_size());
            let txt = self.input_text(display).wrapping(text::Wrapping::None);
            let txt = if let Some(color) = null_text_color {
                txt.color(color)
            } else {
                txt
            };
            row![icon, txt]
                .spacing(self.scale_u16(4))
                .align_y(Center)
                .into()
        } else {
            let txt = self.input_text(display).wrapping(text::Wrapping::None);
            let txt = if let Some(color) = null_text_color {
                txt.color(color)
            } else if self.modern() && column == 0 {
                txt.color(self.driver_marker_color(self.connections.current.driver))
            } else {
                txt
            };
            txt.into()
        };
        let label = container(label).width(Fill).clip(true);

        let content: Element<'_, Message> = if let Some(relation) = relation.filter(|_| !is_null) {
            let icon_height = (self.result_row_height() - self.scale_u16(3) * 2.0).max(0.0);
            let icon_content = container(self.icon_text(ICON_ARROW_RIGHT_UP_LINE))
                .center_y(Length::Fixed(icon_height));
            let icon_button = button(icon_content)
                .padding([0.0, icon_horizontal_padding])
                .style(compact_button_style)
                .on_press(Message::Workspace(
                    crate::app::features::workspace::Message::Results(
                        crate::app::features::workspace::results::Message::FollowRelation {
                            table: relation.referenced_table.clone(),
                            column: relation.referenced_column.clone(),
                            value: value.to_string(),
                        },
                    ),
                ));
            row![label, icon_button]
                .spacing(spacing)
                .align_y(Center)
                .into()
        } else {
            row![label].align_y(Center).into()
        };

        let cell = container(content)
            .padding([self.scale_u16(3), self.scale_u16(5)])
            .width(Length::Fixed(width))
            .height(Length::Fixed(self.result_row_height()))
            .align_y(Center)
            .style(move |theme| {
                let mut style = table_cell_style(theme, is_focused, row_selected, radius);
                if is_pending {
                    style.border.width = 2.0;
                    let mut c = accent;
                    c.a = 0.35 + 0.45 * pulse;
                    style.border.color = c;
                }
                style
            })
            .clip(true);

        let mut area = mouse_area(cell)
            .on_press(Message::Workspace(
                crate::app::features::workspace::Message::Results(
                    crate::app::features::workspace::results::Message::CellFocused { row, column },
                ),
            ))
            .on_right_press(Message::Workspace(
                crate::app::features::workspace::Message::Results(
                    crate::app::features::workspace::results::Message::RowContextMenuRequested {
                        row,
                    },
                ),
            ))
            .on_move(move |_| {
                Message::Workspace(crate::app::features::workspace::Message::Results(
                    crate::app::features::workspace::results::Message::RowSelectionDragged { row },
                ))
            });

        if can_inline_edit || can_open_modal {
            area = area
                .interaction(mouse::Interaction::Text)
                .on_double_click(Message::Workspace(
                    crate::app::features::workspace::Message::Results(
                        crate::app::features::workspace::results::Message::CellDoubleClicked {
                            row,
                            column,
                        },
                    ),
                ));
        }

        area.into()
    }

    pub(crate) fn text_cell_modal(&self) -> Element<'_, Message> {
        let layout = self.shell.layout_mode;
        let modal_width = self.scale_f32(520.0);
        let modal_height = self.scale_f32(420.0);
        let modal_padding = self.scale_u16(12);

        let title = if let (Some(results), Some((row, column))) = (
            self.workspace.results.current.as_ref(),
            self.workspace.results.text_modal_cell,
        ) {
            let column_name = results
                .columns
                .get(column)
                .cloned()
                .unwrap_or_else(|| format!("{} {}", crate::i18n::tr("Column"), column + 1));
            format!("Row {} - {}", row + 1, column_name)
        } else {
            String::from("Cell")
        };

        let header = row![
            self.title_text(title),
            space::horizontal(),
            button(self.action_label("Close", ICON_CLOSE_LINE, layout))
                .padding(self.button_padding())
                .style(compact_button_style)
                .on_press(Message::Workspace(
                    crate::app::features::workspace::Message::Results(
                        crate::app::features::workspace::results::Message::CloseTextModal
                    )
                )),
        ]
        .spacing(self.scale_u16(6))
        .align_y(Center);

        let editor: Element<'_, Message> = if self.workspace.results.text_modal_is_json {
            text_editor(&self.workspace.results.text_modal_content)
                .height(Fill)
                .on_action(|value| {
                    Message::Workspace(crate::app::features::workspace::Message::Results(
                        crate::app::features::workspace::results::Message::TextModalAction(value),
                    ))
                })
                .wrapping(text::Wrapping::WordOrGlyph)
                .font(self.ui_font())
                .size(self.input_text_size())
                .highlight("json", self.settings.theme_choice.highlighter())
                .into()
        } else {
            text_editor(&self.workspace.results.text_modal_content)
                .height(Fill)
                .on_action(|value| {
                    Message::Workspace(crate::app::features::workspace::Message::Results(
                        crate::app::features::workspace::results::Message::TextModalAction(value),
                    ))
                })
                .wrapping(text::Wrapping::WordOrGlyph)
                .font(self.ui_font())
                .size(self.input_text_size())
                .into()
        };

        let editor = container(editor)
            .padding(self.scale_u16(6))
            .width(Fill)
            .height(Fill)
            .style(panel_border_style);

        let can_save = self.text_modal_can_save();
        let has_changes = self.workspace.results.text_modal_content.text()
            != self.workspace.results.text_modal_original;

        let save_button: Element<'_, Message> = {
            let button = button(self.action_label("Save", ICON_CHECK_FILL, layout))
                .padding(self.button_padding())
                .style(compact_primary_button_style);
            if can_save && has_changes {
                button
                    .on_press(Message::Workspace(
                        crate::app::features::workspace::Message::Results(
                            crate::app::features::workspace::results::Message::TextModalSave,
                        ),
                    ))
                    .into()
            } else {
                button.into()
            }
        };

        let content = iced::widget::column![
            header,
            editor,
            row![space::horizontal(), save_button]
                .align_y(Center)
                .spacing(self.scale_u16(6)),
        ]
        .spacing(self.scale_u16(10))
        .width(Fill)
        .height(Fill);

        let modal = container(content)
            .padding(modal_padding)
            .width(Length::Fixed(modal_width))
            .height(Length::Fixed(modal_height))
            .style(panel_style);

        let backdrop = mouse_area(
            container(space::horizontal())
                .width(Fill)
                .height(Fill)
                .style(move |theme| {
                    modal_backdrop_style(theme, self.settings.values.modal_backdrop_dim)
                }),
        )
        .on_press(Message::Shell(crate::app::shell::Message::ModalBlocked))
        .on_scroll(|_| Message::Shell(crate::app::shell::Message::ModalBlocked))
        .interaction(mouse::Interaction::Idle);

        let modal_layer = container(modal).width(Fill).height(Fill).center(Fill);

        stack![backdrop, modal_layer]
            .width(Fill)
            .height(Fill)
            .into()
    }
}
