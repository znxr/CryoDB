use iced::widget::canvas;
use iced::{Event, Point, Rectangle, Renderer, Theme, mouse};
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

pub(crate) struct PressProbe<Message>(pub(crate) fn(Point) -> Message);

impl<Message> canvas::Program<Message> for PressProbe<Message> {
    type State = ();

    fn update(
        &self,
        _state: &mut Self::State,
        event: &Event,
        _bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<canvas::Action<Message>> {
        match event {
            Event::Mouse(mouse::Event::ButtonPressed(_)) => cursor
                .position()
                .map(|position| canvas::Action::publish((self.0)(position))),
            _ => None,
        }
    }

    fn draw(
        &self,
        _state: &Self::State,
        _renderer: &Renderer,
        _theme: &Theme,
        _bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        Vec::new()
    }
}
