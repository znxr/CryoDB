use crate::app::core::App;
use crate::app::message::Message;
use crate::app::types::LayoutMode;
use crate::constants::{
    ICON_BOTTOM_CHAT, ICON_BOTTOM_SIDEBAR, ICON_LOGOUT_BOX_R_LINE, ICON_MODERN_ADD,
    ICON_MODERN_CLOSE, ICON_MODERN_DATABASE, ICON_MODERN_PLUG, ICON_MODERN_SEARCH,
    ICON_MODERN_SETTINGS, ICON_MODERN_TRANSFER,
};
use crate::model::connection::DatabaseDriver;
use crate::ui::ids::table_search_input_id;
use crate::ui::styles::{
    compact_button_style, modern_scrollable_style, panel_border_style, table_input_style,
};
use crate::ui::widgets::header_dropdown::HeaderDropdown;
use crate::utils::text::{max_chars_for_width, truncate_with_ellipsis};
use iced::widget::{Space, button, column, container, mouse_area, row, scrollable, space, text};
use iced::{Background, Center, Color, Element, Fill, Length, Padding};

const CONNECTION_STRIP_HEIGHT: f32 = 38.0;
const SIDEBAR_WIDTH: f32 = 312.0;
const COLUMNS_ROW_HEIGHT: f32 = 28.0;
const FOOTER_HEIGHT: f32 = 34.0;
const REGION_PADDING: f32 = 22.0;

impl App {
    pub(crate) fn modern_client_view(&self) -> Element<'_, Message> {
        let workspace = self.wide_workspace_view(
            LayoutMode::Wide,
            self.is_sidebar_hidden(),
            self.is_query_editor_hidden(),
        );

