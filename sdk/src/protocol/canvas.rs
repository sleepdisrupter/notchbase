//! Canvas shapes, paints and pointer events.

use serde::{Deserialize, Serialize};

use super::node::{HandlerId, ImageSource};
use super::style::{Color, Icon, Tone};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct CanvasNode {
    pub width: f32,
    pub height: f32,
    #[serde(default)]
    pub stretch: bool,
    #[serde(default)]
    pub shapes: Vec<Shape>,
    #[serde(default)]
    pub on_pointer: Option<HandlerId>,
    #[serde(default)]
    pub on_scroll: Option<HandlerId>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Shape {
    Rect {
        rect: [f32; 4],
        #[serde(default)]
        radius: f32,
        #[serde(default)]
        fill: Option<Paint>,
        #[serde(default)]
        stroke: Option<Stroke>,
    },
    Ellipse {
        center: [f32; 2],
        radius: [f32; 2],
        #[serde(default)]
        fill: Option<Paint>,
        #[serde(default)]
        stroke: Option<Stroke>,
    },
    Line {
        points: Vec<[f32; 2]>,
        stroke: Stroke,
    },
    Polygon {
        points: Vec<[f32; 2]>,
        #[serde(default)]
        fill: Option<Paint>,
        #[serde(default)]
        stroke: Option<Stroke>,
    },
    Text {
        at: [f32; 2],
        text: String,
        size: f32,
        #[serde(default)]
        weight: Weight,
        color: Color,
        #[serde(default)]
        anchor: Anchor,
        #[serde(default)]
        width: Option<f32>,
    },
    Icon {
        icon: Icon,
        center: [f32; 2],
        size: f32,
        color: Color,
    },
    Run {
        at: [f32; 2],
        parts: Vec<Shape>,
        #[serde(default)]
        gap: f32,
        #[serde(default)]
        anchor: Anchor,
    },
    Image {
        source: ImageSource,
        rect: [f32; 4],
        #[serde(default)]
        radius: f32,
    },
    Glass {
        rect: [f32; 4],
        #[serde(default)]
        radius: f32,
    },
    Shadow {
        rect: [f32; 4],
        #[serde(default)]
        radius: f32,
        blur: f32,
        color: Color,
    },
    Group {
        shapes: Vec<Shape>,
        #[serde(default = "whole")]
        opacity: f32,
        #[serde(default)]
        offset: [f32; 2],
        #[serde(default = "whole")]
        scale: f32,
        #[serde(default)]
        clip: Option<[f32; 4]>,
    },
}

fn whole() -> f32 {
    1.0
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Paint {
    Solid(Color),
    Linear { from: Color, to: Color, angle: f32 },
    Radial { inner: Color, outer: Color },
}

impl<C: Into<Color>> From<C> for Paint {
    fn from(color: C) -> Self {
        Paint::Solid(color.into())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Stroke {
    pub width: f32,
    pub color: Color,
}

impl Stroke {
    pub fn new(width: f32, color: impl Into<Color>) -> Self {
        Self { width, color: color.into() }
    }
}

impl Default for Stroke {
    fn default() -> Self {
        Self::new(1.0, Tone::Primary)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Weight {
    #[default]
    Regular,
    Semibold,
    Mono,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Anchor {
    #[default]
    TopLeft,
    Top,
    TopRight,
    Left,
    Center,
    Right,
    BottomLeft,
    Bottom,
    BottomRight,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PointerKind {
    Hover,
    Leave,
    Press,
    Drag,
    Release,
    Click,
    Scroll,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Pointer {
    pub kind: PointerKind,
    pub x: f32,
    pub y: f32,
    #[serde(default)]
    pub dx: f32,
    #[serde(default)]
    pub dy: f32,
}

impl Pointer {
    pub fn at(kind: PointerKind, x: f32, y: f32) -> Self {
        Self { kind, x, y, dx: 0.0, dy: 0.0 }
    }

    pub fn moved(kind: PointerKind, [x, y]: [f32; 2], [dx, dy]: [f32; 2]) -> Self {
        Self { kind, x, y, dx, dy }
    }

    pub fn within(&self, x: f32, y: f32, width: f32, height: f32) -> bool {
        (x..x + width).contains(&self.x) && (y..y + height).contains(&self.y)
    }

    pub fn near(&self, x: f32, y: f32, radius: f32) -> bool {
        (self.x - x).hypot(self.y - y) <= radius
    }
}
