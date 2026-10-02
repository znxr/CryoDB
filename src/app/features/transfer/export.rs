use super::Message;
use crate::constants::{
    ICON_CLOSE_LINE, ICON_LOADER_LINE, ICON_PLAY_LINE, ICON_REFRESH_LINE, MODAL_SCROLLBAR_GUTTER,
    SIDEBAR_SCROLLBAR_GUTTER, SIDEBAR_SCROLLBAR_WIDTH,
};
use crate::model::connection::DatabaseDriver;
use crate::model::transfer::TransferStage;
use crate::ui::ids::text_field_id;
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

use super::view::{View, transfer_status_text, with_tooltip};

impl<'a> View<'a> {
    pub(crate) fn export_modal(&self) -> Element<'a, Message> {
        let layout = self.layout_mode;
        let backdrop_dim = self.presentation.settings.modal_backdrop_dim;
        let modal_width = self.presentation.scale_f32(640.0);
        let modal_height = self.presentation.scale_f32(580.0);
        let modal_padding = self.presentation.scale_u16(12);
        let modal_inner_width = (modal_width - (modal_padding * 2.0)).max(0.0);

        let close_button: Element<'_, Message> =
            if self.state.export_stage == TransferStage::Running {
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
                .on_press(Message::CloseExportModal)
                .into()
            };

        let header = row![
            self.presentation.heading_text("Export database"),
            space::horizontal(),
            close_button,
        ]
        .spacing(self.presentation.scale_u16(6))
        .align_y(Center);