        column![
            self.modern_connection_strip(),
            self.horizontal_hairline(),
            container(workspace).width(Fill).height(Fill),
            self.horizontal_hairline(),
            self.modern_footer(),
        ]
        .width(Fill)
        .height(Fill)
        .into()
    }

    fn hairline_style(&self) -> impl Fn(&iced::Theme) -> container::Style + Copy {
        |theme: &iced::Theme| container::Style {
            background: Some(Background::Color(crate::ui::theme::tokens(theme).line)),
            ..container::Style::default()
        }
    }

    pub(crate) fn horizontal_hairline(&self) -> Element<'_, Message> {
        container(Space::new())
            .width(Fill)
            .height(Length::Fixed(1.0))
            .style(self.hairline_style())
            .into()
    }

    fn vertical_hairline(&self) -> Element<'_, Message> {
        container(Space::new())
            .width(Length::Fixed(1.0))
            .height(Fill)
            .style(self.hairline_style())
            .into()
    }

    fn region_style(
        &self,
        pick: fn(&crate::ui::theme::Tokens) -> Color,
    ) -> impl Fn(&iced::Theme) -> container::Style + Copy {
        move |theme: &iced::Theme| container::Style {
            background: Some(Background::Color(pick(&crate::ui::theme::tokens(theme)))),
            ..container::Style::default()
        }
    }

    fn modern_connection_strip(&self) -> Element<'_, Message> {
        let mut tabs = row![].align_y(Center);

        for tab in &self.connections.tabs {
            let active = self.connections.active_tab_id == Some(tab.id);
            let driver_color = self.driver_marker_color(tab.connection.driver);
            let (display, _) = truncate_with_ellipsis(&tab.label, 28);

            let label = row![
                self.icon_text(ICON_MODERN_DATABASE)
                    .size(self.scale_f32(15.0))
                    .color(if active {
                        driver_color
                    } else {
                        self.modern_dim()
                    }),
                text(display)
                    .font(self.ui_font())
                    .size(self.scale_f32(12.0))
                    .color(if active {
                        self.modern_fg()
                    } else {
                        self.modern_dim()
                    }),
                self.with_tooltip(
                    mouse_area(
                        self.icon_text(ICON_MODERN_CLOSE)
                            .size(self.scale_f32(14.0))
                            .color(self.modern_dim())
                    )
                    .on_press(Message::Connections(
                        crate::app::features::connections::Message::ConnectionTabClosed(tab.id)
                    ))
                    .interaction(iced::mouse::Interaction::Pointer),
                    "Close connection",
                ),
            ]
            .spacing(self.scale_f32(10.0))
            .align_y(Center);

            let face = container(label)
                .padding(Padding {
                    top: 0.0,
                    right: self.scale_f32(18.0),
                    bottom: 0.0,
                    left: self.scale_f32(18.0),
                })
                .height(Length::Fixed(
                    self.scale_f32(CONNECTION_STRIP_HEIGHT) - self.scale_f32(2.0),
                ))
                .align_y(Center)
                .style(if active {
                    self.region_style(|t| t.bg)
                } else {
                    self.region_style(|t| t.chrome2)
                });

            let underline = container(Space::new())
                .width(Fill)
                .height(Length::Fixed(self.scale_f32(2.0)))
                .style(move |_: &iced::Theme| container::Style {
                    background: Some(Background::Color(if active {
                        driver_color
                    } else {
                        Color::TRANSPARENT
                    })),
                    ..container::Style::default()
                });

            let chip = column![face, underline].spacing(0);
            let chip: Element<'_, Message> = if active {
                chip.into()
            } else {
                self.with_tooltip(
                    mouse_area(chip)
                        .on_press(Message::Connections(
                            crate::app::features::connections::Message::ConnectionTabSelected(
                                tab.id,
                            ),
                        ))
                        .interaction(iced::mouse::Interaction::Pointer),
                    tab.label.clone(),
                )
            };

            tabs = tabs.push(chip);
            tabs = tabs.push(self.vertical_hairline());
        }

        let add = self.modern_icon_button(
            ICON_MODERN_ADD,
            17.0,
            "New connection",
            Some(Message::Connections(
                crate::app::features::connections::Message::StartNewConnection,
            )),
        );
        let transfer = self.modern_transfer_dropdown();
        let settings = self.modern_icon_button(
            ICON_MODERN_SETTINGS,
            15.0,
            "Settings",
            Some(Message::Settings(
                crate::app::features::settings::Message::Settings,
            )),
        );
        let disconnect = self.with_tooltip(
            button(
                self.icon_text(ICON_LOGOUT_BOX_R_LINE)
                    .size(self.scale_f32(15.0))
                    .color(crate::ui::theme::tokens(&self.theme()).danger),
            )
            .padding(self.button_padding_icon())
            .style(compact_button_style)
            .on_press(Message::Connections(
                crate::app::features::connections::Message::Disconnect,
            )),
            "Disconnect",
        );

        container(
            row![
                scrollable(tabs)
                    .direction(iced::widget::scrollable::Direction::Horizontal(
                        iced::widget::scrollable::Scrollbar::new()
                            .width(0.0)
                            .scroller_width(0.0)
                            .spacing(0.0),
                    ))
                    .width(Length::Shrink),
                space::horizontal(),
                add,
                transfer,
                settings,
                disconnect,
            ]
            .align_y(Center),
        )
        .width(Fill)
        .height(Length::Fixed(self.scale_f32(CONNECTION_STRIP_HEIGHT)))
        .style(self.region_style(|t| t.chrome2))
        .into()
    }

    fn modern_icon_button(
        &self,
        icon: char,
        size: f32,
        tooltip: &'static str,
        message: Option<Message>,
    ) -> Element<'_, Message> {
        let content = button(self.icon_text(icon).size(self.scale_f32(size)))
            .padding(self.button_padding_icon())
            .style(compact_button_style);
        let content: Element<'_, Message> = match message {
            Some(message) => content.on_press(message).into(),
            None => content.into(),
        };
        self.with_tooltip(content, tooltip)
    }

    fn modern_transfer_dropdown(&self) -> Element<'_, Message> {
        let trigger: Element<'_, Message> = mouse_area(
            button(
                self.icon_text(ICON_MODERN_TRANSFER)
                    .size(self.scale_f32(15.0)),
            )
            .padding(self.button_padding_icon())
            .style(compact_button_style)
            .on_press(Message::Transfer(
                crate::app::features::transfer::Message::ToggleTransferMenu,
            )),
        )
        .on_enter(Message::Shell(
            crate::app::shell::Message::SetHoveredTooltip("Import / export database".to_string()),
        ))
        .on_exit(Message::Shell(
            crate::app::shell::Message::ClearHoveredTooltip,
        ))
        .interaction(iced::mouse::Interaction::Pointer)
        .into();

        let panel: Element<'_, Message> = if self.transfer.transfer_menu_open {
            let entry = |label: &'static str, message: Message| {
                button(self.label_text(label))
                    .padding(self.button_padding_tight())
                    .width(Fill)
                    .style(compact_button_style)
                    .on_press(message)
            };
            container(
                column![
                    entry(
                        "Import database",
                        Message::Transfer(crate::app::features::transfer::Message::OpenImportModal)
                    ),
                    entry(
                        "Export database",
                        Message::Transfer(crate::app::features::transfer::Message::OpenExportModal)
                    ),
                ]
                .spacing(self.scale_u16(2)),
            )
            .padding(self.scale_u16(4))
            .width(Length::Fixed(self.scale_f32(190.0)))
            .style(panel_border_style)
            .into()
        } else {
            space::horizontal().into()
        };

        HeaderDropdown::new(
            trigger,
            panel,
            self.transfer.transfer_menu_open,
            Message::Transfer(crate::app::features::transfer::Message::CloseTransferMenu),
        )
        .into()
    }

    fn modern_dsn_label(&self) -> String {
        let connection = &self.connections.current;
        if connection.driver == DatabaseDriver::Sqlite {
            return connection.sqlite_path.clone();
        }
        format!(
            "{}@{}:{}/{}",
            connection.username, connection.host, connection.port, connection.database
        )
    }

    fn modern_fg(&self) -> Color {
        crate::ui::theme::tokens(&self.theme()).fg
    }

    fn modern_dim(&self) -> Color {
        crate::ui::theme::tokens(&self.theme()).muted
    }

    fn modern_faint(&self) -> Color {
        crate::ui::theme::tokens(&self.theme()).ghost
    }

    pub(crate) fn modern_filter_row<'a>(
        &'a self,
        filters: Element<'a, Message>,
    ) -> Element<'a, Message> {
        container(
            row![
                self.icon_text(ICON_MODERN_SEARCH)
                    .size(self.scale_f32(15.0))
                    .color(self.modern_faint()),
                crate::app::view::history_input(
                    table_search_input_id(),
                    "filter tables",
                    &self.workspace.explorer.table_search,
                    |search| {
                        Message::Workspace(crate::app::features::workspace::Message::Explorer(
                            crate::app::features::workspace::explorer::Message::TableSearchChanged(
                                search,
                            ),
                        ))
                    },
                )
                .font(self.ui_font())
                .size(self.scale_f32(12.5))
                .padding(0)
                .style(table_input_style)
                .width(Fill),
                text(self.workspace.explorer.tables.len().to_string())
                    .font(self.ui_font())
                    .size(self.scale_f32(11.0))
                    .color(self.modern_faint()),
                filters,
            ]
            .spacing(self.scale_f32(10.0))
            .align_y(Center),
        )
        .padding(Padding {
            top: 0.0,
            right: self.scale_f32(6.0),
            bottom: 0.0,
            left: self.scale_f32(12.0),
        })
        .width(Fill)
        .height(Length::Fixed(self.scale_f32(44.0)))
        .align_y(Center)
        .into()
    }

    pub(crate) fn modern_sidebar_identity(&self) -> Element<'_, Message> {
        let buttons = (self.button_padding_icon()[1] * 2.0 + self.icon_text_size() as f32) * 2.0;
        let width = self
            .sidebar_content_width(LayoutMode::Wide)
            .map(|available| (available - buttons - self.scale_f32(2.0)).max(self.scale_f32(96.0)))
            .unwrap_or_else(|| self.scale_f32(168.0));

        self.database_dropdown(width, false)
    }

    pub(crate) fn modern_columns_panel(&self) -> Element<'_, Message> {
        let table_label = self
            .workspace
            .query
            .table
            .as_ref()
            .or(self.workspace.selected_table.as_ref())
            .cloned()
            .unwrap_or_default();

        let caption = container(
            text(format!(
                "{} · {table_label}",
                self.tracked_caption("Columns")
            ))
            .font(self.ui_font())
            .size(self.scale_f32(10.5))
            .color(self.modern_faint()),
        )
        .padding(Padding {
            top: 0.0,
            right: self.scale_f32(REGION_PADDING),
            bottom: 0.0,
            left: self.scale_f32(REGION_PADDING),
        })
        .width(Fill)
        .height(Length::Fixed(self.scale_f32(38.0)))
        .align_y(Center);

        container(
            column![
                caption,
                scrollable(iced::widget::responsive(
                    move |size| self.modern_columns_rows(size.width)
                ))
                .style(modern_scrollable_style)
                .height(Fill),
            ]
            .height(Fill),
        )
        .width(Fill)
        .height(Fill)
        .style(self.region_style(|t| t.bg))
        .into()
    }

    fn modern_columns_rows(&self, available_width: f32) -> Element<'_, Message> {
        let padding = self.scale_f32(REGION_PADDING) * 2.0;
        let tag_width = self.scale_f32(64.0);
        let usable = (available_width - padding - tag_width).max(40.0);
        let max_chars = max_chars_for_width(usable / self.font_scale()).max(4);

        let mut rows = column![];
        if let Some(results) = self.workspace.results.current.as_ref() {
            for (index, name) in results.columns.iter().enumerate() {
                let kind = results
                    .column_kinds
                    .get(index)
                    .map(|kind| kind.display_label().to_string())
                    .unwrap_or_default();

                let (display, truncated) = truncate_with_ellipsis(name, max_chars);
                let name_text = text(display)
                    .font(self.ui_font())
                    .size(self.scale_f32(12.5))
                    .color(self.modern_fg())
                    .wrapping(iced::widget::text::Wrapping::None);
                let name_cell: Element<'_, Message> = if truncated {
                    self.with_tooltip(container(name_text).width(Fill).clip(true), name.clone())
                } else {
                    container(name_text).width(Fill).clip(true).into()
                };

                rows = rows.push(
                    container(
                        row![
                            name_cell,
                            text(kind)
                                .font(self.ui_font())
                                .size(self.scale_f32(9.0))
                                .color(self.modern_faint()),
                        ]
                        .spacing(self.scale_f32(12.0))
                        .align_y(Center),
                    )
                    .padding(Padding {
                        top: 0.0,
                        right: self.scale_f32(REGION_PADDING),
                        bottom: 0.0,
                        left: self.scale_f32(REGION_PADDING),
                    })
                    .width(Fill)
                    .height(Length::Fixed(self.scale_f32(COLUMNS_ROW_HEIGHT)))
                    .align_y(Center),
                );
            }
        }

        rows.into()
    }

    fn modern_footer(&self) -> Element<'_, Message> {
        let sidebar = container(self.bottom_bar_toggle(
            ICON_BOTTOM_SIDEBAR,
            !self.shell.sidebar_hidden,
            "Toggle sidebar",
            &self.settings.values.toggle_sidebar_shortcut,
            Message::Shell(crate::app::shell::Message::ToggleSidebar),
        ))
        .padding(Padding {
            top: 0.0,
            right: self.scale_f32(12.0),
            bottom: 0.0,
            left: self.scale_f32(12.0),
        })
        .height(Fill)
        .align_y(Center);
        let chat: Element<'_, Message> = if self.settings.values.ai_enabled {
            container(self.bottom_bar_toggle(
                ICON_BOTTOM_CHAT,
                self.ai.chat_open,
                "Toggle AI chat sidebar",
                &self.settings.values.toggle_chat_sidebar_shortcut,
                Message::Ai(crate::app::features::ai::Message::ToggleChatSidebar),
            ))
            .padding(Padding {
                top: 0.0,
                right: self.scale_f32(12.0),
                bottom: 0.0,
                left: self.scale_f32(12.0),
            })
            .height(Fill)
            .align_y(Center)
            .into()
        } else {
            space::horizontal().width(Length::Shrink).into()
        };
        let dsn = container(
            row![
                self.icon_text(ICON_MODERN_PLUG)
                    .size(self.scale_f32(14.0))
                    .color(self.driver_marker_color(self.connections.current.driver)),
                text(truncate_with_ellipsis(&self.modern_dsn_label(), 40).0)
                    .font(self.ui_font())
                    .size(self.scale_f32(11.5))
                    .color(self.modern_dim())
                    .wrapping(iced::widget::text::Wrapping::None),
            ]
            .spacing(self.scale_f32(10.0))
            .align_y(Center),
        )
        .padding(Padding {
            top: 0.0,
            right: self.scale_f32(REGION_PADDING),
            bottom: 0.0,
            left: self.scale_f32(REGION_PADDING),
        })
        .width(Length::Fixed(self.scale_f32(SIDEBAR_WIDTH)))
        .height(Fill)
        .align_y(Center)
        .clip(true);

        container(
            row![
                sidebar,
                dsn,
                self.vertical_hairline(),
                container(self.results_status_bar(LayoutMode::Wide))
                    .padding(Padding {
                        top: 0.0,
                        right: self.scale_f32(REGION_PADDING),
                        bottom: 0.0,
                        left: self.scale_f32(REGION_PADDING),
                    })
                    .width(Fill)
                    .height(Fill)
                    .align_y(Center),
                chat,
            ]
            .align_y(Center),
        )
        .width(Fill)
        .height(Length::Fixed(self.scale_f32(FOOTER_HEIGHT)))
        .style(self.region_style(|t| t.chrome))
        .into()
    }
}
