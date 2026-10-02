use super::model::{DiagramModalKind, DiagramState, DiagramTarget};
use super::{Message, State};
use crate::constants::ICON_CLOSE_LINE;

use crate::ui::presentation::Presentation;
use crate::ui::styles::{
    borderless_query_editor_style, compact_button_style, compact_input_style,
    compact_pick_list_menu_style, compact_pick_list_style, compact_primary_button_style,
    modal_backdrop_style, panel_border_style, panel_style,
};
use crate::ui::widgets::tooltip_area::tooltip_area;

use iced::widget::{
    Space, button, canvas, container, mouse_area, pick_list, row, scrollable, space, stack, text,
    text_editor, text_input,
};
use iced::{Background, Center, Element, Fill, Length, Padding, mouse};
pub(crate) struct View<'a> {
    pub(crate) state: &'a State,
    pub(crate) presentation: Presentation<'a>,
    pub(crate) keys: Option<(String, String)>,
    pub(crate) spinner: char,
}
impl<'a> View<'a> {
    fn diagram_menu_overlay(&self, state: &'a DiagramState) -> Element<'a, Message> {
        let Some(menu) = &state.menu else {
            return space::horizontal().width(Length::Fixed(0.0)).into();
        };

        let entry = |label: &str, message: Message| -> Element<'a, Message> {
            button(
                self.presentation
                    .label_text(label)
                    .wrapping(text::Wrapping::None),
            )
            .padding(self.presentation.button_padding_tight())
            .width(Fill)
            .style(compact_button_style)
            .on_press(message)
            .into()
        };

        let mut items = iced::widget::column![].spacing(self.presentation.scale_u16(2));
        match menu.target {
            Some(DiagramTarget::Table(index)) => {
                items = items
                    .push(entry("Open Table", Message::DiagramOpenSelectedTable))
                    .push(entry(
                        "Rename table",
                        Message::DiagramModalOpened {
                            kind: DiagramModalKind::RenameTable,
                            table: Some(index),
                        },
                    ))
                    .push(entry(
                        "Add column",
                        Message::DiagramModalOpened {
                            kind: DiagramModalKind::AddColumn,
                            table: Some(index),
                        },
                    ))
                    .push(entry(
                        "Collapse or expand",
                        Message::DiagramTableCollapseToggled(index),
                    ))
                    .push(entry("Drop table", Message::DiagramDropTable(index)));
                if state
                    .diagram
                    .tables
                    .get(index)
                    .is_some_and(|table| table.columns.iter().any(|column| !column.primary))
                {
                    items = items.push(entry(
                        "Drop column…",
                        Message::DiagramModalOpened {
                            kind: DiagramModalKind::DropColumn,
                            table: Some(index),
                        },
                    ));
                }
                if state
                    .diagram
                    .tables
                    .get(index)
                    .is_some_and(|table| !table.columns.is_empty())
                {
                    items = items.push(entry(
                        "Alter column…",
                        Message::DiagramModalOpened {
                            kind: DiagramModalKind::AlterColumn,
                            table: Some(index),
                        },
                    ));
                }
                items = items.push(entry(
                    "Add a note about this table",
                    Message::DiagramModalOpened {
                        kind: DiagramModalKind::AddNote,
                        table: Some(index),
                    },
                ));
                if let Some(edge) = state
                    .diagram
                    .edges
                    .iter()
                    .position(|edge| edge.from == index)
                {
                    items = items.push(entry(
                        "Remove foreign key",
                        Message::DiagramDropRelation(edge),
                    ));
                }
            }
            Some(DiagramTarget::Group(index)) => {
                let group = state.diagram.groups.get(index);
                let collapsed = group.is_some_and(|group| group.collapsed);
                let locked = group.is_some_and(|group| group.locked);
                let contents_locked = group.is_some_and(|group| group.contents_locked);
                items = items
                    .push(entry(
                        "Rename area",
                        Message::DiagramModalOpened {
                            kind: DiagramModalKind::RenameGroup,
                            table: Some(index),
                        },
                    ))
                    .push(entry(
                        "Change area colour",
                        Message::DiagramGroupColorCycled(index),
                    ))
                    .push(entry(
                        if collapsed {
                            "Expand area"
                        } else {
                            "Collapse area"
                        },
                        Message::DiagramGroupCollapseToggled(index),
                    ))
                    .push(entry(
                        if locked { "Unlock area" } else { "Lock area" },
                        Message::DiagramGroupLockToggled(index),
                    ))
                    .push(entry(
                        if contents_locked {
                            "Unlock area contents"
                        } else {
                            "Lock area contents"
                        },
                        Message::DiagramGroupContentsLockToggled(index),
                    ))
                    .push(entry(
                        "Delete area (keeps its tables)",
                        Message::DiagramDeleteGroup(index),
                    ));
            }
            Some(DiagramTarget::Note(index)) => {
                items = items
                    .push(entry("Edit note", Message::DiagramNoteEdit(index)))
                    .push(entry("Delete note", Message::DiagramDeleteNote(index)));
            }
            None => {
                items = items
                    .push(entry(
                        "New table",
                        Message::DiagramModalOpened {
                            kind: DiagramModalKind::CreateTable,
                            table: None,
                        },
                    ))
                    .push(entry(
                        "New area",
                        Message::DiagramModalOpened {
                            kind: DiagramModalKind::AddGroup,
                            table: None,
                        },
                    ))
                    .push(entry(
                        "New note",
                        Message::DiagramModalOpened {
                            kind: DiagramModalKind::AddNote,
                            table: None,
                        },
                    ))
                    .push(entry("Auto layout", Message::DiagramAutoLayout));
            }
        }

        let panel = container(scrollable(items).height(Length::Shrink).width(Fill))
            .padding(self.presentation.scale_u16(4))
            .max_height(self.presentation.scale_f32(360.0))
            .width(Length::Fixed(self.presentation.scale_f32(240.0)))
            .style(panel_border_style);

        container(panel)
            .padding(Padding {
                top: menu.position.y.max(0.0),
                right: 0.0,
                bottom: 0.0,
                left: menu.position.x.max(0.0),
            })
            .width(Fill)
            .height(Fill)
            .into()
    }

