//! The launch effect: the icon's black liquid flows out in one continuous motion into the app's window. A rounded
//! box grows from the icon to the window's frame, its corners easing from the icon's to the window's, and fades as
//! it lands so the window shows through. It aims at the window's frame from last time (or a likely one) and, when
//! the real window shows, steers onto it mid-flight without restarting. Drawn with the dock's own liquid.

use egui::{Rect, pos2};

use crate::liquid::Blob;

/// macOS windows' corner radius, which the shape opens into.
const WINDOW_CORNER: f32 = 16.0;
/// With no window by then, it fades out where it is.
const MAX_WAIT: f32 = 2.0;

/// The full flight to a new window, or a quicker one for an app that's already open.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Reach {
    Launch,
    Switch,
}

impl Reach {
    /// (the spring's response, its damping, how long the last fade takes)
    fn timing(self) -> (f32, f32, f32) {
        match self {
            Reach::Launch => (0.3, 0.9, 0.12),
            Reach::Switch => (0.24, 0.92, 0.1),
        }
    }
}

pub struct Pop {
    pub reach: Reach,
    /// The icon, in global top-left points.
    pub icon: Rect,
    /// Where it's flying: the real window once known, a likely frame until then.
    pub target: Rect,
    known: bool,
    /// The shape's edges (left, top, right, bottom) and their velocities.
    edges: [f32; 4],
    velocity: [f32; 4],
    /// What the edges aim at. It glides to a newly found window instead of jumping, so the flight bends smoothly.
    aim: [f32; 4],
    aim_velocity: [f32; 4],
    clock: f32,
    alpha: f32,
}

impl Pop {
    /// `target` is the window, or a likely frame for it when `known` is false.
    pub fn new(reach: Reach, icon: Rect, target: Rect, known: bool) -> Self {
        Self {
            reach,
            icon,
            target,
            known,
            edges: [icon.min.x, icon.min.y, icon.max.x, icon.max.y],
            velocity: [0.0; 4],
            aim: [target.min.x, target.min.y, target.max.x, target.max.y],
            aim_velocity: [0.0; 4],
            clock: 0.0,
            alpha: 1.0,
        }
    }

    pub fn finished(&self) -> bool {
        self.alpha <= 0.0
    }

    pub fn knows_window(&self) -> bool {
        self.known
    }

    /// The real window showed up, or moved while it settles. The flight steers onto it smoothly.
    pub fn window_at(&mut self, frame: Rect) {
        self.known = true;
        self.target = frame;
    }

    fn shape(&self) -> Rect {
        Rect::from_min_max(pos2(self.edges[0], self.edges[1]), pos2(self.edges[2], self.edges[3]))
    }

    /// How far along it is, 0 at the icon and 1 at the window, by size.
    fn progress(&self) -> f32 {
        let (from, to, now) = (self.icon.size().length(), self.target.size().length(), self.shape().size().length());
        if (to - from).abs() < 1.0 { 1.0 } else { ((now - from) / (to - from)).clamp(0.0, 1.0) }
    }

    pub fn step(&mut self, dt: f32) {
        let (response, damping, fade) = self.reach.timing();
        self.clock += dt;
        let wanted = [self.target.min.x, self.target.min.y, self.target.max.x, self.target.max.y];
        #[allow(clippy::needless_range_loop)] // four edges, each with its own aim and velocity
        for i in 0..4 {
            (self.aim[i], self.aim_velocity[i]) = spring(self.aim[i], self.aim_velocity[i], wanted[i], dt, 0.16, 1.0);
            (self.edges[i], self.velocity[i]) =
                spring(self.edges[i], self.velocity[i], self.aim[i], dt, response, damping);
        }
        let off = (0..4).map(|i| (self.edges[i] - wanted[i]).abs()).fold(0.0, f32::max);
        let moving = self.velocity.iter().map(|v| v.abs()).fold(0.0, f32::max);
        let landed = off < 2.0 && moving < 60.0;
        if (self.known && landed) || self.clock > MAX_WAIT {
            self.alpha = (self.alpha - dt / fade).max(0.0);
        }
    }

    /// How opaque it is: solid as it leaves the icon, easing off as it fills the window so the window shows through,
    /// then gone.
    pub fn alpha(&self) -> f32 {
        let a = self.alpha.clamp(0.0, 1.0);
        let through = 1.0 - 0.45 * smoothstep(0.6, 1.0, self.progress());
        a * a * (3.0 - 2.0 * a) * through
    }

