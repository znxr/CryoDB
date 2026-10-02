use super::Message;
use super::model::{
    DIAGRAM_BLOCK_ZOOM, DIAGRAM_GRID_STEP, DIAGRAM_HEADER_HEIGHT, DIAGRAM_MAX_ZOOM,
    DIAGRAM_MIN_ZOOM, DIAGRAM_NOTE_LINE, DIAGRAM_ROW_HEIGHT, DiagramAgent, DiagramCommand,
    DiagramState, DiagramTarget,
};
use iced::advanced::text::Alignment as TextAlignment;
use iced::widget::canvas;
use iced::{
    Color, Event, Font, Pixels, Point, Rectangle, Renderer, Size, Theme, Vector, alignment,
    keyboard, mouse, window,
};

const MINIMAP_WIDTH: f32 = 200.0;
const MINIMAP_HEIGHT: f32 = 140.0;
const MINIMAP_MARGIN: f32 = 14.0;
const DETAIL_ZOOM: f32 = 0.4;
const LABEL_CROWD_X: f32 = 90.0;
const LABEL_CROWD_Y: f32 = 26.0;

pub(crate) struct ErDiagram<'a> {
    pub(crate) state: &'a DiagramState,
    pub(crate) font: Font,
    pub(crate) icon_font: Font,
    pub(crate) scale: f32,
}

pub(crate) enum Drag {
    Node { index: usize, grab: Vector },
    Group { index: usize, grab: Vector },
    GroupResize { index: usize },
    Note { index: usize, grab: Vector },
    NoteResize { index: usize },
    Pan { origin: Vector },
    Camera,
    Link { from: usize, from_column: usize },
}

#[derive(Default)]
pub(crate) struct Interaction {
    drag: Option<Drag>,
    modifiers: keyboard::Modifiers,
}

struct Minimap {
    area: Rectangle,
    world: Rectangle,
    scale: f32,
    origin: Point,
}

impl Minimap {
    fn to_screen(&self, point: Point) -> Point {
        Point::new(
            self.origin.x + (point.x - self.world.x) * self.scale,
            self.origin.y + (point.y - self.world.y) * self.scale,
        )
    }

    fn to_world(&self, point: Point) -> Point {
        Point::new(
            self.world.x + (point.x - self.origin.x) / self.scale,
            self.world.y + (point.y - self.origin.y) / self.scale,
        )
    }
}

fn with_alpha(color: Color, alpha: f32) -> Color {
    Color { a: alpha, ..color }
}

fn edge_bounds(start: Point, end: Point) -> Rectangle {
    let left = start.x.min(end.x);
    let top = start.y.min(end.y);
    Rectangle::new(
        Point::new(left - 24.0, top - 24.0),
        Size::new(
            (start.x - end.x).abs() + 48.0,
            (start.y - end.y).abs() + 48.0,
        ),
    )
}

fn edge_anchor(state: &DiagramState, edge: usize) -> Option<(Point, Point, bool)> {
    let edge = state.diagram.edges.get(edge)?;
    let source = state.diagram.tables.get(edge.from)?;
    let target = state.diagram.tables.get(edge.to)?;
    if edge.from == edge.to {
        let size = source.size();
        let y = source.column_center_y(edge.from_column);
        return Some((
            Point::new(source.position.x + size.width, y),
            Point::new(
                source.position.x + size.width,
                source.column_center_y(edge.to_column),
            ),
            true,
        ));
    }
    let source_size = source.size();
    let target_size = target.size();
    let forward =
        source.position.x + source_size.width / 2.0 <= target.position.x + target_size.width / 2.0;
    let start = Point::new(
        if forward {
            source.position.x + source_size.width
        } else {
            source.position.x
        },
        source
            .column_center_y(edge.from_column)
            .min(source.position.y + source_size.height - 4.0),
    );
    let end = Point::new(
        if forward {
            target.position.x
        } else {
            target.position.x + target_size.width
        },
        target
            .column_center_y(edge.to_column)
            .min(target.position.y + target_size.height - 4.0),
    );
    Some((start, end, forward))
}

fn bezier_point(start: Point, c1: Point, c2: Point, end: Point, t: f32) -> Point {
    let u = 1.0 - t;
    Point::new(
        u * u * u * start.x + 3.0 * u * u * t * c1.x + 3.0 * u * t * t * c2.x + t * t * t * end.x,
        u * u * u * start.y + 3.0 * u * u * t * c1.y + 3.0 * u * t * t * c2.y + t * t * t * end.y,
    )
}

fn bezier_derivative(start: Point, c1: Point, c2: Point, end: Point, t: f32) -> Vector {
    let u = 1.0 - t;
    Vector::new(
        3.0 * u * u * (c1.x - start.x) + 6.0 * u * t * (c2.x - c1.x) + 3.0 * t * t * (end.x - c2.x),
        3.0 * u * u * (c1.y - start.y) + 6.0 * u * t * (c2.y - c1.y) + 3.0 * t * t * (end.y - c2.y),
    )
}

fn edge_segment(
    start: Point,
    c1: Point,
    c2: Point,
    end: Point,
    from: f32,
    to: f32,
) -> canvas::Path {
    let first = bezier_point(start, c1, c2, end, from);
    let last = bezier_point(start, c1, c2, end, to);
    let scale = (to - from) / 3.0;
    let first_control = first + bezier_derivative(start, c1, c2, end, from) * scale;
    let last_control = last - bezier_derivative(start, c1, c2, end, to) * scale;
    canvas::Path::new(|builder| {
        builder.move_to(first);
        builder.bezier_curve_to(first_control, last_control, last);
    })
}

fn edge_controls(start: Point, end: Point, forward: bool) -> (Point, Point) {
    if (start.x - end.x).abs() < f32::EPSILON {
        let bulge = 46.0 + (start.y - end.y).abs() * 0.25;
        return (
            Point::new(start.x + bulge, start.y),
            Point::new(end.x + bulge, end.y),
        );
    }
    let reach = ((end.x - start.x).abs() * 0.5).clamp(40.0, 160.0);
    (
        Point::new(
            if forward {
                start.x + reach
            } else {
                start.x - reach
            },
            start.y,
        ),
        Point::new(
            if forward {
                end.x - reach
            } else {
                end.x + reach
            },
            end.y,
        ),
    )
}

fn edge_midpoint(start: Point, c1: Point, c2: Point, end: Point) -> Point {
    Point::new(
        (start.x + 3.0 * c1.x + 3.0 * c2.x + end.x) / 8.0,
        (start.y + 3.0 * c1.y + 3.0 * c2.y + end.y) / 8.0,
    )
}

