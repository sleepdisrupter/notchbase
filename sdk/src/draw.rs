//! Shapes for `canvas` elements, with builder methods for fill, stroke, text and transforms.

use std::f32::consts::PI;

use crate::protocol::{Anchor, Color, Icon, ImageSource, Paint, Shape, Stroke, Tone, Weight};

const ARC_STEP: f32 = PI / 48.0;
const CURVE_STEPS: usize = 24;

pub fn rect(x: f32, y: f32, width: f32, height: f32) -> Shape {
    Shape::Rect { rect: [x, y, width, height], radius: 0.0, fill: None, stroke: None }
}

pub fn circle(x: f32, y: f32, radius: f32) -> Shape {
    ellipse(x, y, radius, radius)
}

pub fn ellipse(x: f32, y: f32, across: f32, down: f32) -> Shape {
    Shape::Ellipse { center: [x, y], radius: [across, down], fill: None, stroke: None }
}

pub fn line(points: impl IntoIterator<Item = (f32, f32)>) -> Shape {
    Shape::Line { points: corners(points), stroke: Stroke::default() }
}

pub fn polygon(points: impl IntoIterator<Item = (f32, f32)>) -> Shape {
    Shape::Polygon { points: corners(points), fill: None, stroke: None }
}

pub fn text(text: impl Into<String>, x: f32, y: f32) -> Shape {
    Shape::Text {
        at: [x, y],
        text: text.into(),
        size: 13.0,
        weight: Weight::Regular,
        color: Tone::Primary.into(),
        anchor: Anchor::TopLeft,
        width: None,
    }
}

pub fn icon(icon: Icon, x: f32, y: f32) -> Shape {
    Shape::Icon { icon, center: [x, y], size: 15.0, color: Tone::Primary.into() }
}

pub fn run(parts: impl IntoIterator<Item = Shape>, x: f32, y: f32) -> Shape {
    Shape::Run { at: [x, y], parts: parts.into_iter().collect(), gap: 4.0, anchor: Anchor::TopLeft }
}

pub fn image(source: ImageSource, x: f32, y: f32, width: f32, height: f32) -> Shape {
    Shape::Image { source, rect: [x, y, width, height], radius: 0.0 }
}

pub fn glass(x: f32, y: f32, width: f32, height: f32) -> Shape {
    Shape::Glass { rect: [x, y, width, height], radius: 0.0 }
}

pub fn shadow(x: f32, y: f32, width: f32, height: f32) -> Shape {
    Shape::Shadow { rect: [x, y, width, height], radius: 0.0, blur: 12.0, color: Color::rgba(0, 0, 0, 160) }
}

pub fn group(shapes: impl IntoIterator<Item = Shape>) -> Shape {
    Shape::Group { shapes: shapes.into_iter().collect(), opacity: 1.0, offset: [0.0, 0.0], scale: 1.0, clip: None }
}

pub fn linear(from: impl Into<Color>, to: impl Into<Color>, angle: f32) -> Paint {
    Paint::Linear { from: from.into(), to: to.into(), angle }
}

pub fn radial(inner: impl Into<Color>, outer: impl Into<Color>) -> Paint {
    Paint::Radial { inner: inner.into(), outer: outer.into() }
}

pub fn arc(x: f32, y: f32, radius: f32, from: f32, to: f32) -> Vec<(f32, f32)> {
    let (from, to) = (from.to_radians(), to.to_radians());
    let steps = ((to - from).abs() / ARC_STEP).ceil().max(1.0) as usize;
    let along = |step: usize| from + (to - from) * step as f32 / steps as f32;
    (0..=steps).map(|step| (x + radius * along(step).cos(), y + radius * along(step).sin())).collect()
}

pub fn curve(from: (f32, f32), pull: (f32, f32), push: (f32, f32), to: (f32, f32)) -> Vec<(f32, f32)> {
    let blend = |t: f32, [a, b, c, d]: [f32; 4]| {
        let u = 1.0 - t;
        u * u * u * a + 3.0 * u * u * t * b + 3.0 * u * t * t * c + t * t * t * d
    };
    let at = |step: usize| {
        let t = step as f32 / CURVE_STEPS as f32;
        (blend(t, [from.0, pull.0, push.0, to.0]), blend(t, [from.1, pull.1, push.1, to.1]))
    };
    (0..=CURVE_STEPS).map(at).collect()
}

