use iced::widget::canvas;
use iced::{Point, Rectangle, Renderer, Size, Theme, mouse};

#[derive(Debug, Clone, Copy)]
pub(crate) struct TabPlaceholder {
    pub(crate) stroke_width: f32,
    pub(crate) segments: [f32; 2],
}

impl<Message> canvas::Program<Message> for TabPlaceholder {
    type State = ();

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &Renderer,
        theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let mut frame = canvas::Frame::new(renderer, bounds.size());
        let palette = theme.extended_palette();
        let accent = palette.primary.base.color;

        let mut fill = accent;
        fill.a = 0.14;
        frame.fill(&canvas::Path::rectangle(Point::ORIGIN, bounds.size()), fill);
        frame.fill(
            &canvas::Path::rectangle(
                Point::ORIGIN,
                Size::new(self.stroke_width * 2.0, bounds.height),
            ),
            accent,
        );

        let rect = canvas::Path::rectangle(Point::ORIGIN, bounds.size());
        let stroke = canvas::Stroke {
            style: canvas::Style::Solid(accent),
            width: self.stroke_width,
            line_cap: canvas::LineCap::Round,
            line_join: canvas::LineJoin::Round,
            line_dash: canvas::LineDash {
                segments: &self.segments,
                offset: 0,
            },
        };
        frame.stroke(&rect, stroke);
        vec![frame.into_geometry()]
    }
}
