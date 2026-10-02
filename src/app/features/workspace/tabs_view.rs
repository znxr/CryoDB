use super::tabs::{Message, State, TabEntry};
use crate::constants::{
    ICON_CLOSE_FILL, ICON_DIAGRAM, ICON_DIAGRAM_CODE, ICON_PIN, ICON_TAB_INACTIVE, ICON_TAB_QUERY,
    ICON_TAB_TABLE, MODERN_QUERY_STRIP_HEIGHT, TAB_MAX_WIDTH, TAB_MIN_WIDTH, TAB_UNDERLINE_HEIGHT,
};
use crate::model::settings::ShortcutBinding;
use crate::ui::ids::query_tabs_scroll_id;
use crate::ui::presentation::Presentation;
use crate::ui::styles::{
    compact_button_style, panel_border_style, tab_chip_close_button_style, tab_chip_style,
};
use crate::ui::widgets::tab_placeholder::TabPlaceholder;
use crate::ui::widgets::tooltip_area::tooltip_area;
use crate::utils::text::{max_chars_for_width, truncate_with_ellipsis};
use iced::Task;
use iced::widget::{button, canvas, container, mouse_area, row, scrollable, space, stack, text};
use iced::{
    Background, Center, Element, Fill, Length, Padding, Point, Size, Theme, alignment, mouse,
};
use std::collections::HashSet;
pub(crate) struct DiagramTabInfo {
    pub(crate) title: String,
    pub(crate) display_title: String,
    pub(crate) pinned: bool,
    pub(crate) has_changes: bool,
}
pub(crate) struct View<'a> {
    pub(crate) state: &'a State,
    pub(crate) presentation: Presentation<'a>,
    pub(crate) diagrams: Vec<DiagramTabInfo>,
    pub(crate) active_diagram: Option<usize>,
    pub(crate) selected_table: Option<&'a str>,
    pub(crate) is_running_query: bool,
    pub(crate) is_applying_changes: bool,
    pub(crate) global_cursor: Option<Point>,
    pub(crate) theme: Theme,
    pub(crate) query_badges: HashSet<usize>,
    pub(crate) table_badges: HashSet<String>,
}
impl<'a> View<'a> {
    fn tab_entry_row_width(&self, entry: &TabEntry) -> Option<f32> {
        let tab_padding = self.presentation.button_padding_tight();
        let tab_horizontal_padding = tab_padding[1] * 2.0;
        let char_width = 8.0 * self.presentation.font_scale();
        let tab_min_width = self.presentation.scale_f32(TAB_MIN_WIDTH);
        let tab_max_width = self.presentation.scale_f32(TAB_MAX_WIDTH);
        let close_padding = [tab_padding[0], self.presentation.scale_f32(5.0)];
        let close_icon_size = self.presentation.button_text_size() as f32;
        let close_button_width = close_icon_size + close_padding[1] * 2.0;

        match entry {
            TabEntry::Query(index) => {
                let tab = self.state.query_tabs.get(*index)?;
                let label = tab.display_title();
                let estimated_width =
                    (label.chars().count().max(1) as f32) * char_width + tab_horizontal_padding;
                let tab_width = estimated_width.clamp(tab_min_width, tab_max_width);
                if tab.pinned {
                    Some(tab_width)
                } else {
                    Some(tab_width + close_button_width)
                }
            }
            TabEntry::Table(table) => {
                let is_pinned = self.state.pinned_table_tabs.contains(table);
                let estimated_width =
                    (table.chars().count().max(1) as f32) * char_width + tab_horizontal_padding;
                let tab_width = estimated_width.clamp(tab_min_width, tab_max_width);
                if is_pinned {
                    Some(tab_width)
                } else {
                    Some(tab_width + close_button_width)
                }
            }
            TabEntry::Diagram(index) => {
                let label = self
                    .diagrams
                    .get(*index)
                    .map(|tab| tab.title.clone())
                    .unwrap_or_else(|| format!("New Diagram #{}", index.saturating_add(1)));
                let estimated_width =
                    (label.chars().count().max(1) as f32) * char_width + tab_horizontal_padding;
                let tab_width = estimated_width.clamp(tab_min_width, tab_max_width);
                if self.diagrams.get(*index).is_some_and(|tab| tab.pinned) {
                    Some(tab_width)
                } else {
                    Some(tab_width + close_button_width)
                }
            }
        }
    }
    pub(crate) fn scroll_tab_entry_into_view(&self, active: &TabEntry) -> Task<Message> {
        let Some(viewport) = self.state.tabs_horizontal_viewport else {
            return Task::none();
        };

        let spacing = self.presentation.scale_u16(3);
        let mut cursor_x = 0.0_f32;
        let mut target_bounds = None;
        for entry in self.state.tab_strip.iter().filter(|entry| {
            matches!(entry, TabEntry::Query(_) | TabEntry::Diagram(_))
                || (self.presentation.settings.tabs_enabled && matches!(entry, TabEntry::Table(_)))
        }) {
            let Some(width) = self.tab_entry_row_width(entry) else {
                continue;
            };
            let start = cursor_x;
            let end = start + width;
            if entry == active {
                target_bounds = Some((start, end));
            }
            cursor_x = end + spacing;
        }

        let Some((tab_start, tab_end)) = target_bounds else {
            return Task::none();
        };

        let content_width = (cursor_x - spacing).max(0.0);
        let offset_x = viewport.absolute_offset().x;
        let viewport_width = viewport.bounds().width;
        let max_offset = (content_width - viewport_width).max(0.0);
        if max_offset <= 0.0 {
            return Task::none();
        }

        let margin = self.presentation.scale_f32(12.0);
        let next_offset = if tab_start < offset_x + margin {
            (tab_start - margin).max(0.0)
        } else if tab_end > offset_x + viewport_width - margin {
            (tab_end + margin - viewport_width).max(0.0)
        } else {
            return Task::none();
        }
        .min(max_offset);

        iced::widget::operation::scroll_to(
            query_tabs_scroll_id(),
            scrollable::AbsoluteOffset {
                x: Some(next_offset),
                y: None,
            },
        )
    }
    pub(crate) fn tab_drag_ghost(&self) -> Option<Element<'a, Message>> {
        let drag = self.state.drag_tab.as_ref()?;
        let cursor = self.global_cursor?;
        let title = self.tab_entry_title(self.state.tab_strip.get(drag.from_index)?);
        let chip = container(
            self.presentation
                .button_text(truncate_with_ellipsis(&title, 28).0)
                .wrapping(text::Wrapping::None),
        )
        .padding(self.presentation.button_padding_tight())
        .style(|theme: &iced::Theme| {
            let mut style = tab_chip_style(theme, true);
            style.border.color = theme.extended_palette().primary.base.color;
            style.border.width = 1.0;
            style
        });