impl ErDiagram<'_> {
    fn to_world(&self, point: Point) -> Point {
        Point::new(
            (point.x - self.state.offset.x) / self.state.zoom,
            (point.y - self.state.offset.y) / self.state.zoom,
        )
    }

    fn visible_world(&self, bounds: Rectangle) -> Rectangle {
        Rectangle::new(
            self.to_world(Point::ORIGIN),
            Size::new(
                bounds.width / self.state.zoom,
                bounds.height / self.state.zoom,
            ),
        )
    }

    fn minimap(&self, bounds: Rectangle) -> Option<Minimap> {
        if self.state.diagram.tables.len() < 2 {
            return None;
        }
        let content = self
            .state
            .bounds
            .or_else(|| self.state.diagram.content_bounds())?;
        let visible = self.visible_world(bounds);
        let min_x = content.x.min(visible.x);
        let min_y = content.y.min(visible.y);
        let max_x = (content.x + content.width).max(visible.x + visible.width);
        let max_y = (content.y + content.height).max(visible.y + visible.height);
        let world = Rectangle::new(
            Point::new(min_x, min_y),
            Size::new((max_x - min_x).max(1.0), (max_y - min_y).max(1.0)),
        );

        let width = MINIMAP_WIDTH * self.scale;
        let height = MINIMAP_HEIGHT * self.scale;
        let margin = MINIMAP_MARGIN * self.scale;
        if bounds.width < width * 1.8 || bounds.height < height * 1.8 {
            return None;
        }
        let area = Rectangle::new(
            Point::new(
                bounds.width - width - margin,
                bounds.height - height - margin,
            ),
            Size::new(width, height),
        );
        let inner = 8.0 * self.scale;
        let scale = ((area.width - inner * 2.0) / world.width)
            .min((area.height - inner * 2.0) / world.height);
        let origin = Point::new(
            area.x + (area.width - world.width * scale) / 2.0,
            area.y + (area.height - world.height * scale) / 2.0,
        );
        Some(Minimap {
            area,
            world,
            scale,
            origin,
        })
    }

    fn centered_view(&self, bounds: Rectangle, target: Point) -> Message {
        Message::DiagramViewChanged {
            offset: Vector::new(
                bounds.width / 2.0 - target.x * self.state.zoom,
                bounds.height / 2.0 - target.y * self.state.zoom,
            ),
            zoom: self.state.zoom,
        }
    }

    fn node_at(&self, world: Point) -> Option<usize> {
        (0..self.state.diagram.tables.len()).rev().find(|index| {
            !self.state.table_is_hidden(*index)
                && self
                    .state
                    .table_boxes
                    .get(*index)
                    .is_some_and(|bounds| bounds.contains(world))
        })
    }

    fn node_is_movable(&self, index: usize) -> bool {
        let Some(table) = self.state.diagram.tables.get(index) else {
            return false;
        };
        match self.state.diagram.group_of(&table.name) {
            Some(group) => !self.state.diagram.groups[group].contents_locked,
            None => true,
        }
    }

    fn note_at(&self, world: Point) -> Option<usize> {
        self.state
            .diagram
            .notes
            .iter()
            .rposition(|note| note.bounds().contains(world))
    }

    fn note_resize_at(&self, world: Point) -> Option<usize> {
        self.state
            .diagram
            .notes
            .iter()
            .rposition(|note| note.resize_handle().contains(world))
    }

    fn group_handle_at(&self, world: Point) -> Option<usize> {
        self.state
            .diagram
            .groups
            .iter()
            .rposition(|group| !group.locked && group.header().contains(world))
    }

    fn group_resize_at(&self, world: Point) -> Option<usize> {
        self.state.diagram.groups.iter().rposition(|group| {
            !group.locked && !group.collapsed && group.resize_handle().contains(world)
        })
    }

    fn group_at(&self, world: Point) -> Option<usize> {
        self.state
            .diagram
            .groups
            .iter()
            .rposition(|group| group.outline().contains(world))
    }

    fn collapse_handle_at(&self, world: Point) -> Option<usize> {
        self.state.diagram.tables.iter().rposition(|table| {
            let bounds = table.bounds();
            Rectangle::new(
                Point::new(bounds.x + bounds.width - 24.0, bounds.y),
                Size::new(24.0, DIAGRAM_HEADER_HEIGHT),
            )
            .contains(world)
        })
    }

    fn column_port_at(&self, world: Point) -> Option<(usize, usize)> {
        let index = self.node_at(world)?;
        let table = self.state.diagram.tables.get(index)?;
        let bounds = table.bounds();
        if world.x < bounds.x + bounds.width - 18.0 {
            return None;
        }
        let column = table.column_at(world)?;
        Some((index, column))
    }

    fn edge_at(&self, world: Point) -> Option<usize> {
        let tolerance = (10.0 / self.state.zoom).max(6.0);
        (0..self.state.diagram.edges.len()).find(|index| {
            if !self.state.edge_is_visible(*index) {
                return false;
            }
            let Some((start, end, forward)) = edge_anchor(self.state, *index) else {
                return false;
            };
            if !edge_bounds(start, end).contains(world) {
                return false;
            }
            let (c1, c2) = edge_controls(start, end, forward);
            (0..=14).any(|step| {
                let point = bezier_point(start, c1, c2, end, step as f32 / 14.0);
                (point.x - world.x).abs() < tolerance && (point.y - world.y).abs() < tolerance
            })
        })
    }

    fn target_at(&self, world: Point) -> Option<DiagramTarget> {
        if let Some(index) = self.node_at(world) {
            return Some(DiagramTarget::Table(index));
        }
        if let Some(index) = self.note_at(world) {
            return Some(DiagramTarget::Note(index));
        }
        self.group_at(world).map(DiagramTarget::Group)
    }

    fn command_view(&self, bounds: Rectangle, command: DiagramCommand) -> Option<Message> {
        if let DiagramCommand::Follow {
            point,
            zoom: wanted_zoom,
        } = command
        {
            let ease = 0.12;
            let zoom = (self.state.zoom
                + (wanted_zoom.clamp(DIAGRAM_MIN_ZOOM, DIAGRAM_MAX_ZOOM) - self.state.zoom) * ease)
                .clamp(DIAGRAM_MIN_ZOOM, DIAGRAM_MAX_ZOOM);
            let screen = Point::new(
                point.x * zoom + self.state.offset.x,
                point.y * zoom + self.state.offset.y,
            );
            let margin_x = bounds.width * 0.18;
            let margin_y = bounds.height * 0.18;
            let centred = screen.x > margin_x
                && screen.x < bounds.width - margin_x
                && screen.y > margin_y
                && screen.y < bounds.height - margin_y;
            if centred && (zoom - self.state.zoom).abs() < 0.001 {
                return None;
            }
            let wanted = Vector::new(
                bounds.width / 2.0 - point.x * zoom,
                bounds.height / 2.0 - point.y * zoom,
            );
            let offset = if centred {
                Vector::new(screen.x - point.x * zoom, screen.y - point.y * zoom)
            } else {
                self.state.offset + (wanted - self.state.offset) * ease
            };
            return Some(Message::DiagramViewChanged { offset, zoom });
        }
        let (target, zoom) = match command {
            DiagramCommand::Fit => {
                let world = self
                    .state
                    .bounds
                    .or_else(|| self.state.diagram.content_bounds())?;
                let zoom = ((bounds.width - 60.0) / world.width)
                    .min((bounds.height - 60.0) / world.height)
                    .clamp(DIAGRAM_MIN_ZOOM, 1.0);
                (world, zoom)
            }
            DiagramCommand::Zoom(factor) => {
                let zoom = (self.state.zoom * factor).clamp(DIAGRAM_MIN_ZOOM, DIAGRAM_MAX_ZOOM);
                (self.visible_world(bounds), zoom)
            }
            DiagramCommand::Focus(index) => {
                let table = self.state.diagram.tables.get(index)?;
                (table.bounds(), self.state.zoom.max(0.6))
            }
            DiagramCommand::Follow { .. } => return None,
        };
        let center = Point::new(
            target.x + target.width / 2.0,
            target.y + target.height / 2.0,
        );
        Some(Message::DiagramViewChanged {
            offset: Vector::new(
                bounds.width / 2.0 - center.x * zoom,
                bounds.height / 2.0 - center.y * zoom,
            ),
            zoom,
        })
    }

    fn draw_grid(
        &self,
        frame: &mut canvas::Frame,
        palette: &crate::ui::theme::Tokens,
        visible: Rectangle,
    ) {
        if !self.state.snap || self.state.zoom < 0.5 {
            return;
        }
        let color = with_alpha(palette.line, 0.35);
        let first_x = (visible.x / DIAGRAM_GRID_STEP).floor() * DIAGRAM_GRID_STEP;
        let first_y = (visible.y / DIAGRAM_GRID_STEP).floor() * DIAGRAM_GRID_STEP;
        let mut x = first_x;
        while x < visible.x + visible.width {
            frame.stroke(
                &canvas::Path::line(
                    Point::new(x, visible.y),
                    Point::new(x, visible.y + visible.height),
                ),
                canvas::Stroke::default().with_color(color).with_width(0.5),
            );
            x += DIAGRAM_GRID_STEP;
        }
        let mut y = first_y;
        while y < visible.y + visible.height {
            frame.stroke(
                &canvas::Path::line(
                    Point::new(visible.x, y),
                    Point::new(visible.x + visible.width, y),
                ),
                canvas::Stroke::default().with_color(color).with_width(0.5),
            );
            y += DIAGRAM_GRID_STEP;
        }
    }

    fn draw_groups(&self, frame: &mut canvas::Frame) {
        for group in &self.state.diagram.groups {
            let color = group_color(group.color);
            let outline = group.outline();
            let path =
                canvas::Path::rounded_rectangle(outline.position(), outline.size(), 10.0.into());
            frame.fill(&path, with_alpha(color, 0.06));
            frame.stroke(
                &path,
                canvas::Stroke::default()
                    .with_color(with_alpha(color, if group.locked { 0.8 } else { 0.45 }))
                    .with_width(if group.locked { 2.0 } else { 1.5 }),
            );

            let header = group.header();
            frame.fill(
                &canvas::Path::rounded_rectangle(header.position(), header.size(), 10.0.into()),
                with_alpha(color, 0.16),
            );

            let mut cursor = header.x + 12.0;
            let centre = header.y + header.height / 2.0;
            let badge = |icon: char, frame: &mut canvas::Frame, cursor: &mut f32| {
                frame.fill_text(canvas::Text {
                    content: icon.to_string(),
                    position: Point::new(*cursor, centre),
                    color,
                    size: Pixels(12.0),
                    font: self.icon_font,
                    align_y: alignment::Vertical::Center,
                    ..canvas::Text::default()
                });
                *cursor += 16.0;
            };
            if group.collapsed {
                badge(crate::ICON_ARROW_RIGHT_S_LINE, frame, &mut cursor);
            }
            if group.locked {
                badge(crate::ICON_DIAGRAM_LOCK, frame, &mut cursor);
            } else if group.contents_locked {
                badge(crate::ICON_DIAGRAM_LOCK_CONTENTS, frame, &mut cursor);
            }
            frame.fill_text(canvas::Text {
                content: format!("{}  ({})", group.name, group.members.len()),
                position: Point::new(cursor, centre),
                color,
                size: Pixels(12.0),
                font: self.font,
                align_y: alignment::Vertical::Center,
                ..canvas::Text::default()
            });

            if group.collapsed || group.locked {
                continue;
            }
            let handle = group.resize_handle();
            for step in 0..3 {
                let offset = 4.0 + step as f32 * 4.0;
                frame.stroke(
                    &canvas::Path::line(
                        Point::new(handle.x + handle.width - offset, handle.y + handle.height),
                        Point::new(handle.x + handle.width, handle.y + handle.height - offset),
                    ),
                    canvas::Stroke::default()
                        .with_color(with_alpha(color, 0.7))
                        .with_width(1.2),
                );
            }
        }
    }

    fn draw_notes(&self, frame: &mut canvas::Frame, palette: &crate::ui::theme::Tokens) {
        let paper = Color::from_rgb(0.98, 0.86, 0.42);
        let ink = Color::from_rgb(0.16, 0.13, 0.05);
        for note in &self.state.diagram.notes {
            let bounds = note.bounds();
            if let Some(anchor) = note
                .anchor
                .as_ref()
                .and_then(|name| self.state.diagram.table_index(name))
                .filter(|index| !self.state.table_is_hidden(*index))
                .and_then(|index| self.state.table_boxes.get(index))
            {
                let from = Point::new(bounds.x, bounds.y + bounds.height / 2.0);
                let to = Point::new(
                    anchor.x + anchor.width / 2.0,
                    anchor.y + anchor.height / 2.0,
                );
                frame.stroke(
                    &canvas::Path::line(from, to),
                    canvas::Stroke::default()
                        .with_color(with_alpha(paper, 0.55))
                        .with_width(1.2),
                );
                frame.fill(&canvas::Path::circle(from, 3.0), paper);
            }
            let path =
                canvas::Path::rounded_rectangle(bounds.position(), bounds.size(), 4.0.into());
            frame.fill(
                &canvas::Path::rounded_rectangle(
                    Point::new(bounds.x + 2.0, bounds.y + 3.0),
                    bounds.size(),
                    4.0.into(),
                ),
                with_alpha(Color::BLACK, 0.25),
            );
            frame.fill(&path, paper);

            let header = note.header();
            frame.fill(
                &canvas::Path::rounded_rectangle(header.position(), header.size(), 4.0.into()),
                with_alpha(ink, 0.10),
            );
            frame.fill_text(canvas::Text {
                content: crate::ICON_DIAGRAM_NOTE.to_string(),
                position: Point::new(header.x + 9.0, header.y + header.height / 2.0),
                color: with_alpha(ink, 0.75),
                size: Pixels(11.0),
                font: self.icon_font,
                align_y: alignment::Vertical::Center,
                ..canvas::Text::default()
            });

            if self.state.zoom < DIAGRAM_BLOCK_ZOOM {
                continue;
            }
            for (row, line) in note.lines().iter().enumerate() {
                frame.fill_text(canvas::Text {
                    content: line.clone(),
                    position: Point::new(
                        bounds.x + 10.0,
                        header.y + header.height + 6.0 + row as f32 * DIAGRAM_NOTE_LINE,
                    ),
                    color: ink,
                    size: Pixels(11.0),
                    font: self.font,
                    ..canvas::Text::default()
                });
            }
            let handle = note.resize_handle();
            for step in 0..3 {
                let offset = 4.0 + step as f32 * 4.0;
                frame.stroke(
                    &canvas::Path::line(
                        Point::new(handle.x + handle.width - offset, handle.y + handle.height),
                        Point::new(handle.x + handle.width, handle.y + handle.height - offset),
                    ),
                    canvas::Stroke::default()
                        .with_color(with_alpha(ink, 0.45))
                        .with_width(1.2),
                );
            }
            let _ = palette;
        }
    }

    fn draw_edges(
        &self,
        frame: &mut canvas::Frame,
        palette: &crate::ui::theme::Tokens,
        visible: Rectangle,
    ) {
        let label_points: Vec<Option<Point>> = (0..self.state.diagram.edges.len())
            .map(|index| {
                (self.state.edge_is_visible(index)
                    && !self.state.diagram.edges[index].constraint_name.is_empty())
                .then(|| edge_anchor(self.state, index))
                .flatten()
                .map(|(start, end, forward)| {
                    let (c1, c2) = edge_controls(start, end, forward);
                    edge_midpoint(start, c1, c2, end)
                })
            })
            .collect();
        for index in 0..self.state.diagram.edges.len() {
            if !self.state.edge_is_visible(index) {
                continue;
            }
            let edge = &self.state.diagram.edges[index];
            let Some((start, end, forward)) = edge_anchor(self.state, index) else {
                continue;
            };
            if !self
                .state
                .edge_boxes
                .get(index)
                .is_some_and(|box_| box_.intersects(&visible))
            {
                continue;
            }
            let selected = self
                .state
                .selected
                .is_some_and(|table| table == edge.from || table == edge.to);
            let hovered = self.state.hovered_edge == Some(index);

            let (c1, c2) = edge_controls(start, end, forward);
            let label_position = edge_midpoint(start, c1, c2, end);
            let crowded = label_points.iter().enumerate().any(|(other, point)| {
                other != index
                    && point.is_some_and(|point| {
                        (point.x - label_position.x).abs() < LABEL_CROWD_X
                            && (point.y - label_position.y).abs() < LABEL_CROWD_Y
                    })
            });
            let label = (self.state.zoom >= DETAIL_ZOOM
                && !edge.constraint_name.is_empty()
                && !crowded
                && !self
                    .state
                    .table_boxes
                    .iter()
                    .any(|bounds| bounds.contains(label_position)))
            .then(|| {
                crate::utils::text::truncate_with_ellipsis(
                    &format!(
                        "{}  {}",
                        edge.constraint_name,
                        if edge.one_to_one { "1:1" } else { "N:1" }
                    ),
                    32,
                )
                .0
            });
            let color = if hovered {
                palette.danger
            } else if selected {
                palette.accent
            } else {
                with_alpha(palette.muted, 0.5)
            };
            let width = if hovered {
                2.6
            } else if selected {
                2.0
            } else {
                1.1
            };
            let edge_stroke = canvas::Stroke::default()
                .with_color(color)
                .with_width(width);
            if let Some(label) = &label {
                let length = ((c1.x - start.x).hypot(c1.y - start.y)
                    + (c2.x - c1.x).hypot(c2.y - c1.y)
                    + (end.x - c2.x).hypot(end.y - c2.y))
                .max(1.0);
                let half_gap =
                    ((label.chars().count() as f32 * 4.8 + 7.0) / length).clamp(0.04, 0.22);
                frame.stroke(
                    &edge_segment(start, c1, c2, end, 0.0, 0.5 - half_gap),
                    edge_stroke,
                );
                frame.stroke(
                    &edge_segment(start, c1, c2, end, 0.5 + half_gap, 1.0),
                    edge_stroke,
                );
            } else {
                frame.stroke(&edge_segment(start, c1, c2, end, 0.0, 1.0), edge_stroke);
            }

            if self.state.zoom < DIAGRAM_BLOCK_ZOOM {
                continue;
            }
            let stroke = canvas::Stroke::default()
                .with_color(color)
                .with_width(width);
            let direction = if forward { 1.0 } else { -1.0 };
            if edge.one_to_one {
                self.draw_one_bar(frame, start, -direction, &stroke);
            } else {
                self.draw_crows_foot(frame, start, -direction, &stroke);
            }
            self.draw_one_bar(frame, end, direction, &stroke);
            if let Some(label) = label {
                frame.fill_text(canvas::Text {
                    content: label,
                    position: label_position,
                    color,
                    size: Pixels(9.0),
                    font: self.font,
                    align_x: TextAlignment::Center,
                    align_y: alignment::Vertical::Center,
                    ..canvas::Text::default()
                });
            }
        }

        if let Some(link) = &self.state.link
            && let Some(table) = self.state.diagram.tables.get(link.from)
        {
            let start = Point::new(
                table.position.x + table.size().width,
                table.column_center_y(link.from_column),
            );
            let path = canvas::Path::new(|builder| {
                builder.move_to(start);
                builder.line_to(link.cursor);
            });
            frame.stroke(
                &path,
                canvas::Stroke::default()
                    .with_color(palette.danger)
                    .with_width(2.0),
            );
            frame.fill(&canvas::Path::circle(link.cursor, 4.0), palette.danger);
        }
    }

    fn draw_crows_foot(
        &self,
        frame: &mut canvas::Frame,
        at: Point,
        direction: f32,
        stroke: &canvas::Stroke<'_>,
    ) {
        let length = 9.0;
        let spread = 5.0;
        let tip = Point::new(at.x - length * direction, at.y);
        for offset in [-spread, 0.0, spread] {
            let prong = canvas::Path::new(|builder| {
                builder.move_to(tip);
                builder.line_to(Point::new(at.x, at.y + offset));
            });
            frame.stroke(&prong, *stroke);
        }
    }

    fn draw_one_bar(
        &self,
        frame: &mut canvas::Frame,
        at: Point,
        direction: f32,
        stroke: &canvas::Stroke<'_>,
    ) {
        let bar = canvas::Path::new(|builder| {
            builder.move_to(Point::new(at.x - 7.0 * direction, at.y - 5.0));
            builder.line_to(Point::new(at.x - 7.0 * direction, at.y + 5.0));
        });
        frame.stroke(&bar, *stroke);
    }

    fn draw_nodes(
        &self,
        frame: &mut canvas::Frame,
        palette: &crate::ui::theme::Tokens,
        visible: Rectangle,
    ) {
        let detailed = self.state.zoom >= DETAIL_ZOOM;
        let blocks = self.state.zoom < DIAGRAM_BLOCK_ZOOM;
        for (index, table) in self.state.diagram.tables.iter().enumerate() {
            if self.state.table_is_hidden(index) {
                continue;
            }
            let bounds = table.bounds();
            if !bounds.intersects(&visible) {
                continue;
            }
            let selected = self.state.selected == Some(index);
            let matched = self.state.matches.contains(&index);

            if blocks {
                frame.fill_rectangle(
                    bounds.position(),
                    bounds.size(),
                    if selected || matched {
                        palette.accent
                    } else {
                        palette.chrome
                    },
                );
                continue;
            }

            let body =
                canvas::Path::rounded_rectangle(bounds.position(), bounds.size(), 6.0.into());
            frame.fill(&body, palette.chrome2);
            frame.stroke(
                &body,
                canvas::Stroke::default()
                    .with_color(if selected {
                        palette.accent
                    } else if matched {
                        palette.danger
                    } else {
                        palette.line
                    })
                    .with_width(if selected || matched { 2.0 } else { 1.0 }),
            );

            let header = canvas::Path::rounded_rectangle(
                bounds.position(),
                Size::new(bounds.width, DIAGRAM_HEADER_HEIGHT),
                6.0.into(),
            );
            frame.fill(
                &header,
                if selected {
                    with_alpha(palette.accent, 0.30)
                } else {
                    palette.chrome
                },
            );

            let (title, _) = crate::utils::text::truncate_with_ellipsis(&table.name, 26);
            frame.fill_text(canvas::Text {
                content: title,
                position: Point::new(bounds.x + 10.0, bounds.y + DIAGRAM_HEADER_HEIGHT / 2.0),
                color: palette.fg,
                size: Pixels(12.0),
                font: self.font,
                align_y: alignment::Vertical::Center,
                ..canvas::Text::default()
            });

            frame.fill_text(canvas::Text {
                content: if table.collapsed {
                    crate::ICON_ARROW_RIGHT_S_LINE.to_string()
                } else {
                    crate::ICON_ARROW_DOWN_S_LINE.to_string()
                },
                position: Point::new(
                    bounds.x + bounds.width - 12.0,
                    bounds.y + DIAGRAM_HEADER_HEIGHT / 2.0,
                ),
                color: palette.muted,
                size: Pixels(13.0),
                font: self.icon_font,
                align_x: TextAlignment::Center,
                align_y: alignment::Vertical::Center,
                ..canvas::Text::default()
            });

            if !detailed || table.collapsed {
                continue;
            }

            let separator = canvas::Path::line(
                Point::new(bounds.x, bounds.y + DIAGRAM_HEADER_HEIGHT),
                Point::new(bounds.x + bounds.width, bounds.y + DIAGRAM_HEADER_HEIGHT),
            );
            frame.stroke(
                &separator,
                canvas::Stroke::default()
                    .with_color(palette.line)
                    .with_width(1.0),
            );

            for (row, column) in table.columns.iter().enumerate() {
                let center_y = table.column_center_y(row);
                if center_y < visible.y - DIAGRAM_ROW_HEIGHT
                    || center_y > visible.y + visible.height + DIAGRAM_ROW_HEIGHT
                {
                    continue;
                }
                let marker = if column.primary {
                    Some((crate::ICON_DIAGRAM_KEY, crate::DIAGRAM_PRIMARY_KEY_COLOR))
                } else if column.foreign {
                    Some((crate::ICON_DIAGRAM_FK, crate::DIAGRAM_FOREIGN_KEY_COLOR))
                } else {
                    None
                };
                if let Some((icon, color)) = marker {
                    frame.fill_text(canvas::Text {
                        content: icon.to_string(),
                        position: Point::new(bounds.x + 11.0, center_y),
                        color,
                        size: Pixels(10.0),
                        font: self.icon_font,
                        align_x: TextAlignment::Center,
                        align_y: alignment::Vertical::Center,
                        ..canvas::Text::default()
                    });
                }

                let (name, _) = crate::utils::text::truncate_with_ellipsis(&column.name, 22);
                frame.fill_text(canvas::Text {
                    content: name,
                    position: Point::new(bounds.x + 20.0, center_y),
                    color: if column.primary {
                        palette.fg
                    } else if column.nullable {
                        palette.muted
                    } else {
                        palette.body
                    },
                    size: Pixels(10.5),
                    font: self.font,
                    align_y: alignment::Vertical::Center,
                    ..canvas::Text::default()
                });

                let kind = format!(
                    "{}{}{}",
                    column.data_type,
                    if column.nullable { "?" } else { "" },
                    if column.unique && !column.primary {
                        " UQ"
                    } else if column.indexed && !column.primary {
                        " IX"
                    } else {
                        ""
                    }
                );
                let (kind, _) = crate::utils::text::truncate_with_ellipsis(&kind, 15);
                frame.fill_text(canvas::Text {
                    content: kind,
                    position: Point::new(bounds.x + bounds.width - 16.0, center_y),
                    color: palette.muted,
                    size: Pixels(10.0),
                    font: self.font,
                    align_x: TextAlignment::Right,
                    align_y: alignment::Vertical::Center,
                    ..canvas::Text::default()
                });

                if selected {
                    frame.fill(
                        &canvas::Path::circle(
                            Point::new(bounds.x + bounds.width - 6.0, center_y),
                            2.5,
                        ),
                        with_alpha(palette.accent, 0.9),
                    );
                }
            }
        }
    }

    fn draw_agent(&self, frame: &mut canvas::Frame, palette: &crate::ui::theme::Tokens) {
        let Some(agent) = &self.state.agent else {
            return;
        };

        let world = match self.state.diagram.tables.get(agent.table) {
            Some(table) if agent.arrived() => DiagramAgent::grab(table.bounds()),
            _ => agent.position(),
        };
        let cursor = Point::new(
            world.x * self.state.zoom + self.state.offset.x,
            world.y * self.state.zoom + self.state.offset.y,
        );
        let ink = palette.chrome;
        let paper = palette.chrome2;
        let live = crate::DIAGRAM_AGENT_COLOR;

        if !agent.arrived() {
            let from = Point::new(
                agent.from.x * self.state.zoom + self.state.offset.x,
                agent.from.y * self.state.zoom + self.state.offset.y,
            );
            frame.stroke(
                &canvas::Path::line(from, cursor),
                canvas::Stroke::default()
                    .with_color(with_alpha(live, 0.35))
                    .with_width(2.0),
            );
        }

        let halo = if agent.dragging { 12.0 } else { 10.0 };
        frame.fill(&canvas::Path::circle(cursor, halo), paper);
        frame.stroke(
            &canvas::Path::circle(cursor, halo),
            canvas::Stroke::default().with_color(ink).with_width(1.0),
        );
        frame.fill_text(canvas::Text {
            content: crate::ICON_AGENT.to_string(),
            position: Point::new(cursor.x - 5.5, cursor.y),
            color: live,
            size: Pixels(12.0),
            font: self.icon_font,
            align_y: alignment::Vertical::Center,
            ..canvas::Text::default()
        });

        if agent.label.is_empty() {
            return;
        }
        let anchor = Point::new(cursor.x + halo + 4.0, cursor.y - 9.0);
        let chip = Rectangle::new(
            anchor,
            Size::new(18.0 + agent.label.chars().count() as f32 * 6.4, 22.0),
        );
        let path = canvas::Path::rounded_rectangle(chip.position(), chip.size(), 6.0.into());
        frame.fill_rectangle(chip.position(), chip.size(), ink);
        frame.fill(&path, ink);
        frame.stroke(
            &path,
            canvas::Stroke::default()
                .with_color(with_alpha(live, 0.9))
                .with_width(1.2),
        );
        frame.fill_text(canvas::Text {
            content: agent.label.clone(),
            position: Point::new(chip.x + 8.0, chip.center_y()),
            color: palette.fg,
            size: Pixels(11.0),
            font: self.font,
            align_y: alignment::Vertical::Center,
            ..canvas::Text::default()
        });
    }

    fn draw_minimap(
        &self,
        frame: &mut canvas::Frame,
        palette: &crate::ui::theme::Tokens,
        bounds: Rectangle,
    ) {
        let Some(minimap) = self.minimap(bounds) else {
            return;
        };
        let panel = canvas::Path::rounded_rectangle(
            minimap.area.position(),
            minimap.area.size(),
            6.0.into(),
        );
        frame.fill(&panel, with_alpha(palette.chrome, 0.94));
        frame.stroke(
            &panel,
            canvas::Stroke::default()
                .with_color(palette.line)
                .with_width(1.0),
        );

        let clamp_x = |value: f32| value.clamp(minimap.area.x, minimap.area.x + minimap.area.width);
        let clamp_y =
            |value: f32| value.clamp(minimap.area.y, minimap.area.y + minimap.area.height);
        let clamped = |rect: Rectangle| {
            let left = clamp_x(rect.x);
            let top = clamp_y(rect.y);
            Rectangle::new(
                Point::new(left, top),
                Size::new(
                    (clamp_x(rect.x + rect.width) - left).max(0.0),
                    (clamp_y(rect.y + rect.height) - top).max(0.0),
                ),
            )
        };

        for group in &self.state.diagram.groups {
            let outline = group.outline();
            let cell = clamped(Rectangle::new(
                minimap.to_screen(outline.position()),
                Size::new(
                    (outline.width * minimap.scale).max(3.0),
                    (outline.height * minimap.scale).max(3.0),
                ),
            ));
            if cell.width <= 0.0 || cell.height <= 0.0 {
                continue;
            }
            let color = group_color(group.color);
            frame.fill_rectangle(cell.position(), cell.size(), with_alpha(color, 0.18));
            frame.stroke(
                &canvas::Path::rectangle(cell.position(), cell.size()),
                canvas::Stroke::default()
                    .with_color(with_alpha(color, 0.7))
                    .with_width(1.0),
            );
        }

        for (index, table) in self.state.diagram.tables.iter().enumerate() {
            let node = table.bounds();
            let cell = clamped(Rectangle::new(
                minimap.to_screen(node.position()),
                Size::new(
                    (node.width * minimap.scale).max(2.0),
                    (node.height * minimap.scale).max(2.0),
                ),
            ));
            if cell.width <= 0.0 || cell.height <= 0.0 {
                continue;
            }
            frame.fill_rectangle(
                cell.position(),
                cell.size(),
                if self.state.selected == Some(index) {
                    palette.accent
                } else {
                    with_alpha(palette.muted, 0.7)
                },
            );
        }

        let visible = self.visible_world(bounds);
        let camera = clamped(Rectangle::new(
            minimap.to_screen(visible.position()),
            Size::new(
                visible.width * minimap.scale,
                visible.height * minimap.scale,
            ),
        ));
        if camera.width <= 0.0 || camera.height <= 0.0 {
            return;
        }

        frame.fill_rectangle(
            camera.position(),
            camera.size(),
            with_alpha(palette.fg, 0.10),
        );
        frame.stroke(
            &canvas::Path::rectangle(camera.position(), camera.size()),
            canvas::Stroke::default()
                .with_color(with_alpha(palette.fg, 0.75))
                .with_width(1.5),
        );
        let corner = 5.0;
        for (x, y) in [
            (camera.x, camera.y),
            (camera.x + camera.width, camera.y),
            (camera.x, camera.y + camera.height),
            (camera.x + camera.width, camera.y + camera.height),
        ] {
            frame.fill_rectangle(
                Point::new(x - corner / 2.0, y - corner / 2.0),
                Size::new(corner, corner),
                palette.accent,
            );
        }
    }
}

