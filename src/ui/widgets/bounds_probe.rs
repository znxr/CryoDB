use iced::widget::canvas;
use iced::{Rectangle, Renderer, Theme, mouse};
use std::cell::Cell;

pub(crate) struct BoundsProbe<'a> {
    pub(crate) target: &'a Cell<Rectangle>,
}

impl<Message> canvas::Program<Message> for BoundsProbe<'_> {
    type State = ();

    fn draw(
        &self,
        _state: &Self::State,
        _renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        self.target.set(bounds);
        Vec::new()
    }
}
