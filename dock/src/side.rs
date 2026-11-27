//! Which edge of the screen the dock stands on, and the one map that turns the dock's own layout into its window.
//!
//! The dock lays itself out as if it stood along the bottom: `x` runs along the edge, `y` runs down toward it, and
//! the edge is at the bottom of a window `depth` points tall. Along the bottom that is the window itself. On the left
//! or the right the same layout is turned on its side: `x` runs down the screen and `y` runs toward the side edge.
//! Everything that belongs to the bar (the body, the swells, droplets, dots, the separator, icons' places) goes
//! through [`Frame::pt`] and [`Frame::rect`]; what has to stay upright to be read (icons' pictures, words, badges,
//! menus and panels) is laid out on the window itself, beside where the frame says it belongs.

use egui::{Pos2, Rect, Vec2, pos2, vec2};
use serde::{Deserialize, Serialize};

use crate::liquid::Blob;

/// The edge the dock stands on.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Side {
    #[default]
    Bottom,
    Left,
    Right,
}

impl Side {
    pub fn vertical(self) -> bool {
        self != Side::Bottom
    }

    /// The name `dock.json`, `dock-areas.json` and the system Dock's `orientation` use.
    pub fn key(self) -> &'static str {
        match self {
            Side::Bottom => "bottom",
            Side::Left => "left",
            Side::Right => "right",
        }
    }

    /// A point (in any top-left coordinates) turned so that this side's edge of the screen is the bottom: `x` runs
    /// along the edge and `y` grows toward it.
    pub fn to_bottom(self, p: Pos2) -> Pos2 {
        match self {
            Side::Bottom => p,
            Side::Left => pos2(p.y, -p.x),
            Side::Right => pos2(p.y, p.x),
        }
    }

    /// A box turned the same way.
    pub fn rect_to_bottom(self, r: Rect) -> Rect {
        Rect::from_two_pos(self.to_bottom(r.min), self.to_bottom(r.max))
    }

    /// The dock's window on `screen` (any top-left coordinates): along the bottom `height` tall, across the whole
    /// screen; on a side the whole screen's height, [`SIDE_REACH`] wide at most.
    pub fn window_on(self, screen: Rect, height: f32) -> Rect {
        let reach = screen.width().min(SIDE_REACH);
        match self {
            Side::Bottom => Rect::from_min_max(pos2(screen.left(), screen.bottom() - height), screen.max),
            Side::Left => Rect::from_min_size(screen.min, vec2(reach, screen.height())),
            Side::Right => Rect::from_min_max(pos2(screen.right() - reach, screen.top()), screen.max),
        }
    }
}

/// The pointer over the separator: it sizes the dock by dragging toward or away from the edge, up and down along the
/// bottom and sideways on a side.
#[cfg(target_os = "macos")]
pub fn resize_cursor(over: bool, side: Side) -> winit::window::CursorIcon {
    use winit::window::CursorIcon;
    match (over, side.vertical()) {
        (false, _) => CursorIcon::Default,
        (true, false) => CursorIcon::NsResize,
        (true, true) => CursorIcon::EwResize,
    }
}

/// How far a side dock's window reaches into the screen, at most: room for menus, window previews and a stack's
/// grid beside it.
pub const SIDE_REACH: f32 = 960.0;

/// The dock's layout (along the edge, toward it) on its window.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Frame {
    pub side: Side,
    /// The window's size in points.
    pub window: Vec2,
}

impl Frame {
    pub fn new(side: Side, window: Vec2) -> Self {
        Self { side, window }
    }

    /// The layout's size: the edge's length across, and how far the window reaches in from it.
    pub fn size(&self) -> Vec2 {
        if self.side.vertical() { vec2(self.window.y, self.window.x) } else { self.window }
    }

    /// How deep the layout is: where the edge is, in its own `y`.
    fn depth(&self) -> f32 {
        self.size().y
    }

    /// A point of the layout, on the window.
    pub fn pt(&self, p: Pos2) -> Pos2 {
        match self.side {
            Side::Bottom => p,
            Side::Left => pos2(self.depth() - p.y, p.x),
            Side::Right => pos2(self.window.x - self.depth() + p.y, p.x),
        }
    }

    /// A point on the window, in the layout.
    pub fn unpt(&self, p: Pos2) -> Pos2 {
        match self.side {
            Side::Bottom => p,
            Side::Left => pos2(p.y, self.depth() - p.x),
            Side::Right => pos2(p.y, p.x - (self.window.x - self.depth())),
        }
    }

    /// A box of the layout, on the window: turned on its side for a side dock.
    pub fn rect(&self, r: Rect) -> Rect {
        if self.side == Side::Bottom {
            return r;
        }
        Rect::from_two_pos(self.pt(r.min), self.pt(r.max))
    }

    /// A box on the window, in the layout.
    pub fn unrect(&self, r: Rect) -> Rect {
        if self.side == Side::Bottom {
            return r;
        }
        Rect::from_two_pos(self.unpt(r.min), self.unpt(r.max))
    }

    /// A size of the layout, on the window.
    pub fn extent(&self, v: Vec2) -> Vec2 {
        if self.side.vertical() { vec2(v.y, v.x) } else { v }
    }

    /// A piece of liquid laid out along the bottom, on the window.
    pub fn blob(&self, b: Blob) -> Blob {
        match self.side {
            Side::Bottom => b,
            Side::Left => b.turned(self.pt(b.center), true),
            Side::Right => b.turned(self.pt(b.center), false),
        }
    }

