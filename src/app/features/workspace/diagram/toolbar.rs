use super::Message;
use super::model::{DiagramCommand, DiagramModalKind, DiagramState};
use super::view::View;
use crate::constants::{
    ICON_ARROW_LEFT_S_LINE, ICON_ARROW_RIGHT_S_LINE, ICON_DIAGRAM_APPLY, ICON_DIAGRAM_CODE,
    ICON_DIAGRAM_COLLAPSE, ICON_DIAGRAM_EXPAND, ICON_DIAGRAM_EXPORT, ICON_DIAGRAM_FIT,
    ICON_DIAGRAM_FULLSCREEN, ICON_DIAGRAM_FULLSCREEN_EXIT, ICON_DIAGRAM_GRID, ICON_DIAGRAM_IMAGE,
    ICON_DIAGRAM_RELATIONS, ICON_DIAGRAM_SAVED, ICON_DIAGRAM_ZOOM_IN, ICON_DIAGRAM_ZOOM_OUT,
    ICON_GOTO, ICON_MODERN_ADD, ICON_MODERN_SEARCH, ICON_REDO, ICON_REFRESH_LINE, ICON_TRASH_LINE,
    ICON_UNDO,
};
use crate::model::diagram::DiagramRelations;
use crate::model::settings::PickerOption;
use crate::utils::text::truncate_with_ellipsis;

use crate::ui::styles::{
    alert_style, compact_button_style, compact_input_style, compact_pick_list_menu_style,
    compact_pick_list_style, compact_primary_button_style,
};