    /// The liquid for this moment, in global top-left points.
    pub fn blobs(&self) -> Vec<Blob> {
        let t = self.progress();
        let shape = self.shape();
        let icon_corner = self.icon.width() * 0.23;
        let radius = lerp(icon_corner, WINDOW_CORNER, smoothstep(0.0, 0.5, t));
        let mut blobs = Vec::with_capacity(2);
        // For the first moment it's still joined to the icon, so it pours out rather than appearing.
        let cling = 1.0 - smoothstep(0.0, 0.25, t);
        if cling > 0.0 {
            blobs.push(Blob::rect(self.icon.shrink(self.icon.width() * 0.1 * (1.0 - cling)), icon_corner, 0.0));
        }
        blobs.push(Blob::rect(shape, radius, 24.0 * cling));
        blobs
    }
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// One step of a damped spring, exact for any `dt`.
pub fn spring(x: f32, v: f32, target: f32, dt: f32, response: f32, damping: f32) -> (f32, f32) {
    let omega = std::f32::consts::TAU / response.max(1e-3);
    let x0 = x - target;
    if damping < 1.0 {
        let wd = omega * (1.0 - damping * damping).sqrt();
        let b = (v + damping * omega * x0) / wd;
        let e = (-damping * omega * dt).exp();
        let (s, c) = (wd * dt).sin_cos();
        let xn = e * (x0 * c + b * s);
        let vn = e * ((b * wd - damping * omega * x0) * c - (x0 * wd + damping * omega * b) * s);
        (target + xn, vn)
    } else {
        let e = (-omega * dt).exp();
        let b = v + omega * x0;
        ((x0 + b * dt) * e + target, (b - omega * (x0 + b * dt)) * e)
    }
}

#[cfg(test)]
mod tests {
    use egui::vec2;

    use super::*;

    const DT: f32 = 1.0 / 120.0;

    fn icon() -> Rect {
        Rect::from_center_size(pos2(600.0, 860.0), vec2(48.0, 48.0))
    }

    fn window() -> Rect {
        Rect::from_min_size(pos2(200.0, 100.0), vec2(800.0, 500.0))
    }

    /// Runs it to the end, returning how long it took, the last shape drawn, and the largest change in speed from one
    /// frame to the next (a jump or a stall shows up as a big one).
    fn run(mut pop: Pop) -> (f32, Rect, f32) {
        let (mut t, mut last, mut jump) = (0.0, Rect::NOTHING, 0.0f32);
        let mut moved: Option<[egui::Vec2; 2]> = None;
        while !pop.finished() && t < 5.0 {
            let shape = pop.blobs().last().unwrap().bounds();
            if last.is_positive() {
                let now = [shape.min - last.min, shape.max - last.max];
                if let Some(before) = moved {
                    jump = jump.max((now[0] - before[0]).length()).max((now[1] - before[1]).length());
                }
                moved = Some(now);
            }
            last = shape;
            pop.step(DT);
            t += DT;
        }
        (t, last, jump)
    }

    #[test]
    fn it_flows_from_the_icon_into_the_window_and_fades_quickly() {
        let mut pop = Pop::new(Reach::Launch, icon(), window(), true);
        let first = pop.blobs().last().unwrap().bounds();
        assert!((first.center() - icon().center()).length() < 1.0, "it starts on the icon");
        pop.step(DT);
        let (t, last, jump) = run(pop);
        assert!((0.3..=0.65).contains(&t), "about half a second: {t}");
        assert!((last.min - window().min).length() < 3.0 && (last.max - window().max).length() < 3.0, "{last:?}");
        assert!(jump < 12.0, "no frame jumps: {jump}");
    }

    #[test]
    fn switching_is_quicker() {
        let (launch, ..) = run(Pop::new(Reach::Launch, icon(), window(), true));
        let (switch, ..) = run(Pop::new(Reach::Switch, icon(), window(), true));
        assert!(switch < launch, "{switch} < {launch}");
    }

    #[test]
    fn a_window_that_shows_mid_flight_is_steered_onto_without_a_jump() {
        let guess = Rect::from_center_size(pos2(720.0, 420.0), vec2(860.0, 540.0));
        let mut pop = Pop::new(Reach::Launch, icon(), guess, false);
        for _ in 0..20 {
            pop.step(DT);
        }
        assert_eq!(pop.alpha, 1.0, "it doesn't fade on a guess");
        let real = Rect::from_min_size(pos2(80.0, 60.0), vec2(600.0, 400.0));
        pop.window_at(real);
        let (_, last, jump) = run(pop);
        assert!((last.min - real.min).length() < 3.0 && (last.max - real.max).length() < 3.0, "{last:?}");
        assert!(jump < 12.0, "no frame jumps: {jump}");
    }

    #[test]
    fn it_lets_go_when_no_window_comes() {
        let guess = Rect::from_center_size(pos2(720.0, 420.0), vec2(860.0, 540.0));
        let (t, ..) = run(Pop::new(Reach::Launch, icon(), guess, false));
        assert!(t < MAX_WAIT + 0.3, "{t}");
    }
}
