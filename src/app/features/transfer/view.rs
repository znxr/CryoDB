use super::{Message, State};
use crate::constants::{ICON_CLOSE_LINE, ICON_PLAY_LINE, MODAL_SCROLLBAR_GUTTER};
use crate::model::connection::DatabaseDriver;
use crate::model::transfer::{InnoDbVersion, TransferStage};
use crate::ui::ids::text_field_id;
use crate::ui::presentation::LayoutMode;
use crate::ui::presentation::Presentation;
use crate::ui::styles::{
    compact_button_style, compact_input_style, compact_pick_list_menu_style,
    compact_pick_list_style, compact_primary_button_style, modal_backdrop_style,
    panel_border_style, panel_style,
};
use crate::ui::widgets::history_input::history_input;
use iced::widget::{
    button, checkbox, container, mouse_area, pick_list, progress_bar, row, scrollable, space, stack,
};
use iced::{Center, Element, Fill, Length, mouse};

pub(crate) struct View<'a> {
    pub(crate) state: &'a State,
    pub(crate) driver: DatabaseDriver,
    pub(crate) databases: &'a [String],
    pub(crate) layout_mode: LayoutMode,
    pub(crate) presentation: Presentation<'a>,
}

impl<'a> View<'a> {
    pub(crate) fn import_modal(&self) -> Element<'a, Message> {
        let layout = self.layout_mode;
        let backdrop_dim = self.presentation.settings.modal_backdrop_dim;
        let modal_width = self.presentation.scale_f32(640.0);
        let modal_height = self.presentation.scale_f32(560.0);
        let modal_padding = self.presentation.scale_u16(12);
        let modal_inner_width = (modal_width - (modal_padding * 2.0)).max(0.0);

        let close_button: Element<'_, Message> =
            if self.state.import_stage == TransferStage::Running {
                button(
                    self.presentation
                        .action_label("Close", ICON_CLOSE_LINE, layout),
                )
                .padding(self.presentation.button_padding())
                .style(compact_button_style)
                .into()
            } else {
                button(
                    self.presentation
                        .action_label("Close", ICON_CLOSE_LINE, layout),
                )
                .padding(self.presentation.button_padding())
                .style(compact_button_style)
                .on_press(Message::CloseImportModal)
                .into()
            };

        let header = row![
            self.presentation.heading_text("Import database"),
            space::horizontal(),
            close_button,
        ]
        .spacing(self.presentation.scale_u16(6))
        .align_y(Center);

        let body: Element<'_, Message> = match self.state.import_stage {
            TransferStage::Configure => {
                let import_is_sqlite = self.driver == DatabaseDriver::Sqlite;
                let path_input = history_input(
                    text_field_id("import-path"),
                    if import_is_sqlite {
                        "SQLite source file or SQL script path"
                    } else {
                        "SQL file path"
                    },
                    &self.state.import_path,
                    Message::ImportPathChanged,
                    Message::TextEdited,
                )
                .padding(self.presentation.input_padding())
                .size(self.presentation.input_text_size())
                .font(self.presentation.ui_font())
                .style(compact_input_style)
                .width(Fill);

                let browse_button = button(self.presentation.button_text("Browse"))
                    .padding(self.presentation.button_padding_tight())
                    .style(compact_button_style)
                    .on_press(Message::ImportBrowsePath);

                let file_row = row![path_input, browse_button]
                    .spacing(self.presentation.scale_u16(6))
                    .align_y(Center);

                let source_section = iced::widget::column![
                    self.presentation.title_text("Source"),
                    iced::widget::column![
                        self.presentation.label_text(if import_is_sqlite {
                            "SQLite file or SQL script"
                        } else {
                            "SQL file"
                        }),
                        file_row
                    ]
                    .spacing(self.presentation.scale_u16(4)),
                ]
                .spacing(self.presentation.scale_u16(6));

                let target_database: Element<'_, Message> = if self.databases.is_empty() {
                    container(self.presentation.label_text("No databases loaded."))
                        .padding(self.presentation.scale_u16(6))
                        .width(Fill)
                        .style(panel_border_style)
                        .into()
                } else {
                    pick_list(
                        self.databases.to_vec(),
                        self.state.import_target_database.clone(),
                        Message::ImportTargetDatabaseSelected,
                    )
                    .padding(self.presentation.input_padding())
                    .text_size(self.presentation.input_text_size())
                    .font(self.presentation.ui_font())
                    .handle(self.presentation.pick_list_handle())
                    .style(compact_pick_list_style)
                    .menu_style(compact_pick_list_menu_style)
                    .width(Fill)
                    .into()
                };

