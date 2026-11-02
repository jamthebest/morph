//! Contenedor que muestra, recortado, un contenido de tamaño fijo dentro de una caja
//! de tamaño variable (arriba y centrado). `container` no sirve: acota a sus hijos al
//! tamaño del contenedor y el contenido se recolocaría durante la animación.

use crate::config::Edge;
use cosmic::iced::core::widget::{Operation, Tree};
use cosmic::iced::core::{
    Clipboard, Element, Event, Layout, Length, Point, Rectangle, Shell, Size, Vector, Widget,
    layout, mouse, overlay, renderer,
};

pub struct Reveal<'a, Message, Theme, Renderer> {
    content: Element<'a, Message, Theme, Renderer>,
    full: Size,
    visible: Size,
    edge: Edge,
}

impl<'a, Message, Theme, Renderer> Reveal<'a, Message, Theme, Renderer> {
    /// `content` se maqueta a `full`; solo se ve la parte que cabe en `visible`.
    pub fn new(
        content: impl Into<Element<'a, Message, Theme, Renderer>>,
        full: Size,
        visible: Size,
        edge: Edge,
    ) -> Self {
        Self {
            content: content.into(),
            full,
            visible,
            edge,
        }
    }
}

/// y del contenido dentro de la caja: arriba queda pegado al borde de arriba; abajo, al de
/// abajo (la caja crece hacia arriba y el contenido no se desliza).
fn content_y(edge: Edge, full_h: f32, visible_h: f32) -> f32 {
    match edge {
        Edge::Top => 0.0,
        Edge::Bottom => visible_h - full_h,
    }
}

fn content_layout(layout: Layout<'_>) -> Layout<'_> {
    layout
        .children()
        .next()
        .unwrap()
        .with_virtual_offset(layout.virtual_offset())
}

impl<Message, Theme, Renderer> Widget<Message, Theme, Renderer>
    for Reveal<'_, Message, Theme, Renderer>
where
    Renderer: cosmic::iced::core::Renderer,
{
    fn size(&self) -> Size<Length> {
        Size::new(
            Length::Fixed(self.visible.width),
            Length::Fixed(self.visible.height),
        )
    }

    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.content)]
    }

    fn diff(&mut self, tree: &mut Tree) {
        tree.diff_children(std::slice::from_mut(&mut self.content));
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        _limits: &layout::Limits,
    ) -> layout::Node {
        let child = self.content.as_widget_mut().layout(
            &mut tree.children[0],
            renderer,
            &layout::Limits::new(self.full, self.full),
        );
        let x = (self.visible.width - self.full.width) / 2.0;
        layout::Node::with_children(
            self.visible,
            vec![child.move_to(Point::new(
                x,
                content_y(self.edge, self.full.height, self.visible.height),
            ))],
        )
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        operation.container(None, layout.bounds());
        operation.traverse(&mut |operation| {
            self.content.as_widget_mut().operate(
                &mut tree.children[0],
                content_layout(layout),
                renderer,
                operation,
            );
        });
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        let Some(clipped) = layout.bounds().intersection(viewport) else {
            return;
        };
        self.content.as_widget_mut().update(
            &mut tree.children[0],
            event,
            content_layout(layout),
            cursor,
            renderer,
            clipboard,
            shell,
            &clipped,
        );
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        let Some(clipped) = layout.bounds().intersection(viewport) else {
            return mouse::Interaction::default();
        };
        self.content.as_widget().mouse_interaction(
            &tree.children[0],
            content_layout(layout),
            cursor,
            &clipped,
            renderer,
        )
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        let Some(clipped) = layout.bounds().intersection(viewport) else {
            return;
        };
        renderer.with_layer(clipped, |renderer| {
            self.content.as_widget().draw(
                &tree.children[0],
                renderer,
                theme,
                style,
                content_layout(layout),
                cursor,
                &clipped,
            );
        });
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, Message, Theme, Renderer>> {
        let child = layout
            .children()
            .next()
            .unwrap()
            .with_virtual_offset(layout.virtual_offset());
        self.content.as_widget_mut().overlay(
            &mut tree.children[0],
            child,
            renderer,
            viewport,
            translation,
        )
    }

    fn drag_destinations(
        &self,
        state: &Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        dnd_rectangles: &mut cosmic::iced::core::clipboard::DndDestinationRectangles,
    ) {
        self.content.as_widget().drag_destinations(
            &state.children[0],
            content_layout(layout),
            renderer,
            dnd_rectangles,
        );
    }
}

impl<'a, Message, Theme, Renderer> From<Reveal<'a, Message, Theme, Renderer>>
    for Element<'a, Message, Theme, Renderer>
where
    Message: 'a,
    Theme: 'a,
    Renderer: cosmic::iced::core::Renderer + 'a,
{
    fn from(r: Reveal<'a, Message, Theme, Renderer>) -> Self {
        Element::new(r)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_pins_to_the_edge() {
        assert_eq!(content_y(Edge::Top, 100.0, 40.0), 0.0);
        assert_eq!(content_y(Edge::Bottom, 100.0, 40.0), -60.0);
        assert_eq!(content_y(Edge::Bottom, 100.0, 100.0), 0.0);
    }
}
