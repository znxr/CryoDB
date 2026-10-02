use super::types::AiModalTarget;
use super::types::{ChatBlockKind, ChatRole, ChatSessionOption};
use super::{Message, State};
use crate::ai::AiProvider;
use crate::constants::ICON_AI;
use crate::constants::{
    BRAND_CLAUDE_COLOR, ICON_AGENT, ICON_AGENT_FOLLOW, ICON_BRAND_CHATGPT, ICON_BRAND_CLAUDE,
    ICON_CHAT_CLEAR, ICON_CHECK_FILL, ICON_CLOSE_LINE, ICON_COPY, ICON_FILE_ADD_LINE,
    MODAL_SCROLLBAR_GUTTER,
};
use crate::model::settings::ChatMode;
use crate::ui::ids::chat_scroll_id;
use crate::ui::presentation::Presentation;
use crate::ui::styles::modal_backdrop_style;
use crate::ui::styles::{
    alert_style, borderless_query_editor_style, compact_button_style, compact_pick_list_menu_style,
    compact_pick_list_style, compact_primary_button_style, panel_border_style, panel_style,
};
use crate::ui::widgets::tooltip_area::tooltip_area;
use iced::mouse;
use iced::widget::{button, container, pick_list, row, scrollable, space, text, text_editor};
use iced::widget::{checkbox, mouse_area, stack};
use iced::{Center, Color, Element, Fill, Length, Theme, keyboard};
pub(crate) struct View<'a> {
    pub(crate) layout_mode: crate::ui::presentation::LayoutMode,
    pub(crate) state: &'a State,
    pub(crate) scope: String,
    pub(crate) presentation: Presentation<'a>,
    pub(crate) theme: Theme,
    pub(crate) spinner: char,
}

impl<'a> View<'a> {
    pub(crate) fn ai_query_modal(&self) -> Element<'a, Message> {
        let state = self.state;
        let backdrop_dim = self.presentation.settings.modal_backdrop_dim;
        let layout = self.layout_mode;
        let modal_width = self.presentation.scale_f32(540.0);
        let modal_height = self.presentation.scale_f32(360.0);
        let modal_padding = self.presentation.scale_u16(12);
        let (title, description) = match state.ai_modal_target {
            AiModalTarget::QueryEditor => {
                ("Generate SQL", "Describe the query you want to generate.")
            }
            AiModalTarget::PostgresRoleSql => (
                "Generate Role SQL",
                "Describe how to build SQL for this Login / Group Role. The result is applied to the SQL tab editor.",
            ),
        };

        let header = row![
            self.presentation.title_text(title),
            space::horizontal(),
            button(
                self.presentation
                    .action_label("Close", ICON_CLOSE_LINE, layout)
            )
            .padding(self.presentation.button_padding())
            .style(compact_button_style)
            .on_press(Message::CloseAiModal),
        ]
        .spacing(self.presentation.scale_u16(6))
        .align_y(Center);

        let prompt_editor = text_editor(&state.ai_prompt_content)
            .height(Fill)
            .on_action(Message::AiPromptAction)
            .wrapping(text::Wrapping::WordOrGlyph)
            .font(self.presentation.ui_font())
            .size(self.presentation.input_text_size());

        let prompt_editor = container(prompt_editor)
            .padding(self.presentation.scale_u16(6))
            .width(Fill)
            .height(Fill)
            .style(panel_border_style);

        let mut body = iced::widget::column![
            header,
            self.presentation.label_text(description),
            prompt_editor
        ]
        .spacing(self.presentation.scale_u16(10))
        .width(Fill)
        .height(Fill);

        if !state.ai_modal_sql_context.trim().is_empty() {
            body = body.push(
                checkbox(state.ai_modal_use_current_sql_context)
                    .label(crate::i18n::tr("Use current SQL as context"))
                    .on_toggle(Message::AiUseCurrentSqlContextToggled)
                    .text_size(self.presentation.label_text_size())
                    .font(self.presentation.ui_font()),
            );
        }

        if let Some(error) = &state.ai_prompt_error {
            let alert = container(
                iced::widget::column![
                    self.presentation.label_text("AI error"),
                    self.presentation.input_text(error),
                ]
                .spacing(self.presentation.scale_u16(4)),
            )
            .padding(self.presentation.scale_u16(6))
            .width(Fill)
            .style(alert_style);
            body = body.push(alert);
        }

        let generate_label = if state.is_generating_ai {
            "Generating..."
        } else {
            "Generate"
        };
        let generate_icon = if state.is_generating_ai {
            self.spinner
        } else {
            ICON_AI
        };