                let target_section = if self.driver == DatabaseDriver::Sqlite {
                    iced::widget::column![
                        self.presentation.title_text("Target"),
                        self.presentation
                            .label_text("Imports into the currently open SQLite database file."),
                    ]
                    .spacing(self.presentation.scale_u16(6))
                } else if self.driver == DatabaseDriver::PostgreSql {
                    iced::widget::column![
                        self.presentation.title_text("Target"),
                        iced::widget::column![
                            self.presentation.label_text("Database"),
                            target_database,
                            self.presentation
                                .label_text("Uses external `psql` / `createdb` tools."),
                        ]
                        .spacing(self.presentation.scale_u16(4)),
                    ]
                    .spacing(self.presentation.scale_u16(6))
                } else {
                    let innodb_picker = pick_list(
                        InnoDbVersion::ALL,
                        Some(self.state.import_innodb_version),
                        Message::ImportInnoDbSelected,
                    )
                    .padding(self.presentation.input_padding())
                    .text_size(self.presentation.input_text_size())
                    .font(self.presentation.ui_font())
                    .handle(self.presentation.pick_list_handle())
                    .style(compact_pick_list_style)
                    .menu_style(compact_pick_list_menu_style)
                    .width(Fill);

                    iced::widget::column![
                        self.presentation.title_text("Target"),
                        iced::widget::column![
                            self.presentation.label_text("Database"),
                            target_database,
                            self.presentation.label_text("InnoDB version"),
                            innodb_picker,
                        ]
                        .spacing(self.presentation.scale_u16(4)),
                    ]
                    .spacing(self.presentation.scale_u16(6))
                };

                let options = if self.driver == DatabaseDriver::Sqlite {
                    iced::widget::column![
                        self.presentation.label_text(
                            "Imports a SQLite file by copy, or replays a SQL script when the source ends in .sql.",
                        ),
                        checkbox(self.state.import_drop_existing)
                            .label(crate::i18n::tr("Drop existing objects before SQL import"))
                            .on_toggle(Message::ImportDropExistingToggled)
                            .text_size(self.presentation.label_text_size())
                            .font(self.presentation.ui_font()),
                        checkbox(self.state.import_disable_foreign_keys)
                            .label(crate::i18n::tr("Disable foreign key checks during SQL import"))
                            .on_toggle(Message::ImportDisableForeignKeysToggled)
                            .text_size(self.presentation.label_text_size())
                            .font(self.presentation.ui_font()),
                        checkbox(self.state.import_use_transaction)
                            .label(crate::i18n::tr("Wrap SQL import in a single transaction"))
                            .on_toggle(Message::ImportUseTransactionToggled)
                            .text_size(self.presentation.label_text_size())
                            .font(self.presentation.ui_font()),
                    ]
                    .spacing(self.presentation.scale_u16(4))
                } else if self.driver == DatabaseDriver::PostgreSql {
                    iced::widget::column![
                        checkbox(self.state.import_create_database)
                            .label(crate::i18n::tr("Create database if missing (createdb)"))
                            .on_toggle(Message::ImportCreateDatabaseToggled)
                            .text_size(self.presentation.label_text_size())
                            .font(self.presentation.ui_font()),
                        checkbox(self.state.import_use_transaction)
                            .label(crate::i18n::tr("Use single transaction"))
                            .on_toggle(Message::ImportUseTransactionToggled)
                            .text_size(self.presentation.label_text_size())
                            .font(self.presentation.ui_font()),
                    ]
                    .spacing(self.presentation.scale_u16(4))
                } else {
                    iced::widget::column![
                        checkbox(self.state.import_create_database)
                            .label(crate::i18n::tr("Create database if missing"))
                            .on_toggle(Message::ImportCreateDatabaseToggled)
                            .text_size(self.presentation.label_text_size())
                            .font(self.presentation.ui_font()),
                        checkbox(self.state.import_drop_existing)
                            .label(crate::i18n::tr("Drop existing objects"))
                            .on_toggle(Message::ImportDropExistingToggled)
                            .text_size(self.presentation.label_text_size())
                            .font(self.presentation.ui_font()),
                        checkbox(self.state.import_disable_foreign_keys)
                            .label(crate::i18n::tr("Disable foreign key checks"))
                            .on_toggle(Message::ImportDisableForeignKeysToggled)
                            .text_size(self.presentation.label_text_size())
                            .font(self.presentation.ui_font()),
                        checkbox(self.state.import_use_transaction)
                            .label(crate::i18n::tr("Use single transaction"))
                            .on_toggle(Message::ImportUseTransactionToggled)
                            .text_size(self.presentation.label_text_size())
                            .font(self.presentation.ui_font()),
                    ]
                    .spacing(self.presentation.scale_u16(4))
                };