    /// Something upright of `size` (a label, a menu, a panel) standing out from the dock: its middle across at
    /// `along`, its near side at `near` (both the layout's), kept `margin` inside the window along the edge. Along the
    /// bottom it sits above that point; on a side it sits beside it, vertically centred on it.
    pub fn beside(&self, along: f32, near: f32, size: Vec2, margin: f32) -> Rect {
        let at = self.pt(pos2(along, near));
        match self.side {
            Side::Bottom => {
                let half = size.x * 0.5 + margin;
                let x = at.x.clamp(half, (self.window.x - half).max(half));
                Rect::from_min_max(pos2(x - size.x * 0.5, at.y - size.y), pos2(x + size.x * 0.5, at.y))
            }
            Side::Left | Side::Right => {
                let half = size.y * 0.5 + margin;
                let y = at.y.clamp(half, (self.window.y - half).max(half));
                let x = if self.side == Side::Left { at.x } else { at.x - size.x };
                Rect::from_min_size(pos2(x, y - size.y * 0.5), size)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frames() -> [Frame; 3] {
        [
            Frame::new(Side::Bottom, vec2(1440.0, 680.0)),
            Frame::new(Side::Left, vec2(900.0, 956.0)),
            Frame::new(Side::Right, vec2(900.0, 956.0)),
        ]
    }

    #[test]
    fn points_go_there_and_back() {
        for f in frames() {
            for p in [pos2(10.0, 20.0), pos2(700.0, 880.0), pos2(-5.0, 3.0)] {
                let back = f.unpt(f.pt(p));
                assert!((back - p).length() < 1e-3, "{:?}: {p:?} came back as {back:?}", f.side);
            }
            let r = Rect::from_min_max(pos2(100.0, 400.0), pos2(160.0, 480.0));
            let back = f.unrect(f.rect(r));
            assert!((back.min - r.min).length() < 1e-3 && (back.max - r.max).length() < 1e-3);
        }
    }

    #[test]
    fn the_edge_lands_on_the_screens_edge_and_up_points_into_the_screen() {
        let [_, left, right] = frames();
        for f in [left, right] {
            let size = f.size();
            assert_eq!(size, vec2(956.0, 900.0), "the edge's length is the window's height");
            let on_edge = f.pt(pos2(300.0, size.y));
            let inward = f.pt(pos2(300.0, size.y - 50.0));
            let edge_x = if f.side == Side::Left { 0.0 } else { f.window.x };
            assert!((on_edge.x - edge_x).abs() < 1e-3, "{:?}: the edge is at the window's {edge_x}", f.side);
            assert!(((inward.x - on_edge.x).abs() - 50.0).abs() < 1e-3);
            assert!((inward.x - f.window.x * 0.5).abs() < (on_edge.x - f.window.x * 0.5).abs(), "up goes inward");
            assert_eq!(on_edge.y, 300.0, "along runs down the screen");
        }
    }

    #[test]
    fn upright_things_stand_beside_a_side_dock_and_above_a_bottom_one() {
        let [bottom, left, right] = frames();
        let size = vec2(200.0, 120.0);
        let r = bottom.beside(400.0, 600.0, size, 8.0);
        assert_eq!(r, Rect::from_min_max(pos2(300.0, 480.0), pos2(500.0, 600.0)));
        let near = left.size().y - 80.0;
        let r = left.beside(400.0, near, size, 8.0);
        assert_eq!(r.size(), size, "never turned");
        assert!((r.left() - 80.0).abs() < 1e-3 && (r.center().y - 400.0).abs() < 1e-3);
        let r = right.beside(400.0, near, size, 8.0);
        assert!((r.right() - (900.0 - 80.0)).abs() < 1e-3);
        let r = left.beside(10.0, near, size, 8.0);
        assert!((r.top() - 8.0).abs() < 1e-3, "kept inside the screen along the edge: {r:?}");
    }

    #[test]
    fn a_side_dock_takes_the_screens_height_and_a_reach_of_its_width() {
        let screen = Rect::from_min_size(pos2(-60.0, -1080.0), vec2(1920.0, 1080.0));
        assert_eq!(Side::Bottom.window_on(screen, 680.0), Rect::from_min_max(pos2(-60.0, -680.0), pos2(1860.0, 0.0)));
        assert_eq!(Side::Left.window_on(screen, 680.0), Rect::from_min_size(screen.min, vec2(SIDE_REACH, 1080.0)));
        assert_eq!(Side::Right.window_on(screen, 680.0).max, screen.max);
        assert_eq!(Side::Right.window_on(screen, 680.0).width(), SIDE_REACH);
        let narrow = Rect::from_min_size(Pos2::ZERO, vec2(800.0, 1280.0));
        assert_eq!(Side::Left.window_on(narrow, 680.0), narrow, "never past the screen");
    }

    #[test]
    fn turning_a_side_to_the_bottom_puts_its_edge_there() {
        let screen = Rect::from_min_size(pos2(0.0, 0.0), vec2(1470.0, 956.0));
        let left = Side::Left.rect_to_bottom(screen);
        assert_eq!(left.bottom(), 0.0, "the left edge is the bottom");
        assert_eq!(Side::Left.to_bottom(pos2(3.0, 500.0)), pos2(500.0, -3.0));
        assert!(left.contains(Side::Left.to_bottom(pos2(3.0, 500.0))));
        let right = Side::Right.rect_to_bottom(screen);
        assert_eq!(right.bottom(), 1470.0);
        assert_eq!(right.width(), 956.0, "along the edge runs down the screen");
    }
}
