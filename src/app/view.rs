use crate::app::core::App;
use crate::app::message::Message;
use crate::app::types::{OmniCommandScope, ToastAction, ToastLevel};
use crate::constants::{
    ICON_ARROW_DOWN_S_LINE, ICON_ARROW_RIGHT_S_LINE, ICON_CHECK_FILL, ICON_CLOSE_FILL,
    ICON_CLOSE_LINE, ICON_DIAGRAM, ICON_INFO_LINE, MODAL_SCROLLBAR_GUTTER, MODE_TABLE_SEARCH,
    OMNIBAR_TOP_OFFSET_RATIO,
};
use crate::model::connection::DatabaseDriver;
use crate::model::settings::ShortcutBinding;
use crate::ui::ids::{database_switcher_scroll_id, omni_bar_input_id, omni_bar_results_scroll_id};
use crate::ui::styles::{
    alert_style, changelog_kind_color, compact_button_style, compact_input_style,
    compact_primary_button_style, compact_tab_active_button_style, modal_backdrop_style,
    panel_border_style, panel_style, toast_button_style, toast_level_accent_color,
    tooltip_container_style,
};
use crate::utils::fuzzy::highlighted_spans;
use iced::Background;
use iced::widget::{
    button, container, mouse_area, responsive, row, scrollable, space, stack, text,
};
use iced::{Center, Element, Fill, Length, Padding, Point, alignment, mouse};
use std::fmt::Write;

#[derive(Clone, Copy)]
pub(crate) enum TabContentRegion {
    Editor,
    Results,
}

pub(crate) fn history_input<'a>(
    id: iced::widget::Id,
    placeholder: &str,
    value: &str,
    on_input: impl Fn(String) -> Message + Send + Sync + 'static,
) -> iced::widget::TextInput<'a, Message> {
    crate::ui::widgets::history_input::history_input(id, placeholder, value, on_input, |edit| {
        Message::Shell(crate::app::shell::Message::TextFieldEdited {
            id: edit.id,
            factory: edit.factory,
            previous: edit.previous,
            value: edit.value,
        })
    })
}

