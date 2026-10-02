use crate::app::core::App;
use crate::app::message::Message;
use crate::app::types::LayoutMode;
use crate::app::view::history_input;
use crate::constants::{
    ICON_ARROW_DOWN_S_LINE, ICON_CLIPBOARD_LINE, ICON_CLOSE_LINE, ICON_DATABASE_LINE,
    ICON_FILE_ADD_LINE, ICON_FOLDER_IMPORT, ICON_FOLDER_OPEN, ICON_GOTO, ICON_HEART_LINE,
    ICON_MORE_CONNECTIONS, ICON_SETTINGS, ICON_TRASH_LINE, LOGIN_SIDEBAR_WIDTH,
    POSTGRES_DEFAULT_PORT, SIDEBAR_CONTAINER_PADDING, SIDEBAR_SCROLLBAR_GUTTER,
    SIDEBAR_SCROLLBAR_WIDTH,
};
use crate::model::connection::{DatabaseDriver, StoredConnection, TlsMode};
use crate::model::settings::PickerOption;
use crate::ui::ids::{connection_tabs_scroll_id, text_field_id};
use crate::ui::styles::{
    alert_style, compact_button_style, compact_button_style_with_text_color, compact_input_style,
    compact_pick_list_menu_style, compact_pick_list_style, compact_primary_button_style,
    modal_backdrop_style, panel_border_style, panel_style,
};
use crate::ui::theme::ui_radius;
use crate::ui::widgets::header_dropdown::HeaderDropdown;
use crate::utils::text::truncate_with_ellipsis;
use iced::border::Radius;
use iced::widget::{
    Row, button, container, mouse_area, pick_list, row, scrollable, space, stack, text,
};
use iced::{Background, Border, Center, Color, Element, Fill, Length, Padding, alignment, mouse};

impl App {
    pub(crate) fn database_dropdown(
        &self,
        database_picker_width: f32,
        show_table_count: bool,
    ) -> Element<'_, Message> {
        let database_max_chars = self.pick_list_max_chars(database_picker_width);
        let database_filter = self.connections.database_search.trim().to_ascii_lowercase();
        let database_options: Vec<PickerOption<String>> = self
            .connections
            .databases
            .iter()
            .cloned()
            .filter_map(|database| {
                if !database_filter.is_empty()
                    && !database.to_ascii_lowercase().contains(&database_filter)
                {
                    return None;
                }
                let (display, _) = truncate_with_ellipsis(&database, database_max_chars);
                Some(PickerOption::new(database, display))
            })
            .collect();
        let (database_display, database_tooltip) = if let Some(database) = self.current_database() {
            let label = if show_table_count {
                format!("{} ({})", database, self.workspace.explorer.tables.len())
            } else {
                database
            };
            let (display, truncated) = truncate_with_ellipsis(&label, database_max_chars);
            let tooltip = if truncated { Some(label) } else { None };
            (display, tooltip)
        } else {
            (String::from("Select database"), None)
        };

        let database_picker_button = {
            let icon = container(self.icon_text(ICON_DATABASE_LINE))
                .align_y(iced::alignment::Vertical::Center);
            let label = container(
                self.button_text(database_display)
                    .wrapping(text::Wrapping::None)
                    .width(Fill),
            )
            .width(Fill)
            .clip(true);
            let content = row![icon, label]
                .spacing(self.scale_u16(4))
                .align_y(Center)
                .width(Fill);
            let arrow = container(self.icon_text(ICON_ARROW_DOWN_S_LINE))
                .align_y(iced::alignment::Vertical::Center);
            let content = row![content, arrow]
                .spacing(self.scale_u16(4))
                .align_y(Center)
                .width(Fill);
            let content: Element<'_, Message> = if self.modern() {
                container(content).height(Fill).align_y(Center).into()
            } else {
                content.into()
            };

