use crate::app::core::App;
use crate::app::message::Message;
use crate::app::types::LayoutMode;
use crate::app::view::history_input;
use crate::constants::{
    BASE_EDITOR_HEIGHT_COMPACT, BASE_HISTORY_HEIGHT_COMPACT, BASE_HISTORY_HEIGHT_EXPANDED_COMPACT,
    BASE_HISTORY_HEIGHT_EXPANDED_WIDE, BASE_HISTORY_HEIGHT_WIDE, ICON_ARROW_DOWN_S_LINE,
    ICON_ARROW_UP_S_LINE, ICON_CHAT_AI, ICON_CLIPBOARD_LINE, ICON_MODERN_HISTORY, ICON_STRUCTURE,
    ICON_TERMINAL, ICON_TRASH_LINE, QUERY_SUGGESTION_VISIBLE_ROWS, TRIGGER_EDITOR_EXTRA_HEIGHT,
};
use crate::model::connection::DatabaseDriver;
use crate::model::settings::HistoryViewMode;
use crate::ui::ids::text_field_id;
use crate::ui::styles::{
    compact_button_style, compact_button_style_with_text_color, compact_input_style,
    compact_pick_list_menu_style, compact_pick_list_style, compact_primary_button_style,
    panel_border_style, sidebar_table_button_style,
};
use crate::ui::theme::{carbon_button_text_color, ui_radius};
use crate::ui::widgets::bounds_probe::BoundsProbe;
use crate::utils::helpers::lerp_color;
use crate::utils::text::truncate_with_ellipsis;
use iced::Border;
use iced::widget::{button, canvas, container, pick_list, row, scrollable, space, stack, text};
use iced::{Center, Element, Fill, Font, Length, Padding};

