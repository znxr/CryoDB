use crate::app::features::ai;
use crate::app::features::onboarding;
use crate::app::features::settings;
use crate::app::features::transfer;
use crate::app::features::updater;
use crate::app::message::Message;
use crate::app::types::PaneKind;
use crate::constants::{
    AI_PULSE_INTERVAL_MS, INACTIVE_TAB_RELEASE_TICK_SECS, MODERN_COLUMNS_RATIO,
    TOAST_TICK_INTERVAL_MS, TOOLTIP_TICK_INTERVAL_MS,
};
use crate::model::connection::DatabaseDriver;
use crate::model::settings::{ThemeChoice, ThemeVariant};
use crate::storage::{load_connection_store, load_folder_store, load_settings_store};
use iced::widget::pane_grid;
use iced::{Element, Subscription, Task, keyboard};
use std::time::Duration;

pub(crate) struct App {
    pub(crate) shell: crate::app::shell::State,
    pub(crate) connections: crate::app::features::connections::State,
    pub(crate) ai: ai::State,
    pub(crate) workspace: crate::app::features::workspace::State,
    pub(crate) settings: settings::State,
    pub(crate) transfer: transfer::State,
    pub(crate) updater: updater::State,
    pub(crate) onboarding: onboarding::State,
}

impl App {
    pub(crate) fn new() -> (Self, Task<Message>) {
        let (mut panes, sidebar_pane) = pane_grid::State::new(PaneKind::Sidebar);
        let (editor_pane, sidebar_split) = panes
            .split(pane_grid::Axis::Vertical, sidebar_pane, PaneKind::Editor)
            .expect("split sidebar");
        let (_chat_pane, chat_split) = panes
            .split(pane_grid::Axis::Vertical, editor_pane, PaneKind::Chat)
            .expect("split chat");
        let (_results_pane, editor_split) = panes
            .split(pane_grid::Axis::Horizontal, editor_pane, PaneKind::Results)
            .expect("split editor");
        let (_columns_pane, columns_split) = panes
            .split(pane_grid::Axis::Vertical, editor_pane, PaneKind::Columns)
            .expect("split columns");

        let sidebar_ratio = 0.24;
        panes.resize(sidebar_split, sidebar_ratio);
        panes.resize(chat_split, 1.0);
        panes.resize(editor_split, 0.35);
        let (settings, settings_store_error, settings_theme) = load_settings_store();
        crate::i18n::set_language(settings.language);
        let onboarding_step = (!settings.onboarding_completed).then_some(0);

        panes.resize(
            columns_split,
            if settings.theme_variant == ThemeVariant::Modern {
                MODERN_COLUMNS_RATIO
            } else {
                1.0
            },
        );
        let default_query = DatabaseDriver::MySql.default_query();
        let theme_choice = settings_theme.unwrap_or(ThemeChoice::CarbonFrost);
        let query = Self::new_query_editor(default_query, &settings, theme_choice);
        let (connection_store, connection_store_error) = load_connection_store();
        let (folder_store, folder_store_error) = load_folder_store();

        (
            Self {
                shell: crate::app::shell::State::new(
                    panes,
                    sidebar_split,
                    chat_split,
                    columns_split,
                    editor_split,
                    Self::build_omni_command_registry(),
                ),
                connections: crate::app::features::connections::State::new(
                    connection_store,
                    connection_store_error,
                ),
                ai: ai::State::default(),
                workspace: crate::app::features::workspace::State::new(
                    folder_store,
                    folder_store_error,
                    query,
                ),
                settings: settings::State::new(settings, theme_choice, settings_store_error),
                transfer: transfer::State::default(),
                updater: updater::State::default(),
                onboarding: onboarding::State {
                    step: onboarding_step,
                },
            },
            Task::batch([
                Task::done(Message::Updater(updater::Message::Check {
                    announce: false,
                })),
                Task::done(Message::Shell(
                    crate::app::shell::Message::AnnounceChangelog,
                )),
                crate::app::state::load_font_choices_task(),
            ]),
        )
    }

    pub(crate) fn view(&self) -> Element<'_, Message> {
        if let Some(step) = self.onboarding.step {
            return onboarding::View {
                settings: &self.settings.values,
                font_choices: &self.settings.font_choices,
                theme: self.theme(),
                presentation: self.presentation(),
            }
            .onboarding_view(step)
            .map(Message::Onboarding);
        }

