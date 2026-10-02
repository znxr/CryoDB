use crate::i18n::Language;
use crate::model::settings::{AppearanceColor, FontChoice, UiDensity};
use crate::ui::theme::AppearanceMode;
use crate::ui::theme::ThemeVariant;

mod view;
pub(crate) use view::View;

pub(crate) const LAST_STEP: usize = 3;

#[derive(Debug, Clone)]
pub(crate) enum Message {
    LanguageSelected(Language),
    ThemeVariantSelected(ThemeVariant),
    UiDensitySelected(UiDensity),
    AccentColorSelected(AppearanceColor),
    AppearanceModeSelected(AppearanceMode),
    FontSelected(FontChoice),
    AiEnabled(bool),
    Back,
    Next,
    Finished,
}

pub(crate) enum Output {
    LanguageSelected(Language),
    ThemeVariantSelected(ThemeVariant),
    UiDensitySelected(UiDensity),
    AccentColorSelected(AppearanceColor),
    AppearanceModeSelected(AppearanceMode),
    FontSelected(FontChoice),
    AiEnabled(bool),
    Finished,
}

pub(crate) struct State {
    pub(crate) step: Option<usize>,
}

impl State {
    pub(crate) fn update(&mut self, message: Message) -> Option<Output> {
        Some(match message {
            Message::Back => {
                self.step = self.step.map(|step| step.saturating_sub(1));
                return None;
            }
            Message::Next => {
                self.step = self.step.map(|step| (step + 1).min(LAST_STEP));
                return None;
            }
            Message::Finished => {
                self.step = None;
                Output::Finished
            }
            Message::LanguageSelected(value) => Output::LanguageSelected(value),
            Message::ThemeVariantSelected(value) => Output::ThemeVariantSelected(value),
            Message::UiDensitySelected(value) => Output::UiDensitySelected(value),
            Message::AccentColorSelected(value) => Output::AccentColorSelected(value),
            Message::AppearanceModeSelected(value) => Output::AppearanceModeSelected(value),
            Message::FontSelected(value) => Output::FontSelected(value),
            Message::AiEnabled(value) => Output::AiEnabled(value),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{LAST_STEP, Message, Output, State};

    #[test]
    fn navigation_stays_inside_setup_steps() {
        let mut state = State { step: Some(0) };
        assert!(state.update(Message::Back).is_none());
        assert_eq!(state.step, Some(0));
        for _ in 0..LAST_STEP + 2 {
            assert!(state.update(Message::Next).is_none());
        }
        assert_eq!(state.step, Some(LAST_STEP));
        assert!(matches!(
            state.update(Message::Finished),
            Some(Output::Finished)
        ));
        assert_eq!(state.step, None);
        assert!(state.update(Message::Back).is_none());
        assert!(state.update(Message::Next).is_none());
        assert_eq!(state.step, None);
    }

    #[test]
    fn preferences_leave_navigation_unchanged() {
        let mut state = State { step: Some(2) };
        assert!(matches!(
            state.update(Message::AiEnabled(false)),
            Some(Output::AiEnabled(false))
        ));
        assert_eq!(state.step, Some(2));
    }
}