use iced::widget::{button, container, pick_list, row, space, text_input};
use iced::{Background, Center, Color, Element, Fill, Length, alignment};
impl<'a> View<'a> {
    pub(super) fn diagram_icon(
        &self,
        icon: char,
        tooltip: &'static str,
        active: bool,
        message: Message,
    ) -> Element<'a, Message> {
        let content = button(self.presentation.icon_text(icon))
            .padding(self.presentation.button_padding_icon())
            .style(move |theme, status| {
                if active {
                    compact_primary_button_style(theme, status)
                } else {
                    compact_button_style(theme, status)
                }
            })
            .on_press(message);
        self.with_tooltip(content, tooltip)
    }

    pub(super) fn diagram_toolbar(&self, state: &'a DiagramState) -> Element<'a, Message> {
        let gap = self.presentation.scale_u16(2);
        let saved = self.state.saved_diagram_names(self.keys.clone());
        let control_padding = [
            self.presentation.button_padding_icon()[0]
                + (self
                    .presentation
                    .icon_text_size()
                    .saturating_sub(self.presentation.button_text_size())
                    as f32
                    / 2.0),
            self.presentation.button_padding_tight()[1],
        ];
        let active_title = self
            .state
            .active_diagram_tab
            .map(|index| self.state.diagram_tab_title(index));
        let can_delete = active_title
            .as_ref()
            .is_some_and(|title| saved.iter().any(|saved_name| saved_name == title));
        let picker: Element<'a, Message> = if saved.is_empty() {
            space::horizontal().width(Length::Fixed(0.0)).into()
        } else {
            let options = saved
                .iter()
                .cloned()
                .map(|name| {
                    let label =
                        crate::i18n::tr_with("{name} \u{2014} Diagram", &[("{name}", &name)]);
                    PickerOption::new(name, label)
                })
                .collect::<Vec<_>>();
            let selected = active_title.clone().map(|name| {
                let label = crate::i18n::tr_with("{name} \u{2014} Diagram", &[("{name}", &name)]);
                PickerOption::new(name, label)
            });
            let list = pick_list(options, selected, |option: PickerOption<String>| {
                Message::OpenSavedDiagram(option.value)
            })
            .placeholder(crate::i18n::tr("Open saved diagram…"))
            .padding(control_padding)
            .text_size(self.presentation.button_text_size())
            .font(self.presentation.ui_font())
            .handle(self.presentation.pick_list_handle())
            .style(|theme, status| {
                let mut style = compact_pick_list_style(theme, status);
                style.background = Background::Color(Color::TRANSPARENT);
                style.border.width = 0.0;
                style
            })
            .menu_style(compact_pick_list_menu_style)
            .width(Fill);
            self.with_tooltip(
                container(
                    row![
                        self.presentation
                            .icon_text(ICON_DIAGRAM_SAVED)
                            .size(self.presentation.icon_text_size()),
                        list,
                    ]
                    .spacing(self.presentation.scale_u16(2))
                    .align_y(Center),
                )
                .padding([0.0, self.presentation.scale_f32(5.0)])
                .width(Length::Fixed(self.presentation.scale_f32(250.0)))
                .style(|theme| {
                    let style = compact_button_style(theme, button::Status::Active);
                    container::Style {
                        text_color: Some(style.text_color),
                        background: style.background,
                        border: style.border,
                        ..container::Style::default()
                    }
                }),
                "Saved diagrams",
            )
        };

        let all_collapsed = !state.diagram.tables.is_empty()
            && state.diagram.tables.iter().all(|table| table.collapsed);
        let mut toolbar = row![
            picker,
            self.diagram_icon(
                ICON_DIAGRAM_SAVED,
                "Save diagram…",
                false,
                Message::DiagramModalOpened {
                    kind: DiagramModalKind::SaveDiagramAs,
                    table: None,
                }
            ),
            self.diagram_icon(
                ICON_MODERN_ADD,
                "New diagram",
                false,
                Message::NewSchemaDiagram
            ),
            self.diagram_icon(
                ICON_REFRESH_LINE,
                "Reload diagram",
                false,
                Message::ReloadSchemaDiagram
            ),
            self.diagram_separator(),
            self.diagram_icon(
                ICON_DIAGRAM_GRID,
                "Snap to grid",
                state.snap,
                Message::DiagramSnapToggled
            ),
            self.with_tooltip(
                container(
                    row![
                        self.presentation.icon_text(ICON_DIAGRAM_RELATIONS),
                        pick_list(
                            [
                                DiagramRelations::Auto,
                                DiagramRelations::All,
                                DiagramRelations::Selected,
                            ],
                            Some(state.relations),
                            Message::DiagramRelationsSelected,
                        )
                        .padding(control_padding)
                        .text_size(self.presentation.button_text_size())
                        .font(self.presentation.ui_font())
                        .handle(self.presentation.pick_list_handle())
                        .style(|theme, status| {
                            let mut style = compact_pick_list_style(theme, status);
                            style.background = Background::Color(Color::TRANSPARENT);
                            style.border.width = 0.0;
                            style
                        })
                        .menu_style(compact_pick_list_menu_style)
                        .width(Fill),
                    ]
                    .spacing(self.presentation.scale_u16(2))
                    .align_y(Center),
                )
                .padding([0.0, self.presentation.scale_f32(5.0)])
                .width(Length::Fixed(self.presentation.scale_f32(250.0)))
                .style(|theme| {
                    let style = compact_button_style(theme, button::Status::Active);
                    container::Style {
                        text_color: Some(style.text_color),
                        background: style.background,
                        border: style.border,
                        ..container::Style::default()
                    }
                }),
                "Relations",
            ),
            self.diagram_separator(),
            self.diagram_icon(
                if all_collapsed {
                    ICON_DIAGRAM_EXPAND
                } else {
                    ICON_DIAGRAM_COLLAPSE
                },
                if all_collapsed {
                    "Expand all tables"
                } else {
                    "Collapse all tables"
                },
                false,
                Message::DiagramExpandAll(all_collapsed)
            ),
            self.diagram_icon(
                ICON_MODERN_SEARCH,
                "Search the diagram",
                state.search_open,
                Message::DiagramSearchToggled
            ),
        ]
        .spacing(gap)
        .align_y(Center);

        if can_delete {
            toolbar = toolbar.push(self.diagram_icon(
                ICON_TRASH_LINE,
                "Delete the saved diagram",
                false,
                Message::DeleteSavedDiagram,
            ));
        }

        if state.search_anim > 0.0 {
            toolbar = toolbar.push(
                text_input(&crate::i18n::tr("Find a table or column"), &state.search)
                    .on_input(Message::DiagramSearchChanged)
                    .on_submit(Message::DiagramSearchSubmitted)
                    .padding(control_padding)
                    .size(self.presentation.button_text_size())
                    .font(self.presentation.ui_font())
                    .style(compact_input_style)
                    .width(Length::Fixed(
                        self.presentation.scale_f32(200.0) * state.search_anim,
                    )),
            );
            if state.search_open && !state.matches.is_empty() {
                toolbar = toolbar
                    .push(self.diagram_icon(
                        ICON_ARROW_LEFT_S_LINE,
                        "Previous match",
                        false,
                        Message::DiagramSearchPrevious,
                    ))
                    .push(self.presentation.muted_label_text(format!(
                        "{} / {}",
                        state.match_index + 1,
                        state.matches.len()
                    )))
                    .push(self.diagram_icon(
                        ICON_ARROW_RIGHT_S_LINE,
                        "Next match",
                        false,
                        Message::DiagramSearchNext,
                    ));
            }
        }

        toolbar = toolbar.push(space::horizontal().width(Fill));

        if let Some(table) = state
            .selected
            .and_then(|index| state.diagram.tables.get(index))
        {
            let (label, _) = truncate_with_ellipsis(&table.name, 28);
            toolbar = toolbar
                .push(
                    self.presentation.label_text(crate::i18n::tr_with(
                        "{table} · {count} relations",
                        &[
                            ("{table}", &label),
                            (
                                "{count}",
                                &state
                                    .related_tables(state.selected.unwrap_or(0))
                                    .to_string(),
                            ),
                        ],
                    )),
                )
                .push(self.diagram_icon(
                    ICON_GOTO,
                    "Open Table",
                    false,
                    Message::DiagramOpenSelectedTable,
                ))
                .push(self.diagram_separator());
        }

        toolbar = toolbar
            .push(self.diagram_icon(
                ICON_DIAGRAM_EXPORT,
                "Export as SVG",
                false,
                Message::DiagramExport { png: false },
            ))
            .push(self.diagram_icon(
                ICON_DIAGRAM_IMAGE,
                "Export as PNG",
                false,
                Message::DiagramExport { png: true },
            ))
            .push(self.diagram_icon(
                if state.fullscreen {
                    ICON_DIAGRAM_FULLSCREEN_EXIT
                } else {
                    ICON_DIAGRAM_FULLSCREEN
                },
                "Fullscreen diagram",
                state.fullscreen,
                Message::DiagramFullscreenToggled,
            ));

        toolbar.into()
    }

    pub(super) fn diagram_status_icon(
        &self,
        icon: char,
        tooltip: &'static str,
        message: Message,
    ) -> Element<'a, Message> {
        let content = button(
            self.presentation
                .icon_text(icon)
                .size(self.presentation.label_text_size()),
        )
        .padding(self.presentation.scale_padding([1, 5]))
        .style(compact_button_style)
        .on_press(message);
        self.with_tooltip(content, tooltip)
    }

    pub(super) fn diagram_separator(&self) -> Element<'a, Message> {
        container(space::vertical())
            .width(Length::Fixed(1.0))
            .height(Length::Fixed(self.presentation.scale_f32(16.0)))
            .style(|theme| container::Style {
                background: Some(crate::ui::theme::tokens(theme).line.into()),
                ..container::Style::default()
            })
            .into()
    }

    pub(crate) fn diagram_status_bar(&self, state: &'a DiagramState) -> Element<'a, Message> {
        let summary = if state.loading {
            crate::i18n::tr("Loading schema diagram…")
        } else if let Some(error) = &state.error {
            error.clone()
        } else if state.link.is_some() {
            crate::i18n::tr("Drop on a column to link it. Hold Shift to add it to the last key.")
        } else {
            crate::i18n::tr_with(
                "{tables} tables, {relations} relations",
                &[
                    ("{tables}", &state.diagram.tables.len().to_string()),
                    ("{relations}", &state.diagram.edges.len().to_string()),
                ],
            )
        };

        let mut status = row![
            self.presentation.muted_label_text(summary),
            space::horizontal().width(Fill),
        ]
        .spacing(self.presentation.scale_u16(8))
        .align_y(Center);

        if !state.changes.is_empty() {
            status = status.push(self.presentation.label_text(crate::i18n::tr_with(
                "{count} pending schema change(s)",
                &[("{count}", &state.changes.len().to_string())],
            )));
        }

        if let Some(selected) = state.selected {
            status = status.push(self.diagram_status_icon(
                ICON_GOTO,
                "Fit to selection (F)",
                Message::DiagramCommandRequested(DiagramCommand::Focus(selected)),
            ));
        }

        status = status
            .push(self.diagram_status_icon(
                ICON_DIAGRAM_ZOOM_OUT,
                "Zoom out",
                Message::DiagramCommandRequested(DiagramCommand::Zoom(0.8)),
            ))
            .push(
                container(
                    self.presentation
                        .muted_label_text(format!("{}%", (state.zoom * 100.0).round() as i32)),
                )
                .width(Length::Fixed(self.presentation.scale_f32(42.0)))
                .align_x(alignment::Horizontal::Center),
            )
            .push(self.diagram_status_icon(
                ICON_DIAGRAM_ZOOM_IN,
                "Zoom in",
                Message::DiagramCommandRequested(DiagramCommand::Zoom(1.25)),
            ))
            .push(self.diagram_status_icon(
                ICON_DIAGRAM_FIT,
                "Fit to screen (0)",
                Message::DiagramCommandRequested(DiagramCommand::Fit),
            ));

        status.width(Fill).into()
    }

    pub(super) fn diagram_changes_bar(&self, state: &'a DiagramState) -> Element<'a, Message> {
        if state.changes.is_empty() && state.undone_changes.is_empty() {
            return space::vertical().height(Length::Fixed(0.0)).into();
        }
        let summary = crate::i18n::tr_with(
            "{count} pending schema change(s)",
            &[("{count}", &state.changes.len().to_string())],
        );
        let apply: Element<'a, Message> = if state.applying {
            self.diagram_icon(
                self.spinner,
                "Apply changes",
                false,
                Message::DiagramClosePreview,
            )
        } else {
            self.diagram_icon(
                ICON_DIAGRAM_APPLY,
                "Apply changes",
                true,
                Message::DiagramApplyChanges,
            )
        };
        let mut bar = row![container(self.presentation.label_text(summary)).width(Fill)];
        if !state.changes.is_empty() {
            bar = bar.push(self.diagram_icon(
                ICON_UNDO,
                "Undo schema change",
                false,
                Message::DiagramUndoChange,
            ));
        }
        if !state.undone_changes.is_empty() {
            bar = bar.push(self.diagram_icon(
                ICON_REDO,
                "Redo schema change",
                false,
                Message::DiagramRedoChange,
            ));
        }
        if !state.changes.is_empty() {
            bar = bar
                .push(self.diagram_icon(
                    ICON_DIAGRAM_CODE,
                    "Preview generated SQL",
                    false,
                    Message::DiagramPreviewDdl,
                ))
                .push(apply);
        }
        bar = bar.push(self.diagram_icon(
            ICON_TRASH_LINE,
            "Discard schema changes",
            false,
            Message::DiagramDiscardChanges,
        ));
        container(bar.spacing(self.presentation.scale_u16(2)).align_y(Center))
            .padding(self.presentation.scale_padding([3, 8]))
            .width(Fill)
            .style(alert_style)
            .into()
    }
}
