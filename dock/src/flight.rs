//! Minimizing, drawn by the dock: the window pours down into its tile like liquid through a funnel, keeping its own
//! picture most of the way and melting into the dock's black glass only at the end; restoring is the mirror, the
//! liquid rising out of the tile and filling the window top first. The picture is a mesh of thin strips, each with
//! its own head start: the bottom leads and the top follows, so the sides bend into smooth curves, and every strip
//! arrives at exactly the same moment. One timed move, full speed on the first frame and a soft landing; nothing
//! ever turns back, overshoots or crawls.

use egui::{Color32, Rect, pos2};
use notchbase_paint::spring::{lerp, smoothstep};

/// How long the move takes unless told otherwise, in seconds.
pub const FLY: f32 = 0.4;
/// How many strips the picture bends in.
pub const STRIPS: usize = 32;
/// How far behind the first strip the last one starts, as a part of the move.
const SPREAD: f32 = 0.45;
/// macOS windows' corner radius.
const WINDOW_CORNER: f32 = 12.0;
/// After the window is back in place, how long the picture takes to fade off it.
const REVEAL: f32 = 0.08;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Way {
    /// From the window down into its tile.
    Down,
    /// From the tile back up to the window.
    Up,
}

pub struct Flight {
    pub way: Way,
    pub id: u32,
    /// The window's frame, global top-left points.
    pub window: Rect,
    /// The tile's icon, global top-left points; it moves while the tile slides into place.
    pub tile: Rect,
    /// The tile's corner radius.
    pub tile_corner: f32,
    /// Seconds into the move.
    t: f32,
    /// Frames still to show at the start pose before the clock runs (see `step`).
    held: u8,
    /// How long the whole move takes, in seconds.
    fly: f32,
    /// Going up: the picture fading off the real window once it's back.
    reveal: f32,
    /// Going up: the real window is back in place under the picture.
    pub revealing: bool,
    pub picture: Option<egui::ColorImage>,
    pub texture: Option<egui::TextureHandle>,
    /// Going up: set once the real window is back in place under the picture.
    pub ready: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
    /// Going up: set once the picture is over the window's place, so the window can be put back under it.
    pub landed: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
    /// Seconds since it arrived.
    waited: f32,
    /// Its steps, for the log of flights.
    pub trace: Option<crate::flow::Trace>,
}

/// Frames shown at the start pose before a flight's clock runs: the one that uploads its picture, and the one after.
const HELD_FRAMES: u8 = 2;
/// The most one frame moves a flight on, in seconds: more than a 60 Hz frame, less than a hitch.
const LONGEST_STEP: f32 = 0.025;

/// Full speed at once, then a soft landing that ends exactly at 1.
fn ease(x: f32) -> f32 {
    1.0 - (1.0 - x.clamp(0.0, 1.0)).powi(3)
}

/// One edge across the picture: its height, and its left and right ends.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Edge {
    pub y: f32,
    pub left: f32,
    pub right: f32,
}

impl Flight {
    pub fn new(way: Way, id: u32, window: Rect, tile: Rect, picture: Option<egui::ColorImage>) -> Self {
        Self {
            way,
            id,
            window,
            tile,
            tile_corner: tile.width() * 0.225,
            t: 0.0,
            held: HELD_FRAMES,
            fly: FLY,
            reveal: 0.0,
            revealing: false,
            picture,
            texture: None,
            ready: None,
            landed: None,
            waited: 0.0,
            trace: None,
        }
    }

    /// The same move over `seconds` instead (within reason).
    pub fn lasting(mut self, seconds: f32) -> Self {
        self.fly = seconds.clamp(0.15, 2.0);
        self
    }