        let generate_button = {
            let button = button(self.presentation.action_label(
                generate_label,
                generate_icon,
                layout,
            ))
            .padding(self.presentation.button_padding())
            .style(compact_primary_button_style);

            let prompt_empty = state.ai_prompt_content.text().trim().is_empty();
            if state.is_generating_ai || prompt_empty {
                button
            } else {
                button.on_press(Message::GenerateAiQuery)
            }
        };

        let content = body.push(
            row![space::horizontal(), generate_button]
                .align_y(Center)
                .spacing(self.presentation.scale_u16(6)),
        );

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
    pub(crate) fn chat_panel(&self) -> Element<'a, Message> {
        let state = self.state;
        let sessions = state.chat_session_options(&self.scope);
        let active_session = state
            .chat_active_session(&self.scope)
            .map(|session| ChatSessionOption {
                id: session.id,
                label: session.title.clone(),
            })
            .or_else(|| sessions.first().cloned());

        let mut session_row = row![
            pick_list(sessions, active_session, |value| {
                Message::ChatSessionSelected(value)
            })
            .padding(self.presentation.input_padding())
            .text_size(self.presentation.input_text_size())
            .font(self.presentation.ui_font())
            .handle(self.presentation.pick_list_handle())
            .style(compact_pick_list_style)
            .menu_style(compact_pick_list_menu_style)
            .width(Fill),
            self.with_tooltip(
                button(self.presentation.icon_text(ICON_FILE_ADD_LINE))
                    .padding(self.presentation.button_padding_icon())
                    .style(compact_button_style)
                    .on_press(Message::ChatNewSession),
                "New chat",
            ),
        ]
        .spacing(self.presentation.scale_u16(6))
        .align_y(Center);

        if !state.chat_messages(&self.scope).is_empty() {
            session_row = session_row.push(
                self.with_tooltip(
                    button(self.presentation.icon_text(ICON_CHAT_CLEAR))
                        .padding(self.presentation.button_padding_icon())
                        .style(compact_button_style)
                        .on_press(Message::ChatClear),
                    "Clear this chat and start over",
                ),
            );
        }

        session_row = session_row.push(
            button(self.presentation.icon_text(ICON_CLOSE_LINE))
                .padding(self.presentation.button_padding_icon())
                .style(compact_button_style)
                .on_press(Message::CloseChatSidebar),
        );

        let mut thread = iced::widget::column![].spacing(self.presentation.scale_u16(10));

        if state.chat_messages(&self.scope).is_empty() {
            thread =
                thread.push(self.presentation.label_text(
                    "Ask about the connected database, or describe the query you want.",
                ));
        }

        for (index, message) in state.chat_messages(&self.scope).iter().enumerate() {
            if let Some(summary) = message.summary.as_ref() {
                thread = thread.push(
                    row![
                        self.presentation
                            .icon_text(ICON_CHECK_FILL)
                            .size(self.presentation.label_text_size()),
                        self.presentation.label_text(summary.clone()),
                    ]
                    .spacing(self.presentation.scale_u16(6))
                    .align_y(Center),
                );
                continue;
            }

            let (label, style): (&str, fn(&iced::Theme) -> container::Style) = match message.role {
                ChatRole::Assistant => ("AI", panel_style),
                _ => ("You", panel_border_style),
            };

            let mut bubble = iced::widget::column![self.presentation.label_text(label)]
                .spacing(self.presentation.scale_u16(6))
                .width(Fill);

            for (block_index, block) in message.blocks.iter().enumerate() {
                if let Some(badge) = block.badge.as_ref() {
                    let danger = matches!(
                        badge.to_ascii_lowercase().as_str(),
                        "warning" | "caution" | "danger" | "careful"
                    );
                    bubble = bubble.push(
                        container(self.presentation.label_text(badge.to_ascii_uppercase()))
                            .padding([
                                self.presentation.scale_u16(2),
                                self.presentation.scale_u16(6),
                            ])
                            .style(if danger {
                                alert_style
                            } else {
                                panel_border_style
                            }),
                    );
                }

                match block.kind {
                    ChatBlockKind::Prose => {
                        bubble = bubble.push(
                            text_editor(&block.editor)
                                .on_action(move |action| Message::ChatMessageAction {
                                    index,
                                    block: block_index,
                                    action,
                                })
                                .wrapping(text::Wrapping::WordOrGlyph)
                                .size(self.presentation.input_text_size())
                                .font(self.presentation.ui_font())
                                .padding(0)
                                .style(borderless_query_editor_style),
                        );
                    }
                    ChatBlockKind::Code => {
                        let Some(code) = block.code.as_ref() else {
                            continue;
                        };
                        let lines = block.text.lines().count().max(1) as f32;
                        bubble = bubble.push(
                            container(code.view().map(move |action| Message::ChatCodeAction {
                                index,
                                block: block_index,
                                action,
                            }))
                            .height(Length::Fixed(
                                lines * code.line_height() + self.presentation.scale_f32(26.0),
                            ))
                            .padding(self.presentation.scale_u16(6))
                            .width(Fill)
                            .style(panel_border_style),
                        );

                        if message.role == ChatRole::Assistant {
                            let sql = block.text.clone();
                            bubble = bubble.push(
                                row![
                                    button(self.presentation.button_text("Insert"))
                                        .padding(self.presentation.button_padding_tight())
                                        .style(compact_button_style)
                                        .on_press(Message::ChatInsertSql(sql.clone())),
                                    self.with_tooltip(
                                        button(self.presentation.icon_text(ICON_COPY))
                                            .padding(self.presentation.button_padding_icon())
                                            .style(compact_button_style)
                                            .on_press(Message::ChatCopySql(sql)),
                                        "Copy SQL",
                                    ),
                                ]
                                .spacing(self.presentation.scale_u16(4))
                                .align_y(Center),
                            );
                        }
                    }
                }
            }

            thread = thread.push(
                container(bubble)
                    .padding(self.presentation.scale_u16(8))
                    .width(Fill)
                    .style(style),
            );
        }

