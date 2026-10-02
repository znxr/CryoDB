use iced::advanced::{
    Clipboard as AdvancedClipboard, Layout as AdvancedLayout, Shell as AdvancedShell,
    Widget as AdvancedWidget, layout as advanced_layout, overlay as advanced_overlay,
    renderer as advanced_renderer, widget as advanced_widget,
};
use iced::{Element, Length, Point, Rectangle, Renderer, Size, Theme, Vector, mouse};

pub(crate) struct HeaderDropdown<'a, Message> {
    button: Element<'a, Message>,
    panel: Element<'a, Message>,
    is_open: bool,
    close_message: Message,
}

impl<'a, Message: Clone + 'a> HeaderDropdown<'a, Message> {
    pub(crate) fn new(
        button: Element<'a, Message>,
        panel: Element<'a, Message>,
        is_open: bool,
        close_message: Message,
    ) -> Self {
        Self {
            button,
            panel,
            is_open,
            close_message,
        }
    }
}

impl<'a, Message: Clone + 'a> AdvancedWidget<Message, Theme, Renderer>
    for HeaderDropdown<'a, Message>
{
    fn size(&self) -> Size<Length> {
        self.button.as_widget().size()
    }

    fn children(&self) -> Vec<advanced_widget::Tree> {
        vec![
            advanced_widget::Tree::new(&self.button),
            advanced_widget::Tree::new(&self.panel),
        ]
    }

    fn diff(&self, tree: &mut advanced_widget::Tree) {
        tree.diff_children(&[&self.button, &self.panel]);
    }

    fn layout(
        &mut self,
        tree: &mut advanced_widget::Tree,
        renderer: &Renderer,
        limits: &advanced_layout::Limits,
    ) -> advanced_layout::Node {
        self.button
            .as_widget_mut()
            .layout(&mut tree.children[0], renderer, limits)
    }

    fn draw(
        &self,
        tree: &advanced_widget::Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        style: &advanced_renderer::Style,
        layout: AdvancedLayout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        self.button.as_widget().draw(
            &tree.children[0],
            renderer,
            theme,
            style,
            layout,
            cursor,
            viewport,
        );
    }

    fn update(
        &mut self,
        tree: &mut advanced_widget::Tree,
        event: &iced::Event,
        layout: AdvancedLayout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
        clipboard: &mut dyn AdvancedClipboard,
        shell: &mut AdvancedShell<'_, Message>,
        viewport: &Rectangle,
    ) {
        self.button.as_widget_mut().update(
            &mut tree.children[0],
            event,
            layout,
            cursor,
            renderer,
            clipboard,
            shell,
            viewport,
        );
    }

    fn mouse_interaction(
        &self,
        tree: &advanced_widget::Tree,
        layout: AdvancedLayout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        self.button.as_widget().mouse_interaction(
            &tree.children[0],
            layout,
            cursor,
            viewport,
            renderer,
        )
    }

    fn operate(
        &mut self,
        tree: &mut advanced_widget::Tree,
        layout: AdvancedLayout<'_>,
        renderer: &Renderer,
        operation: &mut dyn advanced_widget::Operation,
    ) {
        operation.container(None, layout.bounds());
        operation.traverse(&mut |operation| {
            self.button
                .as_widget_mut()
                .operate(&mut tree.children[0], layout, renderer, operation);
        });
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut advanced_widget::Tree,
        layout: AdvancedLayout<'b>,
        renderer: &Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<advanced_overlay::Element<'b, Message, Theme, Renderer>> {
        let mut children = tree.children.iter_mut();

        let content = self.button.as_widget_mut().overlay(
            children.next().unwrap(),
            layout,
            renderer,
            viewport,
            translation,
        );

        let panel = if self.is_open {
            let position = layout.position() + translation;
            let target_height = layout.bounds().height;

            Some(advanced_overlay::Element::new(Box::new(
                DropdownPanelOverlay {
                    position,
                    target_height,
                    panel: &mut self.panel,
                    tree: children.next().unwrap(),
                    close_message: self.close_message.clone(),
                },
            )))
        } else {
            None
        };

        if content.is_some() || panel.is_some() {
            Some(
                advanced_overlay::Group::with_children(content.into_iter().chain(panel).collect())
                    .overlay(),
            )
        } else {
            None
        }
    }
}

impl<'a, Message: Clone + 'a> From<HeaderDropdown<'a, Message>> for Element<'a, Message> {
    fn from(dropdown: HeaderDropdown<'a, Message>) -> Self {
        Element::new(dropdown)
    }
}

struct DropdownPanelOverlay<'a, 'b, Message> {
    position: Point,
    target_height: f32,
    panel: &'b mut Element<'a, Message>,
    tree: &'b mut advanced_widget::Tree,
    close_message: Message,
}

impl<Message: Clone> advanced_overlay::Overlay<Message, Theme, Renderer>
    for DropdownPanelOverlay<'_, '_, Message>
{
    fn layout(&mut self, renderer: &Renderer, bounds: Size) -> advanced_layout::Node {
        let space_below = (bounds.height - (self.position.y + self.target_height)).max(0.0);
        let space_above = self.position.y.max(0.0);
        let max_height = if space_below > space_above {
            space_below
        } else {
            space_above
        };
        let max_width = bounds.width;
        let limits = advanced_layout::Limits::new(Size::ZERO, Size::new(max_width, max_height));

        let node = self
            .panel
            .as_widget_mut()
            .layout(self.tree, renderer, &limits);
        let size = node.size();

        let offset = if space_below > space_above {
            Vector::new(0.0, self.target_height)
        } else {
            Vector::new(0.0, -size.height)
        };

        let max_x = (bounds.width - size.width - 6.0).max(0.0);
        let x = self.position.x.clamp(0.0, max_x);
        node.move_to(Point::new(x, self.position.y) + offset)
    }

    fn update(
        &mut self,
        event: &iced::Event,
        layout: AdvancedLayout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
        clipboard: &mut dyn AdvancedClipboard,
        shell: &mut AdvancedShell<'_, Message>,
    ) {
        let bounds = layout.bounds();

        self.panel.as_widget_mut().update(
            self.tree, event, layout, cursor, renderer, clipboard, shell, &bounds,
        );

        if let iced::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) = event
            && !cursor.is_over(bounds)
        {
            shell.publish(self.close_message.clone());
            shell.capture_event();
        }
    }

    fn draw(
        &self,
        renderer: &mut Renderer,
        theme: &Theme,
        style: &advanced_renderer::Style,
        layout: AdvancedLayout<'_>,
        cursor: mouse::Cursor,
    ) {
        let bounds = layout.bounds();
        self.panel
            .as_widget()
            .draw(self.tree, renderer, theme, style, layout, cursor, &bounds);
    }

    fn mouse_interaction(
        &self,
        layout: AdvancedLayout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        let bounds = layout.bounds();
        self.panel
            .as_widget()
            .mouse_interaction(self.tree, layout, cursor, &bounds, renderer)
    }
}