impl canvas::Program<Message> for ErDiagram<'_> {
    type State = Interaction;

    fn update(
        &self,
        state: &mut Self::State,
        event: &Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<canvas::Action<Message>> {
        match event {
            Event::Window(window::Event::RedrawRequested(_)) => {
                let command = self.state.pending?;
                self.command_view(bounds, command)
                    .or(Some(Message::DiagramViewChanged {
                        offset: self.state.offset,
                        zoom: self.state.zoom,
                    }))
                    .map(canvas::Action::publish)
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right)) => {
                let position = cursor.position_in(bounds)?;
                let world = self.to_world(position);
                Some(
                    canvas::Action::publish(Message::DiagramMenuRequested {
                        position,
                        target: self.target_at(world),
                    })
                    .and_capture(),
                )
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                let position = cursor.position_in(bounds)?;
                if self.state.menu.is_some() {
                    return Some(canvas::Action::publish(Message::DiagramMenuClosed).and_capture());
                }
                if let Some(minimap) = self.minimap(bounds)
                    && minimap.area.contains(position)
                {
                    state.drag = Some(Drag::Camera);
                    return Some(
                        canvas::Action::publish(
                            self.centered_view(bounds, minimap.to_world(position)),
                        )
                        .and_capture(),
                    );
                }
                let world = self.to_world(position);
                if let Some(index) = self.collapse_handle_at(world) {
                    return Some(
                        canvas::Action::publish(Message::DiagramTableCollapseToggled(index))
                            .and_capture(),
                    );
                }
                if let Some((from, from_column)) = self.column_port_at(world)
                    && self.state.selected == Some(from)
                {
                    state.drag = Some(Drag::Link { from, from_column });
                    return Some(
                        canvas::Action::publish(Message::DiagramLinkStarted {
                            from,
                            from_column,
                            cursor: world,
                        })
                        .and_capture(),
                    );
                }
                if let Some(index) = self.group_resize_at(world) {
                    state.drag = Some(Drag::GroupResize { index });
                    return Some(canvas::Action::request_redraw().and_capture());
                }
                if let Some(index) = self.note_resize_at(world) {
                    state.drag = Some(Drag::NoteResize { index });
                    return Some(canvas::Action::request_redraw().and_capture());
                }
                if let Some(index) = self.node_at(world) {
                    if self.node_is_movable(index) {
                        let table = &self.state.diagram.tables[index];
                        state.drag = Some(Drag::Node {
                            index,
                            grab: table.position - world,
                        });
                    }
                    return Some(
                        canvas::Action::publish(Message::DiagramTableSelected(Some(index)))
                            .and_capture(),
                    );
                }
                if let Some(index) = self.note_at(world) {
                    let note = &self.state.diagram.notes[index];
                    state.drag = Some(Drag::Note {
                        index,
                        grab: note.position - world,
                    });
                    return Some(canvas::Action::request_redraw().and_capture());
                }
                if let Some(index) = self.group_handle_at(world) {
                    let group = &self.state.diagram.groups[index];
                    state.drag = Some(Drag::Group {
                        index,
                        grab: group.bounds.position() - world,
                    });
                    return Some(canvas::Action::request_redraw().and_capture());
                }
                if let Some(edge) = self.edge_at(world) {
                    return Some(
                        canvas::Action::publish(Message::DiagramRelationFollowed(edge))
                            .and_capture(),
                    );
                }
                state.drag = Some(Drag::Pan {
                    origin: self.state.offset - Vector::new(position.x, position.y),
                });
                Some(canvas::Action::publish(Message::DiagramTableSelected(None)).and_capture())
            }
            Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                let position = cursor.position_in(bounds)?;
                let world = self.to_world(position);
                let Some(drag) = state.drag.as_ref() else {
                    let hovered = self.edge_at(world);
                    if hovered == self.state.hovered_edge {
                        return None;
                    }
                    return Some(canvas::Action::publish(Message::DiagramEdgeHovered(
                        hovered,
                    )));
                };
                let message = match drag {
                    Drag::Node { index, grab } => Message::DiagramTableMoved {
                        index: *index,
                        position: world + *grab,
                    },
                    Drag::Group { index, grab } => Message::DiagramGroupMoved {
                        index: *index,
                        position: world + *grab,
                    },
                    Drag::GroupResize { index } => Message::DiagramGroupResized {
                        index: *index,
                        corner: world,
                    },
                    Drag::Note { index, grab } => Message::DiagramNoteMoved {
                        index: *index,
                        position: world + *grab,
                    },
                    Drag::NoteResize { index } => Message::DiagramNoteResized {
                        index: *index,
                        corner: world,
                    },
                    Drag::Pan { origin } => Message::DiagramViewChanged {
                        offset: *origin + Vector::new(position.x, position.y),
                        zoom: self.state.zoom,
                    },
                    Drag::Camera => {
                        let minimap = self.minimap(bounds)?;
                        self.centered_view(bounds, minimap.to_world(position))
                    }
                    Drag::Link { from, from_column } => Message::DiagramLinkStarted {
                        from: *from,
                        from_column: *from_column,
                        cursor: world,
                    },
                };
                Some(canvas::Action::publish(message).and_capture())
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                let drag = state.drag.take()?;
                let message = match drag {
                    Drag::Link { .. } => {
                        let world = cursor
                            .position_in(bounds)
                            .map(|position| self.to_world(position));
                        Message::DiagramLinkReleased {
                            target: world.and_then(|world| {
                                let index = self.node_at(world)?;
                                let column = self.state.diagram.tables[index]
                                    .column_at(world)
                                    .unwrap_or(0);
                                Some((index, column))
                            }),
                            extend: state.modifiers.shift(),
                        }
                    }
                    _ => Message::DiagramLayoutCommitted,
                };
                Some(canvas::Action::publish(message).and_capture())
            }
            Event::Mouse(mouse::Event::WheelScrolled { delta }) => {
                let position = cursor.position_in(bounds)?;
                let steps = match delta {
                    mouse::ScrollDelta::Lines { y, .. } => *y,
                    mouse::ScrollDelta::Pixels { y, .. } => *y / 40.0,
                };
                if steps == 0.0 {
                    return None;
                }
                let zoom = (self.state.zoom * (1.0 + steps * 0.12))
                    .clamp(DIAGRAM_MIN_ZOOM, DIAGRAM_MAX_ZOOM);
                let minimap_target = self
                    .minimap(bounds)
                    .filter(|minimap| minimap.area.contains(position))
                    .map(|minimap| minimap.to_world(position));
                let world = minimap_target.unwrap_or_else(|| self.to_world(position));
                let anchor = if minimap_target.is_some() {
                    Point::new(bounds.width / 2.0, bounds.height / 2.0)
                } else {
                    position
                };
                Some(
                    canvas::Action::publish(Message::DiagramViewChanged {
                        offset: Vector::new(anchor.x - world.x * zoom, anchor.y - world.y * zoom),
                        zoom,
                    })
                    .and_capture(),
                )
            }
            Event::Keyboard(keyboard::Event::ModifiersChanged(modifiers)) => {
                state.modifiers = *modifiers;
                None
            }
            Event::Keyboard(keyboard::Event::KeyPressed { key, modifiers, .. }) => {
                cursor.position_in(bounds)?;
                if modifiers.control() || modifiers.alt() || modifiers.logo() {
                    return None;
                }
                let keyboard::Key::Character(key) = key else {
                    return None;
                };
                let message = if key.eq_ignore_ascii_case("f") {
                    Message::DiagramCommandRequested(
                        self.state
                            .selected
                            .map(DiagramCommand::Focus)
                            .unwrap_or(DiagramCommand::Fit),
                    )
                } else if key == "0" {
                    Message::DiagramCommandRequested(DiagramCommand::Fit)
                } else if key == "+" || key == "=" {
                    Message::DiagramCommandRequested(DiagramCommand::Zoom(1.25))
                } else if key == "-" {
                    Message::DiagramCommandRequested(DiagramCommand::Zoom(0.8))
                } else if key.eq_ignore_ascii_case("a") {
                    Message::DiagramAutoLayout
                } else if key.eq_ignore_ascii_case("r") {
                    Message::DiagramRelationsSelected(crate::DiagramRelations::Auto)
                } else if key.eq_ignore_ascii_case("s") {
                    Message::DiagramSnapToggled
                } else if key == "/" {
                    Message::DiagramSearchToggled
                } else {
                    return None;
                };
                Some(canvas::Action::publish(message).and_capture())
            }
            _ => None,
        }
    }

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &Renderer,
        theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let palette = crate::ui::theme::tokens(theme);
        let mut frame = canvas::Frame::new(renderer, bounds.size());
        frame.fill_rectangle(Point::ORIGIN, bounds.size(), palette.bg);

        let visible = self.visible_world(bounds);
        frame.with_save(|frame| {
            frame.translate(self.state.offset);
            frame.scale(self.state.zoom);
            self.draw_grid(frame, &palette, visible);
            self.draw_groups(frame);
            self.draw_edges(frame, &palette, visible);
            self.draw_nodes(frame, &palette, visible);
        });

        let mut overlay = canvas::Frame::new(renderer, bounds.size());
        overlay.with_save(|overlay| {
            overlay.translate(self.state.offset);
            overlay.scale(self.state.zoom);
            self.draw_notes(overlay, &palette);
        });

        let mut top = canvas::Frame::new(renderer, bounds.size());
        self.draw_minimap(&mut top, &palette, bounds);
        self.draw_agent(&mut top, &palette);

        vec![
            frame.into_geometry(),
            overlay.into_geometry(),
            top.into_geometry(),
        ]
    }

    fn mouse_interaction(
        &self,
        state: &Self::State,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        match state.drag {
            Some(Drag::Link { .. }) => return mouse::Interaction::Crosshair,
            Some(_) => return mouse::Interaction::Grabbing,
            None => {}
        }
        let Some(position) = cursor.position_in(bounds) else {
            return mouse::Interaction::default();
        };
        if let Some(minimap) = self.minimap(bounds)
            && minimap.area.contains(position)
        {
            return mouse::Interaction::Pointer;
        }
        let world = self.to_world(position);
        if self.collapse_handle_at(world).is_some() {
            return mouse::Interaction::Pointer;
        }
        if self.group_resize_at(world).is_some() {
            return mouse::Interaction::ResizingDiagonallyDown;
        }
        if self.note_resize_at(world).is_some() {
            return mouse::Interaction::ResizingDiagonallyDown;
        }
        if self.state.selected.is_some() && self.column_port_at(world).is_some() {
            return mouse::Interaction::Crosshair;
        }
        if self.node_at(world).is_some()
            || self.note_at(world).is_some()
            || self.group_handle_at(world).is_some()
        {
            return mouse::Interaction::Grab;
        }
        if self.edge_at(world).is_some() {
            return mouse::Interaction::Pointer;
        }
        mouse::Interaction::default()
    }
}