    /// Moves the clock on by `dt`. The first frames of a flight are slow ones (its picture goes up to the graphics
    /// card, the screen's bottom is captured for the cover): they show the start pose, so the move begins from rest
    /// instead of leaping ahead to catch up. After that no frame moves it on more than `LONGEST_STEP`, so a frame
    /// that comes late slows it a touch rather than making it skip.
    pub fn step(&mut self, dt: f32) {
        if self.held > 0 {
            self.held -= 1;
            return;
        }
        let dt = dt.min(LONGEST_STEP);
        if self.arrived() {
            self.waited += dt;
        }
        self.t = (self.t + dt).min(self.fly);
        if self.revealing {
            self.reveal += dt;
        }
    }

    /// The move has reached its end: down in the tile, or up over the window.
    pub fn arrived(&self) -> bool {
        self.t >= self.fly
    }

    /// Seconds since it arrived (waiting there for the window, coming up).
    pub fn waited(&self) -> f32 {
        self.waited
    }

    pub fn finished(&self) -> bool {
        match self.way {
            Way::Down => self.arrived(),
            Way::Up => self.reveal >= REVEAL,
        }
    }

    /// How far toward the tile the edge `v` down the picture is (0 at the window, 1 in the tile). Going down the
    /// bottom leads; coming up the top does. Every edge starts moving at once and gets there at the same time.
    pub fn along(&self, v: f32) -> f32 {
        let x = self.t / self.fly;
        let lead = match self.way {
            Way::Down => 1.0 - v,
            Way::Up => v,
        };
        // Each edge's own clock: a later edge holds back early on and catches up, so they all land together.
        // Held back mostly toward the far end, so the sides bend into a funnel rather than a straight taper.
        let own = x.clamp(0.0, 1.0).powf(1.0 + SPREAD * lead * (2.0 - lead) * 2.0);
        let k = ease(own);
        if self.way == Way::Down { k } else { 1.0 - k }
    }

    /// Edge `v` (0 top, 1 bottom) of the picture now.
    pub fn edge(&self, v: f32) -> Edge {
        let k = self.along(v);
        let (w, t) = (self.window, self.tile);
        let y = lerp(w.top() + v * w.height(), t.top() + v * t.height(), k);
        // It narrows a little ahead of where it is, so the sides curve in like a funnel.
        let width = lerp(w.width(), t.width(), 1.0 - (1.0 - k).powf(1.6));
        // Side to side trails the drop, so its path curves into the tile.
        let x = lerp(w.center().x, t.center().x, k.powf(1.35));
        Edge { y, left: x - width * 0.5, right: x + width * 0.5 }
    }

    /// The picture's edges, top to bottom.
    pub fn edges(&self) -> Vec<Edge> {
        (0..=STRIPS).map(|n| self.edge(n as f32 / STRIPS as f32)).collect()
    }

    /// The box around it all now.
    pub fn shape(&self) -> Rect {
        let edges = self.edges();
        let left = edges.iter().map(|e| e.left).fold(f32::MAX, f32::min);
        let right = edges.iter().map(|e| e.right).fold(f32::MIN, f32::max);
        let top = edges.first().map_or(0.0, |e| e.y);
        let bottom = edges.last().map_or(0.0, |e| e.y);
        Rect::from_min_max(pos2(left, top), pos2(right.max(left), bottom.max(top)))
    }

    /// The top edge's way along, 0 at the window, 1 in the tile: last into the tile, first out of it.
    fn progress(&self) -> f32 {
        self.along(0.0)
    }

    pub fn corner(&self) -> f32 {
        let r = lerp(WINDOW_CORNER, self.tile_corner, smoothstep(0.3, 1.0, self.progress()));
        let s = self.shape();
        r.min(s.width().min(s.height()) * 0.5).max(0.0)
    }

