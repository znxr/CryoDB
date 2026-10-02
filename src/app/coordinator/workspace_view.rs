use crate::app::core::App;
use crate::app::features::workspace::diagram::model::DiagramState;
use crate::app::features::workspace::tabs::TabContentKind;
use crate::app::message::Message;
use crate::app::types::{LayoutMode, PaneKind};
use crate::app::view::TabContentRegion;
use crate::constants::{
    CHAT_SIDEBAR_MAX_WIDTH, CHAT_SIDEBAR_MIN_WIDTH, HEADER_PICKER_HIDE_WIDTH, ICON_CHAT_AI,
    ICON_CLOSE_LINE, ICON_DELETE_BIN_2_LINE, ICON_DIAGRAM, ICON_DIAGRAM_CODE, ICON_FILE_ADD_LINE,
    ICON_INFO_LINE, ICON_LOGOUT_BOX_R_LINE, ICON_MODERN_CLOSE, ICON_MODERN_PLAY, ICON_MODERN_STAR,
    ICON_MORE_OPTIONS, ICON_PLAY_LINE, ICON_REFRESH_LINE, ICON_RESIZE_HORIZONTAL, ICON_SETTINGS,
    ICON_STAR_LINE, ICON_TERMINAL, PANE_MIN_SIZE, RESPONSIVE_BREAKPOINT, TABLE_INFO_SIDEBAR_RATIO,
};
use crate::model::connection::DatabaseDriver;
use crate::ui::styles::{
    compact_button_style, compact_primary_button_style, disconnect_button_style,
    panel_border_style, panel_style,
};
use crate::ui::widgets::header_dropdown::HeaderDropdown;
use iced::widget::{button, container, pane_grid, responsive, row, space, stack, text};
use iced::{Center, Color, Element, Fill, Length, Padding, Size, alignment};

