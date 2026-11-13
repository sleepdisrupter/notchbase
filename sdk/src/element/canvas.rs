//! A canvas to paint shapes on, with pointer and scroll events.

use super::{Element, Kind, Map};
use crate::protocol::{CanvasNode, Pointer, Shape};

pub struct Canvas<Msg> {
    node: CanvasNode,
    on_pointer: Option<Map<Pointer, Msg>>,
    on_scroll: Option<Map<Pointer, Msg>>,
}

pub fn canvas<Msg>(width: f32, height: f32) -> Canvas<Msg> {
    let node = CanvasNode { width: width.max(1.0), height: height.max(1.0), ..CanvasNode::default() };
    Canvas { node, on_pointer: None, on_scroll: None }
}

impl<Msg> Canvas<Msg> {
    pub fn push(mut self, shape: Shape) -> Self {
        self.node.shapes.push(shape);
        self
    }

    pub fn extend(mut self, shapes: impl IntoIterator<Item = Shape>) -> Self {
        self.node.shapes.extend(shapes);
        self
    }

    pub fn stretch(mut self) -> Self {
        self.node.stretch = true;
        self
    }

    pub fn on_pointer(mut self, map: impl Fn(Pointer) -> Msg + 'static) -> Self {
        self.on_pointer = Some(Box::new(map));
        self
    }

    pub fn on_scroll(mut self, map: impl Fn(Pointer) -> Msg + 'static) -> Self {
        self.on_scroll = Some(Box::new(map));
        self
    }
}

impl<Msg> From<Canvas<Msg>> for Element<Msg> {
    fn from(canvas: Canvas<Msg>) -> Self {
        Element::new(Kind::Canvas { node: canvas.node, on_pointer: canvas.on_pointer, on_scroll: canvas.on_scroll })
    }
}