        Some(
            container(chip)
                .padding(Padding {
                    top: (cursor.y - self.presentation.scale_f32(14.0)).max(0.0),
                    left: (cursor.x - drag.width * 0.5).max(0.0),
                    right: 0.0,
                    bottom: 0.0,
                })
                .width(Fill)
                .height(Fill)
                .clip(true)
                .into(),
        )
    }
    fn tab_placeholder(&self, width: f32, height: f32) -> Element<'a, Message> {
        let stroke_width = self.presentation.scale_f32(1.0);
        let segments = [
            self.presentation.scale_f32(4.0),
            self.presentation.scale_f32(4.0),
        ];
        let placeholder = canvas::Canvas::new(TabPlaceholder {
            stroke_width,
            segments,
        })
        .width(Length::Fixed(width))
        .height(Length::Fixed(height));
        container(placeholder)
            .width(Length::Fixed(width))
            .height(Length::Fixed(height))
            .into()
    }
    pub(crate) fn table_tabs_view(&self) -> Element<'a, Message> {
        let visible_entries: Vec<(usize, &TabEntry)> = self
            .state
            .tab_strip
            .iter()
            .enumerate()
            .filter(|(_, entry)| {
                matches!(entry, TabEntry::Query(_) | TabEntry::Diagram(_))
                    || (self.presentation.settings.tabs_enabled
                        && matches!(entry, TabEntry::Table(_)))
            })
            .collect();

        if visible_entries.is_empty() {
            let placeholder = container(self.presentation.label_text(""))
                .padding(self.presentation.button_padding_tight())
                .width(Fill);

            return placeholder.into();
        }

        let mut tab_row = row![]
            .spacing(if self.presentation.modern() {
                0.0
            } else {
                self.presentation.scale_f32(3.0)
            })
            .align_y(Center);
        let tab_min_width = self.presentation.scale_f32(TAB_MIN_WIDTH);
        let tab_max_width = self.presentation.scale_f32(TAB_MAX_WIDTH);
        let tab_padding = self.presentation.button_padding_tight();
        let tab_horizontal_padding = tab_padding[1] * 2.0;
        let tab_button_height = self.presentation.button_text_size() as f32 + tab_padding[0] * 2.0;
        let close_padding = [tab_padding[0], self.presentation.scale_f32(5.0)];
        let close_icon_size = self.presentation.button_text_size() as f32;
        let close_button_width = close_icon_size + close_padding[1] * 2.0;
        let base_close_button_height = close_icon_size + close_padding[0] * 2.0;

        let tab_row_height = if self.presentation.modern() {
            self.presentation.scale_f32(MODERN_QUERY_STRIP_HEIGHT)
        } else {
            tab_button_height.max(base_close_button_height) + self.presentation.scale_f32(2.0)
        };
        let close_button_height = tab_row_height;
        let tab_inner_spacing = 0.0;
        let char_width = 8.0 * self.presentation.font_scale();
        let status_icon_width =
            self.presentation.icon_text_size() as f32 + self.presentation.scale_f32(3.0);
        let drag_width = self.state.drag_tab.as_ref().map(|drag| drag.width);
        let can_switch_query = !self.is_running_query && !self.is_applying_changes;

        for (strip_index, entry) in visible_entries {
            if let (Some(target), Some(width)) = (self.state.drag_target_index, drag_width)
                && target == strip_index
            {
                tab_row = tab_row.push(self.tab_placeholder(width, tab_row_height));
            }

            if self
                .state
                .drag_tab
                .as_ref()
                .is_some_and(|drag| drag.from_index == strip_index)
            {
                continue;
            }

            let (tab_button, close_button, tab_row_width, tab_active) = match entry {
                TabEntry::Query(query_index) => {
                    let Some(tab) = self.state.query_tabs.get(*query_index) else {
                        continue;
                    };
                    let is_active = self.state.active_query_tab == Some(*query_index)
                        && self.active_diagram.is_none();
                    let is_pinned = tab.pinned;
                    let has_inactive_badge = self.query_badges.contains(query_index);
                    let status_icon_count =
                        1 + usize::from(is_pinned) + usize::from(has_inactive_badge);
                    let status_icons_width = status_icon_count as f32 * status_icon_width;
                    let label = if tab.renamed {
                        crate::i18n::tr_with(
                            "{name} \u{2014} Query",
                            &[("{name}", tab.display_title())],
                        )
                    } else {
                        tab.display_title().to_string()
                    };
                    let estimated_width =
                        (label.chars().count().max(1) as f32) * char_width + tab_horizontal_padding;
                    let tab_width = estimated_width.clamp(tab_min_width, tab_max_width);
                    let available_width =
                        (tab_width - tab_horizontal_padding - status_icons_width).max(40.0);
                    let adjusted = available_width / self.presentation.font_scale();
                    let max_chars = max_chars_for_width(adjusted);
                    let (display, truncated) = truncate_with_ellipsis(&label, max_chars);
                    let label_content: Element<'a, Message> = {
                        let mut status_row = row![
                            self.presentation
                                .icon_text(ICON_TAB_QUERY)
                                .size(self.presentation.label_text_size())
                        ]
                        .spacing(self.presentation.scale_u16(3))
                        .align_y(Center);
                        if is_pinned {
                            status_row = status_row.push(
                                self.presentation
                                    .icon_text(ICON_PIN)
                                    .size(self.presentation.label_text_size()),
                            );
                        }
                        if has_inactive_badge {
                            let inactive_badge = self.with_tooltip(
                                container(
                                    self.presentation
                                        .icon_text(ICON_TAB_INACTIVE)
                                        .size(self.presentation.label_text_size()),
                                ),
                                "Results unloaded to reduce memory. Run the query again to reload.",
                            );
                            status_row = status_row.push(inactive_badge);
                        }
                        status_row
                            .push(
                                self.presentation
                                    .button_text(display)
                                    .wrapping(text::Wrapping::None)
                                    .width(Fill),
                            )
                            .into()
                    };
                    let tab_label = container(label_content).width(Fill).clip(true);
                    let tab_row_width = if is_pinned {
                        tab_width
                    } else {
                        tab_width + close_button_width + tab_inner_spacing
                    };
                    let tab_button = mouse_area(
                        container(tab_label)
                            .padding(tab_padding)
                            .width(Length::Fixed(tab_width))
                            .height(Length::Fixed(tab_row_height))
                            .align_y(Center),
                    )
                    .on_press(Message::TabPressed {
                        index: strip_index,
                        width: tab_row_width,
                    })
                    .interaction(mouse::Interaction::Pointer);
                    let tab_button: Element<'a, Message> = if truncated {
                        self.with_tooltip(tab_button, label.clone())
                    } else {
                        tab_button.into()
                    };
                    let close_button: Element<'a, Message> = if is_pinned {
                        space::horizontal().width(Length::Fixed(0.0)).into()
                    } else if can_switch_query {
                        button(
                            container(
                                self.presentation
                                    .icon_text(ICON_CLOSE_FILL)
                                    .size(self.presentation.button_text_size()),
                            )
                            .height(Fill)
                            .align_y(Center),
                        )
                        .padding(close_padding)
                        .height(Length::Fixed(close_button_height))
                        .style(move |theme, status| {
                            tab_chip_close_button_style(theme, is_active, status)
                        })
                        .on_press(Message::CloseEntry(TabEntry::Query(*query_index)))
                        .into()
                    } else {
                        button(
                            container(
                                self.presentation
                                    .icon_text(ICON_CLOSE_FILL)
                                    .size(self.presentation.button_text_size()),
                            )
                            .height(Fill)
                            .align_y(Center),
                        )
                        .padding(close_padding)
                        .height(Length::Fixed(close_button_height))
                        .style(move |theme, status| {
                            tab_chip_close_button_style(theme, is_active, status)
                        })
                        .into()
                    };
                    (tab_button, close_button, tab_row_width, is_active)
                }
                TabEntry::Table(table) => {
                    let is_active = self.active_diagram.is_none()
                        && self.selected_table.is_some_and(|active| active == table);
                    let is_pinned = self.state.pinned_table_tabs.contains(table);
                    let has_inactive_badge = self.table_badges.contains(table);
                    let status_icon_count =
                        1 + usize::from(is_pinned) + usize::from(has_inactive_badge);
                    let status_icons_width = status_icon_count as f32 * status_icon_width;
                    let label = crate::i18n::tr_with("{name} \u{2014} Table", &[("{name}", table)]);
                    let estimated_width =
                        (label.chars().count().max(1) as f32) * char_width + tab_horizontal_padding;
                    let tab_width = estimated_width.clamp(tab_min_width, tab_max_width);
                    let available_width =
                        (tab_width - tab_horizontal_padding - status_icons_width).max(40.0);
                    let adjusted = available_width / self.presentation.font_scale();
                    let max_chars = max_chars_for_width(adjusted);
                    let (display, truncated) = truncate_with_ellipsis(&label, max_chars);
                    let label_content: Element<'a, Message> = {
                        let mut status_row = row![
                            self.presentation
                                .icon_text(ICON_TAB_TABLE)
                                .size(self.presentation.label_text_size())
                        ]
                        .spacing(self.presentation.scale_u16(3))
                        .align_y(Center);
                        if is_pinned {
                            status_row = status_row.push(
                                self.presentation
                                    .icon_text(ICON_PIN)
                                    .size(self.presentation.label_text_size()),
                            );
                        }
                        if has_inactive_badge {
                            let inactive_badge = self.with_tooltip(
                                container(
                                    self.presentation
                                        .icon_text(ICON_TAB_INACTIVE)
                                        .size(self.presentation.label_text_size()),
                                ),
                                "Results unloaded to reduce memory. Select tab to reload.",
                            );
                            status_row = status_row.push(inactive_badge);
                        }
                        status_row
                            .push(
                                self.presentation
                                    .button_text(display)
                                    .wrapping(text::Wrapping::None)
                                    .width(Fill),
                            )
                            .into()
                    };
                    let tab_label = container(label_content).width(Fill).clip(true);
                    let tab_row_width = if is_pinned {
                        tab_width
                    } else {
                        tab_width + close_button_width + tab_inner_spacing
                    };
                    let tab_button = mouse_area(
                        container(tab_label)
                            .padding(tab_padding)
                            .width(Length::Fixed(tab_width))
                            .height(Length::Fixed(tab_row_height))
                            .align_y(Center),
                    )
                    .on_press(Message::TabPressed {
                        index: strip_index,
                        width: tab_row_width,
                    })
                    .interaction(mouse::Interaction::Pointer);
                    let tab_button: Element<'a, Message> = if truncated {
                        self.with_tooltip(tab_button, label.clone())
                    } else {
                        tab_button.into()
                    };
                    let can_close = !(self.is_running_query && is_active);
                    let close_button: Element<'a, Message> = if is_pinned {
                        space::horizontal().width(Length::Fixed(0.0)).into()
                    } else if can_close {
                        button(
                            container(
                                self.presentation
                                    .icon_text(ICON_CLOSE_FILL)
                                    .size(self.presentation.button_text_size()),
                            )
                            .height(Fill)
                            .align_y(Center),
                        )
                        .padding(close_padding)
                        .height(Length::Fixed(close_button_height))
                        .style(move |theme, status| {
                            tab_chip_close_button_style(theme, is_active, status)
                        })
                        .on_press(Message::CloseEntry(TabEntry::Table(table.clone())))
                        .into()
                    } else {
                        button(
                            container(
                                self.presentation
                                    .icon_text(ICON_CLOSE_FILL)
                                    .size(self.presentation.button_text_size()),
                            )
                            .height(Fill)
                            .align_y(Center),
                        )
                        .padding(close_padding)
                        .height(Length::Fixed(close_button_height))
                        .style(move |theme, status| {
                            tab_chip_close_button_style(theme, is_active, status)
                        })
                        .into()
                    };
                    (tab_button, close_button, tab_row_width, is_active)
                }
                TabEntry::Diagram(diagram_index) => {
                    let is_active = self.active_diagram == Some(*diagram_index);
                    let label = self
                        .diagrams
                        .get(*diagram_index)
                        .map(|tab| tab.display_title.clone())
                        .unwrap_or_default();
                    let has_changes = self
                        .diagrams
                        .get(*diagram_index)
                        .is_some_and(|tab| tab.has_changes);
                    let is_pinned = self
                        .diagrams
                        .get(*diagram_index)
                        .is_some_and(|tab| tab.pinned);
                    let status_icon_count = 1 + usize::from(has_changes) + usize::from(is_pinned);
                    let status_icons_width = status_icon_count as f32 * status_icon_width;
                    let estimated_width =
                        (label.chars().count().max(1) as f32) * char_width + tab_horizontal_padding;
                    let tab_width = estimated_width.clamp(tab_min_width, tab_max_width);
                    let available_width =
                        (tab_width - tab_horizontal_padding - status_icons_width).max(40.0);
                    let adjusted = available_width / self.presentation.font_scale();
                    let (display, truncated) =
                        truncate_with_ellipsis(&label, max_chars_for_width(adjusted));
                    let mut status_row = row![
                        self.presentation
                            .icon_text(ICON_DIAGRAM)
                            .size(self.presentation.label_text_size())
                    ]
                    .spacing(self.presentation.scale_u16(3))
                    .align_y(Center);
                    if is_pinned {
                        status_row = status_row.push(
                            self.presentation
                                .icon_text(ICON_PIN)
                                .size(self.presentation.label_text_size()),
                        );
                    }
                    if has_changes {
                        status_row = status_row.push(
                            self.with_tooltip(
                                container(
                                    self.presentation
                                        .icon_text(ICON_DIAGRAM_CODE)
                                        .size(self.presentation.label_text_size()),
                                ),
                                "This diagram has schema changes that are not applied yet.",
                            ),
                        );
                    }
                    status_row = status_row.push(
                        self.presentation
                            .button_text(display)
                            .wrapping(text::Wrapping::None)
                            .width(Fill),
                    );
                    let tab_row_width = if is_pinned {
                        tab_width
                    } else {
                        tab_width + close_button_width + tab_inner_spacing
                    };
                    let tab_button = mouse_area(
                        container(container(status_row).width(Fill).clip(true))
                            .padding(tab_padding)
                            .width(Length::Fixed(tab_width))
                            .height(Length::Fixed(tab_row_height))
                            .align_y(Center),
                    )
                    .on_press(Message::TabPressed {
                        index: strip_index,
                        width: tab_row_width,
                    })
                    .interaction(mouse::Interaction::Pointer);
                    let tab_button: Element<'a, Message> = if truncated {
                        self.with_tooltip(tab_button, label.clone())
                    } else {
                        tab_button.into()
                    };
                    let close_button: Element<'a, Message> = if is_pinned {
                        space::horizontal().width(Length::Fixed(0.0)).into()
                    } else {
                        button(
                            container(
                                self.presentation
                                    .icon_text(ICON_CLOSE_FILL)
                                    .size(self.presentation.button_text_size()),
                            )
                            .height(Fill)
                            .align_y(Center),
                        )
                        .padding(close_padding)
                        .height(Length::Fixed(close_button_height))
                        .style(move |theme, status| {
                            tab_chip_close_button_style(theme, is_active, status)
                        })
                        .on_press(Message::CloseEntry(TabEntry::Diagram(*diagram_index)))
                        .into()
                    };
                    (tab_button, close_button, tab_row_width, is_active)
                }
            };

            let tab_group = container(
                row![tab_button, close_button]
                    .spacing(tab_inner_spacing)
                    .align_y(Center),
            )
            .align_y(Center)
            .height(Length::Fixed(tab_row_height))
            .style(move |theme| tab_chip_style(theme, tab_active))
            .clip(true);
            let tab_group: Element<'a, Message> = if self.presentation.modern() {
                iced::widget::column![tab_group, self.tab_inset_bar(tab_active)]
                    .spacing(0)
                    .into()
            } else {
                tab_group.into()
            };
            let tab_group = mouse_area(tab_group)
                .on_move(move |position| Message::TabDragMoved {
                    index: strip_index,
                    x: position.x,
                    width: tab_row_width,
                })
                .on_right_press(Message::TabContextMenuRequested { index: strip_index })
                .on_release(Message::TabDragReleased)
                .interaction(if self.state.drag_tab.is_some() {
                    mouse::Interaction::Grabbing
                } else {
                    mouse::Interaction::Pointer
                });

            tab_row = tab_row.push(tab_group);
        }

        if let (Some(target), Some(width)) = (self.state.drag_target_index, drag_width)
            && target == self.state.tab_strip.len()
        {
            tab_row = tab_row.push(self.tab_placeholder(width, tab_row_height));
        }

        let tab_strip = container(tab_row);

        let tabs = scrollable(tab_strip)
            .id(query_tabs_scroll_id())
            .on_scroll(Message::TabsScrolled)
            .direction(iced::widget::scrollable::Direction::Horizontal(
                iced::widget::scrollable::Scrollbar::new()
                    .width(0.0)
                    .scroller_width(0.0)
                    .spacing(0.0),
            ))
            .height(Length::Shrink)
            .width(Fill);
        let tabs = mouse_area(tabs)
            .on_move(Message::TabsCursorMoved)
            .on_exit(Message::TabsCursorLeft)
            .on_scroll(Message::TabsWheelScrolled)
            .on_press(Message::CloseTabContextMenu)
            .on_release(Message::TabDragReleased);

        tabs.into()
    }
    pub(crate) fn tab_context_menu_overlay(&self, bounds: Size) -> Element<'a, Message> {
        let Some(state) = &self.state.tab_context_menu else {
            return container(space::horizontal()).into();
        };
        let Some(entry) = self.state.tab_strip.get(state.strip_index) else {
            return container(space::horizontal()).into();
        };

        let is_pinned = self.is_tab_entry_pinned(entry);
        let can_close_current = !is_pinned
            && match entry {
                TabEntry::Query(_) => !self.is_running_query && !self.is_applying_changes,
                TabEntry::Table(table) => {
                    !(self.is_running_query && self.selected_table == Some(table.as_str()))
                }
                TabEntry::Diagram(_) => true,
            };
        let can_close_any = self
            .state
            .tab_strip
            .iter()
            .any(|entry| !self.is_tab_entry_pinned(entry));
        let can_close_right = self
            .state
            .tab_strip
            .iter()
            .skip(state.strip_index.saturating_add(1))
            .any(|entry| !self.is_tab_entry_pinned(entry));
        let pin_label = if is_pinned { "Remove pin" } else { "Pin" };

        let close_button: Element<'a, Message> = {
            let button = button(self.presentation.label_text("Close"))
                .padding(self.presentation.button_padding_tight())
                .width(Fill)
                .style(compact_button_style);
            let button: Element<'a, Message> = if can_close_current {
                button.on_press(Message::TabContextClose).into()
            } else {
                button.into()
            };
            self.with_shortcut_tooltip(
                button,
                "Close",
                &self.presentation.settings.close_tab_shortcut,
            )
        };
        let close_all_button: Element<'a, Message> = {
            let button = button(self.presentation.label_text("Close all"))
                .padding(self.presentation.button_padding_tight())
                .width(Fill)
                .style(compact_button_style);
            if can_close_any {
                button.on_press(Message::TabContextCloseAll).into()
            } else {
                button.into()
            }
        };
        let close_right_button: Element<'a, Message> = {
            let button = button(self.presentation.label_text("Close all to the right"))
                .padding(self.presentation.button_padding_tight())
                .width(Fill)
                .style(compact_button_style);
            if can_close_right {
                button.on_press(Message::TabContextCloseRight).into()
            } else {
                button.into()
            }
        };
        let pin_button = self.with_shortcut_tooltip(
            button(self.presentation.label_text(pin_label))
                .padding(self.presentation.button_padding_tight())
                .width(Fill)
                .style(compact_button_style)
                .on_press(Message::TabContextTogglePin),
            pin_label,
            &self.presentation.settings.toggle_tab_pin_shortcut,
        );

        let mut menu = iced::widget::column![
            close_button,
            close_all_button,
            close_right_button,
            pin_button
        ]
        .spacing(self.presentation.scale_u16(2))
        .width(Length::Fixed(self.presentation.scale_f32(250.0)));
        let is_query = matches!(entry, TabEntry::Query(_));
        if is_query {
            menu = menu.push(
                button(self.presentation.label_text("Rename tab"))
                    .padding(self.presentation.button_padding_tight())
                    .width(Fill)
                    .style(compact_button_style)
                    .on_press(Message::TabContextRename),
            );
        }

        let panel = container(menu)
            .padding(self.presentation.scale_u16(4))
            .style(panel_border_style);

        let menu_width = self.presentation.scale_f32(250.0);
        let menu_height = self
            .presentation
            .context_menu_height(4 + usize::from(is_query));
        let padding = self.presentation.context_menu_padding(
            state.position,
            bounds,
            menu_width,
            menu_height,
            self.presentation.scale_f32(4.0),
            self.presentation.scale_f32(8.0),
        );

        let menu_layer = container(panel)
            .padding(padding)
            .width(Fill)
            .height(Fill)
            .align_x(alignment::Horizontal::Left)
            .align_y(alignment::Vertical::Top);

        let backdrop = mouse_area(container(space::horizontal()).width(Fill).height(Fill))
            .on_press(Message::CloseTabContextMenu)
            .on_right_press(Message::CloseTabContextMenu);

        stack![backdrop, menu_layer].width(Fill).height(Fill).into()
    }
    fn is_tab_entry_pinned(&self, entry: &TabEntry) -> bool {
        match entry {
            TabEntry::Query(index) => self
                .state
                .query_tabs
                .get(*index)
                .is_some_and(|tab| tab.pinned),
            TabEntry::Table(table) => self.state.pinned_table_tabs.contains(table),
            TabEntry::Diagram(index) => self.diagrams.get(*index).is_some_and(|tab| tab.pinned),
        }
    }
    fn tab_entry_title(&self, entry: &TabEntry) -> String {
        match entry {
            TabEntry::Query(index) => self
                .state
                .query_tabs
                .get(*index)
                .map(|tab| tab.title.clone())
                .unwrap_or_else(|| format!("Query #{}", index.saturating_add(1))),
            TabEntry::Table(table) => table.clone(),
            TabEntry::Diagram(index) => self
                .diagrams
                .get(*index)
                .map(|tab| tab.title.clone())
                .unwrap_or_else(|| format!("New Diagram #{}", index.saturating_add(1))),
        }
    }
    fn with_tooltip(
        &self,
        element: impl Into<Element<'a, Message>>,
        text: impl Into<String>,
    ) -> Element<'a, Message> {
        tooltip_area(
            element,
            text,
            |text| Message::Tooltip(Some(text)),
            Message::Tooltip(None),
        )
    }
    fn with_shortcut_tooltip(
        &self,
        element: impl Into<Element<'a, Message>>,
        label: impl Into<String>,
        shortcut: &ShortcutBinding,
    ) -> Element<'a, Message> {
        self.with_tooltip(
            element,
            format!("{} · {shortcut}", crate::i18n::tr(&label.into())),
        )
    }
    pub(crate) fn tab_inset_bar(&self, active: bool) -> Element<'a, Message> {
        let color = if active {
            crate::ui::theme::appearance_color(
                self.presentation.settings.accent_color,
                self.theme.palette().primary,
            )
        } else {
            let mut color = self.theme.extended_palette().background.strong.color;
            color.a = 0.45;
            color
        };
        container(space::Space::new())
            .height(Length::Fixed(TAB_UNDERLINE_HEIGHT))
            .width(Fill)
            .style(move |_| container::Style {
                background: Some(Background::Color(color)),
                ..container::Style::default()
            })
            .into()
    }
}