        let content: Element<'_, Message> = if self.connections.connected {
            let content = self.client_view();
            self.with_overlays(content)
        } else {
            let content = self.login_view();
            self.with_overlays(content)
        };

        let content: Element<'_, Message> = match self
            .updater
            .banner(self.presentation())
            .map(|banner| banner.map(Message::Updater))
        {
            Some(banner) => iced::widget::column![banner, content]
                .width(iced::Length::Fill)
                .height(iced::Length::Fill)
                .into(),
            None => content,
        };

        let content: Element<'_, Message> = iced::widget::mouse_area(content)
            .on_move(|value| Message::Shell(crate::app::shell::Message::GlobalCursorMoved(value)))
            .on_release(Message::Shell(
                crate::app::shell::Message::GlobalPointerReleased,
            ))
            .into();

        iced::widget::keyed_column![(self.settings.theme_choice, content)]
            .width(iced::Length::Fill)
            .height(iced::Length::Fill)
            .into()
    }

    pub(crate) fn subscription(&self) -> Subscription<Message> {
        let keyboard = if self.onboarding.step.is_some() {
            Subscription::none()
        } else if self.connections.connected {
            keyboard::listen()
                .map(|value| Message::Shell(crate::app::shell::Message::ClientKeyPressed(value)))
        } else {
            keyboard::listen().map(|value| {
                Message::Connections(crate::app::features::connections::Message::LoginKeyPressed(
                    value,
                ))
            })
        };

        let mut subscriptions = vec![
            keyboard,
            iced::event::listen_with(|event, status, _| {
                if status != iced::event::Status::Captured {
                    return None;
                }
                match event {
                    iced::Event::Keyboard(event) => {
                        Some(Message::Workspace(crate::app::features::workspace::Message::Query(crate::app::features::workspace::query::Message::QueryEditorCapturedKeyPressed(event))))
                    }
                    _ => None,
                }
            }),
            iced::window::resize_events()
                .map(|(_, size)| Message::Shell(crate::app::shell::Message::WindowResized(size))),
        ];

        if self.workspace.query.editor_resize_drag.is_some()
            || self.workspace.tabs.drag_tab.is_some()
            || self.workspace.tabs.pending_tab_drag.is_some()
        {
            subscriptions.push(iced::event::listen_with(|event, _, _| {
                matches!(
                    event,
                    iced::Event::Mouse(iced::mouse::Event::ButtonReleased(_))
                )
                .then_some(Message::Shell(
                    crate::app::shell::Message::GlobalPointerReleased,
                ))
            }));
        }

        if self.has_active_loading_animation()
            || self.workspace.query.inline_suggestion_pending
            || self.workspace.query.inline_suggestion_loading
            || self.diagram_agent_animating()
            || self.diagram_search_animating()
            || !self.workspace.results.pending_edits.is_empty()
        {
            subscriptions.push(
                iced::time::every(Duration::from_millis(AI_PULSE_INTERVAL_MS))
                    .map(|_| Message::Shell(crate::app::shell::Message::AiPulseTick)),
            );
        }

        if self.settings.values.release_inactive_tab_memory
            && (self.workspace.tabs.query_tabs.len() > 1
                || self.workspace.tabs.open_tables.len() > 1)
        {
            subscriptions.push(
                iced::time::every(Duration::from_secs(INACTIVE_TAB_RELEASE_TICK_SECS)).map(|_| {
                    Message::Workspace(
                        crate::app::features::workspace::Message::InactiveTabPruneTick,
                    )
                }),
            );
        }

        if !self.shell.toasts.is_empty() {
            subscriptions.push(
                iced::time::every(Duration::from_millis(TOAST_TICK_INTERVAL_MS))
                    .map(|_| Message::Shell(crate::app::shell::Message::ToastTick)),
            );
        }

        if self.shell.tooltip_visible_since.is_some() {
            subscriptions.push(
                iced::time::every(Duration::from_millis(TOOLTIP_TICK_INTERVAL_MS))
                    .map(|_| Message::Shell(crate::app::shell::Message::TooltipTick)),
            );
        }

        if let Some(terminal) = &self.connections.postgres_terminal {
            subscriptions.push(terminal.subscription().map(|value| {
                Message::Connections(
                    crate::app::features::connections::Message::PostgresTerminalEvent(value),
                )
            }));
        }

        Subscription::batch(subscriptions)
    }
}