fn svg_color(color: Color) -> String {
    let [r, g, b, _] = color.into_rgba8();
    format!("#{r:02x}{g:02x}{b:02x}")
}

fn svg_escape(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

pub(crate) fn diagram_svg(state: &DiagramState, theme: &Theme, font_family: &str) -> String {
    let palette = crate::ui::theme::tokens(theme);
    let world = state
        .diagram
        .content_bounds()
        .unwrap_or(Rectangle::new(Point::ORIGIN, Size::new(800.0, 600.0)));
    let pad = 40.0;
    let width = world.width + pad * 2.0;
    let height = world.height + pad * 2.0;
    let mut svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{width:.0}\" height=\"{height:.0}\" \
         viewBox=\"0 0 {width:.0} {height:.0}\">\n\
         <rect width=\"100%\" height=\"100%\" fill=\"{}\"/>\n\
         <g transform=\"translate({:.1} {:.1})\" font-family=\"{}\">\n",
        svg_color(palette.bg),
        pad - world.x,
        pad - world.y,
        svg_escape(&font_family.replace('"', "")),
    );

    for group in &state.diagram.groups {
        let color = group_color(group.color);
        svg.push_str(&format!(
            "<rect x=\"{:.1}\" y=\"{:.1}\" width=\"{:.1}\" height=\"{:.1}\" rx=\"10\" \
             fill=\"{}\" fill-opacity=\"0.07\" stroke=\"{}\" stroke-opacity=\"0.5\"/>\n\
             <text x=\"{:.1}\" y=\"{:.1}\" font-size=\"12\" fill=\"{}\">{}</text>\n",
            group.bounds.x,
            group.bounds.y,
            group.bounds.width,
            group.bounds.height,
            svg_color(color),
            svg_color(color),
            group.bounds.x + 12.0,
            group.bounds.y + 24.0,
            svg_color(color),
            svg_escape(&group.name)
        ));
    }

    for index in 0..state.diagram.edges.len() {
        let Some((start, end, forward)) = edge_anchor(state, index) else {
            continue;
        };
        let (c1, c2) = edge_controls(start, end, forward);
        let edge = &state.diagram.edges[index];
        let middle = edge_midpoint(start, c1, c2, end);
        let label = format!(
            "{}  {}",
            edge.constraint_name,
            if edge.one_to_one { "1:1" } else { "N:1" }
        );
        svg.push_str(&format!(
            "<path d=\"M {:.1} {:.1} C {:.1} {:.1}, {:.1} {:.1}, {:.1} {:.1}\" fill=\"none\" \
             stroke=\"{}\" stroke-opacity=\"0.6\" stroke-width=\"1.4\"/>\n\
             <circle cx=\"{:.1}\" cy=\"{:.1}\" r=\"3\" fill=\"{}\"/>\n\
             <text x=\"{:.1}\" y=\"{:.1}\" font-size=\"9\" text-anchor=\"middle\" fill=\"{}\">{}</text>\n",
            start.x,
            start.y,
            c1.x,
            c1.y,
            c2.x,
            c2.y,
            end.x,
            end.y,
            svg_color(palette.muted),
            start.x,
            start.y,
            svg_color(palette.muted),
            middle.x,
            middle.y,
            svg_color(palette.muted),
            svg_escape(&label),
        ));
    }

    for (index, table) in state.diagram.tables.iter().enumerate() {
        let bounds = table.bounds();
        let selected = state.selected == Some(index);
        let (stroke, stroke_width) = if selected {
            (palette.accent, 2.0)
        } else if state.matches.contains(&index) {
            (palette.danger, 2.0)
        } else {
            (palette.line, 1.0)
        };
        let highlight = if selected {
            format!(
                "<rect x=\"{:.1}\" y=\"{:.1}\" width=\"{:.1}\" height=\"{:.1}\" fill=\"{}\" \
                 fill-opacity=\"0.3\"/>\n",
                bounds.x,
                bounds.y,
                bounds.width,
                DIAGRAM_HEADER_HEIGHT,
                svg_color(palette.accent)
            )
        } else {
            String::new()
        };
        svg.push_str(&format!(
            "<rect x=\"{:.1}\" y=\"{:.1}\" width=\"{:.1}\" height=\"{:.1}\" rx=\"6\" fill=\"{}\" \
             stroke=\"{}\" stroke-width=\"{:.0}\"/>\n\
             <rect x=\"{:.1}\" y=\"{:.1}\" width=\"{:.1}\" height=\"{:.1}\" fill=\"{}\"/>\n\
             {highlight}\
             <text x=\"{:.1}\" y=\"{:.1}\" font-size=\"12\" fill=\"{}\">{}</text>\n",
            bounds.x,
            bounds.y,
            bounds.width,
            bounds.height,
            svg_color(palette.chrome2),
            svg_color(stroke),
            stroke_width,
            bounds.x,
            bounds.y,
            bounds.width,
            DIAGRAM_HEADER_HEIGHT,
            svg_color(palette.chrome),
            bounds.x + 10.0,
            bounds.y + DIAGRAM_HEADER_HEIGHT / 2.0 + 4.0,
            svg_color(palette.fg),
            svg_escape(&table.name)
        ));

        if table.collapsed {
            continue;
        }
        for (row, column) in table.columns.iter().enumerate() {
            let y = table.column_center_y(row) + 3.5;
            let marker = if column.primary {
                format!(
                    "<circle cx=\"{:.1}\" cy=\"{:.1}\" r=\"3.5\" fill=\"{}\"/>\n",
                    bounds.x + 11.0,
                    y - 3.5,
                    svg_color(crate::DIAGRAM_PRIMARY_KEY_COLOR)
                )
            } else if column.foreign {
                format!(
                    "<circle cx=\"{:.1}\" cy=\"{:.1}\" r=\"3.5\" fill=\"none\" stroke=\"{}\"/>\n",
                    bounds.x + 11.0,
                    y - 3.5,
                    svg_color(crate::DIAGRAM_FOREIGN_KEY_COLOR)
                )
            } else {
                String::new()
            };
            let kind = format!(
                "{}{}{}",
                column.data_type,
                if column.nullable { "?" } else { "" },
                if column.unique && !column.primary {
                    " UQ"
                } else if column.indexed && !column.primary {
                    " IX"
                } else {
                    ""
                }
            );
            svg.push_str(&format!(
                "{marker}<text x=\"{:.1}\" y=\"{:.1}\" font-size=\"10.5\" fill=\"{}\">{}</text>\n\
                 <text x=\"{:.1}\" y=\"{:.1}\" font-size=\"10\" text-anchor=\"end\" \
                 fill=\"{}\">{}</text>\n",
                bounds.x + 20.0,
                y,
                svg_color(palette.body),
                svg_escape(&column.name),
                bounds.x + bounds.width - 10.0,
                y,
                svg_color(palette.muted),
                svg_escape(&kind)
            ));
        }
    }

    for note in &state.diagram.notes {
        let bounds = note.bounds();
        svg.push_str(&format!(
            "<rect x=\"{:.1}\" y=\"{:.1}\" width=\"{:.1}\" height=\"{:.1}\" rx=\"6\" fill=\"{}\" \
             fill-opacity=\"0.16\" stroke=\"{}\"/>\n\
             <text x=\"{:.1}\" y=\"{:.1}\" font-size=\"11\" fill=\"{}\">{}</text>\n",
            bounds.x,
            bounds.y,
            bounds.width,
            bounds.height,
            svg_color(palette.accent),
            svg_color(palette.accent),
            note.position.x + 10.0,
            note.position.y + 22.0,
            svg_color(palette.fg),
            svg_escape(&note.text)
        ));
    }

    svg.push_str("</g>\n</svg>\n");
    svg
}