        let body: Element<'_, Message> = match self.state.export_stage {
            TransferStage::Configure => {
                let export_is_sqlite = self.driver == DatabaseDriver::Sqlite;
                let path_input = history_input(
                    text_field_id("export-path"),
                    if export_is_sqlite {
                        "SQLite destination file or SQL dump path"
                    } else {
                        "Export file path"
                    },
                    &self.state.export_path,
                    Message::ExportPathChanged,
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
                    .on_press(Message::ExportBrowsePath);

                let file_row = row![path_input, browse_button]
                    .spacing(self.presentation.scale_u16(6))
                    .align_y(Center);

                let destination_section = iced::widget::column![
                    self.presentation.title_text("Destination"),
                    iced::widget::column![
                        self.presentation.label_text(if export_is_sqlite {
                            "SQLite file or SQL dump"
                        } else {
                            "Export file"
                        }),
                        file_row
                    ]
                    .spacing(self.presentation.scale_u16(4)),
                ]
                .spacing(self.presentation.scale_u16(6));

                let database_picker: Element<'_, Message> = if self.databases.is_empty() {
                    container(self.presentation.label_text("No databases loaded."))
                        .padding(self.presentation.scale_u16(6))
                        .width(Fill)
                        .style(panel_border_style)
                        .into()
                } else {
                    pick_list(
                        self.databases.to_vec(),
                        self.state.export_database.clone(),
                        Message::ExportDatabaseSelected,
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

                let database_section = iced::widget::column![
                    self.presentation.title_text("Database"),
                    iced::widget::column![
                        self.presentation.label_text(if export_is_sqlite {
                            "Uses the currently open SQLite database file"
                        } else {
                            "Database to export"
                        }),
                        database_picker,
                    ]
                    .spacing(self.presentation.scale_u16(4)),
                ]
                .spacing(self.presentation.scale_u16(6));

                let options = if self.driver == DatabaseDriver::Sqlite {
                    iced::widget::column![self.presentation.label_text(
                        "Copies the SQLite database by default, or writes a SQL schema+data dump when the destination ends in .sql.",
                    )]
                    .spacing(self.presentation.scale_u16(4))
                } else if self.driver == DatabaseDriver::PostgreSql {
                    iced::widget::column![
                        checkbox(self.state.export_include_drop)
                            .label(crate::i18n::tr("Include DROP statements (--clean)"))
                            .on_toggle(Message::ExportIncludeDropToggled)
                            .text_size(self.presentation.label_text_size())
                            .font(self.presentation.ui_font()),
                        checkbox(self.state.export_include_create)
                            .label(crate::i18n::tr("Include schema"))
                            .on_toggle(Message::ExportIncludeCreateToggled)
                            .text_size(self.presentation.label_text_size())
                            .font(self.presentation.ui_font()),
                        checkbox(self.state.export_include_inserts)
                            .label(crate::i18n::tr("Include data"))
                            .on_toggle(Message::ExportIncludeInsertsToggled)
                            .text_size(self.presentation.label_text_size())
                            .font(self.presentation.ui_font()),
                        self.presentation
                            .label_text("Uses external `pg_dump` tool."),
                    ]
                    .spacing(self.presentation.scale_u16(4))
                } else {
                    iced::widget::column![
                        checkbox(self.state.export_include_drop)
                            .label(crate::i18n::tr("Include DROP statements"))
                            .on_toggle(Message::ExportIncludeDropToggled)
                            .text_size(self.presentation.label_text_size())
                            .font(self.presentation.ui_font()),
                        checkbox(self.state.export_include_create)
                            .label(crate::i18n::tr("Include CREATE TABLE statements"))
                            .on_toggle(Message::ExportIncludeCreateToggled)
                            .text_size(self.presentation.label_text_size())
                            .font(self.presentation.ui_font()),
                        checkbox(self.state.export_include_inserts)
                            .label(crate::i18n::tr("Include INSERT statements"))
                            .on_toggle(Message::ExportIncludeInsertsToggled)
                            .text_size(self.presentation.label_text_size())
                            .font(self.presentation.ui_font()),
                        checkbox(self.state.export_use_values)
                            .label(crate::i18n::tr("Use VALUES (extended inserts)"))
                            .on_toggle(Message::ExportUseValuesToggled)
                            .text_size(self.presentation.label_text_size())
                            .font(self.presentation.ui_font()),
                        checkbox(self.state.export_include_routines)
                            .label(crate::i18n::tr("Include routines and events"))
                            .on_toggle(Message::ExportIncludeRoutinesToggled)
                            .text_size(self.presentation.label_text_size())
                            .font(self.presentation.ui_font()),
                    ]
                    .spacing(self.presentation.scale_u16(4))
                };

                let options_section =
                    iced::widget::column![self.presentation.title_text("Options"), options]
                        .spacing(self.presentation.scale_u16(6));

                let objects_mode = self.driver == DatabaseDriver::PostgreSql;
                let search_placeholder = if objects_mode {
                    "Search objects"
                } else {
                    "Search tables"
                };

                let search_input = history_input(
                    text_field_id("export-table-search"),
                    search_placeholder,
                    &self.state.export_table_search,
                    Message::ExportTableSearchChanged,
                    Message::TextEdited,
                )
                .padding(self.presentation.input_padding())
                .size(self.presentation.input_text_size())
                .font(self.presentation.ui_font())
                .style(compact_input_style)
                .width(Fill);

                let select_all_button = button(self.presentation.button_text("Select all"))
                    .padding(self.presentation.button_padding_tight())
                    .style(compact_button_style)
                    .on_press(Message::ExportSelectAllTables);

                let select_none_button = button(self.presentation.button_text("Select none"))
                    .padding(self.presentation.button_padding_tight())
                    .style(compact_button_style)
                    .on_press(Message::ExportSelectNoneTables);

                let selected_count = self.state.export_selected_table_count();
                let total_count = self.state.export_tables.len();

                let refresh_button: Element<'_, Message> = {
                    let icon = if self.state.export_tables_loading {
                        ICON_LOADER_LINE
                    } else {
                        ICON_REFRESH_LINE
                    };
                    let button = button(self.presentation.icon_text(icon))
                        .padding(self.presentation.button_padding_tight())
                        .style(compact_button_style);
                    let button: Element<'_, Message> = if self.state.export_tables_loading {
                        button.into()
                    } else {
                        button.on_press(Message::RefreshExportTables).into()
                    };
                    let refresh_label = if self.driver == DatabaseDriver::PostgreSql {
                        "Refresh objects"
                    } else {
                        "Refresh tables"
                    };
                    with_tooltip(button, refresh_label)
                };

                let tables_header = row![
                    self.presentation.label_text(format!(
                        "{selected_count}/{total_count} {}",
                        crate::i18n::tr("selected")
                    )),
                    space::horizontal(),
                    refresh_button,
                    select_all_button,
                    select_none_button,
                ]
                .spacing(self.presentation.scale_u16(6))
                .align_y(Center);

                let visible_tables = self.state.export_visible_tables();
                let mut table_entries =
                    iced::widget::column![].spacing(self.presentation.scale_u16(4));
                if self.state.export_tables_loading {
                    table_entries =
                        table_entries.push(self.presentation.label_text(if objects_mode {
                            "Loading objects..."
                        } else {
                            "Loading tables..."
                        }));
                } else if let Some(error) = &self.state.export_tables_error {
                    table_entries =
                        table_entries.push(self.presentation.label_text(if objects_mode {
                            format!("{}: {error}", crate::i18n::tr("Object error"))
                        } else {
                            format!("{}: {error}", crate::i18n::tr("Table error"))
                        }));
                } else if self.state.export_tables.is_empty() {
                    table_entries =
                        table_entries.push(self.presentation.label_text(if objects_mode {
                            "No objects loaded."
                        } else {
                            "No tables loaded."
                        }));
                } else if visible_tables.is_empty() {
                    table_entries =
                        table_entries.push(self.presentation.label_text(if objects_mode {
                            "No matching objects."
                        } else {
                            "No matching tables."
                        }));
                } else {
                    for table in &visible_tables {
                        let is_selected = !self.state.export_excluded_tables.contains(table);
                        let table_name = table.clone();
                        let entry = checkbox(is_selected)
                            .label(table.clone())
                            .on_toggle(move |checked| Message::ExportTableToggled {
                                table: table_name.clone(),
                                include: checked,
                            })
                            .text_size(self.presentation.label_text_size())
                            .font(self.presentation.ui_font());
                        table_entries = table_entries.push(entry);
                    }
                }

                let table_scroll = scrollable(table_entries)
                    .height(Length::Fixed(self.presentation.scale_f32(200.0)))
                    .width(Fill)
                    .direction(iced::widget::scrollable::Direction::Vertical(
                        iced::widget::scrollable::Scrollbar::new()
                            .width(self.presentation.scale_f32(SIDEBAR_SCROLLBAR_WIDTH))
                            .scroller_width(self.presentation.scale_f32(SIDEBAR_SCROLLBAR_WIDTH))
                            .spacing(self.presentation.scale_f32(SIDEBAR_SCROLLBAR_GUTTER)),
                    ));

                let tables_section = iced::widget::column![
                    self.presentation
                        .title_text(if objects_mode { "Objects" } else { "Tables" }),
                    search_input,
                    tables_header,
                    table_scroll,
                ]
                .spacing(self.presentation.scale_u16(6));

                let sections = if export_is_sqlite {
                    iced::widget::column![
                        container(destination_section)
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
                    .width(Fill)
                } else {
                    iced::widget::column![
                        container(destination_section)
                            .padding(self.presentation.scale_u16(10))
                            .width(Fill)
                            .max_width(modal_inner_width)
                            .style(panel_border_style),
                        container(database_section)
                            .padding(self.presentation.scale_u16(10))
                            .width(Fill)
                            .max_width(modal_inner_width)
                            .style(panel_border_style),
                        container(options_section)
                            .padding(self.presentation.scale_u16(10))
                            .width(Fill)
                            .max_width(modal_inner_width)
                            .style(panel_border_style),
                        container(tables_section)
                            .padding(self.presentation.scale_u16(10))
                            .width(Fill)
                            .max_width(modal_inner_width)
                            .style(panel_border_style),
                    ]
                    .spacing(self.presentation.scale_u16(10))
                    .width(Fill)
                };

                let scroll =
                    scrollable(container(sections).width(Fill).max_width(modal_inner_width))
                        .height(Fill)
                        .width(Fill)
                        .direction(iced::widget::scrollable::Direction::Vertical(
                            iced::widget::scrollable::Scrollbar::new()
                                .spacing(self.presentation.scale_f32(MODAL_SCROLLBAR_GUTTER)),
                        ));

                let start_button = button(self.presentation.action_label(
                    "Start export",
                    ICON_PLAY_LINE,
                    layout,
                ))
                .padding(self.presentation.button_padding())
                .style(compact_primary_button_style)
                .on_press(Message::StartExport);

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
                let progress = self.state.export_progress.clamp(0.0, 1.0);
                let status = if !self.state.export_status.is_empty() {
                    self.state.export_status.clone()
                } else if self.state.export_stage == TransferStage::Completed {
                    String::from("Export complete.")
                } else {
                    transfer_status_text("Exporting", progress)
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
                    if self.state.export_stage == TransferStage::Completed {
                        button(
                            self.presentation
                                .action_label("Close", ICON_CLOSE_LINE, layout),
                        )
                        .padding(self.presentation.button_padding())
                        .style(compact_button_style)
                        .on_press(Message::CloseExportModal)
                        .into()
                    } else {
                        button(
                            self.presentation
                                .action_label("Cancel", ICON_CLOSE_LINE, layout),
                        )
                        .padding(self.presentation.button_padding())
                        .style(compact_button_style)
                        .on_press(Message::CancelExport)
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