impl App {
    pub(crate) fn ai_prompt_bar(&self) -> Element<'_, Message> {
        row![space::horizontal(), self.context_actions()]
            .spacing(self.scale_u16(6))
            .align_y(Center)
            .width(Fill)
            .into()
    }

    pub(crate) fn ai_query_button(&self) -> Element<'_, Message> {
        let content = if self.ai.is_generating_ai || self.ai.is_fixing_query_with_ai {
            self.icon_text(self.loading_spinner_icon())
        } else {
            self.icon_text(ICON_CHAT_AI)
        };

        let pulse_t = if self.ai.is_generating_ai || self.ai.is_fixing_query_with_ai {
            self.ai_pulse_amount()
        } else {
            0.0
        };
        let button =
            button(content)
                .padding(self.button_padding_icon())
                .style(move |theme, status| {
                    let palette = theme.extended_palette();
                    let base = carbon_button_text_color(theme, palette.background.weakest.text);
                    let target = palette.primary.base.color;
                    let text_color = lerp_color(base, target, pulse_t);
                    compact_button_style_with_text_color(theme, status, text_color)
                });

        let button: Element<'_, Message> =
            if self.ai.is_generating_ai || self.ai.is_fixing_query_with_ai {
                button.into()
            } else {
                button
                    .on_press(Message::Ai(
                        crate::app::features::ai::Message::ToggleChatSidebar,
                    ))
                    .into()
            };

        self.with_tooltip(button, "Chat with AI")
    }

    pub(crate) fn postgres_terminal_button(&self) -> Element<'_, Message> {
        let button = button(self.icon_text(ICON_TERMINAL))
            .padding(self.button_padding_icon())
            .style(compact_button_style);
        let button: Element<'_, Message> = if self.can_open_postgres_terminal() {
            button
                .on_press(Message::Connections(
                    crate::app::features::connections::Message::OpenPostgresTerminal,
                ))
                .into()
        } else {
            button.into()
        };

        self.with_tooltip(button, "PostgreSQL terminal")
    }

    pub(crate) fn structure_toggle_button(&self) -> Element<'_, Message> {
        let structure_active = self.is_structure_view_active();
        let tooltip_label = if structure_active {
            "View Data"
        } else {
            "View Structure"
        };
        let can_toggle_structure =
            self.workspace.selected_table.is_some() && !self.workspace.query.running;

        let button = button(self.icon_text(ICON_STRUCTURE))
            .padding(self.button_padding_icon())
            .style(compact_button_style);

        let button: Element<'_, Message> = if can_toggle_structure {
            button
                .on_press(Message::Workspace(
                    crate::app::features::workspace::Message::Query(
                        crate::app::features::workspace::query::Message::ToggleStructureView,
                    ),
                ))
                .into()
        } else {
            button.into()
        };

        self.with_tooltip(button, tooltip_label)
    }

    pub(crate) fn query_suggestions_overlay(&self) -> Option<Element<'_, Message>> {
        if !self.workspace.query.suggestions_open || self.workspace.query.suggestions.is_empty() {
            return None;
        }
        let editor = self.workspace.query.editor_bounds.get();
        if editor.width <= 0.0 {
            return None;
        }
        let cursor = self.workspace.query.editor.cursor_screen_position()?;

        let row_height = self.label_text_size() as f32 + self.scale_f32(9.0);
        let visible = self
            .workspace
            .query
            .suggestions
            .len()
            .min(QUERY_SUGGESTION_VISIBLE_ROWS);
        let first = self
            .workspace
            .query
            .suggestion_index
            .saturating_sub(visible.saturating_sub(1))
            .min(self.workspace.query.suggestions.len() - visible);
        let width = self.scale_f32(300.0);
        let height = visible as f32 * row_height + self.scale_f32(4.0);

        let mut list = iced::widget::column![].spacing(0);
        for (offset, suggestion) in self.workspace.query.suggestions[first..first + visible]
            .iter()
            .enumerate()
        {
            let index = first + offset;
            let selected = index == self.workspace.query.suggestion_index;
            list = list.push(
                button(
                    self.label_text(suggestion.clone())
                        .wrapping(text::Wrapping::None),
                )
                .padding([self.scale_u16(2), self.scale_u16(8)])
                .width(Fill)
                .height(Length::Fixed(row_height))
                .style(move |theme, status| sidebar_table_button_style(selected, theme, status))
                .on_press(Message::Workspace(
                    crate::app::features::workspace::Message::Query(
                        crate::app::features::workspace::query::Message::ApplyQuerySuggestion(
                            index,
                        ),
                    ),
                )),
            );
        }

        let (viewport_width, viewport_height) = if self.shell.window_size.height > 0.0 {
            (self.shell.window_size.width, self.shell.window_size.height)
        } else {
            (f32::MAX, f32::MAX)
        };
        let caret = cursor.y - self.workspace.query.editor.viewport_scroll();
        let below = editor.y + caret + self.workspace.query.editor.line_height();
        let top = if below + height > viewport_height {
            (editor.y + caret - height).max(0.0)
        } else {
            below
        };
        let left = (editor.x + cursor.x)
            .min((viewport_width - width).max(0.0))
            .max(0.0);

        Some(
            container(
                container(list)
                    .padding(self.scale_u16(2))
                    .width(Length::Fixed(width))
                    .height(Length::Fixed(height))
                    .style(panel_border_style),
            )
            .padding(Padding {
                top,
                left,
                right: 0.0,
                bottom: 0.0,
            })
            .width(Fill)
            .height(Fill)
            .into(),
        )
    }

    pub(crate) fn trigger_editor_panel(&self, layout: LayoutMode) -> Option<Element<'_, Message>> {
        if !matches!(
            self.connections.current.driver,
            DatabaseDriver::MySql | DatabaseDriver::MariaDb
        ) || self.workspace.explorer.selected_trigger.is_none()
        {
            return None;
        }

        let name_input = history_input(
            text_field_id("trigger-name"),
            "Trigger name",
            &self.workspace.explorer.trigger_edit_name,
            |value| {
                Message::Workspace(crate::app::features::workspace::Message::Explorer(
                    crate::app::features::workspace::explorer::Message::TriggerEditNameChanged(
                        value,
                    ),
                ))
            },
        )
        .padding(self.input_padding())
        .size(self.input_text_size())
        .font(self.ui_font())
        .style(compact_input_style)
        .width(Fill);
        const TRIGGER_TIMING_OPTIONS: [&str; 2] = ["BEFORE", "AFTER"];
        const TRIGGER_EVENT_OPTIONS: [&str; 3] = ["INSERT", "UPDATE", "DELETE"];
        let timing_selected = TRIGGER_TIMING_OPTIONS
            .iter()
            .copied()
            .find(|opt| opt.eq_ignore_ascii_case(&self.workspace.explorer.trigger_edit_timing));
        let event_selected = TRIGGER_EVENT_OPTIONS
            .iter()
            .copied()
            .find(|opt| opt.eq_ignore_ascii_case(&self.workspace.explorer.trigger_edit_event));
        let timing_input = pick_list(TRIGGER_TIMING_OPTIONS, timing_selected, |value: &str| {
            Message::Workspace(crate::app::features::workspace::Message::Explorer(
                crate::app::features::workspace::explorer::Message::TriggerEditTimingChanged(
                    value.to_string(),
                ),
            ))
        })
        .padding(self.input_padding())
        .text_size(self.input_text_size())
        .font(self.ui_font())
        .handle(self.pick_list_handle())
        .style(compact_pick_list_style)
        .menu_style(compact_pick_list_menu_style)
        .width(Fill);
        let event_input = pick_list(TRIGGER_EVENT_OPTIONS, event_selected, |value: &str| {
            Message::Workspace(crate::app::features::workspace::Message::Explorer(
                crate::app::features::workspace::explorer::Message::TriggerEditEventChanged(
                    value.to_string(),
                ),
            ))
        })
        .padding(self.input_padding())
        .text_size(self.input_text_size())
        .font(self.ui_font())
        .handle(self.pick_list_handle())
        .style(compact_pick_list_style)
        .menu_style(compact_pick_list_menu_style)
        .width(Fill);
        let definer_user_input = history_input(
            text_field_id("trigger-definer-user"),
            "Definer user",
            &self.workspace.explorer.trigger_edit_definer_user,
            |value| {
                Message::Workspace(crate::app::features::workspace::Message::Explorer(
                    crate::app::features::workspace::explorer::Message::TriggerEditDefinerUserChanged(
                        value,
                    ),
                ))
            },
        )
        .padding(self.input_padding())
        .size(self.input_text_size())
        .font(self.ui_font())
        .style(compact_input_style)
        .width(Fill);
        let definer_host_input = history_input(
            text_field_id("trigger-definer-host"),
            "Definer host",
            &self.workspace.explorer.trigger_edit_definer_host,
            |value| {
                Message::Workspace(crate::app::features::workspace::Message::Explorer(
                    crate::app::features::workspace::explorer::Message::TriggerEditDefinerHostChanged(
                        value,
                    ),
                ))
            },
        )
        .padding(self.input_padding())
        .size(self.input_text_size())
        .font(self.ui_font())
        .style(compact_input_style)
        .width(Fill);
        let update_button = button(self.button_text("Update Trigger"))
            .padding(self.button_padding())
            .style(compact_primary_button_style);
        let update_button: Element<'_, Message> =
            if self.workspace.explorer.can_update_selected_trigger(
                self.connections.current.driver,
                self.workspace.query.running,
                self.connections.pool.is_some(),
                &self.workspace.query.editor.content(),
            ) {
                update_button
                    .on_press(Message::Workspace(
                        crate::app::features::workspace::Message::Explorer(
                            crate::app::features::workspace::explorer::Message::UpdateTrigger,
                        ),
                    ))
                    .into()
            } else {
                update_button.into()
            };

        let fields: Element<'_, Message> = match layout {
            LayoutMode::Wide => row![
                name_input,
                timing_input,
                event_input,
                definer_user_input,
                definer_host_input,
                update_button,
            ]
            .spacing(self.scale_u16(6))
            .align_y(Center)
            .into(),
            LayoutMode::Compact => iced::widget::column![
                row![name_input, timing_input, event_input]
                    .spacing(self.scale_u16(6))
                    .align_y(Center),
                row![definer_user_input, definer_host_input, update_button]
                    .spacing(self.scale_u16(6))
                    .align_y(Center),
            ]
            .spacing(self.scale_u16(6))
            .into(),
        };

        Some(
            container(
                iced::widget::column![
                    self.label_text("Edit selected trigger"),
                    fields,
                    self.label_text("Edit the trigger body below, then update.")
                ]
                .spacing(self.scale_u16(6)),
            )
            .padding(self.scale_u16(6))
            .width(Fill)
            .style(panel_border_style)
            .into(),
        )
    }

    pub(crate) fn editor_panel(&self, layout: LayoutMode) -> Element<'_, Message> {
        let editor_height = match layout {
            LayoutMode::Wide => Fill,
            LayoutMode::Compact if self.trigger_editor_visible() => Length::Fixed(
                self.scale_f32(BASE_EDITOR_HEIGHT_COMPACT + TRIGGER_EDITOR_EXTRA_HEIGHT),
            ),
            LayoutMode::Compact => Length::Fixed(self.scale_f32(BASE_EDITOR_HEIGHT_COMPACT)),
        };

        let editor: Element<'_, Message> = self.workspace.query.editor.view().map(|value| {
            Message::Workspace(crate::app::features::workspace::Message::Query(
                crate::app::features::workspace::query::Message::QueryAction(value),
            ))
        });
        let editor = stack![
            container(editor).height(Fill).width(Fill),
            self.query_inline_suggestion_overlay(),
            canvas::Canvas::new(BoundsProbe {
                target: &self.workspace.query.editor_bounds,
            })
            .width(Fill)
            .height(Fill),
        ]
        .height(Fill)
        .width(Fill);

        let mut content = iced::widget::column![].spacing(self.scale_u16(4));
        if let Some(trigger_editor) = self.trigger_editor_panel(layout) {
            content = content.push(trigger_editor);
        }
        content = content.push(editor);

        let panel = container(content.width(Fill))
            .height(editor_height)
            .width(Fill);

        if self.modern() {
            panel.into()
        } else {
            panel
                .padding(1)
                .style(|theme: &iced::Theme| container::Style {
                    border: Border {
                        width: 1.0,
                        radius: ui_radius().into(),
                        color: theme.extended_palette().background.strong.color,
                    },
                    ..container::Style::default()
                })
                .clip(true)
                .into()
        }
    }

    pub(crate) fn query_inline_suggestion_overlay(&self) -> Element<'_, Message> {
        let Some(suggestion) = self.workspace.query.inline_suggestion.as_deref() else {
            return space::horizontal().into();
        };
        let Some(cursor) = self.workspace.query.editor.cursor_screen_position() else {
            return space::horizontal().into();
        };
        let y = cursor.y - self.workspace.query.editor.viewport_scroll();
        if y < 0.0 || y >= self.workspace.query.editor.viewport_height() {
            return space::horizontal().into();
        }
        let mut color = self.theme().palette().text;
        color.a = 0.38;
        container(iced::widget::column![
            space::vertical().height(Length::Fixed(y)),
            row![
                space::horizontal().width(Length::Fixed(cursor.x)),
                text(suggestion)
                    .font(self.editor_font())
                    .size(self.input_text_size())
                    .color(color)
                    .wrapping(text::Wrapping::None),
            ]
        ])
        .width(Fill)
        .height(Fill)
        .clip(true)
        .into()
    }

    pub(crate) fn history_view(&self, layout: LayoutMode) -> Element<'_, Message> {
        let height = match (layout, self.workspace.query.history_panel_expanded) {
            (LayoutMode::Wide, true) => self.scale_f32(BASE_HISTORY_HEIGHT_EXPANDED_WIDE),
            (LayoutMode::Compact, true) => self.scale_f32(BASE_HISTORY_HEIGHT_EXPANDED_COMPACT),
            (LayoutMode::Wide, false) => self.scale_f32(BASE_HISTORY_HEIGHT_WIDE),
            (LayoutMode::Compact, false) => self.scale_f32(BASE_HISTORY_HEIGHT_COMPACT),
        };

        let mut entries = iced::widget::column![].spacing(self.scale_u16(3));

        match self.workspace.query.history_view_mode {
            HistoryViewMode::Recent => {
                if self.workspace.query.history.is_empty() {
                    entries = entries.push(self.label_text("No recent queries."));
                } else {
                    for (index, query) in self.workspace.query.history.iter().enumerate() {
                        let (display, truncated) =
                            truncate_with_ellipsis(query, if self.modern() { 34 } else { 48 });

                        let label: Element<'_, Message> = if self.modern() {
                            let tokens = crate::ui::theme::tokens(&self.theme());
                            row![
                                self.icon_text(ICON_MODERN_HISTORY)
                                    .size(self.scale_f32(13.0))
                                    .color(tokens.ghost),
                                text(
                                    self.workspace
                                        .query
                                        .history_times
                                        .get(index)
                                        .cloned()
                                        .unwrap_or_default()
                                )
                                .font(self.ui_font())
                                .size(self.scale_f32(11.0))
                                .color(tokens.ghost),
                                self.label_text(display)
                                    .wrapping(text::Wrapping::None)
                                    .width(Fill),
                            ]
                            .spacing(self.scale_u16(8))
                            .align_y(Center)
                            .into()
                        } else {
                            self.label_text(display).into()
                        };

                        let entry: Element<'_, Message> = if truncated {
                            self.with_tooltip(
                                button(label)
                                    .padding(self.button_padding_tight())
                                    .width(Fill)
                                    .style(compact_button_style)
                                    .on_press(Message::Workspace(crate::app::features::workspace::Message::Query(crate::app::features::workspace::query::Message::HistorySelected(query.clone())))),
                                query.clone(),
                            )
                        } else {
                            button(label)
                                .padding(self.button_padding_tight())
                                .width(Fill)
                                .style(compact_button_style)
                                .on_press(Message::Workspace(crate::app::features::workspace::Message::Query(crate::app::features::workspace::query::Message::HistorySelected(query.clone()))))
                                .into()
                        };

                        entries = entries.push(entry);
                    }
                }
            }
            HistoryViewMode::Saved => {
                if self.settings.values.saved_queries.is_empty() {
                    entries = entries.push(self.label_text("No saved queries."));
                } else {
                    for (index, query) in self.settings.values.saved_queries.iter().enumerate() {
                        let (display, truncated) = truncate_with_ellipsis(query, 44);

                        let label = container(
                            self.label_text(display)
                                .wrapping(text::Wrapping::None)
                                .width(Fill),
                        )
                        .width(Fill)
                        .clip(true);

                        let select: Element<'_, Message> = if truncated {
                            self.with_tooltip(
                                button(label)
                                    .padding(self.button_padding_tight())
                                    .width(Fill)
                                    .style(compact_button_style)
                                    .on_press(Message::Workspace(crate::app::features::workspace::Message::Query(crate::app::features::workspace::query::Message::SavedQuerySelected(query.clone())))),
                                query.clone(),
                            )
                        } else {
                            button(label)
                                .padding(self.button_padding_tight())
                                .width(Fill)
                                .style(compact_button_style)
                                .on_press(Message::Workspace(crate::app::features::workspace::Message::Query(crate::app::features::workspace::query::Message::SavedQuerySelected(query.clone()))))
                                .into()
                        };

                        let copy = button(self.icon_text(ICON_CLIPBOARD_LINE))
                            .padding(self.button_padding_icon())
                            .style(compact_button_style)
                            .on_press(Message::Workspace(
                                crate::app::features::workspace::Message::Query(
                                    crate::app::features::workspace::query::Message::CopySavedQuery(
                                        query.clone(),
                                    ),
                                ),
                            ));

                        let delete = button(self.icon_text(ICON_TRASH_LINE))
                            .padding(self.button_padding_icon())
                            .style(compact_button_style)
                            .on_press(Message::Workspace(crate::app::features::workspace::Message::Query(
                                crate::app::features::workspace::query::Message::DeleteSavedQuery(
                                    index,
                                ),
                            )));

                        entries = entries.push(
                            row![select, copy, delete]
                                .spacing(self.scale_u16(3))
                                .align_y(Center)
                                .width(Fill),
                        );
                    }
                }
            }
        }

        let list = scrollable(entries).height(Fill);

        let modern_tokens = crate::ui::theme::tokens(&self.theme());
        let recent_active = self.workspace.query.history_view_mode == HistoryViewMode::Recent;
        let recent_label = text(self.tracked_caption("Recent"))
            .color_maybe(self.modern().then_some(if recent_active {
                modern_tokens.fg
            } else {
                modern_tokens.ghost
            }))
            .font(
                if self.workspace.query.history_view_mode == HistoryViewMode::Recent {
                    Font {
                        weight: iced::font::Weight::Bold,
                        ..self.ui_font()
                    }
                } else {
                    self.ui_font()
                },
            )
            .size(self.label_text_size());
        let recent_label: Element<'_, Message> = recent_label.into();
        let recent_button = button(recent_label)
            .padding(self.button_padding_tight())
            .style(compact_button_style)
            .on_press(Message::Workspace(
                crate::app::features::workspace::Message::Query(
                    crate::app::features::workspace::query::Message::HistoryViewModeSelected(
                        HistoryViewMode::Recent,
                    ),
                ),
            ));
        let saved_active = self.workspace.query.history_view_mode == HistoryViewMode::Saved;
        let saved_label = text(self.tracked_caption("Saved"))
            .color_maybe(self.modern().then_some(if saved_active {
                modern_tokens.fg
            } else {
                modern_tokens.ghost
            }))
            .font(
                if self.workspace.query.history_view_mode == HistoryViewMode::Saved {
                    Font {
                        weight: iced::font::Weight::Bold,
                        ..self.ui_font()
                    }
                } else {
                    self.ui_font()
                },
            )
            .size(self.label_text_size());
        let saved_label: Element<'_, Message> = saved_label.into();
        let saved_button = button(saved_label)
            .padding(self.button_padding_tight())
            .style(compact_button_style)
            .on_press(Message::Workspace(
                crate::app::features::workspace::Message::Query(
                    crate::app::features::workspace::query::Message::HistoryViewModeSelected(
                        HistoryViewMode::Saved,
                    ),
                ),
            ));

        let tabs: Element<'_, Message> = if self.modern() {
            row![
                iced::widget::column![
                    recent_button,
                    self.inset_bar(
                        self.workspace.query.history_view_mode == HistoryViewMode::Recent,
                        false
                    ),
                ]
                .spacing(0)
                .width(Length::Shrink),
                iced::widget::column![
                    saved_button,
                    self.inset_bar(
                        self.workspace.query.history_view_mode == HistoryViewMode::Saved,
                        false
                    ),
                ]
                .spacing(0)
                .width(Length::Shrink),
            ]
            .spacing(self.scale_u16(10))
            .align_y(Center)
            .width(Length::Shrink)
            .into()
        } else {
            row![recent_button, saved_button]
                .spacing(self.scale_u16(3))
                .align_y(Center)
                .into()
        };

        let expand_icon = if self.workspace.query.history_panel_expanded {
            ICON_ARROW_DOWN_S_LINE
        } else {
            ICON_ARROW_UP_S_LINE
        };
        let expand_button = self.with_tooltip(
            button(self.icon_text(expand_icon))
                .padding(self.button_padding_icon())
                .style(compact_button_style)
                .on_press(Message::Workspace(
                    crate::app::features::workspace::Message::Query(
                        crate::app::features::workspace::query::Message::ToggleHistoryPanel,
                    ),
                )),
            if self.workspace.query.history_panel_expanded {
                "Collapse"
            } else {
                "Expand"
            },
        );

        let list_empty = match self.workspace.query.history_view_mode {
            HistoryViewMode::Recent => self.workspace.query.history.is_empty(),
            HistoryViewMode::Saved => self.settings.values.saved_queries.is_empty(),
        };
        let clear_message = match self.workspace.query.history_view_mode {
            HistoryViewMode::Recent => {
                Message::Workspace(crate::app::features::workspace::Message::Query(
                    crate::app::features::workspace::query::Message::ClearQueryHistory,
                ))
            }
            HistoryViewMode::Saved => {
                Message::Workspace(crate::app::features::workspace::Message::Query(
                    crate::app::features::workspace::query::Message::ClearSavedQueries,
                ))
            }
        };
        let clear_tooltip = match self.workspace.query.history_view_mode {
            HistoryViewMode::Recent => "Clear history",
            HistoryViewMode::Saved => "Clear saved",
        };

        let clear_button: Element<'_, Message> = {
            let button = button(self.icon_text(ICON_TRASH_LINE))
                .padding(self.button_padding_icon())
                .style(compact_button_style);
            if list_empty {
                button.into()
            } else {
                self.with_tooltip(button.on_press(clear_message), clear_tooltip)
            }
        };

        let header = row![tabs, space::horizontal(), expand_button, clear_button]
            .spacing(self.scale_u16(4))
            .align_y(Center);

        container(iced::widget::column![header, list].spacing(self.scale_u16(5)))
            .padding(self.scale_u16(6))
            .height(Length::Fixed(height))
            .width(Fill)
            .style(panel_border_style)
            .into()
    }

    pub(crate) fn ai_query_modal(&self) -> Element<'_, Message> {
        crate::app::features::ai::view::View {
            state: &self.ai,
            scope: self.chat_scope_key(),
            presentation: self.presentation(),
            theme: self.theme(),
            spinner: self.loading_spinner_icon(),
            layout_mode: self.shell.layout_mode,
        }
        .ai_query_modal()
        .map(Message::Ai)
    }
}