pub(crate) fn group_color(color: u8) -> Color {
    crate::DIAGRAM_AREA_COLORS[color as usize % crate::DIAGRAM_AREA_COLORS.len()]
}

pub(crate) fn svg_to_png(svg: &str, size: (f32, f32)) -> Result<Vec<u8>, String> {
    use resvg::{tiny_skia, usvg};

    let mut options = usvg::Options::default();
    options.fontdb_mut().load_system_fonts();
    let tree = usvg::Tree::from_str(svg, &options).map_err(|error| error.to_string())?;
    let width = size.0.max(1.0).ceil() as u32;
    let height = size.1.max(1.0).ceil() as u32;
    let mut pixmap =
        tiny_skia::Pixmap::new(width, height).ok_or_else(|| String::from("Image too large."))?;
    resvg::render(
        &tree,
        tiny_skia::Transform::identity(),
        &mut pixmap.as_mut(),
    );
    pixmap.encode_png().map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DiagramColumn, DiagramNote, SchemaDiagram, SidebarRelationEntry};

    fn state() -> DiagramState {
        let diagram = SchemaDiagram::build(
            vec![
                (
                    String::from("orders"),
                    vec![
                        DiagramColumn {
                            name: String::from("id"),
                            data_type: String::from("INTEGER"),
                            primary: true,
                            foreign: false,
                            nullable: false,
                            default: None,
                            indexed: true,
                            unique: true,
                            attributes: String::new(),
                        },
                        DiagramColumn {
                            name: String::from("customer_id"),
                            data_type: String::from("INTEGER"),
                            primary: false,
                            foreign: false,
                            nullable: true,
                            default: None,
                            indexed: true,
                            unique: false,
                            attributes: String::new(),
                        },
                    ],
                ),
                (
                    String::from("customers"),
                    vec![DiagramColumn {
                        name: String::from("id"),
                        data_type: String::from("INTEGER"),
                        primary: true,
                        foreign: false,
                        nullable: false,
                        default: None,
                        indexed: true,
                        unique: true,
                        attributes: String::new(),
                    }],
                ),
            ],
            &[SidebarRelationEntry {
                table: String::from("orders"),
                relation: crate::RelationInfo {
                    column: String::from("customer_id"),
                    referenced_table: String::from("customers"),
                    referenced_column: String::from("id"),
                },
                constraint_name: String::from("orders_customer_id_fkey"),
            }],
        );
        DiagramState {
            diagram,
            loading: false,
            ..DiagramState::default()
        }
    }

    fn program(state: &DiagramState) -> ErDiagram<'_> {
        ErDiagram {
            state,
            font: Font::MONOSPACE,
            icon_font: Font::MONOSPACE,
            scale: 1.0,
        }
    }

    fn canvas_bounds() -> Rectangle {
        Rectangle::new(Point::ORIGIN, Size::new(1200.0, 800.0))
    }

    fn published(
        program: &ErDiagram<'_>,
        interaction: &mut Interaction,
        event: Event,
        cursor: Point,
    ) -> Option<Message> {
        canvas::Program::update(
            program,
            interaction,
            &event,
            canvas_bounds(),
            mouse::Cursor::Available(cursor),
        )
        .and_then(|action| action.into_inner().0)
    }

    fn key_press(key: &str, modifiers: keyboard::Modifiers) -> Event {
        let key = keyboard::Key::Character(key.into());
        Event::Keyboard(keyboard::Event::KeyPressed {
            key: key.clone(),
            modified_key: key,
            physical_key: keyboard::key::Physical::Unidentified(
                keyboard::key::NativeCode::Unidentified,
            ),
            location: keyboard::Location::Standard,
            modifiers,
            text: None,
            repeat: false,
        })
    }

    #[test]
    fn pressing_and_dragging_a_table_moves_it_by_the_cursor_delta() {
        let mut state = state();
        state.refresh_derived();
        let program = program(&state);
        let mut interaction = Interaction::default();
        let origin = state.diagram.tables[0].position;
        let grab = Point::new(origin.x + 20.0, origin.y + 8.0);

        assert!(matches!(
            published(
                &program,
                &mut interaction,
                Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
                grab,
            ),
            Some(Message::DiagramTableSelected(Some(0)))
        ));

        let moved = Point::new(grab.x + 40.0, grab.y + 25.0);
        match published(
            &program,
            &mut interaction,
            Event::Mouse(mouse::Event::CursorMoved { position: moved }),
            moved,
        ) {
            Some(Message::DiagramTableMoved { index, position }) => {
                assert_eq!(index, 0);
                assert_eq!(position, Point::new(origin.x + 40.0, origin.y + 25.0));
            }
            other => panic!("expected a table move, got {other:?}"),
        }

        assert!(matches!(
            published(
                &program,
                &mut interaction,
                Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
                moved,
            ),
            Some(Message::DiagramLayoutCommitted)
        ));
    }

    #[test]
    fn notes_drag_from_their_body_and_resize_from_their_corner() {
        let mut state = state();
        state.diagram.notes.push(DiagramNote::new(
            String::from("hi"),
            Point::new(700.0, 420.0),
            None,
        ));
        state.refresh_derived();
        let bounds = state.diagram.notes[0].bounds();
        let program = program(&state);
        let mut interaction = Interaction::default();

        let grab = Point::new(bounds.x + 20.0, bounds.y + 10.0);
        assert!(
            published(
                &program,
                &mut interaction,
                Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
                grab,
            )
            .is_none()
        );
        let moved = Point::new(grab.x + 30.0, grab.y + 15.0);
        match published(
            &program,
            &mut interaction,
            Event::Mouse(mouse::Event::CursorMoved { position: moved }),
            moved,
        ) {
            Some(Message::DiagramNoteMoved { index, position }) => {
                assert_eq!(index, 0);
                assert_eq!(position, Point::new(bounds.x + 30.0, bounds.y + 15.0));
            }
            other => panic!("expected a note move, got {other:?}"),
        }
        published(
            &program,
            &mut interaction,
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
            moved,
        );

        let handle = state.diagram.notes[0].resize_handle();
        let corner = Point::new(handle.x + 2.0, handle.y + 2.0);
        assert!(
            published(
                &program,
                &mut interaction,
                Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
                corner,
            )
            .is_none()
        );
        let dragged = Point::new(corner.x + 40.0, corner.y + 20.0);
        assert!(matches!(
            published(
                &program,
                &mut interaction,
                Event::Mouse(mouse::Event::CursorMoved { position: dragged }),
                dragged,
            ),
            Some(Message::DiagramNoteResized { index: 0, corner }) if corner == dragged
        ));
    }

    #[test]
    fn areas_drag_from_their_header_and_resize_from_their_corner() {
        let mut state = state();
        state.diagram.groups.push(crate::DiagramGroup::new(
            String::from("billing"),
            Rectangle::new(Point::new(600.0, 90.0), Size::new(240.0, 160.0)),
        ));
        state.refresh_derived();
        let group = state.diagram.groups[0].bounds;
        let program = program(&state);
        let mut interaction = Interaction::default();

        let header = Point::new(group.x + 30.0, group.y + 8.0);
        assert!(
            published(
                &program,
                &mut interaction,
                Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
                header,
            )
            .is_none()
        );
        let moved = Point::new(header.x + 25.0, header.y + 35.0);
        match published(
            &program,
            &mut interaction,
            Event::Mouse(mouse::Event::CursorMoved { position: moved }),
            moved,
        ) {
            Some(Message::DiagramGroupMoved { index, position }) => {
                assert_eq!(index, 0);
                assert_eq!(position, Point::new(group.x + 25.0, group.y + 35.0));
            }
            other => panic!("expected an area move, got {other:?}"),
        }
        published(
            &program,
            &mut interaction,
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
            moved,
        );

        let handle = state.diagram.groups[0].resize_handle();
        let corner = Point::new(handle.x + 2.0, handle.y + 2.0);
        published(
            &program,
            &mut interaction,
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
            corner,
        );
        let dragged = Point::new(corner.x + 50.0, corner.y + 30.0);
        assert!(matches!(
            published(
                &program,
                &mut interaction,
                Event::Mouse(mouse::Event::CursorMoved { position: dragged }),
                dragged,
            ),
            Some(Message::DiagramGroupResized { index: 0, corner }) if corner == dragged
        ));
    }

    #[test]
    fn dragging_a_column_port_links_it_to_the_table_it_is_dropped_on() {
        let mut state = state();
        state.selected = Some(0);
        state.refresh_derived();
        let source = state.diagram.tables[0].bounds();
        let target = state.diagram.tables[1].bounds();
        let program = program(&state);
        let mut interaction = Interaction::default();

        let port = Point::new(
            source.x + source.width - 6.0,
            source.y + DIAGRAM_HEADER_HEIGHT + DIAGRAM_ROW_HEIGHT + 4.0,
        );
        match published(
            &program,
            &mut interaction,
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
            port,
        ) {
            Some(Message::DiagramLinkStarted {
                from, from_column, ..
            }) => {
                assert_eq!(from, 0);
                assert_eq!(from_column, 1);
            }
            other => panic!("expected a link to start, got {other:?}"),
        }

        let drop = Point::new(target.x + 20.0, target.y + DIAGRAM_HEADER_HEIGHT + 4.0);
        assert!(matches!(
            published(
                &program,
                &mut interaction,
                Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
                drop,
            ),
            Some(Message::DiagramLinkReleased {
                target: Some((1, 0)),
                extend: false,
            })
        ));

        published(
            &program,
            &mut interaction,
            Event::Keyboard(keyboard::Event::ModifiersChanged(
                keyboard::Modifiers::SHIFT,
            )),
            port,
        );
        published(
            &program,
            &mut interaction,
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
            port,
        );
        assert!(matches!(
            published(
                &program,
                &mut interaction,
                Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
                drop,
            ),
            Some(Message::DiagramLinkReleased {
                target: Some((1, 0)),
                extend: true,
            })
        ));
    }

    #[test]
    fn canvas_shortcuts_fit_focus_and_ignore_modified_keys() {
        let mut state = state();
        let mut interaction = Interaction::default();
        let cursor = Point::new(400.0, 300.0);

        assert!(matches!(
            published(
                &program(&state),
                &mut interaction,
                key_press("f", keyboard::Modifiers::default()),
                cursor,
            ),
            Some(Message::DiagramCommandRequested(DiagramCommand::Fit))
        ));

        state.selected = Some(1);
        assert!(matches!(
            published(
                &program(&state),
                &mut interaction,
                key_press("f", keyboard::Modifiers::default()),
                cursor,
            ),
            Some(Message::DiagramCommandRequested(DiagramCommand::Focus(1)))
        ));

        assert!(
            published(
                &program(&state),
                &mut interaction,
                key_press("f", keyboard::Modifiers::CTRL),
                cursor,
            )
            .is_none()
        );
    }

    #[test]
    fn the_minimap_maps_screen_and_world_points_both_ways() {
        let state = state();
        let program = program(&state);
        let minimap = program.minimap(canvas_bounds()).expect("minimap");
        let world = Point::new(120.0, 60.0);
        let round = minimap.to_world(minimap.to_screen(world));

        assert!((round.x - world.x).abs() < 0.01);
        assert!((round.y - world.y).abs() < 0.01);
        assert!(minimap.area.contains(minimap.to_screen(world)));
    }

    #[test]
    fn scrolling_zooms_around_the_cursor() {
        let state = state();
        let program = program(&state);
        let mut interaction = Interaction::default();
        let cursor = Point::new(240.0, 180.0);
        assert!(
            !program
                .minimap(canvas_bounds())
                .expect("minimap")
                .area
                .contains(cursor)
        );
        let world = program.to_world(cursor);

        match published(
            &program,
            &mut interaction,
            Event::Mouse(mouse::Event::WheelScrolled {
                delta: mouse::ScrollDelta::Lines { x: 0.0, y: 1.0 },
            }),
            cursor,
        ) {
            Some(Message::DiagramViewChanged { offset, zoom }) => {
                assert!(zoom > state.zoom);
                assert!((world.x * zoom + offset.x - cursor.x).abs() < 0.01);
                assert!((world.y * zoom + offset.y - cursor.y).abs() < 0.01);
            }
            other => panic!("expected a view change, got {other:?}"),
        }
    }

    #[test]
    fn canvas_text_literals_never_contain_emoji() {
        for (path, source) in [
            ("er_diagram.rs", include_str!("canvas.rs")),
            (
                "model/diagram.rs",
                include_str!("../../../../model/diagram.rs"),
            ),
            ("app/diagram.rs", include_str!("../../../diagram.rs")),
        ] {
            for (number, line) in source.lines().enumerate() {
                let trimmed = line.trim_start();
                if trimmed.starts_with("//") || trimmed.starts_with("///") {
                    continue;
                }
                assert!(
                    line.is_ascii(),
                    "{path} line {} carries a non-ASCII literal: {line}",
                    number + 1
                );
            }
        }
    }

    #[test]
    fn a_self_referencing_relation_loops_out_of_the_same_side() {
        let mut state = state();
        state.diagram.edges.push(crate::DiagramEdge {
            from: 0,
            from_column: 1,
            to: 0,
            to_column: 0,
            constraint_name: String::from("orders_parent_fkey"),
            one_to_one: false,
        });
        let index = state.diagram.edges.len() - 1;
        let (start, end, forward) = edge_anchor(&state, index).expect("anchors");

        assert!(forward);
        assert_eq!(start.x, end.x);
        assert_ne!(start.y, end.y);

        let (c1, c2) = edge_controls(start, end, forward);
        assert!(c1.x > start.x, "the loop has to bulge away from the table");
        assert!(c2.x > end.x);
    }

    #[test]
    fn edge_bounds_reject_points_far_from_the_curve() {
        let bounds = edge_bounds(Point::new(0.0, 0.0), Point::new(100.0, 40.0));
        assert!(bounds.contains(Point::new(50.0, 20.0)));
        assert!(!bounds.contains(Point::new(400.0, 20.0)));
    }

    #[test]
    fn key_markers_ignore_the_theme_accent() {
        for theme in [Theme::Dark, Theme::Light, Theme::Dracula] {
            let svg = diagram_svg(&state(), &theme, "monospace");
            let accent = svg_color(crate::ui::theme::tokens(&theme).accent);
            let primary = svg_color(crate::DIAGRAM_PRIMARY_KEY_COLOR);
            let foreign = svg_color(crate::DIAGRAM_FOREIGN_KEY_COLOR);

            assert!(svg.contains(&format!("r=\"3.5\" fill=\"{primary}\"")));
            assert!(svg.contains(&format!("fill=\"none\" stroke=\"{foreign}\"")));
            assert_ne!(primary, accent);
            assert_ne!(foreign, accent);
        }
    }

    #[test]
    fn svg_export_carries_tables_columns_and_relations() {
        let mut state = state();
        state
            .diagram
            .notes
            .push(DiagramNote::new(String::from("a & b"), Point::ORIGIN, None));
        let svg = diagram_svg(&state, &Theme::Dark, "monospace");

        assert!(svg.starts_with("<svg"));
        assert!(svg.trim_end().ends_with("</svg>"));
        assert!(svg.contains(">orders<"));
        assert!(svg.contains(">customer_id<"));
        assert!(svg.contains("<path d=\"M "));
        assert!(svg.contains("orders_customer_id_fkey  N:1"));
        assert!(svg.contains("INTEGER? IX"));
        assert!(svg.contains("a &amp; b"));

        state.selected = Some(1);
        state.matches = vec![0];
        let svg = diagram_svg(&state, &Theme::Dark, "monospace");
        let palette = crate::ui::theme::tokens(&Theme::Dark);

        assert!(svg.contains(&format!(
            "stroke=\"{}\" stroke-width=\"2\"",
            svg_color(palette.accent)
        )));
        assert!(svg.contains(&format!(
            "stroke=\"{}\" stroke-width=\"2\"",
            svg_color(palette.danger)
        )));
    }

    #[test]
    fn png_export_produces_a_decodable_image() {
        let svg = diagram_svg(&state(), &Theme::Dark, "Fira Sans");
        assert!(svg.contains("font-family=\"Fira Sans\""));
        let png = svg_to_png(&svg, (400.0, 300.0)).expect("png renders");
        assert_eq!(&png[1..4], b"PNG");
    }
}