impl App {
    pub(crate) fn tabs_view(&self) -> crate::app::features::workspace::tabs_view::View<'_> {
        use crate::app::features::workspace::tabs_view::{DiagramTabInfo, View};
        View {
            state: &self.workspace.tabs,
            presentation: self.presentation(),
            diagrams: self
                .workspace
                .diagram
                .diagram_tabs
                .iter()
                .enumerate()
                .map(|(index, tab)| DiagramTabInfo {
                    title: tab.title.clone(),
                    display_title: self.diagram_tab_display_title(index),
                    pinned: tab.pinned,
                    has_changes: !tab.state.changes.is_empty(),
                })
                .collect(),
            active_diagram: self.workspace.diagram.active_diagram_tab,
            selected_table: self.workspace.selected_table.as_deref(),
            is_running_query: self.workspace.query.running,
            is_applying_changes: self.workspace.results.applying_changes,
            global_cursor: self.shell.global_cursor,
            theme: self.theme(),
            query_badges: (0..self.workspace.tabs.query_tabs.len())
                .filter(|index| self.query_tab_has_inactive_results_badge(*index))
                .collect(),
            table_badges: self
                .workspace
                .tabs
                .open_tables
                .iter()
                .filter(|table| self.table_tab_has_inactive_results_badge(table))
                .cloned()
                .collect(),
        }
    }

    pub(crate) fn transfer_view(&self) -> crate::app::features::transfer::view::View<'_> {
        crate::app::features::transfer::view::View {
            state: &self.transfer,
            driver: self.connections.current.driver,
            databases: &self.connections.databases,
            layout_mode: self.shell.layout_mode,
            presentation: self.presentation(),
        }
    }

    pub(crate) fn header_view(&self) -> Element<'_, Message> {
        let layout = self.shell.layout_mode;
        let hide_editor = self.is_query_editor_hidden();
        let show_header_pickers = matches!(layout, LayoutMode::Compact)
            || self.shell.window_size.width >= HEADER_PICKER_HIDE_WIDTH;

        let database_picker_width = if matches!(layout, LayoutMode::Compact) {
            self.scale_f32(200.0)
        } else {
            self.scale_f32(240.0)
        };
        let database_dropdown = self.database_dropdown(database_picker_width, false);

        let refresh_dbs_button: Element<'_, Message> = {
            let icon = if self.connections.loading_databases {
                self.loading_spinner_icon()
            } else {
                ICON_REFRESH_LINE
            };
            let content = self.icon_text(icon);

            let button = button(content)
                .padding(self.button_padding())
                .style(compact_button_style);

            let button = if self.connections.loading_databases {
                button
            } else {
                button.on_press(Message::Connections(
                    crate::app::features::connections::Message::RefreshDatabases,
                ))
            };

            self.with_tooltip(button, "Refresh databases")
        };

        let database_controls: Element<'_, Message> =
            if self.settings.values.multiple_connections_layout {
                space::horizontal().width(Length::Shrink).into()
            } else {
                match layout {
                    LayoutMode::Wide => row![database_dropdown, refresh_dbs_button]
                        .spacing(self.scale_u16(4))
                        .align_y(Center)
                        .into(),
                    LayoutMode::Compact => row![database_dropdown, refresh_dbs_button]
                        .spacing(self.scale_u16(4))
                        .align_y(Center)
                        .into(),
                }
            };

        let mut top_controls: Element<'_, Message> = database_controls;
        if !show_header_pickers {
            top_controls = space::horizontal().width(Length::Shrink).into();
        }

        let run_tooltip = if self.workspace.query.running {
            String::from("Running query...")
        } else {
            String::from("Run query")
        };

        let run_button: Element<'_, Message> = {
            let content = if self.workspace.query.running {
                self.icon_text(ICON_CLOSE_LINE)
            } else {
                self.icon_text(ICON_PLAY_LINE)
            };

            let button = button(content)
                .padding(self.button_padding())
                .style(compact_primary_button_style);

            let button: Element<'_, Message> = if self.workspace.query.running {
                button
                    .on_press(Message::Workspace(
                        crate::app::features::workspace::Message::Query(
                            crate::app::features::workspace::query::Message::CancelQuery,
                        ),
                    ))
                    .into()
            } else {
                button
                    .on_press(Message::Workspace(
                        crate::app::features::workspace::Message::Query(
                            crate::app::features::workspace::query::Message::RunQuery,
                        ),
                    ))
                    .into()
            };

            self.with_shortcut_tooltip(
                button,
                run_tooltip,
                &self.settings.values.run_query_shortcut,
            )
        };

        let new_button: Element<'_, Message> = {
            let button = button(self.icon_text(ICON_FILE_ADD_LINE))
                .padding(self.button_padding())
                .style(compact_button_style)
                .on_press(Message::Workspace(
                    crate::app::features::workspace::Message::Query(
                        crate::app::features::workspace::query::Message::NewQuery,
                    ),
                ));

            self.with_shortcut_tooltip(
                button,
                "New query",
                &self.settings.values.new_query_shortcut,
            )
        };

        let clear_button: Element<'_, Message> = {
            let button = button(self.icon_text(ICON_DELETE_BIN_2_LINE))
                .padding(self.button_padding())
                .style(compact_button_style);

            let button: Element<'_, Message> = if self.workspace.results.current.is_some()
                || self.workspace.query.error.is_some()
            {
                button
                    .on_press(Message::Workspace(
                        crate::app::features::workspace::Message::Query(
                            crate::app::features::workspace::query::Message::ClearResults,
                        ),
                    ))
                    .into()
            } else {
                button.into()
            };

            self.with_tooltip(button, "Clear results")
        };

        let save_query_button: Element<'_, Message> = {
            let button = button(self.icon_text(ICON_STAR_LINE))
                .padding(self.button_padding())
                .style(compact_button_style);
            let button: Element<'_, Message> =
                if self.workspace.query.editor.content().trim().is_empty() {
                    button.into()
                } else {
                    button
                        .on_press(Message::Workspace(
                            crate::app::features::workspace::Message::Query(
                                crate::app::features::workspace::query::Message::SaveCurrentQuery,
                            ),
                        ))
                        .into()
                };
            self.with_tooltip(button, "Save query")
        };

        let settings_button: Element<'_, Message> = self.with_shortcut_tooltip(
            button(self.icon_text(ICON_SETTINGS))
                .padding(self.button_padding())
                .style(compact_button_style)
                .on_press(Message::Settings(
                    crate::app::features::settings::Message::Settings,
                )),
            "Settings",
            &self.settings.values.open_settings_shortcut,
        );

        let more_options_button: Element<'_, Message> = button(self.icon_text(ICON_MORE_OPTIONS))
            .padding(self.button_padding())
            .style(compact_button_style)
            .on_press(Message::Shell(
                crate::app::shell::Message::ToggleMoreOptions,
            ))
            .into();

        let more_options_panel: Element<'_, Message> = if self.connections.more_options_open {
            let import_label = container(
                self.button_text("Import database")
                    .wrapping(text::Wrapping::None)
                    .width(Fill),
            )
            .width(Fill)
            .clip(true);
            let import_button = button(import_label)
                .padding(self.button_padding_tight())
                .width(Fill)
                .style(compact_button_style)
                .on_press(Message::Transfer(
                    crate::app::features::transfer::Message::OpenImportModal,
                ));

            let export_label = container(
                self.button_text("Export database")
                    .wrapping(text::Wrapping::None)
                    .width(Fill),
            )
            .width(Fill)
            .clip(true);
            let export_button = button(export_label)
                .padding(self.button_padding_tight())
                .width(Fill)
                .style(compact_button_style)
                .on_press(Message::Transfer(
                    crate::app::features::transfer::Message::OpenExportModal,
                ));

            container(
                iced::widget::column![import_button, export_button].spacing(self.scale_u16(4)),
            )
            .padding(self.scale_u16(6))
            .width(Length::Fixed(self.scale_f32(200.0)))
            .style(panel_border_style)
            .into()
        } else {
            space::horizontal().into()
        };

        let more_options_dropdown = HeaderDropdown::new(
            more_options_button,
            more_options_panel,
            self.connections.more_options_open,
            Message::Shell(crate::app::shell::Message::CloseMoreOptions),
        );

        let disconnect_button: Element<'_, Message> =
            button(self.icon_text(ICON_LOGOUT_BOX_R_LINE))
                .padding(self.button_padding())
                .style(disconnect_button_style)
                .on_press(Message::Connections(
                    crate::app::features::connections::Message::Disconnect,
                ))
                .into();

        let header_context_actions = self
            .workspace
            .tabs
            .tabs_hidden
            .then(|| self.context_actions());

        let query_actions = (!hide_editor && !self.settings.values.multiple_connections_layout)
            .then(|| {
                row![run_button, new_button, save_query_button, clear_button]
                    .spacing(self.scale_u16(4))
                    .align_y(Center)
            });

        let connection_actions: Element<'_, Message> =
            if self.settings.values.multiple_connections_layout {
                match layout {
                    LayoutMode::Wide => {
                        let left_island = container(
                            row![disconnect_button, settings_button, more_options_dropdown]
                                .spacing(self.scale_u16(4))
                                .align_y(Center),
                        )
                        .padding(self.scale_u16(3))
                        .style(panel_style);
                        let tabs_island = container(self.connection_tabs_bar())
                            .padding(self.scale_u16(3))
                            .width(Fill)
                            .style(panel_style);

                        row![left_island, tabs_island,]
                            .spacing(self.scale_u16(6))
                            .align_y(Center)
                            .width(Fill)
                            .into()
                    }
                    LayoutMode::Compact => {
                        let left_island = container(
                            row![disconnect_button, settings_button, more_options_dropdown]
                                .spacing(self.scale_u16(4))
                                .align_y(Center),
                        )
                        .padding(self.scale_u16(3))
                        .style(panel_style);
                        let tabs_island = container(self.connection_tabs_bar())
                            .padding(self.scale_u16(3))
                            .width(Fill)
                            .style(panel_style);

                        iced::widget::column![left_island, tabs_island,]
                            .spacing(self.scale_u16(6))
                            .into()
                    }
                }
            } else {
                match layout {
                    LayoutMode::Wide => row![
                        disconnect_button,
                        settings_button,
                        more_options_dropdown,
                        top_controls,
                        space::horizontal(),
                    ]
                    .spacing(self.scale_u16(4))
                    .align_y(Center)
                    .into(),
                    LayoutMode::Compact => iced::widget::column![
                        row![disconnect_button, settings_button, more_options_dropdown]
                            .spacing(self.scale_u16(4))
                            .align_y(Center),
                        top_controls,
                    ]
                    .spacing(self.scale_u16(6))
                    .into(),
                }
            };

        let toolbar: Element<'_, Message> = match layout {
            LayoutMode::Wide => {
                let mut toolbar = if self.settings.values.multiple_connections_layout {
                    row![connection_actions].align_y(Center).width(Fill)
                } else {
                    row![connection_actions, space::horizontal()]
                        .spacing(self.scale_u16(6))
                        .align_y(Center)
                };

                if let Some(context_actions) = header_context_actions {
                    toolbar = toolbar.push(context_actions);
                }

                if let Some(query_actions) = query_actions {
                    toolbar = toolbar.push(query_actions);
                }

                toolbar.into()
            }
            LayoutMode::Compact => {
                let mut toolbar =
                    iced::widget::column![connection_actions].spacing(self.scale_u16(6));

                if let Some(context_actions) = header_context_actions {
                    toolbar = toolbar.push(context_actions);
                }

                if let Some(query_actions) = query_actions {
                    toolbar = toolbar.push(query_actions);
                }

                toolbar.into()
            }
        };

        let mut header = iced::widget::column![toolbar].spacing(self.scale_u16(4));

        if let Some(error) = &self.connections.database_error {
            header = header.push(self.label_text(format!(
                "{}: {}",
                crate::i18n::tr("Database error"),
                error
            )));
        }

        if self.settings.values.multiple_connections_layout {
            container(header).width(Fill).into()
        } else {
            container(header)
                .padding(self.scale_u16(6))
                .width(Fill)
                .style(panel_style)
                .into()
        }
    }

    pub(crate) fn client_view(&self) -> Element<'_, Message> {
        if self.is_diagram_fullscreen() {
            return self.diagram_panel();
        }
        if self.modern() && !self.shell.zen_mode {
            return self.modern_client_view();
        }

        let show_header = !self.shell.zen_mode;
        let hide_sidebar = self.is_sidebar_hidden();
        let hide_editor = self.is_query_editor_hidden();
        let content = responsive(move |size| {
            let layout = if size.width < RESPONSIVE_BREAKPOINT {
                LayoutMode::Compact
            } else {
                LayoutMode::Wide
            };

            let workspace: Element<'_, Message> = if self.shell.zen_mode {
                let main: Element<'_, Message> = match layout {
                    LayoutMode::Wide => self.wide_workspace_view(layout, true, hide_editor),
                    LayoutMode::Compact => self.main_panel(layout),
                };

                let centered_main = container(main)
                    .width(Fill)
                    .max_width(self.scale_f32(1320.0))
                    .height(Fill);

                container(centered_main)
                    .width(Fill)
                    .height(Fill)
                    .center_x(Fill)
                    .into()
            } else {
                match layout {
                    LayoutMode::Compact => {
                        let main = self.main_panel(layout);
                        if hide_sidebar {
                            main
                        } else {
                            let sidebar = self.sidebar(layout);
                            iced::widget::column![sidebar, main]
                                .spacing(self.region_gap())
                                .height(Fill)
                                .into()
                        }
                    }
                    LayoutMode::Wide => self.wide_workspace_view(layout, hide_sidebar, hide_editor),
                }
            };

            let content: Element<'_, Message> = if !self.ai.chat_open || layout == LayoutMode::Wide
            {
                workspace
            } else {
                let chat_width = (size.width * TABLE_INFO_SIDEBAR_RATIO)
                    .clamp(
                        self.scale_f32(CHAT_SIDEBAR_MIN_WIDTH),
                        self.scale_f32(CHAT_SIDEBAR_MAX_WIDTH),
                    )
                    .min(size.width * 0.5);

                row![
                    container(workspace).width(Fill).height(Fill),
                    container(self.chat_panel())
                        .width(Length::Fixed(chat_width))
                        .height(Fill),
                ]
                .spacing(self.region_gap())
                .height(Fill)
                .into()
            };

            iced::widget::column![content, self.classic_footer(layout)]
                .spacing(self.region_gap())
                .height(Fill)
                .into()
        });

        if show_header {
            let header = self.header_view();
            container(iced::widget::column![header, content].spacing(self.region_gap()))
                .padding(self.region_padding())
                .width(Fill)
                .height(Fill)
                .into()
        } else {
            container(content)
                .padding([self.scale_u16(10), self.scale_u16(20)])
                .width(Fill)
                .height(Fill)
                .into()
        }
    }

    pub(crate) fn wide_workspace_view(
        &self,
        layout: LayoutMode,
        hide_sidebar: bool,
        hide_editor: bool,
    ) -> Element<'_, Message> {
        pane_grid::PaneGrid::new(&self.shell.panes, |_, pane_kind, _| {
            let content: Element<'_, Message> = match pane_kind {
                PaneKind::Sidebar => {
                    if hide_sidebar {
                        container(space::horizontal().width(Length::Fixed(0.0)))
                            .height(Fill)
                            .into()
                    } else {
                        let sidebar = container(self.sidebar(layout))
                            .padding(Padding {
                                top: 0.0,
                                right: self.region_gap(),
                                bottom: 0.0,
                                left: 0.0,
                            })
                            .width(Fill)
                            .height(Fill);
                        if self.modern() {
                            sidebar.into()
                        } else {
                            stack![sidebar, self.split_grip(alignment::Horizontal::Right)]
                                .width(Fill)
                                .height(Fill)
                                .into()
                        }
                    }
                }
                PaneKind::Editor => {
                    let editor: Element<'_, Message> = if hide_editor {
                        container(space::horizontal().width(Length::Fixed(0.0)))
                            .height(Fill)
                            .into()
                    } else if let Some(tabs) = self.tabs_panel() {
                        iced::widget::column![
                            tabs,
                            self.tab_content_panel(layout, TabContentRegion::Editor)
                        ]
                        .spacing(if self.modern() {
                            0.0
                        } else {
                            self.scale_u16(8)
                        })
                        .height(Fill)
                        .into()
                    } else {
                        self.tab_content_panel(layout, TabContentRegion::Editor)
                    };

                    editor
                }
                PaneKind::Results => match self.tabs_panel().filter(|_| hide_editor) {
                    Some(tabs) => iced::widget::column![
                        tabs,
                        self.tab_content_panel(layout, TabContentRegion::Results)
                    ]
                    .spacing(if self.modern() {
                        0.0
                    } else {
                        self.scale_u16(8)
                    })
                    .height(Fill)
                    .into(),
                    None => self.tab_content_panel(layout, TabContentRegion::Results),
                },
                PaneKind::Columns => {
                    if self.columns_panel_visible() {
                        self.modern_columns_panel()
                    } else {
                        container(space::horizontal().width(Length::Fixed(0.0)))
                            .height(Fill)
                            .into()
                    }
                }
                PaneKind::Chat => {
                    if self.ai.chat_open {
                        let chat = container(self.chat_panel())
                            .padding(Padding {
                                top: 0.0,
                                right: 0.0,
                                bottom: 0.0,
                                left: self.scale_f32(6.0),
                            })
                            .width(Fill)
                            .height(Fill);
                        if self.modern() {
                            chat.into()
                        } else {
                            stack![chat, self.split_grip(alignment::Horizontal::Left)]
                                .width(Fill)
                                .height(Fill)
                                .into()
                        }
                    } else {
                        container(space::horizontal().width(Length::Fixed(0.0)))
                            .height(Fill)
                            .into()
                    }
                }
            };

            pane_grid::Content::new(content)
        })
        .on_resize(8, |value| {
            Message::Shell(crate::app::shell::Message::PaneResized(value))
        })
        .style(|theme| {
            let mut style = pane_grid::default(theme);
            style.hovered_split = pane_grid::Line {
                color: Color::TRANSPARENT,
                width: 0.0,
            };
            style.picked_split = pane_grid::Line {
                color: Color::TRANSPARENT,
                width: 0.0,
            };
            style
        })
        .spacing(0)
        .min_size(
            if hide_sidebar || !self.ai.chat_open || !self.columns_panel_visible() {
                0.0
            } else {
                PANE_MIN_SIZE
            },
        )
        .into()
    }

    pub(crate) fn main_panel(&self, layout: LayoutMode) -> Element<'_, Message> {
        let spacing = match layout {
            LayoutMode::Wide => self.scale_u16(8),
            LayoutMode::Compact => self.scale_u16(5),
        };

        let tabs = self.tabs_panel();
        let mut content = iced::widget::column![].spacing(spacing);

        if let Some(tabs) = tabs {
            content = content.push(tabs);
        }

        if !self.is_query_editor_hidden() {
            content = content.push(self.tab_content_panel(layout, TabContentRegion::Editor));
        }

        content
            .push(self.tab_content_panel(layout, TabContentRegion::Results))
            .height(Fill)
            .into()
    }

    pub(crate) fn tab_content_panel(
        &self,
        layout: LayoutMode,
        region: TabContentRegion,
    ) -> Element<'_, Message> {
        match (
            self.active_tab_entry().map(|entry| entry.content_kind()),
            region,
        ) {
            (Some(TabContentKind::Query | TabContentKind::Table), TabContentRegion::Editor) => {
                self.editor_panel(layout)
            }
            (Some(TabContentKind::Query | TabContentKind::Table), TabContentRegion::Results) => {
                self.results_panel(layout)
            }
            (Some(TabContentKind::Diagram), TabContentRegion::Results) => self.diagram_panel(),
            _ => container(space::horizontal())
                .width(Fill)
                .height(Fill)
                .into(),
        }
    }

    pub(crate) fn diagram_view(&self) -> crate::app::features::workspace::diagram::view::View<'_> {
        crate::app::features::workspace::diagram::view::View {
            state: &self.workspace.diagram,
            presentation: self.presentation(),
            keys: self.diagram_store_keys(),
            spinner: self.loading_spinner_icon(),
        }
    }
    pub(crate) fn diagram_panel(&self) -> Element<'_, Message> {
        self.diagram_view().diagram_panel().map(|value| {
            Message::Workspace(crate::app::features::workspace::Message::Diagram(value))
        })
    }
    pub(crate) fn diagram_status_bar<'a>(
        &'a self,
        state: &'a DiagramState,
    ) -> Element<'a, Message> {
        self.diagram_view().diagram_status_bar(state).map(|value| {
            Message::Workspace(crate::app::features::workspace::Message::Diagram(value))
        })
    }
    pub(crate) fn tabs_panel(&self) -> Option<Element<'_, Message>> {
        if self.workspace.tabs.tabs_hidden {
            return None;
        }

        let tabs: Element<'_, Message> =
            if self.settings.values.tabs_enabled || !self.workspace.tabs.query_tabs.is_empty() {
                self.table_tabs_view()
            } else {
                self.ai_prompt_bar()
            };

        Some(tabs)
    }

    pub(crate) fn modern_context_actions(&self) -> Element<'_, Message> {
        if self.is_diagram_active() {
            let mut actions = row![].spacing(self.scale_u16(6)).align_y(Center);
            if self.settings.values.ai_enabled {
                actions = actions.push(self.modern_toolbar_icon(
                    ICON_CHAT_AI,
                    "Chat with AI",
                    Some(Message::Ai(
                        crate::app::features::ai::Message::ToggleChatSidebar,
                    )),
                ));
            }
            return actions.into();
        }

        let has_query = !self.workspace.query.editor.content().trim().is_empty();

        let run_label = row![
            self.icon_text(if self.workspace.query.running {
                ICON_MODERN_CLOSE
            } else {
                ICON_MODERN_PLAY
            }),
            self.label_text(self.tracked_caption("Run")),
        ]
        .spacing(self.scale_u16(8))
        .align_y(Center);

        let run_button = button(run_label)
            .padding(self.button_padding())
            .style(compact_primary_button_style);
        let run_button: Element<'_, Message> = if self.workspace.query.running {
            run_button
                .on_press(Message::Workspace(
                    crate::app::features::workspace::Message::Query(
                        crate::app::features::workspace::query::Message::CancelQuery,
                    ),
                ))
                .into()
        } else if has_query {
            run_button
                .on_press(Message::Workspace(
                    crate::app::features::workspace::Message::Query(
                        crate::app::features::workspace::query::Message::RunQuery,
                    ),
                ))
                .into()
        } else {
            run_button.into()
        };
        let run_button = self.with_shortcut_tooltip(
            run_button,
            if self.workspace.query.running {
                "Cancel query"
            } else {
                "Run query"
            },
            &self.settings.values.run_query_shortcut,
        );

        let mut actions = row![run_button].spacing(self.scale_u16(4)).align_y(Center);

        if !self.is_query_editor_hidden() {
            actions = actions
                .push(self.modern_toolbar_icon(
                    ICON_FILE_ADD_LINE,
                    "New query",
                    Some(Message::Workspace(
                        crate::app::features::workspace::Message::Query(
                            crate::app::features::workspace::query::Message::NewQuery,
                        ),
                    )),
                ))
                .push(self.modern_toolbar_icon(
                    ICON_MODERN_STAR,
                    "Save query",
                    has_query.then_some(Message::Workspace(
                        crate::app::features::workspace::Message::Query(
                            crate::app::features::workspace::query::Message::SaveCurrentQuery,
                        ),
                    )),
                ))
                .push(
                    self.modern_toolbar_icon(
                        ICON_DELETE_BIN_2_LINE,
                        "Clear results",
                        (self.workspace.results.current.is_some()
                            || self.workspace.query.error.is_some())
                        .then_some(Message::Workspace(
                            crate::app::features::workspace::Message::Query(
                                crate::app::features::workspace::query::Message::ClearResults,
                            ),
                        )),
                    ),
                );
        }

        if !self.is_query_editor_hidden() && self.settings.values.ai_enabled {
            actions = actions.push(self.modern_toolbar_icon(
                ICON_CHAT_AI,
                "Chat with AI",
                Some(Message::Ai(
                    crate::app::features::ai::Message::ToggleChatSidebar,
                )),
            ));
        }

        if self.connections.connected {
            actions = actions
                .push(self.modern_toolbar_icon(
                    ICON_DIAGRAM,
                    "Schema Diagram",
                    Some(Message::Workspace(
                        crate::app::features::workspace::Message::Diagram(
                            crate::app::features::workspace::diagram::Message::OpenSchemaDiagram,
                        ),
                    )),
                ))
                .push(
                    self.modern_toolbar_icon(
                        ICON_DIAGRAM_CODE,
                        "Show table DDL",
                        self.current_table_for_info()
                            .is_some()
                            .then_some(Message::Workspace(
                                crate::app::features::workspace::Message::OpenTableDdl,
                            )),
                    ),
                );
        }

        let table_info_available =
            self.current_table_for_info().is_some() && !self.workspace.explorer.table_info_loading;
        actions = actions.push(self.modern_toolbar_icon(
            ICON_INFO_LINE,
            "Table info",
            table_info_available.then_some(Message::Workspace(
                crate::app::features::workspace::Message::OpenTableInfoSidebar,
            )),
        ));

        if self.connections.current.driver == DatabaseDriver::PostgreSql {
            actions = actions.push(
                self.modern_toolbar_icon(
                    ICON_TERMINAL,
                    "PostgreSQL terminal",
                    self.can_open_postgres_terminal()
                        .then_some(Message::Connections(
                            crate::app::features::connections::Message::OpenPostgresTerminal,
                        )),
                ),
            );
        }

        actions.into()
    }

    pub(crate) fn modern_toolbar_icon(
        &self,
        icon: char,
        tooltip: &'static str,
        message: Option<Message>,
    ) -> Element<'_, Message> {
        let content = button(self.icon_text(icon))
            .padding(self.button_padding())
            .style(compact_button_style);
        let content: Element<'_, Message> = match message {
            Some(message) => content.on_press(message).into(),
            None => content.into(),
        };
        self.with_tooltip(content, tooltip)
    }

    pub(crate) fn context_actions(&self) -> Element<'_, Message> {
        if self.modern() {
            return self.modern_context_actions();
        }

        let hide_editor = self.is_query_editor_hidden();
        let mut actions = row![].spacing(self.scale_u16(6)).align_y(Center);

        if self.is_diagram_active() {
            if self.settings.values.ai_enabled {
                actions = actions.push(self.ai_query_button());
            }
            return actions.into();
        }

        if self.settings.values.multiple_connections_layout && !hide_editor {
            let run_tooltip = if self.workspace.query.running {
                String::from("Running query...")
            } else {
                String::from("Run query")
            };
            let run_icon = if self.workspace.query.running {
                ICON_CLOSE_LINE
            } else {
                ICON_PLAY_LINE
            };
            let run_button = button(self.icon_text(run_icon))
                .padding(self.button_padding_icon())
                .style(compact_button_style);
            let run_button: Element<'_, Message> = if self.workspace.query.running {
                run_button
                    .on_press(Message::Workspace(
                        crate::app::features::workspace::Message::Query(
                            crate::app::features::workspace::query::Message::CancelQuery,
                        ),
                    ))
                    .into()
            } else {
                run_button
                    .on_press(Message::Workspace(
                        crate::app::features::workspace::Message::Query(
                            crate::app::features::workspace::query::Message::RunQuery,
                        ),
                    ))
                    .into()
            };
            let new_button = button(self.icon_text(ICON_FILE_ADD_LINE))
                .padding(self.button_padding_icon())
                .style(compact_button_style)
                .on_press(Message::Workspace(
                    crate::app::features::workspace::Message::Query(
                        crate::app::features::workspace::query::Message::NewQuery,
                    ),
                ));
            let clear_button = button(self.icon_text(ICON_DELETE_BIN_2_LINE))
                .padding(self.button_padding_icon())
                .style(compact_button_style);
            let clear_button: Element<'_, Message> = if self.workspace.results.current.is_some()
                || self.workspace.query.error.is_some()
            {
                clear_button
                    .on_press(Message::Workspace(
                        crate::app::features::workspace::Message::Query(
                            crate::app::features::workspace::query::Message::ClearResults,
                        ),
                    ))
                    .into()
            } else {
                clear_button.into()
            };

            let save_query_button: Element<'_, Message> = {
                let button = button(self.icon_text(ICON_STAR_LINE))
                    .padding(self.button_padding_icon())
                    .style(compact_button_style);
                if self.workspace.query.editor.content().trim().is_empty() {
                    button.into()
                } else {
                    button
                        .on_press(Message::Workspace(
                            crate::app::features::workspace::Message::Query(
                                crate::app::features::workspace::query::Message::SaveCurrentQuery,
                            ),
                        ))
                        .into()
                }
            };

            actions = actions
                .push(self.with_shortcut_tooltip(
                    run_button,
                    run_tooltip,
                    &self.settings.values.run_query_shortcut,
                ))
                .push(self.with_shortcut_tooltip(
                    new_button,
                    "New query",
                    &self.settings.values.new_query_shortcut,
                ))
                .push(self.with_tooltip(save_query_button, "Save query"))
                .push(self.with_tooltip(clear_button, "Clear results"));
        }

        actions = actions.push(self.structure_toggle_button());

        if !hide_editor && self.settings.values.ai_enabled {
            actions = actions.push(self.ai_query_button());
        }

        if self.connections.connected {
            actions = actions.push(self.diagram_toolbar_button());
            if !self.is_diagram_active() {
                actions = actions.push(self.table_ddl_button());
            }
        }

        actions = actions.push(self.table_info_button());

        if self.connections.current.driver == DatabaseDriver::PostgreSql {
            actions = actions.push(self.postgres_terminal_button());
        }

        actions.into()
    }

    pub(crate) fn tab_drag_ghost(&self) -> Option<Element<'_, Message>> {
        self.tabs_view().tab_drag_ghost().map(|view| {
            view.map(|value| {
                Message::Workspace(crate::app::features::workspace::Message::Tabs(value))
            })
        })
    }

    pub(crate) fn split_grip(&self, edge: alignment::Horizontal) -> Element<'_, Message> {
        let mut color = self.theme().palette().text;
        color.a = 0.62;
        container(
            text(ICON_RESIZE_HORIZONTAL.to_string())
                .font(self.icon_font())
                .size(self.scale_f32(12.0))
                .color(color)
                .wrapping(text::Wrapping::None),
        )
        .width(Fill)
        .height(Fill)
        .align_x(edge)
        .align_y(alignment::Vertical::Center)
        .into()
    }

    pub(crate) fn table_tabs_view(&self) -> Element<'_, Message> {
        row![
            self.tabs_view()
                .table_tabs_view()
                .map(
                    |value| Message::Workspace(crate::app::features::workspace::Message::Tabs(
                        value
                    ))
                ),
            self.context_actions()
        ]
        .spacing(self.scale_u16(6))
        .align_y(Center)
        .width(Fill)
        .into()
    }

    pub(crate) fn tab_context_menu_overlay(&self, bounds: Size) -> Element<'_, Message> {
        self.tabs_view()
            .tab_context_menu_overlay(bounds)
            .map(|value| Message::Workspace(crate::app::features::workspace::Message::Tabs(value)))
    }
}