    /// The picture, bent along its edges: one mesh whose strips share their edges, so no seams, with a feathered
    /// rim along the curved sides so they stay smooth.
    pub fn paint(&self, painter: &egui::Painter, texture: egui::TextureId, alpha: f32) {
        let color = Color32::WHITE.gamma_multiply(alpha);
        let clear = Color32::TRANSPARENT;
        // Half a pixel each side of the true edge.
        let feather = 0.5 / painter.ctx().pixels_per_point();
        let mut mesh = egui::Mesh::with_texture(texture);
        for (n, e) in self.edges().iter().enumerate() {
            let v = n as f32 / STRIPS as f32;
            let vertex = |x: f32, u: f32, color| egui::epaint::Vertex { pos: pos2(x, e.y), uv: pos2(u, v), color };
            mesh.vertices.push(vertex(e.left - feather, 0.0, clear));
            mesh.vertices.push(vertex(e.left + feather, 0.0, color));
            mesh.vertices.push(vertex(e.right - feather, 1.0, color));
            mesh.vertices.push(vertex(e.right + feather, 1.0, clear));
            if n > 0 {
                let a = (n as u32 - 1) * 4;
                let b = a + 4;
                for c in 0..3 {
                    mesh.add_triangle(a + c, a + c + 1, b + c);
                    mesh.add_triangle(a + c + 1, b + c + 1, b + c);
                }
            }
        }
        painter.add(egui::Shape::mesh(mesh));
    }

    /// The liquid under the picture: bands following its bend, melted together, and at the very end a faint neck
    /// joining it to the tile.
    pub fn blobs(&self) -> Vec<crate::liquid::Blob> {
        let edges = self.edges();
        let k = self.progress();
        // At the window the glass hides just inside the picture, so it can't peek out at its rounded corners.
        let inset = 3.0 * (1.0 - smoothstep(0.0, 0.15, k));
        let corner = self.corner();
        let mut out = Vec::new();
        for band in edges.chunks(STRIPS / 8 + 1).collect::<Vec<_>>().windows(1) {
            let band = band[0];
            let (Some(first), Some(last)) = (band.first(), band.last()) else { continue };
            let left = band.iter().map(|e| e.left).fold(f32::MAX, f32::min);
            let right = band.iter().map(|e| e.right).fold(f32::MIN, f32::max);
            let rect = Rect::from_min_max(pos2(left, first.y), pos2(right, last.y)).shrink(inset);
            if rect.is_positive() {
                out.push(crate::liquid::Blob::rect(rect, corner.min(rect.height() * 0.5), 10.0));
            }
        }
        let body = self.shape();
        let reach = smoothstep(0.6, 0.88, k) * (1.0 - smoothstep(0.95, 1.0, k));
        let gap = self.tile.center().y - body.max.y;
        if reach > 0.02 && gap > 1.0 {
            let width = (self.tile.width() * 0.34 * reach).max(2.0);
            let x = lerp(body.center().x, self.tile.center().x, 0.5);
            let neck = Rect::from_min_max(
                pos2(x - width * 0.5, body.max.y - 4.0),
                pos2(x + width * 0.5, self.tile.center().y),
            );
            out.push(crate::liquid::Blob::rect(neck, width * 0.5, 10.0 * reach));
        }
        out
    }

    /// How much of the window's picture shows over the glass: all of it nearly all the way; it melts into the glass
    /// only by the tile (and coming out of the tile it shows almost at once).
    pub fn picture_alpha(&self) -> f32 {
        let fade = 1.0 - (self.reveal / REVEAL).clamp(0.0, 1.0);
        match self.way {
            Way::Down => (1.0 - smoothstep(0.8, 0.96, self.progress())) * fade,
            // Coming up, the tile's own picture lifts straight out of it: no black on the way.
            Way::Up => fade,
        }
    }

    /// How much of the dock's black glass shows under the picture: only toward the tile, where the picture melts into
    /// it (or all the way with no picture); gone once the real window is back under it, so the picture fades
    /// straight onto the window and never darkens it.
    pub fn glass_alpha(&self) -> f32 {
        if self.revealing {
            return 0.0;
        }
        if self.picture.is_none() && self.texture.is_none() {
            return 1.0;
        }
        match self.way {
            Way::Down => smoothstep(0.5, 0.8, self.progress()),
            Way::Up => 0.0,
        }
    }

