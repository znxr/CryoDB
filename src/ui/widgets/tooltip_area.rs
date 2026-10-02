use iced::widget::mouse_area;
use iced::{Element, mouse};

pub(crate) fn tooltip_area<'a, M: Clone + 'a>(
    element: impl Into<Element<'a, M>>,
    text: impl Into<String>,
    on_enter: impl FnOnce(String) -> M,
    on_exit: M,
) -> Element<'a, M> {
    mouse_area(element.into())
        .on_enter(on_enter(crate::i18n::tr(&text.into())))
        .on_exit(on_exit)
        .interaction(mouse::Interaction::Pointer)
        .into()
}