        let streaming = state
            .chat_streaming_reply
            .as_deref()
            .filter(|text| !text.trim().is_empty());

        if let Some(text) = state
            .chat_streaming_reasoning
            .as_deref()
            .filter(|text| !text.trim().is_empty())
        {
            let mut font = self.presentation.ui_font();
            font.style = iced::font::Style::Italic;
            let mut muted = self.theme.palette().text;
            muted.a = 0.55;

            thread = thread.push(
                iced::widget::column![
                    self.presentation.label_text("Reasoning").color(muted),
                    iced::widget::text(text.to_string())
                        .size(self.presentation.label_text_size())
                        .font(font)
                        .color(muted)
                        .wrapping(text::Wrapping::WordOrGlyph),
                ]
                .spacing(self.presentation.scale_u16(2))
                .width(Fill),
            );
        }

        if let Some(text) = streaming {
            thread = thread.push(
                container(
                    iced::widget::column![
                        self.presentation.label_text("AI"),
                        self.presentation.input_text(text)
                    ]
                    .spacing(self.presentation.scale_u16(6))
                    .width(Fill),
                )
                .padding(self.presentation.scale_u16(8))
                .width(Fill)
                .style(panel_style),
            );
        }

        if (state.chat_sending && streaming.is_none()) || state.chat_auto_run_pending {
            let status = if state.chat_auto_run_pending {
                String::from("Running the query...")
            } else {
                state
                    .chat_activity
                    .clone()
                    .unwrap_or_else(|| String::from("Thinking..."))
            };
            thread = thread.push(
                row![
                    self.presentation
                        .icon_text(if state.chat_diagram_steps > 0 {
                            ICON_AGENT
                        } else {
                            self.spinner
                        }),
                    self.presentation.label_text(status),
                ]
                .spacing(self.presentation.scale_u16(6))
                .align_y(Center),
            );
        }

        if let Some(error) = &state.chat_error {
            thread = thread.push(
                container(
                    iced::widget::column![
                        self.presentation.label_text("AI error"),
                        self.presentation.input_text(error)
                    ]
                    .spacing(self.presentation.scale_u16(4)),
                )
                .padding(self.presentation.scale_u16(6))
                .width(Fill)
                .style(alert_style),
            );
        }

        let body = scrollable(thread)
            .id(chat_scroll_id())
            .height(Fill)
            .width(Fill)
            .direction(iced::widget::scrollable::Direction::Vertical(
                iced::widget::scrollable::Scrollbar::new()
                    .spacing(self.presentation.scale_f32(MODAL_SCROLLBAR_GUTTER)),
            ));

        let input = container(
            text_editor(&state.chat_input)
                .height(Length::Fixed(self.presentation.scale_f32(96.0)))
                .on_action(Message::ChatInputAction)
                .wrapping(text::Wrapping::WordOrGlyph)
                .font(self.presentation.ui_font())
                .size(self.presentation.input_text_size())
                .key_binding(|press| match press.key.as_ref() {
                    keyboard::Key::Named(keyboard::key::Named::Enter)
                        if !press.modifiers.shift() =>
                    {
                        Some(text_editor::Binding::Custom(Message::ChatSend))
                    }
                    _ => text_editor::Binding::from_key_press(press),
                }),
        )
        .padding(self.presentation.scale_u16(6))
        .width(Fill)
        .style(panel_border_style);