    /// Where the middle of the bottom edge is.
    #[cfg(test)]
    pub fn tip(&self) -> egui::Pos2 {
        let e = self.edge(1.0);
        pos2((e.left + e.right) * 0.5, e.y)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flight(way: Way) -> Flight {
        let window = Rect::from_min_max(pos2(200.0, 100.0), pos2(1000.0, 700.0));
        let tile = Rect::from_min_max(pos2(1100.0, 900.0), pos2(1148.0, 948.0));
        Flight::new(way, 2, window, tile, None)
    }

    /// Every frame of the move, after the held ones at the start.
    fn fly(f: &mut Flight) -> Vec<Vec<Edge>> {
        for _ in 0..HELD_FRAMES {
            f.step(1.0 / 120.0);
        }
        let mut frames = vec![f.edges()];
        while !f.arrived() {
            f.step(1.0 / 120.0);
            frames.push(f.edges());
        }
        frames
    }

    /// Every strip's edge only ever goes one way, and never crosses the one below it.
    fn one_way(frames: &[Vec<Edge>]) {
        for n in 0..=STRIPS {
            let first = frames[0][n];
            let last = frames[frames.len() - 1][n];
            let dy = (last.y - first.y).signum();
            let dw = ((last.right - last.left) - (first.right - first.left)).signum();
            let dx = ((last.left + last.right) - (first.left + first.right)).signum();
            for pair in frames.windows(2) {
                let (a, b) = (pair[0][n], pair[1][n]);
                assert!((b.y - a.y) * dy >= -0.01, "edge {n} turned back up or down");
                assert!(((b.right - b.left) - (a.right - a.left)) * dw >= -0.01, "edge {n} turned back in size");
                assert!(((b.left + b.right) - (a.left + a.right)) * dx >= -0.01, "edge {n} turned back across");
            }
        }
        for f in frames {
            for pair in f.windows(2) {
                assert!(pair[1].y >= pair[0].y - 0.01, "strips never fold over");
            }
        }
    }

    #[test]
    fn a_slow_first_frame_never_makes_it_leap() {
        // What restores did before: the frame that uploads the picture took 30 to 48 ms, and the next one leapt
        // four or five frames' worth up the screen.
        for way in [Way::Up, Way::Down] {
            let mut f = flight(way);
            let start = f.shape();
            for dt in [0.008, 0.048] {
                f.step(dt);
                assert_eq!(f.shape(), start, "the slow first frames show the start pose");
            }
            let mut last = f.shape().center();
            let mut steps = Vec::new();
            for dt in [0.0083, 0.0083, 0.045, 0.0083, 0.0083, 0.0083] {
                f.step(dt);
                let now = f.shape().center();
                steps.push((now - last).length());
                last = now;
            }
            assert!(steps[0] > 0.0, "then it moves at once");
            // Going up it speeds up early on, so the late frame is held against the frames either side of it.
            let normal = steps[1].max(steps[3]);
            assert!(steps[2] < normal * 3.3, "a late frame moves it about three frames' worth at most: {steps:?}");
        }
    }

    #[test]
    fn down_pours_into_the_tile_in_one_move() {
        let mut f = flight(Way::Down);
        assert_eq!(f.shape(), f.window, "it starts exactly over the window");
        for _ in 0..=HELD_FRAMES {
            f.step(1.0 / 120.0);
        }
        assert!(f.tip().y - f.window.bottom() > 1.0, "already on its way after one frame");
        let mut f = flight(Way::Down);
        let frames = fly(&mut f);
        assert!((48..=50).contains(&frames.len()), "about half a second: {}", frames.len());
        assert!((f.shape().min - f.tile.min).length() < 0.5 && (f.shape().max - f.tile.max).length() < 0.5);
        one_way(&frames);
    }

    #[test]
    fn it_bends_like_liquid_and_its_path_curves() {
        let mut f = flight(Way::Down);
        for _ in 0..18 + HELD_FRAMES {
            f.step(1.0 / 120.0);
        }
        let (top, mid, bottom) = (f.edge(0.0), f.edge(0.5), f.edge(1.0));
        let width = |e: Edge| e.right - e.left;
        assert!(width(bottom) < width(mid) && width(mid) < width(top), "a funnel, narrowest at the bottom");
        // The sides curve: the middle isn't halfway between top and bottom.
        let straight = (width(top) + width(bottom)) * 0.5;
        assert!((width(mid) - straight).abs() > 4.0, "the sides bend ({} vs {straight})", width(mid));
        // The tip drops further than it slides across.
        let down = (f.tip().y - f.window.bottom()) / (f.tile.bottom() - f.window.bottom());
        let across = (f.tip().x - f.window.center().x) / (f.tile.center().x - f.window.center().x);
        assert!(down > across + 0.03, "the path curves ({down} vs {across})");
    }

    #[test]
    fn up_is_the_mirror_and_reveals_without_darkening() {
        let mut up = flight(Way::Up);
        assert!((up.shape().center() - up.tile.center()).length() < 0.5, "it starts in the tile");
        let frames = fly(&mut up);
        assert!((48..=50).contains(&frames.len()), "about half a second: {}", frames.len());
        assert!((up.shape().min - up.window.min).length() < 0.5 && (up.shape().max - up.window.max).length() < 0.5);
        one_way(&frames);
        let mut early = flight(Way::Up);
        early.picture = Some(egui::ColorImage::new([1, 1], vec![Color32::WHITE]));
        for _ in 0..20 + HELD_FRAMES {
            early.step(1.0 / 120.0);
        }
        assert!(early.picture_alpha() > 0.99, "its own picture, not black glass, early on");
        assert!(early.glass_alpha() < 0.01, "no glass under it once it's out");
        let start = flight(Way::Up);
        let mut start = start;
        start.picture = Some(egui::ColorImage::new([1, 1], vec![Color32::WHITE]));
        assert!(start.picture_alpha() > 0.99 && start.glass_alpha() < 0.01, "no black blob at the tile");
        up.revealing = true;
        assert_eq!(up.glass_alpha(), 0.0, "no black under the picture as it fades onto the window");
    }

    #[test]
    fn down_melts_into_the_glass_only_at_the_end() {
        let mut f = flight(Way::Down);
        f.picture = Some(egui::ColorImage::new([1, 1], vec![Color32::WHITE]));
        let (mut shown, mut frames) = (0, 0);
        while !f.arrived() {
            f.step(1.0 / 120.0);
            frames += 1;
            shown += usize::from(f.picture_alpha() > 0.99);
        }
        assert!(shown * 2 > frames, "its own picture most of the way ({shown} of {frames})");
        assert!(f.glass_alpha() > 0.99, "black glass in the tile");
    }
}

#[cfg(test)]
mod sketch {
    use super::*;