    fn diagram_modal_overlay(&self, state: &'a DiagramState) -> Element<'a, Message> {
        let Some(modal) = &state.modal else {
            return space::horizontal().width(Length::Fixed(0.0)).into();
        };

        let mut form = iced::widget::column![self.presentation.heading_text(modal.title())]
            .spacing(self.presentation.scale_u16(8))
            .width(Fill);

        match modal.kind {
            DiagramModalKind::DeleteSavedDiagram => {
                form = form.push(self.presentation.label_text(crate::i18n::tr_with(
                    "Delete the saved diagram {name}? The database schema will not be changed.",
                    &[("{name}", &modal.name)],
                )));
            }
            DiagramModalKind::DropColumn | DiagramModalKind::AlterColumn => {
                let altering = modal.kind == DiagramModalKind::AlterColumn;
                let columns: Vec<String> = modal
                    .table
                    .and_then(|index| state.diagram.tables.get(index))
                    .map(|table| {
                        table
                            .columns
                            .iter()
                            .filter(|column| altering || !column.primary)
                            .map(|column| column.name.clone())
                            .collect()
                    })
                    .unwrap_or_default();
                let selected = (!modal.name.trim().is_empty()).then(|| modal.name.clone());
                form = form.push(self.presentation.label_text("Column")).push(
                    pick_list(columns, selected, |value| {
                        Message::DiagramModalNameChanged(value)
                    })
                    .padding(self.presentation.input_padding())
                    .text_size(self.presentation.input_text_size())
                    .style(compact_pick_list_style)
                    .menu_style(compact_pick_list_menu_style)
                    .width(Fill),
                );
                if altering {
                    let label = "New definition: TYPE [NULL|NOT NULL] [DEFAULT value]";
                    form = form.push(self.presentation.label_text(label)).push(
                        text_input(&crate::i18n::tr(label), &modal.detail)
                            .on_input(Message::DiagramModalDetailChanged)
                            .on_submit(Message::DiagramModalSubmitted)
                            .padding(self.presentation.input_padding())
                            .size(self.presentation.input_text_size())
                            .style(compact_input_style),
                    );
                }
            }
            DiagramModalKind::AddNote | DiagramModalKind::EditNote => {
                if let Some(table) = modal
                    .table
                    .filter(|_| modal.kind == DiagramModalKind::AddNote)
                    .and_then(|index| state.diagram.tables.get(index))
                {
                    form = form.push(self.presentation.muted_label_text(crate::i18n::tr_with(
                        "Attached to {table}",
                        &[("{table}", &table.name)],
                    )));
                }
                form = form.push(self.presentation.label_text("Note")).push(
                    text_editor(&modal.note)
                        .on_action(Message::DiagramModalNoteEdited)
                        .height(Length::Fixed(self.presentation.scale_f32(140.0)))
                        .wrapping(text::Wrapping::WordOrGlyph)
                        .font(self.presentation.ui_font())
                        .padding(self.presentation.input_padding())
                        .size(self.presentation.input_text_size())
                        .style(borderless_query_editor_style),
                );
            }
            _ => {
                let label = match modal.kind {
                    DiagramModalKind::SaveDiagramAs => "Diagram name",
                    DiagramModalKind::AddGroup => "Area name",
                    DiagramModalKind::RenameGroup => "Area name",
                    _ => "Name",
                };
                form = form.push(self.presentation.label_text(label)).push(
                    text_input(&crate::i18n::tr(label), &modal.name)
                        .on_input(Message::DiagramModalNameChanged)
                        .on_submit(Message::DiagramModalSubmitted)
                        .padding(self.presentation.input_padding())
                        .size(self.presentation.input_text_size())
                        .style(compact_input_style),
                );

                let detail_label = match modal.kind {
                    DiagramModalKind::CreateTable => Some("Columns, one per line: name TYPE"),
                    DiagramModalKind::AddColumn => Some("Column type"),
                    _ => None,
                };
                if let Some(detail_label) = detail_label {
                    form = form.push(self.presentation.label_text(detail_label)).push(
                        text_input(&crate::i18n::tr(detail_label), &modal.detail)
                            .on_input(Message::DiagramModalDetailChanged)
                            .on_submit(Message::DiagramModalSubmitted)
                            .padding(self.presentation.input_padding())
                            .size(self.presentation.input_text_size())
                            .style(compact_input_style),
                    );
                }
            }
        }

        if let Some(error) = &modal.error {
            form = form.push(self.presentation.muted_label_text(error.clone()));
        }

        form = form.push(
            row![
                space::horizontal(),
                button(self.presentation.button_text("Cancel"))
                    .padding(self.presentation.button_padding_tight())
                    .style(compact_button_style)
                    .on_press(Message::DiagramModalClosed),
                button(self.presentation.button_text(
                    if modal.kind == DiagramModalKind::DeleteSavedDiagram {
                        "Delete"
                    } else {
                        "Save"
                    }
                ))
                .padding(self.presentation.button_padding_tight())
                .style(compact_primary_button_style)
                .on_press(Message::DiagramModalSubmitted),
            ]
            .spacing(self.presentation.scale_u16(6))
            .align_y(Center),
        );

        self.centered_diagram_overlay(
            container(form)
                .padding(self.presentation.scale_u16(14))
                .width(Length::Fixed(self.presentation.scale_f32(460.0)))
                .style(panel_style)
                .into(),
        )
    }

    fn diagram_preview_overlay(&self, state: &'a DiagramState) -> Element<'a, Message> {
        let Some(ddl) = &state.ddl_preview else {
            return space::horizontal().width(Length::Fixed(0.0)).into();
        };

        let changes = state.changes.iter().enumerate().fold(
            iced::widget::column![].spacing(self.presentation.scale_u16(2)),
            |list, (index, change)| {
                list.push(
                    row![
                        self.presentation
                            .muted_label_text(change.summary())
                            .width(Fill),
                        self.diagram_icon(
                            ICON_CLOSE_LINE,
                            "Remove this schema change",
                            false,
                            Message::DiagramRemoveChange(index),
                        ),
                    ]
                    .align_y(Center),
                )
            },
        );

        let body = iced::widget::column![
            self.presentation.label_text("Schema diff"),
            changes,
            self.presentation.label_text("Generated SQL"),
            scrollable(
                container(self.presentation.button_text(ddl.clone()))
                    .padding(self.presentation.scale_u16(6))
            )
            .height(Length::Fixed(self.presentation.scale_f32(240.0))),
            row![
                button(self.presentation.label_text("Close"))
                    .padding(self.presentation.button_padding_tight())
                    .style(compact_button_style)
                    .on_press(Message::DiagramClosePreview),
                button(self.presentation.label_text("Apply changes"))
                    .padding(self.presentation.button_padding_tight())
                    .style(compact_primary_button_style)
                    .on_press(Message::DiagramApplyChanges),
            ]
            .spacing(self.presentation.scale_u16(6)),
        ]
        .spacing(self.presentation.scale_u16(8));

        self.centered_diagram_overlay(
            container(body)
                .padding(self.presentation.scale_u16(14))
                .width(Length::Fixed(self.presentation.scale_f32(620.0)))
                .style(panel_border_style)
                .into(),
        )
    }

    fn centered_diagram_overlay(&self, panel: Element<'a, Message>) -> Element<'a, Message> {
        let backdrop_dim = self.presentation.settings.modal_backdrop_dim;
        let backdrop = mouse_area(
            container(space::horizontal())
                .width(Fill)
                .height(Fill)
                .style(move |theme| modal_backdrop_style(theme, backdrop_dim)),
        )
        .on_press(Message::DiagramModalClosed)
        .on_scroll(|_| Message::ModalBlocked)
        .interaction(mouse::Interaction::Idle);

        stack![
            backdrop,
            container(panel).width(Fill).height(Fill).center(Fill)
        ]
        .width(Fill)
        .height(Fill)
        .into()
    }

    pub(crate) fn diagram_panel(&self) -> Element<'a, Message> {
        let Some(state) = self.state.active_diagram() else {
            return container(space::horizontal())
                .width(Fill)
                .height(Fill)
                .into();
        };

        let canvas = canvas::Canvas::new(super::canvas::ErDiagram {
            state,
            font: self.presentation.ui_font(),
            icon_font: self.presentation.icon_font(),
            scale: self.presentation.font_scale(),
        })
        .width(Fill)
        .height(Fill);

        let gutter = if state.fullscreen { 12 } else { 0 };
        let stacked = stack![
            container(canvas).width(Fill).height(Fill),
            self.diagram_menu_overlay(state),
            self.diagram_modal_overlay(state),
            self.diagram_preview_overlay(state),
        ];

        let panel = iced::widget::column![
            container(self.diagram_toolbar(state))
                .padding(self.presentation.scale_padding([2, gutter])),
            self.horizontal_hairline(),
            self.diagram_changes_bar(state),
            container(stacked).width(Fill).height(Fill),
        ]
        .spacing(0)
        .width(Fill)
        .height(Fill);

        panel.into()
    }

    pub(super) fn with_tooltip(
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
    fn horizontal_hairline(&self) -> Element<'a, Message> {
        container(Space::new())
            .width(Fill)
            .height(Length::Fixed(1.0))
            .style(|theme| container::Style {
                background: Some(Background::Color(crate::ui::theme::tokens(theme).line)),
                ..container::Style::default()
            })
            .into()
    }
}