        let send_button: Element<'_, Message> = if state.chat_sending {
            button(self.presentation.button_text("Cancel"))
                .padding(self.presentation.button_padding_tight())
                .style(compact_button_style)
                .on_press(Message::ChatCancel)
                .into()
        } else {
            let button = button(self.presentation.button_text("Send"))
                .padding(self.presentation.button_padding_tight())
                .style(compact_primary_button_style);
            if state.chat_input.text().trim().is_empty() {
                button.into()
            } else {
                button.on_press(Message::ChatSend).into()
            }
        };

        let mode_picker = pick_list(
            ChatMode::ALL,
            Some(self.presentation.settings.chat_mode),
            Message::ChatModeSelected,
        )
        .padding(self.presentation.input_padding())
        .text_size(self.presentation.input_text_size())
        .font(self.presentation.ui_font())
        .handle(self.presentation.pick_list_handle())
        .style(compact_pick_list_style)
        .menu_style(compact_pick_list_menu_style);

        let mut footer = row![mode_picker, space::horizontal()]
            .spacing(self.presentation.scale_u16(6))
            .align_y(Center);

        let agentic = state.chat_diagram_steps > 0;
        let follow = button(self.presentation.icon_text(ICON_AGENT_FOLLOW))
            .padding(self.presentation.button_padding_icon())
            .style(move |theme, status| {
                if agentic && state.chat_follows_diagram_agent {
                    compact_primary_button_style(theme, status)
                } else {
                    compact_button_style(theme, status)
                }
            });
        footer = footer.push(self.with_tooltip(
            if agentic {
                follow.on_press(Message::ChatDiagramFollowToggled)
            } else {
                follow
            },
            if agentic {
                if state.chat_follows_diagram_agent {
                    crate::i18n::tr("Following the AI cursor")
                } else {
                    crate::i18n::tr("Follow the AI cursor")
                }
            } else {
                crate::i18n::tr("Follow is available while the diagram agent is working")
            },
        ));

        if let Some((icon, color)) = Self::chat_provider_brand(self.presentation.settings) {
            let mut brand = self.presentation.icon_text(icon);
            if let Some(color) = color {
                brand = brand.color(color);
            }
            footer = footer.push(brand);
        }

        let footer = footer.push(send_button);

        container(
            iced::widget::column![session_row, body, input, footer]
                .spacing(self.presentation.scale_u16(8)),
        )
        .padding(self.presentation.scale_u16(10))
        .width(Fill)
        .height(Fill)
        .style(panel_style)
        .into()
    }
    pub(crate) fn chat_provider_brand(
        settings: &crate::model::settings::Settings,
    ) -> Option<(char, Option<Color>)> {
        let haystack = if settings.ai_provider == AiProvider::LocalCli {
            settings
                .ai_cli_command
                .split_whitespace()
                .next()
                .unwrap_or_default()
                .rsplit(['/', '\\'])
                .next()
                .unwrap_or_default()
                .to_ascii_lowercase()
        } else {
            format!(
                "{} {}",
                settings.ai_endpoint.to_ascii_lowercase(),
                settings.ai_model.to_ascii_lowercase()
            )
        };

        if haystack.contains("claude") || haystack.contains("anthropic") {
            return Some((ICON_BRAND_CLAUDE, Some(BRAND_CLAUDE_COLOR)));
        }
        if haystack.contains("codex")
            || haystack.contains("openai")
            || haystack.contains("chatgpt")
            || haystack.contains("gpt-")
        {
            return Some((ICON_BRAND_CHATGPT, None));
        }

        None
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
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn chat_provider_brand_is_detected_from_cli_and_endpoint() {
        let mut settings = crate::model::settings::Settings {
            ai_provider: crate::ai::AiProvider::LocalCli,
            ai_cli_command: String::from("/usr/local/bin/claude -p"),
            ..Default::default()
        };
        assert_eq!(
            View::chat_provider_brand(&settings).map(|brand| brand.0),
            Some(ICON_BRAND_CLAUDE)
        );

        settings.ai_cli_command = String::from("codex exec");
        assert_eq!(
            View::chat_provider_brand(&settings),
            Some((ICON_BRAND_CHATGPT, None))
        );

        settings.ai_cli_command = String::from("opencode run");
        assert_eq!(View::chat_provider_brand(&settings), None);

        settings.ai_provider = crate::ai::AiProvider::OpenAI;
        settings.ai_cli_command.clear();
        settings.ai_endpoint = String::from("https://api.openai.com/v1");
        settings.ai_model = String::from("gpt-4o-mini");
        assert_eq!(
            View::chat_provider_brand(&settings),
            Some((ICON_BRAND_CHATGPT, None))
        );

        settings.ai_endpoint = String::from("http://localhost:11434");
        settings.ai_model = String::from("llama3");
        assert_eq!(View::chat_provider_brand(&settings), None);
    }
}