            button(content)
                .padding(self.input_padding())
                .style(compact_button_style)
                .width(if self.modern() {
                    Fill
                } else {
                    Length::Fixed(database_picker_width)
                })
                .height(if self.modern() { Fill } else { Length::Shrink })
                .on_press(Message::Connections(
                    crate::app::features::connections::Message::ToggleDatabasePicker,
                ))
        };

        let database_picker_button: Element<'_, Message> = if let Some(label) = database_tooltip {
            self.with_tooltip(database_picker_button, label)
        } else {
            database_picker_button.into()
        };

        let database_panel: Element<'_, Message> = if self.connections.database_picker_open {
            let search_input = history_input(
                text_field_id("database-search"),
                "Search databases",
                &self.connections.database_search,
                |value| {
                    Message::Connections(
                        crate::app::features::connections::Message::DatabaseSearchChanged(value),
                    )
                },
            )
            .padding(self.input_padding())
            .size(self.input_text_size())
            .font(self.ui_font())
            .style(compact_input_style)
            .width(Fill);

            let mut list = iced::widget::column![].spacing(self.scale_u16(3));
            if self.connections.databases.is_empty() {
                list = list.push(self.label_text("No databases loaded."));
            } else if database_options.is_empty() {
                list = list.push(self.label_text("No matching databases."));
            } else {
                let current = self.current_database();
                for option in &database_options {
                    let is_selected = current
                        .as_deref()
                        .is_some_and(|name| name == option.value.as_str());
                    let item_label = container(
                        self.button_text(option.label.clone())
                            .wrapping(text::Wrapping::None)
                            .width(Fill),
                    )
                    .width(Fill)
                    .clip(true);

                    let entry = button(item_label)
                        .padding(self.button_padding_tight())
                        .width(Fill)
                        .style(if is_selected {
                            compact_primary_button_style
                        } else {
                            compact_button_style
                        })
                        .on_press(Message::Connections(
                            crate::app::features::connections::Message::DatabaseSelected(
                                option.value.clone(),
                            ),
                        ));

                    list = list.push(entry);
                }
            }

            let list = scrollable(list)
                .height(Length::Fixed(self.scale_f32(240.0)))
                .width(Fill)
                .direction(iced::widget::scrollable::Direction::Vertical(
                    iced::widget::scrollable::Scrollbar::new()
                        .width(self.scale_f32(SIDEBAR_SCROLLBAR_WIDTH))
                        .scroller_width(self.scale_f32(SIDEBAR_SCROLLBAR_WIDTH))
                        .spacing(self.scale_f32(SIDEBAR_SCROLLBAR_GUTTER)),
                ));

            container(iced::widget::column![search_input, list].spacing(self.scale_u16(4)))
                .padding(self.scale_u16(6))
                .width(Length::Fixed(database_picker_width))
                .style(panel_border_style)
                .into()
        } else {
            space::horizontal().into()
        };

        HeaderDropdown::new(
            database_picker_button,
            database_panel,
            self.connections.database_picker_open,
            Message::Connections(crate::app::features::connections::Message::ToggleDatabasePicker),
        )
        .into()
    }

    pub(crate) fn connection_picker_section<F>(
        &self,
        title: &str,
        entries: &[StoredConnection],
        on_press: F,
        max_chars: usize,
        empty_label: &str,
    ) -> Element<'_, Message>
    where
        F: Fn(usize) -> Message + Copy,
    {
        let mut list = iced::widget::column![].spacing(self.scale_u16(3));

        if entries.is_empty() {
            list = list.push(self.label_text(empty_label));
        } else {
            for (index, entry) in entries.iter().enumerate() {
                let label = entry.profile_label();
                let (display, truncated) = truncate_with_ellipsis(&label, max_chars);
                let marker = self.with_tooltip(
                    self.driver_marker(entry.driver, self.scale_f32(12.0)),
                    entry.driver.to_string(),
                );
                let mut row = row![
                    marker,
                    container(self.button_text(display).wrapping(text::Wrapping::None))
                        .width(Fill)
                        .clip(true)
                ]
                .spacing(self.scale_u16(6))
                .align_y(Center)
                .width(Fill);

                if !entry.tags.is_empty() {
                    let tag_color = self.driver_marker_color(entry.driver);
                    let mut badge_bg = tag_color;
                    badge_bg.a = 0.15;
                    let theme = self.theme();
                    let bg = theme.palette().background;
                    let is_dark_theme = (0.299 * bg.r + 0.587 * bg.g + 0.114 * bg.b) < 0.5;
                    let badge_text_color = if is_dark_theme {
                        Color::WHITE
                    } else {
                        Color::from_rgb(0.12, 0.12, 0.12)
                    };
                    let tag_badge_children: Vec<Element<'_, Message>> = entry
                        .tags
                        .iter()
                        .map(|tag| {
                            container(
                                text(tag.clone())
                                    .font(self.ui_font())
                                    .size(self.label_text_size().saturating_sub(2))
                                    .color(badge_text_color),
                            )
                            .padding([self.scale_u16(1), self.scale_u16(5)])
                            .style(move |_| container::Style {
                                background: Some(Background::Color(badge_bg)),
                                border: Border {
                                    color: tag_color,
                                    width: 1.0,
                                    radius: Radius::from(12.0),
                                },
                                ..container::Style::default()
                            })
                            .into()
                        })
                        .collect();
                    let tag_badges: Element<'_, Message> = Row::with_children(tag_badge_children)
                        .spacing(self.scale_u16(3))
                        .align_y(Center)
                        .into();
                    row = row.push(tag_badges);
                }
                let button = button(container(row).width(Fill))
                    .padding(self.button_padding_tight())
                    .width(Fill)
                    .style(compact_button_style);
                let button = if self.connections.connecting {
                    button
                } else {
                    button.on_press(on_press(index))
                };
                let item: Element<'_, Message> = if truncated {
                    self.with_tooltip(button, label)
                } else {
                    button.into()
                };
                list = list.push(item);
            }
        }

        iced::widget::column![self.label_text(title), list]
            .spacing(self.scale_u16(4))
            .into()
    }

    pub(crate) fn connection_picker_dropdown(&self) -> Element<'_, Message> {
        let panel_width = self.scale_f32(280.0);
        let max_chars = self.pick_list_max_chars(panel_width).saturating_sub(4);
        let add_button = button(self.icon_text(ICON_MORE_CONNECTIONS))
            .padding(self.button_padding())
            .style(compact_button_style)
            .on_press(Message::Connections(
                crate::app::features::connections::Message::ToggleConnectionPicker,
            ));
        let add_button = self.with_tooltip(add_button, "Add or switch connection");

        let panel: Element<'_, Message> = if self.connections.connection_picker_open {
            let favorites = self.connection_picker_section(
                "Favorites",
                &self.connections.favorites,
                |value| {
                    Message::Connections(
                        crate::app::features::connections::Message::FavoriteSelected(value),
                    )
                },
                max_chars,
                "No favorites yet.",
            );
            let recents = self.connection_picker_section(
                "Recent",
                &self.connections.recents,
                |value| {
                    Message::Connections(
                        crate::app::features::connections::Message::RecentSelected(value),
                    )
                },
                max_chars,
                "No recent connections.",
            );
            let sections = scrollable(
                iced::widget::column![favorites, recents]
                    .spacing(self.scale_u16(8))
                    .width(Fill),
            )
            .height(Length::Fixed(self.scale_f32(260.0)))
            .width(Fill)
            .direction(iced::widget::scrollable::Direction::Vertical(
                iced::widget::scrollable::Scrollbar::new()
                    .width(self.scale_f32(SIDEBAR_SCROLLBAR_WIDTH))
                    .scroller_width(self.scale_f32(SIDEBAR_SCROLLBAR_WIDTH))
                    .spacing(self.scale_f32(SIDEBAR_SCROLLBAR_GUTTER)),
            ));
            let add_new_label = container(
                self.button_text("Add new")
                    .wrapping(text::Wrapping::None)
                    .width(Fill),
            )
            .width(Fill)
            .clip(true);
            let add_new_button = button(add_new_label)
                .padding(self.button_padding_tight())
                .width(Fill)
                .style(compact_primary_button_style)
                .on_press(Message::Connections(
                    crate::app::features::connections::Message::StartNewConnection,
                ));

            container(
                iced::widget::column![sections, add_new_button]
                    .spacing(self.scale_u16(6))
                    .width(Fill),
            )
            .padding(self.scale_u16(6))
            .width(Length::Fixed(panel_width))
            .style(panel_border_style)
            .into()
        } else {
            space::horizontal().into()
        };

        HeaderDropdown::new(
            add_button,
            panel,
            self.connections.connection_picker_open,
            Message::Connections(crate::app::features::connections::Message::CloseConnectionPicker),
        )
        .into()
    }

    pub(crate) fn connection_tab_marker(
        &self,
        driver: DatabaseDriver,
        opacity: f32,
    ) -> Element<'_, Message> {
        let mut color = self.driver_marker_color(driver);
        color.a *= opacity;
        self.driver_marker_in(driver, self.scale_f32(12.0), color)
    }

    pub(crate) fn connection_tabs_bar(&self) -> Element<'_, Message> {
        let mut tabs = row![].spacing(self.scale_u16(4)).align_y(Center);
        let tab_max_chars = self.pick_list_max_chars(self.scale_f32(180.0));
        let active_theme = self.theme();
        let palette = active_theme.extended_palette();
        let background = active_theme.palette().background;
        let is_dark_theme =
            (0.299 * background.r + 0.587 * background.g + 0.114 * background.b) < 0.5;
        let active_text_color = if is_dark_theme {
            Color::WHITE
        } else {
            Color::from_rgb(0.12, 0.12, 0.12)
        };
        let mut inactive_text_color = palette.background.weakest.text;
        inactive_text_color.a *= 0.7;

        if self.connections.tabs.is_empty() {
            let label = self.current_connection_tab_label();
            let (display, truncated) = truncate_with_ellipsis(&label, tab_max_chars);
            let content = row![
                self.connection_tab_marker(self.connections.current.driver, 1.0),
                container(
                    self.button_text(display)
                        .wrapping(text::Wrapping::None)
                        .color(active_text_color)
                )
                .width(Fill)
                .clip(true)
            ]
            .spacing(self.scale_u16(6))
            .align_y(Center)
            .width(Length::Fixed(self.scale_f32(180.0)));
            let button = button(container(content).width(Length::Fixed(self.scale_f32(190.0))))
                .padding(self.button_padding())
                .style(move |theme, status| {
                    compact_button_style_with_text_color(theme, status, active_text_color)
                });
            let tab: Element<'_, Message> = if truncated {
                self.with_tooltip(button, label)
            } else {
                button.into()
            };
            tabs = tabs.push(tab);
        } else {
            for tab in &self.connections.tabs {
                let active = self.connections.active_tab_id == Some(tab.id);
                let opacity = if active { 1.0 } else { 0.7 };
                let driver_color = self.driver_marker_color(tab.connection.driver);
                let text_color = if active {
                    active_text_color
                } else {
                    inactive_text_color
                };
                let (display, truncated) = truncate_with_ellipsis(&tab.label, tab_max_chars);
                let content = row![
                    self.connection_tab_marker(tab.connection.driver, opacity),
                    container(
                        self.button_text(display)
                            .wrapping(text::Wrapping::None)
                            .color(text_color)
                    )
                    .width(Fill)
                    .clip(true)
                ]
                .spacing(self.scale_u16(6))
                .align_y(Center)
                .width(Length::Fixed(self.scale_f32(168.0)));
                let tab_hit_area = container(content)
                    .padding(self.button_padding())
                    .width(Length::Fixed(self.scale_f32(178.0)));
                let tab_hit_area: Element<'_, Message> = if active || self.connections.connecting {
                    tab_hit_area.into()
                } else {
                    mouse_area(tab_hit_area)
                        .on_press(Message::Connections(
                            crate::app::features::connections::Message::ConnectionTabSelected(
                                tab.id,
                            ),
                        ))
                        .interaction(mouse::Interaction::Pointer)
                        .into()
                };
                let tab_hit_area: Element<'_, Message> = if truncated {
                    self.with_tooltip(tab_hit_area, tab.label.clone())
                } else {
                    tab_hit_area
                };
                let close_hit_area = mouse_area(
                    container(self.icon_text(ICON_CLOSE_LINE).color(text_color))
                        .padding(self.button_padding_icon()),
                )
                .on_press(Message::Connections(
                    crate::app::features::connections::Message::ConnectionTabClosed(tab.id),
                ))
                .interaction(mouse::Interaction::Pointer);
                let chip = container(
                    row![tab_hit_area, close_hit_area]
                        .spacing(0)
                        .align_y(Center),
                )
                .style(move |theme| {
                    let palette = theme.extended_palette();
                    let mut background = palette.background.weakest.color;
                    let mut border_color = palette.background.strong.color;
                    if opacity < 1.0 {
                        background.a *= opacity;
                        border_color.a *= opacity;
                    }
                    container::Style {
                        background: Some(Background::Color(background)),
                        text_color: Some(text_color),
                        border: Border {
                            width: 1.0,
                            radius: ui_radius().into(),
                            color: border_color,
                        },
                        ..container::Style::default()
                    }
                })
                .clip(true);
                let chip: Element<'_, Message> = if self.modern() {
                    let thickness = self.scale_f32(2.0);
                    iced::widget::column![
                        chip,
                        container(space::Space::new())
                            .height(Length::Fixed(thickness))
                            .width(Fill)
                            .style(move |_| container::Style {
                                background: Some(Background::Color(if active {
                                    driver_color
                                } else {
                                    Color::TRANSPARENT
                                })),
                                ..container::Style::default()
                            }),
                    ]
                    .spacing(0)
                    .into()
                } else {
                    chip.into()
                };
                tabs = tabs.push(chip);
            }
        }

        let tab_strip = container(tabs).width(Length::Shrink);
        let tabs = scrollable(tab_strip)
            .id(connection_tabs_scroll_id())
            .on_scroll(|value| {
                Message::Connections(
                    crate::app::features::connections::Message::ConnectionTabsScrolled(value),
                )
            })
            .direction(iced::widget::scrollable::Direction::Horizontal(
                iced::widget::scrollable::Scrollbar::new()
                    .width(0.0)
                    .scroller_width(0.0)
                    .spacing(0.0),
            ))
            .height(Length::Shrink)
            .width(Fill);

        let tabs: Element<'_, Message> = container(mouse_area(tabs).on_scroll(|value| {
            Message::Connections(
                crate::app::features::connections::Message::ConnectionTabsWheelScrolled(value),
            )
        }))
        .width(Fill)
        .clip(true)
        .into();

        row![tabs, self.connection_picker_dropdown()]
            .spacing(self.scale_u16(4))
            .align_y(Center)
            .width(Fill)
            .into()
    }

    pub(crate) fn login_view(&self) -> Element<'_, Message> {
        let layout = self.shell.layout_mode;
        let heading = if self.connections.current.driver == DatabaseDriver::Sqlite {
            "Open SQLite Database"
        } else {
            "Connect to Database"
        };
        let mut form = iced::widget::column![self.heading_text(heading)].spacing(self.scale_u16(6));

        match self.connections.current.driver {
            DatabaseDriver::Sqlite => {
                let path_input = history_input(
                    text_field_id("sqlite-path"),
                    "SQLite file path",
                    &self.connections.current.sqlite_path,
                    |value| {
                        Message::Connections(
                            crate::app::features::connections::Message::SqlitePathChanged(value),
                        )
                    },
                )
                .padding(self.input_padding())
                .size(self.input_text_size())
                .font(self.ui_font())
                .style(compact_input_style)
                .width(Length::FillPortion(4));

                let import_button =
                    button(self.action_label("Import File", ICON_FOLDER_IMPORT, layout))
                        .padding(self.button_padding())
                        .style(compact_button_style)
                        .on_press(Message::Connections(
                            crate::app::features::connections::Message::SqliteBrowsePath,
                        ))
                        .width(Length::FillPortion(2));

                let create_button = button(self.action_label(
                    "Create New SQLite Database",
                    ICON_FILE_ADD_LINE,
                    layout,
                ))
                .padding(self.button_padding())
                .style(compact_button_style)
                .on_press(Message::Connections(
                    crate::app::features::connections::Message::SqliteCreatePath,
                ))
                .width(Fill);

                let sqlite_panel = container(
                    iced::widget::column![
                        self.title_text("SQLite Setup"),
                        self.label_text("Import an existing file or create a new database file.",),
                        row![path_input, import_button]
                            .spacing(self.scale_u16(6))
                            .width(Fill),
                        create_button,
                    ]
                    .spacing(self.scale_u16(6))
                    .width(Fill),
                )
                .padding(self.scale_u16(10))
                .style(panel_border_style)
                .width(Fill);

                form = form.push(sqlite_panel);
            }
            _ => {
                form = form
                    .push(
                        row![
                            history_input(
                                text_field_id("connection-host"),
                                "Host",
                                &self.connections.current.host,
                                |value| Message::Connections(
                                    crate::app::features::connections::Message::HostChanged(value)
                                )
                            )
                            .padding(self.input_padding())
                            .size(self.input_text_size())
                            .font(self.ui_font())
                            .style(compact_input_style)
                            .width(Length::FillPortion(3)),
                            history_input(
                                text_field_id("connection-port"),
                                "Port",
                                &self.connections.current.port,
                                |value| Message::Connections(
                                    crate::app::features::connections::Message::PortChanged(value)
                                )
                            )
                            .padding(self.input_padding())
                            .size(self.input_text_size())
                            .font(self.ui_font())
                            .style(compact_input_style)
                            .width(Length::FillPortion(1)),
                        ]
                        .spacing(self.scale_u16(6)),
                    )
                    .push(
                        history_input(
                            text_field_id("connection-database"),
                            "Database",
                            &self.connections.current.database,
                            |value| {
                                Message::Connections(
                                    crate::app::features::connections::Message::DatabaseChanged(
                                        value,
                                    ),
                                )
                            },
                        )
                        .padding(self.input_padding())
                        .size(self.input_text_size())
                        .font(self.ui_font())
                        .style(compact_input_style)
                        .width(Fill),
                    )
                    .push(
                        row![
                            history_input(
                                text_field_id("connection-username"),
                                "Username",
                                &self.connections.current.username,
                                |value| Message::Connections(
                                    crate::app::features::connections::Message::UsernameChanged(
                                        value
                                    )
                                )
                            )
                            .padding(self.input_padding())
                            .size(self.input_text_size())
                            .font(self.ui_font())
                            .style(compact_input_style)
                            .width(Length::FillPortion(1)),
                            history_input(
                                text_field_id("connection-password"),
                                "Password",
                                &self.connections.current.password,
                                |value| Message::Connections(
                                    crate::app::features::connections::Message::PasswordChanged(
                                        value
                                    )
                                )
                            )
                            .padding(self.input_padding())
                            .size(self.input_text_size())
                            .font(self.ui_font())
                            .style(compact_input_style)
                            .secure(true)
                            .width(Length::FillPortion(1)),
                        ]
                        .spacing(self.scale_u16(6)),
                    )
                    .push(
                        iced::widget::column![
                            self.label_text("TLS mode"),
                            pick_list(
                                TlsMode::ALL.to_vec(),
                                Some(self.connections.current.tls_mode),
                                |value| Message::Connections(
                                    crate::app::features::connections::Message::TlsModeSelected(
                                        value
                                    )
                                ),
                            )
                            .padding(self.input_padding())
                            .text_size(self.input_text_size())
                            .font(self.ui_font())
                            .handle(self.pick_list_handle())
                            .style(compact_pick_list_style)
                            .menu_style(compact_pick_list_menu_style)
                            .width(Fill)
                        ]
                        .spacing(self.scale_u16(4)),
                    );

                if matches!(
                    self.connections.current.tls_mode,
                    TlsMode::VerifyCa | TlsMode::VerifyFull
                ) {
                    form = form.push(
                        iced::widget::column![
                            self.label_text("CA certificate"),
                            row![
                                history_input(
                                    text_field_id("tls-ca-cert-path"),
                                    "Path to CA certificate",
                                    &self.connections.current.tls_ca_cert_path,
                                    |value| Message::Connections(crate::app::features::connections::Message::TlsCaCertPathChanged(value))
                                )
                                .padding(self.input_padding())
                                .size(self.input_text_size())
                                .font(self.ui_font())
                                .style(compact_input_style)
                                .width(Length::FillPortion(4)),
                                button(self.button_text("Browse"))
                                    .padding(self.button_padding_tight())
                                    .style(compact_button_style)
                                    .on_press(Message::Connections(crate::app::features::connections::Message::TlsCaCertBrowsePath))
                                    .width(Length::FillPortion(1)),
                            ]
                            .spacing(self.scale_u16(6))
                        ]
                        .spacing(self.scale_u16(4)),
                    );
                }
            }
        }

        if let Some(message) = &self.connections.connect_error {
            let copy_connect_error_button: Element<'_, Message> = {
                let button = button(self.icon_text(ICON_CLIPBOARD_LINE))
                    .padding(self.button_padding_icon())
                    .style(compact_button_style)
                    .on_press(Message::Connections(
                        crate::app::features::connections::Message::CopyConnectError,
                    ));
                self.with_tooltip(button, "Copy error")
            };

            let alert = container(
                iced::widget::column![
                    row![
                        self.label_text("Connection error").width(Fill),
                        copy_connect_error_button,
                    ]
                    .spacing(self.scale_u16(6))
                    .align_y(Center),
                    self.input_text(message),
                ]
                .spacing(self.scale_u16(4)),
            )
            .padding(self.scale_u16(6))
            .width(Fill)
            .style(alert_style);

            form = form.push(alert);
        }

        let can_connect = self.can_connect();

        let connect_label = if self.connections.connecting {
            "Connecting..."
        } else if self.connections.current.driver == DatabaseDriver::Sqlite {
            "Open Database"
        } else {
            "Connect"
        };
        let connect_icon = if self.connections.current.driver == DatabaseDriver::Sqlite {
            ICON_FOLDER_OPEN
        } else {
            ICON_GOTO
        };

        let connect_button = {
            let content: Element<'_, Message> =
                if self.connections.connecting && matches!(layout, LayoutMode::Compact) {
                    self.icon_text(self.loading_spinner_icon()).into()
                } else if self.connections.connecting {
                    self.button_text(connect_label).into()
                } else {
                    self.action_label(connect_label, connect_icon, layout)
                };

            let button = button(content)
                .padding(self.button_padding())
                .style(compact_primary_button_style);

            if can_connect {
                button.on_press(Message::Connections(
                    crate::app::features::connections::Message::Connect,
                ))
            } else {
                button
            }
        };

        let favorite_candidate = StoredConnection::from_info(&self.connections.current);
        let can_favorite = !self.connections.connecting && favorite_candidate.is_valid();
        let is_favorite = self
            .connections
            .favorites
            .iter()
            .any(|entry| entry.matches_identity(&favorite_candidate));
        let favorite_tooltip_label = if self.connections.current.driver == DatabaseDriver::Sqlite {
            "Add SQLite profile to favorites"
        } else {
            "Add connection to favorites"
        };
        let favorite_button: Element<'_, Message> = {
            let button = button(self.icon_text(ICON_HEART_LINE))
                .padding(self.button_padding_icon())
                .style(compact_button_style);
            let button = if can_favorite && !is_favorite {
                button.on_press(Message::Connections(
                    crate::app::features::connections::Message::AddFavorite,
                ))
            } else {
                button
            };
            self.with_tooltip(button, favorite_tooltip_label)
        };

        form = form.push(
            row![favorite_button, connect_button]
                .spacing(self.scale_u16(6))
                .align_y(Center),
        );

        let card_width = if self.connections.current.driver == DatabaseDriver::Sqlite {
            520.0
        } else {
            460.0
        };

        let card = container(form)
            .padding(self.scale_u16(14))
            .width(Length::Fixed(self.scale_f32(card_width)))
            .style(panel_style);

        let sidebar = self.login_sidebar();

        let base_content = row![sidebar, container(card).center(Fill)]
            .spacing(self.scale_u16(8))
            .padding(SIDEBAR_CONTAINER_PADDING)
            .width(Fill)
            .height(Fill);

        let base: Element<'_, Message> = if self.settings.values.multiple_connections_layout
            && !self.connections.tabs.is_empty()
        {
            let tabs_island = container(self.connection_tabs_bar())
                .padding(self.scale_u16(3))
                .width(Fill)
                .style(panel_style);
            let tabs_header = container(tabs_island).padding(Padding {
                top: self.scale_f32(6.0),
                right: self.scale_f32(6.0),
                bottom: 0.0,
                left: self.scale_f32(6.0),
            });

            iced::widget::column![tabs_header, base_content,]
                .spacing(self.scale_u16(6))
                .width(Fill)
                .height(Fill)
                .into()
        } else {
            base_content.into()
        };

        let settings_button: Element<'_, Message> = {
            let btn = button(self.icon_text(ICON_SETTINGS))
                .padding(self.button_padding())
                .style(compact_button_style)
                .on_press(Message::Settings(
                    crate::app::features::settings::Message::Settings,
                ));

            self.with_shortcut_tooltip(
                btn,
                "Settings",
                &self.settings.values.open_settings_shortcut,
            )
        };

        let floating = container(
            row![
                space::horizontal().width(Fill),
                iced::widget::column![
                    space::vertical().height(Fill),
                    container(settings_button).padding(self.scale_u16(6)),
                ]
                .spacing(self.scale_u16(0)),
            ]
            .spacing(self.scale_u16(0)),
        )
        .width(Fill)
        .height(Fill);

        stack![base, floating].width(Fill).height(Fill).into()
    }

    pub(crate) fn login_sidebar(&self) -> Element<'_, Message> {
        let max_chars = self.login_sidebar_max_chars();
        let sidebar_padding = self.scale_u16(8) * 2.0;
        let driver_picker_width = (self.scale_f32(LOGIN_SIDEBAR_WIDTH) - sidebar_padding).max(0.0);
        let favorites = self.connection_section(
            "Favorites",
            &self.connections.favorites,
            |value| {
                Message::Connections(
                    crate::app::features::connections::Message::FavoriteSelected(value),
                )
            },
            |value| {
                Message::Connections(crate::app::features::connections::Message::FavoriteDeleted(
                    value,
                ))
            },
            Some(|index| {
                Message::Connections(
                    crate::app::features::connections::Message::FavoriteContextMenuRequested {
                        index,
                    },
                )
            }),
            max_chars,
            "No favorites yet.",
            "Remove favorite",
        );
        let recents = self.connection_section(
            "Recent",
            &self.connections.recents,
            |value| {
                Message::Connections(crate::app::features::connections::Message::RecentSelected(
                    value,
                ))
            },
            |value| {
                Message::Connections(crate::app::features::connections::Message::RecentDeleted(
                    value,
                ))
            },
            None::<fn(usize) -> Message>,
            max_chars,
            "No recent connections.",
            "Remove recent connection",
        );

        let mut content = iced::widget::column![favorites, recents].spacing(self.scale_u16(10));

        if let Some(error) = &self.connections.store_error {
            let copy_connection_store_error_button: Element<'_, Message> = {
                let button = button(self.icon_text(ICON_CLIPBOARD_LINE))
                    .padding(self.button_padding_icon())
                    .style(compact_button_style)
                    .on_press(Message::Connections(
                        crate::app::features::connections::Message::CopyConnectionStoreError,
                    ));
                self.with_tooltip(button, "Copy error")
            };

            let error_panel = container(
                iced::widget::column![
                    row![
                        self.label_text("Connection warning").width(Fill),
                        copy_connection_store_error_button,
                    ]
                    .spacing(self.scale_u16(6))
                    .align_y(Center),
                    self.label_text(error).wrapping(text::Wrapping::Word),
                ]
                .spacing(self.scale_u16(4))
                .width(Fill),
            )
            .padding(self.scale_u16(6))
            .width(Fill)
            .style(alert_style);

            content = content.push(error_panel);
        }

        let scrollable = scrollable(content).height(Fill).direction(
            iced::widget::scrollable::Direction::Vertical(
                iced::widget::scrollable::Scrollbar::new()
                    .width(self.scale_f32(SIDEBAR_SCROLLBAR_WIDTH))
                    .scroller_width(self.scale_f32(SIDEBAR_SCROLLBAR_WIDTH))
                    .spacing(self.scale_f32(SIDEBAR_SCROLLBAR_GUTTER)),
            ),
        );

        let driver_marker_size = self.scale_f32(12.0);
        let driver_picker_button = {
            let content = row![
                self.driver_marker(self.connections.current.driver, driver_marker_size),
                self.button_text(self.connections.current.driver.to_string())
                    .width(Fill),
            ]
            .spacing(self.scale_u16(8))
            .align_y(Center)
            .width(Fill);

            let button = button(content)
                .padding(self.input_padding())
                .style(compact_button_style)
                .width(Length::Fixed(driver_picker_width));

            if self.connections.connecting {
                button
            } else {
                button.on_press(Message::Connections(
                    crate::app::features::connections::Message::ToggleDriverPicker,
                ))
            }
        };

        let driver_panel: Element<'_, Message> = if self.connections.driver_picker_open {
            let mut options = iced::widget::column![].spacing(self.scale_u16(3));
            for driver in DatabaseDriver::ALL {
                let marker = if driver == self.connections.current.driver {
                    self.driver_marker_in(
                        driver,
                        driver_marker_size,
                        crate::ui::styles::primary_foreground_color(&self.theme()),
                    )
                } else {
                    self.driver_marker(driver, driver_marker_size)
                };
                let content = row![marker, self.button_text(driver.to_string()).width(Fill),]
                    .spacing(self.scale_u16(8))
                    .align_y(Center)
                    .width(Fill);

                let style = if driver == self.connections.current.driver {
                    compact_primary_button_style
                } else {
                    compact_button_style
                };

                let button = button(content)
                    .padding(self.button_padding_tight())
                    .width(Fill)
                    .style(style);

                let button = if self.connections.connecting {
                    button
                } else {
                    button.on_press(Message::Connections(
                        crate::app::features::connections::Message::DriverSelected(driver),
                    ))
                };

                options = options.push(button);
            }

            container(options)
                .padding(self.scale_u16(6))
                .width(Length::Fixed(driver_picker_width))
                .style(panel_border_style)
                .into()
        } else {
            space::horizontal().into()
        };

        let driver_dropdown = HeaderDropdown::new(
            driver_picker_button.into(),
            driver_panel,
            self.connections.driver_picker_open,
            Message::Connections(crate::app::features::connections::Message::ToggleDriverPicker),
        );

        let driver_section = iced::widget::column![self.label_text("Driver"), driver_dropdown]
            .spacing(self.scale_u16(4))
            .width(Fill);

        let layout = iced::widget::column![scrollable, driver_section]
            .spacing(self.scale_u16(8))
            .height(Fill);

        container(layout)
            .padding(self.scale_u16(8))
            .width(Length::Fixed(self.scale_f32(LOGIN_SIDEBAR_WIDTH)))
            .height(Fill)
            .style(panel_style)
            .into()
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn connection_section<F, G, H>(
        &self,
        title: &str,
        entries: &[StoredConnection],
        on_press: F,
        on_delete: G,
        on_right_press: Option<H>,
        max_chars: usize,
        empty_label: &str,
        delete_label: &str,
    ) -> Element<'_, Message>
    where
        F: Fn(usize) -> Message + Copy,
        G: Fn(usize) -> Message + Copy,
        H: Fn(usize) -> Message + Copy,
    {
        let mut list = iced::widget::column![].spacing(self.scale_u16(3));

        if entries.is_empty() {
            list = list.push(self.label_text(empty_label));
        } else {
            let label_max_chars = max_chars.saturating_sub(3);
            let marker_size = self.scale_f32(12.0);
            for (index, entry) in entries.iter().enumerate() {
                let label = entry.profile_label();
                let (display, truncated) = truncate_with_ellipsis(&label, label_max_chars);
                let marker = self.with_tooltip(
                    self.driver_marker(entry.driver, marker_size),
                    entry.driver.to_string(),
                );

                let mut label_row = row![
                    marker,
                    container(self.button_text(display).wrapping(text::Wrapping::None))
                        .width(Fill)
                        .clip(true)
                ]
                .spacing(self.scale_u16(6))
                .align_y(Center)
                .width(Fill);

                if !entry.tags.is_empty() {
                    let tag_color = self.driver_marker_color(entry.driver);
                    let mut badge_bg = tag_color;
                    badge_bg.a = 0.15;
                    let theme = self.theme();
                    let bg = theme.palette().background;
                    let is_dark_theme = (0.299 * bg.r + 0.587 * bg.g + 0.114 * bg.b) < 0.5;
                    let badge_text_color = if is_dark_theme {
                        Color::WHITE
                    } else {
                        Color::from_rgb(0.12, 0.12, 0.12)
                    };
                    let tag_badge_children: Vec<Element<'_, Message>> = entry
                        .tags
                        .iter()
                        .map(|tag| {
                            container(
                                text(tag.clone())
                                    .font(self.ui_font())
                                    .size(self.label_text_size().saturating_sub(2))
                                    .color(badge_text_color),
                            )
                            .padding([self.scale_u16(1), self.scale_u16(5)])
                            .style(move |_| container::Style {
                                background: Some(Background::Color(badge_bg)),
                                border: Border {
                                    color: tag_color,
                                    width: 1.0,
                                    radius: Radius::from(12.0),
                                },
                                ..container::Style::default()
                            })
                            .into()
                        })
                        .collect();
                    let tag_badges: Element<'_, Message> = Row::with_children(tag_badge_children)
                        .spacing(self.scale_u16(3))
                        .align_y(Center)
                        .into();
                    label_row = label_row.push(tag_badges);
                }

                let item_label = container(label_row).width(Fill).clip(true);

                let label_button = button(item_label)
                    .padding(self.button_padding_tight())
                    .width(Fill)
                    .style(compact_button_style);
                let label_button = if self.connections.connecting {
                    label_button
                } else {
                    label_button.on_press(on_press(index))
                };

                let mut label_entry: Element<'_, Message> = if truncated {
                    self.with_tooltip(label_button, label)
                } else {
                    label_button.into()
                };

                if let Some(handler) = on_right_press
                    && !self.connections.connecting
                {
                    label_entry = mouse_area(label_entry)
                        .on_right_press(handler(index))
                        .into();
                }

                let label_entry: Element<'_, Message> = container(label_entry).width(Fill).into();
                let delete_button = self.with_tooltip(
                    button(self.icon_text(ICON_TRASH_LINE))
                        .padding(self.button_padding_icon())
                        .style(compact_button_style)
                        .on_press(on_delete(index)),
                    delete_label,
                );

                let row_entry = row![label_entry, delete_button]
                    .spacing(self.scale_u16(4))
                    .align_y(Center);

                list = list.push(row_entry);
            }
        }

        iced::widget::column![self.label_text(title), list]
            .spacing(self.scale_u16(4))
            .into()
    }

    pub(crate) fn favorite_context_menu_overlay(&self) -> Element<'_, Message> {
        let Some(state) = &self.connections.favorite_context_menu else {
            return container(space::horizontal()).into();
        };

        let edit_button = button(self.label_text("Edit favorite"))
            .padding(self.button_padding_tight())
            .width(Fill)
            .style(compact_button_style)
            .on_press(Message::Connections(
                crate::app::features::connections::Message::OpenFavoriteEditModal(state.index),
            ));

        let delete_button = button(self.label_text("Delete"))
            .padding(self.button_padding_tight())
            .width(Fill)
            .style(compact_button_style)
            .on_press(Message::Connections(
                crate::app::features::connections::Message::FavoriteDeleted(state.index),
            ));

        let menu = iced::widget::column![edit_button, delete_button]
            .spacing(self.scale_u16(2))
            .width(Length::Fixed(self.scale_f32(180.0)));

        let panel = container(menu)
            .padding(self.scale_u16(4))
            .style(panel_border_style);

        let menu_layer = container(panel)
            .padding(self.scale_u16(8))
            .width(Fill)
            .height(Fill)
            .align_x(alignment::Horizontal::Left)
            .align_y(alignment::Vertical::Top);

        let backdrop = mouse_area(container(space::horizontal()).width(Fill).height(Fill))
            .on_press(Message::Connections(
                crate::app::features::connections::Message::CloseFavoriteContextMenu,
            ))
            .on_right_press(Message::Connections(
                crate::app::features::connections::Message::CloseFavoriteContextMenu,
            ));

        stack![backdrop, menu_layer].width(Fill).height(Fill).into()
    }

    pub(crate) fn favorite_edit_modal_overlay(&self) -> Element<'_, Message> {
        let Some(state) = &self.connections.favorite_edit_modal else {
            return container(space::horizontal()).into();
        };

        let name_input = history_input(
            text_field_id("favorite-edit-name"),
            "Connection name",
            &state.name_input,
            |value| {
                Message::Connections(
                    crate::app::features::connections::Message::FavoriteEditNameChanged(value),
                )
            },
        )
        .padding(self.input_padding())
        .size(self.input_text_size())
        .font(self.ui_font())
        .style(compact_input_style)
        .width(Fill);

        let tags_input = history_input(
            text_field_id("favorite-edit-tags"),
            "Tags (comma separated, max 12 chars each)",
            &state.tags_input,
            |value| {
                Message::Connections(
                    crate::app::features::connections::Message::FavoriteEditTagsChanged(value),
                )
            },
        )
        .padding(self.input_padding())
        .size(self.input_text_size())
        .font(self.ui_font())
        .style(compact_input_style)
        .width(Fill);

        let save_button = button(self.button_text("Save"))
            .padding(self.button_padding_tight())
            .style(compact_primary_button_style)
            .on_press(Message::Connections(
                crate::app::features::connections::Message::SaveFavoriteEdit,
            ));

        let cancel_button = button(self.button_text("Cancel"))
            .padding(self.button_padding_tight())
            .style(compact_button_style)
            .on_press(Message::Connections(
                crate::app::features::connections::Message::CloseFavoriteEditModal,
            ));

        let close_button = button(self.icon_text(ICON_CLOSE_LINE))
            .padding(self.button_padding_icon())
            .style(compact_button_style)
            .on_press(Message::Connections(
                crate::app::features::connections::Message::CloseFavoriteEditModal,
            ));

        let header = row![
            self.heading_text("Edit connection appearance"),
            space::horizontal(),
            close_button,
        ]
        .spacing(self.scale_u16(6))
        .align_y(Center);

        let content = iced::widget::column![
            header,
            self.label_text(
                "Set the display name and short tag labels shown in Favorites and connection tabs.",
            )
            .wrapping(text::Wrapping::Word),
            iced::widget::column![self.label_text("Name"), name_input].spacing(self.scale_u16(4)),
            iced::widget::column![self.label_text("Tags"), tags_input].spacing(self.scale_u16(4)),
            row![space::horizontal(), cancel_button, save_button]
                .spacing(self.scale_u16(6))
                .align_y(Center),
        ]
        .spacing(self.scale_u16(10))
        .width(Fill);

        let panel = container(content)
            .padding(self.scale_u16(14))
            .width(Length::Fixed(self.scale_f32(460.0)))
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

        let modal_layer = container(panel).width(Fill).height(Fill).center(Fill);

        stack![backdrop, modal_layer]
            .width(Fill)
            .height(Fill)
            .into()
    }

    pub(crate) fn postgres_terminal_modal(&self) -> Element<'_, Message> {
        let layout = self.shell.layout_mode;
        let viewport_width = if self.shell.window_size.width > 0.0 {
            self.shell.window_size.width
        } else {
            self.scale_f32(1280.0)
        };
        let viewport_height = if self.shell.window_size.height > 0.0 {
            self.shell.window_size.height
        } else {
            self.scale_f32(720.0)
        };

        let modal_width = (viewport_width - self.scale_f32(24.0))
            .max(self.scale_f32(440.0))
            .min(self.scale_f32(1320.0));
        let modal_height = (viewport_height - self.scale_f32(24.0))
            .max(self.scale_f32(280.0))
            .min(self.scale_f32(880.0));
        let modal_padding = self.scale_u16(10);

        let title = if self.connections.postgres_terminal_title.trim().is_empty() {
            String::from("PostgreSQL terminal")
        } else {
            self.connections.postgres_terminal_title.clone()
        };
        let host = self.connections.current.host.trim();
        let host = if host.is_empty() { "localhost" } else { host };
        let port = self.connections.current.port.trim();
        let port = if port.is_empty() {
            POSTGRES_DEFAULT_PORT
        } else {
            port
        };
        let database = self
            .current_database()
            .unwrap_or_else(|| String::from("database"));
        let subtitle = format!("{host}:{port} · {database}");

        let close_button = button(self.action_label("Close", ICON_CLOSE_LINE, layout))
            .padding(self.button_padding())
            .style(compact_button_style)
            .on_press(Message::Connections(
                crate::app::features::connections::Message::ClosePostgresTerminal,
            ));

        let header = row![
            iced::widget::column![self.title_text(title), self.label_text(subtitle)]
                .spacing(self.scale_u16(2)),
            space::horizontal(),
            close_button
        ]
        .spacing(self.scale_u16(6))
        .align_y(Center);

        let terminal_view: Element<'_, Message> =
            if let Some(terminal) = &self.connections.postgres_terminal {
                iced_term::TerminalView::show(terminal).map(|value| {
                    Message::Connections(
                        crate::app::features::connections::Message::PostgresTerminalEvent(value),
                    )
                })
            } else {
                container(self.label_text("Terminal session is not running."))
                    .width(Fill)
                    .height(Fill)
                    .center(Fill)
                    .into()
            };

        let terminal_view = container(terminal_view)
            .padding(self.scale_u16(2))
            .width(Fill)
            .height(Fill)
            .style(panel_border_style);

        let hint = self
            .label_text("Copy: Ctrl/Cmd+Shift+C  Paste: Ctrl/Cmd+Shift+V")
            .wrapping(text::Wrapping::Word);

        let content = iced::widget::column![header, hint, terminal_view]
            .spacing(self.scale_u16(8))
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