    /// Draws the outline every 40 ms, for a look by eye: `NOTCHBASE_DOCK_SKETCH=out.svg cargo test sketch`.
    #[test]
    fn sketch() {
        let Some(out) = std::env::var_os("NOTCHBASE_DOCK_SKETCH") else { return };
        let mut svg = String::from(
            "<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 1470 960' width='1470' height='960'><rect width='1470' height='960' fill='#fff'/>",
        );
        for way in [Way::Down] {
            let mut f = Flight::new(
                way,
                1,
                Rect::from_min_max(pos2(80.0, 60.0), pos2(1000.0, 640.0)),
                Rect::from_min_max(pos2(1100.0, 900.0), pos2(1148.0, 948.0)),
                None,
            );
            let mut n = 0;
            while !f.arrived() {
                if n % 5 == 0 {
                    let e = f.edges();
                    let mut pts: Vec<String> = e.iter().map(|e| format!("{},{}", e.left, e.y)).collect();
                    pts.extend(e.iter().rev().map(|e| format!("{},{}", e.right, e.y)));
                    svg +=
                        &format!("<polygon points='{}' fill='none' stroke='#000' stroke-width='1.5'/>", pts.join(" "));
                }
                f.step(1.0 / 120.0);
                n += 1;
            }
        }
        svg += "</svg>";
        std::fs::write(out, svg).unwrap();
    }
}
