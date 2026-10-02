use super::{Message, State, Status};
use crate::constants::{ICON_CLOSE_LINE, ICON_REFRESH_LINE};
use crate::ui::presentation::Presentation;
use crate::ui::styles::{compact_button_style, compact_primary_button_style, update_banner_style};
use iced::widget::{button, container, row, space, text};
use iced::{Alignment::Center, Element, Length::Fill};

impl State {
    pub(crate) fn banner(&self, appearance: Presentation<'_>) -> Option<Element<'_, Message>> {
        if self.banner_dismissed {
            return None;
        }

        let (headline, action): (String, Option<Element<'_, Message>>) = match &self.status {
            Status::Available(available) => {
                let headline = crate::i18n::tr_with(
                    "CryoDB {version} is available — you are on {current}.",
                    &[
                        ("{version}", &available.version.to_string()),
                        ("{current}", crate::update::current_version()),
                    ],
                );
                let action: Element<'_, Message> = if available.self_updatable() {
                    button(
                        text(crate::i18n::tr("Update available"))
                            .font(appearance.ui_font())
                            .size(appearance.button_text_size()),
                    )
                    .padding(appearance.button_padding())
                    .style(compact_primary_button_style)
                    .on_press(Message::Install)
                    .into()
                } else {
                    text(crate::i18n::tr(&crate::i18n::tr(
                        available.install.manual_hint(),
                    )))
                    .font(appearance.ui_font())
                    .size(appearance.label_text_size())
                    .into()
                };
                (headline, Some(action))
            }
            Status::Downloading(version) => (
                crate::i18n::tr_with(
                    "Downloading CryoDB {version}…",
                    &[("{version}", &version.to_string())],
                ),
                None,
            ),
            _ => return None,
        };

        let dismiss = button(
            text(ICON_CLOSE_LINE.to_string())
                .font(appearance.icon_font())
                .size(appearance.icon_text_size()),
        )
        .padding([4.0 * appearance.ui_scale(), 6.0 * appearance.ui_scale()])
        .style(compact_button_style)
        .on_press(Message::DismissBanner);

        let mut bar = row![
            text(ICON_REFRESH_LINE.to_string())
                .font(appearance.icon_font())
                .size(appearance.icon_text_size()),
            text(crate::i18n::tr(&headline))
                .font(appearance.ui_font())
                .size(appearance.label_text_size()),
            space::horizontal().width(Fill),
        ]
        .spacing(8.0 * appearance.ui_scale())
        .align_y(Center)
        .width(Fill);

        if let Some(action) = action {
            bar = bar.push(action);
        }
        bar = bar.push(dismiss);

        Some(
            container(bar)
                .padding([4.0 * appearance.ui_scale(), 10.0 * appearance.ui_scale()])
                .width(Fill)
                .style(update_banner_style)
                .into(),
        )
    }
}