                let options_section =
                    iced::widget::column![self.presentation.title_text("Options"), options]
                        .spacing(self.presentation.scale_u16(6));

                let sections = iced::widget::column![
                    container(source_section)
                        .padding(self.presentation.scale_u16(10))
                        .width(Fill)
                        .max_width(modal_inner_width)
                        .style(panel_border_style),
                    container(target_section)
                        .padding(self.presentation.scale_u16(10))
                        .width(Fill)
                        .max_width(modal_inner_width)
                        .style(panel_border_style),
                    container(options_section)
                        .padding(self.presentation.scale_u16(10))
                        .width(Fill)
                        .max_width(modal_inner_width)
                        .style(panel_border_style),
                ]
                .spacing(self.presentation.scale_u16(10))
                .width(Fill);

                let scroll =
                    scrollable(container(sections).width(Fill).max_width(modal_inner_width))
                        .height(Fill)
                        .width(Fill)
                        .direction(iced::widget::scrollable::Direction::Vertical(
                            iced::widget::scrollable::Scrollbar::new()
                                .spacing(self.presentation.scale_f32(MODAL_SCROLLBAR_GUTTER)),
                        ));

                let start_button = button(self.presentation.action_label(
                    "Start import",
                    ICON_PLAY_LINE,
                    layout,
                ))
                .padding(self.presentation.button_padding())
                .style(compact_primary_button_style)
                .on_press(Message::StartImport);

                iced::widget::column![
                    scroll,
                    row![space::horizontal(), start_button]
                        .align_y(Center)
                        .spacing(self.presentation.scale_u16(6)),
                ]
                .spacing(self.presentation.scale_u16(10))
                .width(Fill)
                .height(Fill)
                .into()
            }
            TransferStage::Running | TransferStage::Completed => {
                let progress = self.state.import_progress.clamp(0.0, 1.0);
                let status = if !self.state.import_status.is_empty() {
                    self.state.import_status.clone()
                } else if self.state.import_stage == TransferStage::Completed {
                    String::from("Import complete.")
                } else {
                    transfer_status_text("Importing", progress)
                };

                let progress_block = container(
                    iced::widget::column![
                        self.presentation.label_text(status),
                        progress_bar(0.0..=1.0, progress).length(Length::Fill),
                    ]
                    .spacing(self.presentation.scale_u16(6))
                    .width(Fill),
                )
                .padding(self.presentation.scale_u16(10))
                .width(Fill)
                .style(panel_border_style);

                let action_button: Element<'_, Message> =
                    if self.state.import_stage == TransferStage::Completed {
                        button(
                            self.presentation
                                .action_label("Close", ICON_CLOSE_LINE, layout),
                        )
                        .padding(self.presentation.button_padding())
                        .style(compact_button_style)
                        .on_press(Message::CloseImportModal)
                        .into()
                    } else {
                        button(
                            self.presentation
                                .action_label("Cancel", ICON_CLOSE_LINE, layout),
                        )
                        .padding(self.presentation.button_padding())
                        .style(compact_button_style)
                        .on_press(Message::CancelImport)
                        .into()
                    };

                iced::widget::column![
                    progress_block,
                    row![space::horizontal(), action_button]
                        .align_y(Center)
                        .spacing(self.presentation.scale_u16(6)),
                ]
                .spacing(self.presentation.scale_u16(10))
                .width(Fill)
                .height(Fill)
                .into()
            }
        };

        let content = iced::widget::column![header, body]
            .spacing(self.presentation.scale_u16(10))
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
                .style(move |theme| modal_backdrop_style(theme, backdrop_dim)),
        )
        .on_press(Message::ModalBlocked)
        .on_scroll(|_| Message::ModalBlocked)
        .interaction(mouse::Interaction::Idle);

        let modal_layer = container(modal).width(Fill).height(Fill).center(Fill);

        stack![backdrop, modal_layer]
            .width(Fill)
            .height(Fill)
            .into()
    }
}
pub(super) fn transfer_status_text(verb: &str, progress: f32) -> String {
    let percent = (progress.clamp(0.0, 1.0) * 100.0).round() as u32;
    let remaining = 100_u32.saturating_sub(percent);
    format!("{verb} {percent}% ({remaining}% left)")
}

pub(super) fn with_tooltip<'a>(
    element: impl Into<Element<'a, Message>>,
    label: impl Into<String>,
) -> Element<'a, Message> {
    mouse_area(element.into())
        .on_enter(Message::Tooltip(crate::i18n::tr(&label.into())))
        .on_exit(Message::ClearTooltip)
        .interaction(mouse::Interaction::Pointer)
        .into()
}
