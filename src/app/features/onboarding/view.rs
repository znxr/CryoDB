use super::{
    AppearanceMode as OnboardingAppearanceMode, LAST_STEP as ONBOARDING_LAST_STEP, Message,
};
use crate::i18n::Language;
use crate::model::settings::{AppearanceColor, FontChoice, Settings, SystemThemeMode, UiDensity};
use crate::ui::presentation::Presentation;
use crate::ui::styles::{
    compact_button_style, compact_pick_list_menu_style, compact_pick_list_style,
    compact_primary_button_style, panel_border_style,
};
use crate::ui::theme::{ThemeVariant, ui_radius};
use iced::widget::{button, container, pick_list, row, space};
use iced::{Center, Element, Fill, Length, Theme};

pub(crate) struct View<'a> {
    pub(crate) settings: &'a Settings,
    pub(crate) font_choices: &'a [FontChoice],
    pub(crate) theme: Theme,
    pub(crate) presentation: Presentation<'a>,
}

impl View<'_> {
    fn onboarding_mode(&self) -> OnboardingAppearanceMode {
        if self.settings.system_theme_mode == SystemThemeMode::System {
            OnboardingAppearanceMode::System
        } else if crate::ui::theme::is_dark_theme(&self.theme) {
            OnboardingAppearanceMode::Dark
        } else {
            OnboardingAppearanceMode::Light
        }
    }

    fn onboarding_option(
        &self,
        label: String,
        selected: bool,
        message: Message,
    ) -> Element<'static, Message> {
        button(
            container(self.presentation.button_text(label))
                .width(Fill)
                .align_x(Center),
        )
        .padding(self.presentation.input_padding())
        .width(Fill)
        .style(if selected {
            compact_primary_button_style
        } else {
            compact_button_style
        })
        .on_press(message)
        .into()
    }

    fn onboarding_field<'a>(
        &self,
        label: &str,
        control: Element<'a, Message>,
    ) -> Element<'a, Message> {
        iced::widget::column![self.presentation.muted_label_text(label), control]
            .spacing(self.presentation.scale_u16(4))
            .width(Fill)
            .into()
    }

    fn onboarding_choices(
        &self,
        label: &str,
        choices: Vec<(String, bool, Message)>,
    ) -> Element<'static, Message> {
        let mut options = row![].spacing(self.presentation.scale_u16(6)).width(Fill);
        for (text, selected, message) in choices {
            options = options.push(self.onboarding_option(text, selected, message));
        }
        self.onboarding_field(label, options.into())
    }

    fn onboarding_picker<'a, T, L>(
        &self,
        options: L,
        selected: T,
        on_select: impl Fn(T) -> Message + 'a,
    ) -> Element<'a, Message>
    where
        T: ToString + PartialEq + Clone + 'a,
        L: std::borrow::Borrow<[T]> + 'a,
    {
        pick_list(options, Some(selected), on_select)
            .padding(self.presentation.input_padding())
            .text_size(self.presentation.input_text_size())
            .font(self.presentation.ui_font())
            .handle(self.presentation.pick_list_handle())
            .style(compact_pick_list_style)
            .menu_style(compact_pick_list_menu_style)
            .width(Fill)
            .into()
    }

    fn onboarding_step_body(&self, step: usize) -> (String, String, Element<'static, Message>) {
        let mut body = iced::widget::column![]
            .spacing(self.presentation.scale_u16(12))
            .width(Fill);
        let key = match step {
            0 => {
                let mut options = iced::widget::column![]
                    .spacing(self.presentation.scale_u16(6))
                    .width(Fill);
                for language in Language::ALL {
                    options = options.push(self.onboarding_option(
                        language.to_string(),
                        self.settings.language == *language,
                        Message::LanguageSelected(*language),
                    ));
                }
                body = body.push(options);
                "language"
            }
            1 => {
                let mode = self.onboarding_mode();
                let accent = self.settings.accent_color;
                let swatch = container(space::horizontal())
                    .width(Length::Fixed(self.presentation.scale_f32(30.0)))
                    .height(Length::Fixed(self.presentation.scale_f32(30.0)))
                    .style(move |theme: &Theme| container::Style {
                        background: Some(
                            crate::ui::theme::appearance_color(
                                accent,
                                theme.extended_palette().primary.base.color,
                            )
                            .into(),
                        ),
                        border: iced::Border {
                            radius: ui_radius().into(),
                            ..iced::Border::default()
                        },
                        ..container::Style::default()
                    });

                body = body
                    .push(
                        self.onboarding_choices(
                            "Theme",
                            [
                                (ThemeVariant::Original, "Classic"),
                                (ThemeVariant::Modern, "Modern"),
                            ]
                            .into_iter()
                            .map(|(variant, label)| {
                                (
                                    label.to_string(),
                                    self.settings.theme_variant == variant,
                                    Message::ThemeVariantSelected(variant),
                                )
                            })
                            .collect(),
                        ),
                    )
                    .push(
                        self.onboarding_choices(
                            "Mode",
                            [
                                (OnboardingAppearanceMode::Dark, "Dark"),
                                (OnboardingAppearanceMode::Light, "Light"),
                                (OnboardingAppearanceMode::System, "System"),
                            ]
                            .into_iter()
                            .map(|(value, label)| {
                                (
                                    label.to_string(),
                                    mode == value,
                                    Message::AppearanceModeSelected(value),
                                )
                            })
                            .collect(),
                        ),
                    )
                    .push(
                        self.onboarding_choices(
                            "Layout",
                            [
                                (UiDensity::Compact, "Compact"),
                                (UiDensity::Normal, "Normal"),
                            ]
                            .into_iter()
                            .map(|(density, label)| {
                                (
                                    label.to_string(),
                                    self.settings.ui_density == density,
                                    Message::UiDensitySelected(density),
                                )
                            })
                            .collect(),
                        ),
                    )
                    .push(
                        self.onboarding_field(
                            "Accent color",
                            row![
                                self.onboarding_picker(
                                    AppearanceColor::ALL,
                                    accent,
                                    Message::AccentColorSelected,
                                ),
                                swatch,
                            ]
                            .spacing(self.presentation.scale_u16(8))
                            .align_y(Center)
                            .into(),
                        ),
                    )
                    .push(self.onboarding_field(
                        "Font",
                        self.onboarding_picker(
                            self.font_choices.to_vec(),
                            self.settings.font.clone(),
                            Message::FontSelected,
                        ),
                    ));
                "appearance"
            }
            2 => {
                body = body.push(
                    self.onboarding_choices(
                        "AI features",
                        [(true, "Enable AI"), (false, "Disable AI")]
                            .into_iter()
                            .map(|(enabled, label)| {
                                (
                                    label.to_string(),
                                    self.settings.ai_enabled == enabled,
                                    Message::AiEnabled(enabled),
                                )
                            })
                            .collect(),
                    ),
                );
                "ai"
            }
            _ => "welcome",
        };

        let (title, subtitle) = match key {
            "language" => ("Language", "Choose the language CryoDB should use."),
            "appearance" => (
                "Appearance",
                "Classic keeps the original layout. Modern is flatter and denser.",
            ),
            "ai" => (
                "AI features",
                "Chat, inline autocomplete and query fixes. Disabling hides every AI element.",
            ),
            _ => (
                "You are all set",
                "Everything here can be changed later in Settings.",
            ),
        };

        (title.to_string(), subtitle.to_string(), body.into())
    }

    pub(crate) fn onboarding_view(&self, step: usize) -> Element<'static, Message> {
        let (title, subtitle, body) = self.onboarding_step_body(step);

        let mut footer = row![space::horizontal()]
            .spacing(self.presentation.scale_u16(6))
            .align_y(Center)
            .width(Fill);
        if step > 0 {
            footer = footer.push(
                button(self.presentation.button_text("Back"))
                    .padding(self.presentation.input_padding())
                    .style(compact_button_style)
                    .on_press(Message::Back),
            );
        }
        footer = footer.push(
            button(
                self.presentation
                    .button_text(if step == ONBOARDING_LAST_STEP {
                        "Start using CryoDB"
                    } else {
                        "Next"
                    }),
            )
            .padding(self.presentation.input_padding())
            .style(compact_primary_button_style)
            .on_press(if step == ONBOARDING_LAST_STEP {
                Message::Finished
            } else {
                Message::Next
            }),
        );

        let panel = container(
            iced::widget::column![
                self.presentation.title_text("Welcome to CryoDB"),
                self.presentation.heading_text(title),
                self.presentation.muted_label_text(subtitle),
                body,
                space::vertical().height(Length::Fixed(self.presentation.scale_f32(4.0))),
                row![
                    self.presentation.muted_label_text(format!(
                        "{}/{}",
                        step + 1,
                        ONBOARDING_LAST_STEP + 1
                    )),
                    footer,
                ]
                .spacing(self.presentation.scale_u16(8))
                .align_y(Center),
            ]
            .spacing(self.presentation.scale_u16(10))
            .width(Fill),
        )
        .padding(self.presentation.scale_u16(20))
        .width(Length::Fixed(self.presentation.scale_f32(460.0)))
        .style(panel_border_style);

        container(panel)
            .width(Fill)
            .height(Fill)
            .align_x(Center)
            .align_y(Center)
            .into()
    }
}