impl App {
    pub(crate) fn with_overlays<'a>(
        &'a self,
        content: Element<'a, Message>,
    ) -> Element<'a, Message> {
        let mut layers = stack![content];

        if self.shell.database_switcher_open {
            layers = layers.push(self.database_switcher_modal());
        }

        if self.workspace.results.text_modal_open {
            layers = layers.push(self.text_cell_modal());
        }

        if self.settings.settings_open {
            layers = layers.push(
                crate::app::features::settings::view::View {
                    state: &self.settings,
                    presentation: self.presentation(),
                    layout_mode: self.shell.layout_mode,
                    window_size: self.shell.window_size,
                    query_inline_suggestion_error: self.ai.query_inline_suggestion_error.as_deref(),
                    diagrams: self.saved_diagram_entries(),
                    update_status: self.updater.status.label(),
                    update_busy: self.updater.status.is_busy(),
                }
                .settings_modal()
                .map(Message::Settings),
            );
        }

        if self.transfer.import_modal_open {
            layers = layers.push(self.transfer_view().import_modal().map(Message::Transfer));
        }

        if self.transfer.export_modal_open {
            layers = layers.push(self.transfer_view().export_modal().map(Message::Transfer));
        }

        if self.shell.error_modal.is_some() {
            layers = layers.push(self.error_modal());
        }

        if self.shell.changelog_open {
            layers = layers.push(self.changelog_modal());
        }

        if self.workspace.explorer.table_modal.is_some() {
            layers = layers.push(self.table_modal_view());
        }

        if self.workspace.explorer.postgres_role_modal.is_some() {
            layers = layers.push(self.postgres_role_modal_view());
        }

        if self.workspace.explorer.table_info_sidebar_open {
            layers = layers.push(self.table_info_sidebar());
        }

        if self.connections.postgres_terminal_open {
            layers = layers.push(self.postgres_terminal_modal());
        }

        if self.ai.ai_modal_open && self.settings.values.ai_enabled {
            layers = layers.push(self.ai_query_modal());
        }

        if self.workspace.explorer.table_ddl.is_some() {
            layers = layers.push(self.table_ddl_overlay());
        }

        if self.workspace.tabs.tab_context_menu.is_some() {
            layers = layers.push(responsive(move |size| self.tab_context_menu_overlay(size)));
        }

        if let Some(ghost) = self.tab_drag_ghost() {
            layers = layers.push(ghost);
        }

        if let Some(suggestions) = self.query_suggestions_overlay() {
            layers = layers.push(suggestions);
        }

        if self.connections.favorite_context_menu.is_some() {
            layers = layers.push(self.favorite_context_menu_overlay());
        }

        if self.connections.favorite_edit_modal.is_some() {
            layers = layers.push(self.favorite_edit_modal_overlay());
        }

        if self.is_omni_bar_visible() {
            layers = layers.push(self.omni_bar_overlay());
        }

        if !self.shell.toasts.is_empty() {
            layers = layers.push(self.toast_overlay());
        }

        if let Some(text) = self.shell.hovered_tooltip_text.borrow().clone() {
            let cursor = self.shell.global_cursor.unwrap_or(Point::new(0.0, 0.0));
            let viewport_width = if self.shell.window_size.width > 0.0 {
                self.shell.window_size.width
            } else {
                self.scale_f32(1280.0)
            };
            let tooltip_width =
                ((text.chars().count().max(1) as f32) * self.button_text_size() as f32 * 0.62
                    + self.scale_f32(16.0))
                .min(self.scale_f32(360.0));
            let left = (cursor.x - self.scale_f32(40.0))
                .max(0.0)
                .min((viewport_width - tooltip_width).max(0.0));
            let tip = container(self.label_text(&text))
                .padding(self.scale_u16(5))
                .style(tooltip_container_style);
            let flip_below = cursor.y < self.scale_f32(96.0);
            let top = if flip_below {
                cursor.y + self.scale_f32(22.0)
            } else {
                (cursor.y - self.scale_f32(30.0)).max(0.0)
            };
            let positioned = container(tip).padding(Padding {
                top,
                left,
                right: 0.0,
                bottom: 0.0,
            });
            layers = layers.push(positioned.width(Fill).height(Fill));
        }

        layers.width(Fill).height(Fill).into()
    }

    fn toast_overlay(&self) -> Element<'_, Message> {
        let viewport_width = if self.shell.window_size.width > 0.0 {
            self.shell.window_size.width
        } else {
            self.scale_f32(1280.0)
        };
        let toast_width = (viewport_width - self.scale_f32(24.0))
            .max(self.scale_f32(140.0))
            .min(self.scale_f32(360.0));

        let mut stack_column = iced::widget::column![].spacing(self.scale_u16(6));

        for toast in &self.shell.toasts {
            let icon = match toast.level {
                ToastLevel::Info => ICON_INFO_LINE,
                ToastLevel::Success => ICON_CHECK_FILL,
                ToastLevel::Error => ICON_CLOSE_FILL,
            };
            let icon_color = toast_level_accent_color(toast.level);

            let content = row![
                container(self.icon_text(icon).color(icon_color))
                    .width(Length::Fixed(self.scale_f32(18.0)))
                    .align_x(alignment::Horizontal::Center),
                self.button_text(toast.message.clone())
                    .wrapping(text::Wrapping::Word)
                    .width(Fill),
            ]
            .spacing(self.scale_u16(6))
            .align_y(Center)
            .width(Fill);

            let press = match toast.action {
                ToastAction::Dismiss => {
                    Message::Shell(crate::app::shell::Message::ToastDismissed(toast.id))
                }
                ToastAction::OpenChangelog => {
                    Message::Shell(crate::app::shell::Message::OpenChangelog)
                }
            };

            let toast_button = button(container(content).width(Fill))
                .padding(self.button_padding())
                .width(Length::Fixed(toast_width))
                .style(move |theme, status| toast_button_style(toast.level, theme, status))
                .on_press(press);

            stack_column = stack_column.push(toast_button);
        }

        container(stack_column)
            .padding(Padding {
                top: self.scale_f32(10.0),
                right: self.scale_f32(10.0),
                bottom: 0.0,
                left: 0.0,
            })
            .width(Fill)
            .height(Fill)
            .align_x(alignment::Horizontal::Right)
            .align_y(alignment::Vertical::Top)
            .into()
    }

    fn omni_bar_overlay(&self) -> Element<'_, Message> {
        let Some(omni_bar) = self.shell.omni_bar.as_ref() else {
            return container(space::horizontal()).into();
        };
        let progress = self.shell.omni_bar_anim_progress.clamp(0.0, 1.0);
        if progress <= 0.0 {
            return container(space::horizontal()).into();
        }

        let active_theme = self.theme();
        let palette = active_theme.extended_palette();
        let normal_text = palette.background.weakest.text;
        let highlight_text = palette.primary.base.color;

        let results = self.omni_bar_results();
        let total = results.len();
        let selected = if total == 0 {
            0
        } else {
            omni_bar.selected_index.min(total - 1)
        };
        let row_height = self.omni_bar_result_row_height();
        let row_gap = self.omni_bar_result_row_gap();
        let row_button_height = (row_height - row_gap).max(self.scale_f32(36.0));
        let result_height = self.scale_f32(320.0);
        let viewport_rows = ((result_height / row_height).ceil() as usize).max(1);
        let overscan = 3_usize;
        let window_rows = viewport_rows + overscan * 2;
        let start = if total <= window_rows {
            0
        } else {
            let scroll_row = (omni_bar.scroll_offset_y / row_height).floor().max(0.0) as usize;
            scroll_row
                .saturating_sub(overscan)
                .min(total.saturating_sub(window_rows))
        };
        let end = (start + window_rows).min(total);
        let hidden_above = start;
        let hidden_below = total.saturating_sub(end);

        let placeholder = if omni_bar.mode == MODE_TABLE_SEARCH {
            if self.connections.current.driver == DatabaseDriver::PostgreSql {
                format!(
                    "Search objects (type {} for commands)",
                    self.settings.values.omni_prefix
                )
            } else {
                format!(
                    "Search tables (type {} for commands)",
                    self.settings.values.omni_prefix
                )
            }
        } else {
            match omni_bar.command_scope {
                OmniCommandScope::Default => String::from("Type a command"),
                OmniCommandScope::Databases => String::from("Select a database"),
                OmniCommandScope::Drivers => String::from("Select a driver"),
                OmniCommandScope::FolderTableTargets => String::from("Select a table"),
                OmniCommandScope::FolderDissolveTargets => String::from("Select a folder"),
                OmniCommandScope::FavoriteConnections => {
                    String::from("Select a favorite connection")
                }
                OmniCommandScope::RecentConnections => String::from("Select a recent connection"),
                OmniCommandScope::TabSwitcher => String::from("Switch to tab"),
                OmniCommandScope::FocusTargets => String::from("Focus on"),
                OmniCommandScope::SettingsRoot => String::from("Type a settings command"),
                OmniCommandScope::SettingsThemes => String::from("Select appearance setting"),
                OmniCommandScope::SettingsThemeModes => String::from("Select appearance mode"),
                OmniCommandScope::SettingsThemeVariants => String::from("Select interface style"),
                OmniCommandScope::SettingsManualThemes => {
                    String::from("Select manual color palette")
                }
                OmniCommandScope::SettingsDarkThemes => String::from("Select dark color palette"),
                OmniCommandScope::SettingsLightThemes => String::from("Select light color palette"),
                OmniCommandScope::SettingsAccentColors => String::from("Select accent color"),
                OmniCommandScope::SettingsFonts => String::from("Select a font family"),
                OmniCommandScope::SettingsFontSizes => String::from("Select a font size"),
                OmniCommandScope::SettingsUiDensity => String::from("Select UI density"),
                OmniCommandScope::SettingsAiProviders => String::from("Select AI provider"),
                OmniCommandScope::SettingsTableShortcuts => {
                    String::from("Select table-search shortcut")
                }
                OmniCommandScope::SettingsCommandShortcuts => {
                    String::from("Select command-palette shortcut")
                }
            }
        };

        let input = history_input(
            omni_bar_input_id(),
            &placeholder,
            &omni_bar.query,
            |value| Message::Shell(crate::app::shell::Message::OmniBarQueryChanged(value)),
        )
        .padding(self.input_padding())
        .size(self.input_text_size())
        .font(self.ui_font())
        .style(compact_input_style)
        .on_submit(Message::Shell(crate::app::shell::Message::OmniBarSubmit))
        .width(Fill);

        let mut list = iced::widget::column![].spacing(0);

        if results.is_empty() {
            list = list.push(
                container(self.label_text("No results."))
                    .padding([self.scale_u16(5), self.scale_u16(8)])
                    .width(Fill)
                    .style(panel_border_style),
            );
        } else {
            if hidden_above > 0 {
                list = list.push(
                    space::Space::new().height(Length::Fixed(row_height * hidden_above as f32)),
                );
            }
            for index in start..end {
                let Some(item) = results.get(index) else {
                    continue;
                };
                let title: Element<'_, Message> = if item.match_indices.is_empty() {
                    iced::widget::text(item.title.clone())
                        .font(self.ui_font())
                        .size(self.button_text_size())
                        .width(Fill)
                        .into()
                } else {
                    iced::widget::text::Rich::with_spans(highlighted_spans(
                        &item.title,
                        &item.match_indices,
                        normal_text,
                        highlight_text,
                    ))
                    .font(self.ui_font())
                    .size(self.button_text_size())
                    .width(Fill)
                    .into()
                };

                let row_content = iced::widget::column![
                    title,
                    self.label_text(format!("{} • {}", item.category, item.subtitle))
                ]
                .spacing(self.scale_u16(1))
                .width(Fill);

                let selected_row = index == selected;
                let row = button(container(row_content).width(Fill))
                    .padding([self.scale_u16(5), self.scale_u16(8)])
                    .height(Length::Fixed(row_button_height))
                    .width(Fill)
                    .style(move |theme, status| {
                        if selected_row {
                            compact_tab_active_button_style(theme, status)
                        } else {
                            compact_button_style(theme, status)
                        }
                    })
                    .on_press(Message::Shell(
                        crate::app::shell::Message::OmniBarResultPressed(index),
                    ));
                let row = container(row)
                    .width(Fill)
                    .height(Length::Fixed(row_height))
                    .padding(Padding {
                        top: 0.0,
                        right: 0.0,
                        bottom: row_gap,
                        left: 0.0,
                    });
                list = list.push(row);
            }
            if hidden_below > 0 {
                list = list.push(
                    space::Space::new().height(Length::Fixed(row_height * hidden_below as f32)),
                );
            }
        }

        let results = scrollable(list)
            .id(omni_bar_results_scroll_id())
            .height(Length::Fixed(result_height))
            .width(Fill)
            .on_scroll(|value| {
                Message::Shell(crate::app::shell::Message::OmniBarResultsScrolled(value))
            })
            .direction(iced::widget::scrollable::Direction::Vertical(
                iced::widget::scrollable::Scrollbar::default(),
            ));

        let header = row![
            self.title_text(omni_bar.mode.title(self.connections.current.driver)),
            space::horizontal(),
            self.label_text("Enter to run • Tab to autocomplete • Esc to close"),
        ]
        .spacing(self.scale_u16(6))
        .align_y(Center);

        let panel_width = self.scale_f32(760.0);
        let panel =
            container(iced::widget::column![header, input, results].spacing(self.scale_u16(6)))
                .padding(self.scale_u16(10))
                .width(Length::Fixed(panel_width))
                .style(panel_style);

        let mut backdrop_color = palette.background.strong.color;
        backdrop_color.a = 0.55 * progress;
        let backdrop = mouse_area(
            container(space::horizontal())
                .width(Fill)
                .height(Fill)
                .style(move |_| container::Style {
                    background: Some(Background::Color(backdrop_color)),
                    ..container::Style::default()
                }),
        )
        .on_press(Message::Shell(crate::app::shell::Message::CloseOmniBar))
        .on_scroll(|_| Message::Shell(crate::app::shell::Message::ModalBlocked))
        .interaction(mouse::Interaction::Idle);

        let base_height = if self.shell.window_size.height > 0.0 {
            self.shell.window_size.height
        } else {
            900.0
        };
        let settle_top = (base_height * OMNIBAR_TOP_OFFSET_RATIO).max(self.scale_f32(26.0));
        let animated_top = (settle_top - self.scale_f32(18.0) * (1.0 - progress)).max(0.0);

        let panel_layer = container(panel)
            .padding(Padding {
                top: animated_top,
                left: 0.0,
                right: 0.0,
                bottom: 0.0,
            })
            .width(Fill)
            .height(Fill)
            .align_x(alignment::Horizontal::Center)
            .align_y(alignment::Vertical::Top);

        stack![backdrop, panel_layer]
            .width(Fill)
            .height(Fill)
            .into()
    }

    pub(crate) fn diagram_toolbar_button(&self) -> Element<'_, Message> {
        let button = button(self.icon_text(ICON_DIAGRAM))
            .padding(self.button_padding_icon())
            .style(compact_button_style)
            .on_press(Message::Workspace(
                crate::app::features::workspace::Message::Diagram(
                    crate::app::features::workspace::diagram::Message::OpenSchemaDiagram,
                ),
            ));
        self.with_tooltip(button, "Schema Diagram")
    }

    pub(crate) fn chat_panel(&self) -> Element<'_, Message> {
        crate::app::features::ai::view::View {
            state: &self.ai,
            scope: self.chat_scope_key(),
            presentation: self.presentation(),
            theme: self.theme(),
            spinner: self.loading_spinner_icon(),
            layout_mode: self.shell.layout_mode,
        }
        .chat_panel()
        .map(Message::Ai)
    }

    pub(crate) fn database_switcher_modal(&self) -> Element<'_, Message> {
        let layout = self.shell.layout_mode;
        let modal_width = self.scale_f32(360.0);
        let modal_padding = self.scale_u16(12);
        let modal_inner_width = (modal_width - (modal_padding * 2.0)).max(0.0);

        let mut list = iced::widget::column![].spacing(self.scale_u16(4));

        if self.connections.databases.is_empty() {
            list = list.push(self.label_text("No databases loaded."));
        } else {
            for (index, database) in self.connections.databases.iter().enumerate() {
                let is_selected = index == self.shell.database_switcher_index;
                let entry = button(self.button_text(database.clone()))
                    .padding(self.button_padding_tight())
                    .width(Fill)
                    .style(if is_selected {
                        compact_primary_button_style
                    } else {
                        compact_button_style
                    })
                    .on_press(Message::Connections(
                        crate::app::features::connections::Message::DatabaseSelected(
                            database.clone(),
                        ),
                    ));
                list = list.push(entry);
            }
        }

        let list = scrollable(list)
            .id(database_switcher_scroll_id())
            .height(Length::Fixed(self.scale_f32(240.0)))
            .width(Fill);

        let header = row![
            self.title_text("Switch database"),
            space::horizontal(),
            button(self.action_label("Close", ICON_CLOSE_LINE, layout))
                .padding(self.button_padding())
                .style(compact_button_style)
                .on_press(Message::Connections(
                    crate::app::features::connections::Message::CloseDatabaseSwitcher
                )),
        ]
        .spacing(self.scale_u16(6))
        .align_y(Center);

        let content = iced::widget::column![
            header,
            self.label_text("Use arrows to select, Enter to apply."),
            list,
        ]
        .spacing(self.scale_u16(6))
        .width(Fill);

        let modal = container(content)
            .padding(modal_padding)
            .width(Length::Fixed(modal_width))
            .max_width(modal_inner_width + (modal_padding * 2.0))
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

    pub(crate) fn error_modal(&self) -> Element<'_, Message> {
        let Some(error) = &self.shell.error_modal else {
            return space::horizontal().into();
        };

        let layout = self.shell.layout_mode;
        let modal_width = self.scale_f32(520.0);
        let modal_height = self.scale_f32(360.0);
        let modal_padding = self.scale_u16(12);

        let copy_button = button(self.button_text("Copy"))
            .padding(self.button_padding())
            .style(compact_button_style)
            .on_press(Message::Shell(crate::app::shell::Message::CopyErrorModal));

        let close_button = button(self.action_label("Close", ICON_CLOSE_LINE, layout))
            .padding(self.button_padding())
            .style(compact_button_style)
            .on_press(Message::Shell(crate::app::shell::Message::CloseErrorModal));

        let header = row![
            self.heading_text(&error.title),
            space::horizontal(),
            copy_button,
            close_button,
        ]
        .spacing(self.scale_u16(6))
        .align_y(Center);

        let summary = container(self.label_text(&error.summary))
            .padding(self.scale_u16(6))
            .width(Fill)
            .style(alert_style);

        let details = scrollable(
            container(
                self.label_text(&error.details)
                    .wrapping(text::Wrapping::Word),
            )
            .padding(self.scale_u16(6))
            .width(Fill)
            .style(panel_border_style),
        )
        .height(Fill)
        .width(Fill)
        .direction(iced::widget::scrollable::Direction::Vertical(
            iced::widget::scrollable::Scrollbar::new()
                .spacing(self.scale_f32(MODAL_SCROLLBAR_GUTTER)),
        ));

        let content = iced::widget::column![header, summary, self.label_text("Details"), details,]
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

    pub(crate) fn changelog_modal(&self) -> Element<'_, Message> {
        let layout = self.shell.layout_mode;
        let releases = self.changelog_visible_releases();
        let modal_width = self.scale_f32(560.0);
        let modal_height = self.scale_f32(420.0);

        let close_button = button(self.action_label("Close", ICON_CLOSE_LINE, layout))
            .padding(self.button_padding())
            .style(compact_button_style)
            .on_press(Message::Shell(crate::app::shell::Message::CloseChangelog));

        let header = row![
            self.heading_text(crate::i18n::tr("What's new")),
            space::horizontal(),
            close_button,
        ]
        .spacing(self.scale_u16(6))
        .align_y(Center);

        let mut list = iced::widget::column![]
            .spacing(self.scale_u16(8))
            .width(Fill);

        for (index, release) in releases.iter().enumerate() {
            let expanded = self.shell.changelog_expanded == index;
            let heading = if release.date.is_empty() {
                format!("CryoDB {}", release.version)
            } else {
                format!("CryoDB {} · {}", release.version, release.date)
            };
            let chevron = if expanded {
                ICON_ARROW_DOWN_S_LINE
            } else {
                ICON_ARROW_RIGHT_S_LINE
            };

            let release_button = button(
                container(
                    row![
                        self.icon_text(chevron),
                        self.label_text(heading),
                        space::horizontal(),
                    ]
                    .spacing(self.scale_u16(6))
                    .align_y(Center),
                )
                .width(Fill),
            )
            .padding(self.button_padding())
            .width(Fill)
            .style(compact_button_style)
            .on_press(Message::Shell(
                crate::app::shell::Message::ChangelogReleaseToggled(index),
            ));

            list = list.push(release_button);

            if !expanded {
                continue;
            }

            let mut entries = iced::widget::column![]
                .spacing(self.scale_u16(6))
                .width(Fill);
            for entry in &release.entries {
                entries = entries.push(
                    row![
                        container(
                            self.muted_label_text(crate::i18n::tr(entry.kind.label()))
                                .color(changelog_kind_color(entry.kind, &self.theme())),
                        )
                        .width(Length::Fixed(self.scale_f32(74.0))),
                        self.label_text(crate::i18n::tr(&entry.text))
                            .wrapping(text::Wrapping::Word)
                            .width(Fill),
                    ]
                    .spacing(self.scale_u16(6))
                    .width(Fill),
                );
            }

            list = list.push(
                container(entries)
                    .padding(self.scale_u16(8))
                    .width(Fill)
                    .style(panel_border_style),
            );
        }

        let body = scrollable(list).height(Fill).width(Fill).direction(
            iced::widget::scrollable::Direction::Vertical(
                iced::widget::scrollable::Scrollbar::new()
                    .spacing(self.scale_f32(MODAL_SCROLLBAR_GUTTER)),
            ),
        );

        let acknowledge = button(self.button_text("Got it"))
            .padding(self.button_padding())
            .style(compact_primary_button_style)
            .on_press(Message::Shell(crate::app::shell::Message::CloseChangelog));

        let content = iced::widget::column![
            header,
            body,
            row![space::horizontal(), acknowledge].align_y(Center),
        ]
        .spacing(self.scale_u16(10))
        .width(Fill)
        .height(Fill);

        let modal = container(content)
            .padding(self.scale_u16(12))
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
        .on_press(Message::Shell(crate::app::shell::Message::CloseChangelog))
        .on_scroll(|_| Message::Shell(crate::app::shell::Message::ModalBlocked))
        .interaction(mouse::Interaction::Idle);

        let modal_layer = container(modal).width(Fill).height(Fill).center(Fill);

        stack![backdrop, modal_layer]
            .width(Fill)
            .height(Fill)
            .into()
    }

    pub(crate) fn error_modal_text(&self) -> Option<String> {
        let error = self.shell.error_modal.as_ref()?;
        let mut output = String::new();
        let _ = writeln!(&mut output, "{}", error.title);
        let _ = writeln!(&mut output, "\nSummary:\n{}", error.summary);
        let _ = writeln!(&mut output, "\nDetails:\n{}", error.details);
        Some(output)
    }

    pub(crate) fn with_shortcut_tooltip<'a>(
        &'a self,
        element: impl Into<Element<'a, Message>>,
        label: impl Into<String>,
        shortcut: &ShortcutBinding,
    ) -> Element<'a, Message> {
        self.with_tooltip(
            element,
            format!("{} · {shortcut}", crate::i18n::tr(&label.into())),
        )
    }

    pub(crate) fn with_tooltip<'a>(
        &'a self,
        element: impl Into<Element<'a, Message>>,
        text: impl Into<String>,
    ) -> Element<'a, Message> {
        let text = crate::i18n::tr(&text.into());
        mouse_area(element.into())
            .on_enter(Message::Shell(
                crate::app::shell::Message::SetHoveredTooltip(text),
            ))
            .on_exit(Message::Shell(
                crate::app::shell::Message::ClearHoveredTooltip,
            ))
            .interaction(mouse::Interaction::Pointer)
            .into()
    }
}