fn corners(points: impl IntoIterator<Item = (f32, f32)>) -> Vec<[f32; 2]> {
    points.into_iter().map(|(x, y)| [x, y]).collect()
}

impl Shape {
    pub fn fill(mut self, paint: impl Into<Paint>) -> Self {
        if let Shape::Rect { fill, .. } | Shape::Ellipse { fill, .. } | Shape::Polygon { fill, .. } = &mut self {
            *fill = Some(paint.into());
        }
        self
    }

    pub fn stroke(mut self, width: f32, color: impl Into<Color>) -> Self {
        let new = Stroke::new(width, color);
        match &mut self {
            Shape::Rect { stroke, .. } | Shape::Ellipse { stroke, .. } | Shape::Polygon { stroke, .. } => {
                *stroke = Some(new);
            }
            Shape::Line { stroke, .. } => *stroke = new,
            _ => {}
        }
        self
    }

    pub fn radius(mut self, to: f32) -> Self {
        if let Shape::Rect { radius, .. }
        | Shape::Image { radius, .. }
        | Shape::Glass { radius, .. }
        | Shape::Shadow { radius, .. } = &mut self
        {
            *radius = to;
        }
        self
    }

    pub fn size(mut self, to: f32) -> Self {
        if let Shape::Text { size, .. } | Shape::Icon { size, .. } = &mut self {
            *size = to;
        }
        self
    }

    pub fn color(mut self, to: impl Into<Color>) -> Self {
        if let Shape::Text { color, .. } | Shape::Icon { color, .. } | Shape::Shadow { color, .. } = &mut self {
            *color = to.into();
        }
        self
    }

    pub fn weight(mut self, to: Weight) -> Self {
        if let Shape::Text { weight, .. } = &mut self {
            *weight = to;
        }
        self
    }

    pub fn semibold(self) -> Self {
        self.weight(Weight::Semibold)
    }

    pub fn mono(self) -> Self {
        self.weight(Weight::Mono)
    }

    pub fn anchor(mut self, to: Anchor) -> Self {
        if let Shape::Text { anchor, .. } | Shape::Run { anchor, .. } = &mut self {
            *anchor = to;
        }
        self
    }

    pub fn gap(mut self, to: f32) -> Self {
        if let Shape::Run { gap, .. } = &mut self {
            *gap = to;
        }
        self
    }

    pub fn centered(self) -> Self {
        self.anchor(Anchor::Center)
    }

    pub fn wrap(mut self, within: f32) -> Self {
        if let Shape::Text { width, .. } = &mut self {
            *width = Some(within);
        }
        self
    }

    pub fn blur(mut self, to: f32) -> Self {
        if let Shape::Shadow { blur, .. } = &mut self {
            *blur = to;
        }
        self
    }

    pub fn opacity(self, to: f32) -> Self {
        let mut group = self.grouped();
        if let Shape::Group { opacity, .. } = &mut group {
            *opacity = to.clamp(0.0, 1.0);
        }
        group
    }

    pub fn offset(self, x: f32, y: f32) -> Self {
        let mut group = self.grouped();
        if let Shape::Group { offset, .. } = &mut group {
            *offset = [x, y];
        }
        group
    }

    pub fn scale(self, by: f32) -> Self {
        let mut group = self.grouped();
        if let Shape::Group { scale, .. } = &mut group {
            *scale = by;
        }
        group
    }

    pub fn clip(self, x: f32, y: f32, width: f32, height: f32) -> Self {
        let mut group = self.grouped();
        if let Shape::Group { clip, .. } = &mut group {
            *clip = Some([x, y, width, height]);
        }
        group
    }

    fn grouped(self) -> Self {
        match self {
            group @ Shape::Group { .. } => group,
            other => group([other]),
        }
    }
}
