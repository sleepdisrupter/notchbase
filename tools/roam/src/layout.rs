//! The desk: where each computer's screens sit in one shared 2D space, and how the cursor moves
//! from one computer to the next.
//!
//! Each computer reports its displays in its own units (points on a Mac, pixels on Windows) and
//! how many of those units make one point (`ppu`). The desk itself is laid out in points, so a
//! 4K Windows screen at 150% and a MacBook look the size they really are next to each other, and
//! the cursor moves at the same speed on both.

use serde::{Deserialize, Serialize};

/// A rectangle, `x`/`y` at the top left, `y` growing downwards.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl Rect {
    pub const fn new(x: f64, y: f64, w: f64, h: f64) -> Self {
        Self { x, y, w, h }
    }

    pub fn right(&self) -> f64 {
        self.x + self.w
    }

    pub fn bottom(&self) -> f64 {
        self.y + self.h
    }

    pub fn contains(&self, [x, y]: [f64; 2]) -> bool {
        x >= self.x && x < self.right() && y >= self.y && y < self.bottom()
    }

    /// The nearest point inside, at least `inset` from the far edges.
    pub fn clamp(&self, [x, y]: [f64; 2]) -> [f64; 2] {
        let last_x = (self.right() - 1.0).max(self.x);
        let last_y = (self.bottom() - 1.0).max(self.y);
        [x.clamp(self.x, last_x), y.clamp(self.y, last_y)]
    }

    fn distance(&self, point: [f64; 2]) -> f64 {
        let [cx, cy] = self.clamp(point);
        (point[0] - cx).hypot(point[1] - cy)
    }

    pub fn overlaps(&self, other: &Rect) -> bool {
        self.x < other.right() && other.x < self.right() && self.y < other.bottom() && other.y < self.bottom()
    }
}

/// The bounding box of a set of displays.
pub fn bounds(displays: &[Rect]) -> Rect {
    let Some(first) = displays.first() else { return Rect::new(0.0, 0.0, 1.0, 1.0) };
    let mut left = first.x;
    let mut top = first.y;
    let mut right = first.right();
    let mut bottom = first.bottom();
    for display in &displays[1..] {
        left = left.min(display.x);
        top = top.min(display.y);
        right = right.max(display.right());
        bottom = bottom.max(display.bottom());
    }
    Rect::new(left, top, (right - left).max(1.0), (bottom - top).max(1.0))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Edge {
    Left,
    Right,
    Top,
    Bottom,
}

impl Edge {
    pub(crate) fn step(self) -> [f64; 2] {
        match self {
            Edge::Left => [-1.0, 0.0],
            Edge::Right => [1.0, 0.0],
            Edge::Top => [0.0, -1.0],
            Edge::Bottom => [0.0, 1.0],
        }
    }

    pub fn horizontal(self) -> bool {
        matches!(self, Edge::Left | Edge::Right)
    }

    /// The edge facing this one across a crossing.
    pub fn opposite(self) -> Edge {
        match self {
            Edge::Left => Edge::Right,
            Edge::Right => Edge::Left,
            Edge::Top => Edge::Bottom,
            Edge::Bottom => Edge::Top,
        }
    }

    pub const ALL: [Edge; 4] = [Edge::Left, Edge::Right, Edge::Top, Edge::Bottom];

    pub fn code(self) -> u8 {
        match self {
            Edge::Left => 0,
            Edge::Right => 1,
            Edge::Top => 2,
            Edge::Bottom => 3,
        }
    }

    pub fn from_code(code: u8) -> Option<Edge> {
        Edge::ALL.get(usize::from(code)).copied()
    }
}

/// One computer on the desk.
#[derive(Clone, Debug, PartialEq)]
pub struct Computer {
    pub id: String,
    /// Displays in the computer's own units.
    pub displays: Vec<Rect>,
    /// Native units per point (a Mac is 1, Windows at 150% is 1.5).
    pub ppu: f64,
    /// Where the top left of its bounding box sits on the desk, in points.
    pub at: [f64; 2],
}

impl Computer {
    pub fn new(id: impl Into<String>, displays: Vec<Rect>, ppu: f64, at: [f64; 2]) -> Self {
        let ppu = if ppu.is_finite() && ppu > 0.0 { ppu } else { 1.0 };
        Self { id: id.into(), displays, ppu, at }
    }

    /// Its bounding box on the desk, in points.
    pub fn rect(&self) -> Rect {
        let native = bounds(&self.displays);
        Rect::new(self.at[0], self.at[1], native.w / self.ppu, native.h / self.ppu)
    }

    /// Each display on the desk, in points.
    pub fn desk_displays(&self) -> Vec<Rect> {
        let origin = bounds(&self.displays);
        let ppu = self.ppu;
        let at = self.at;
        let place = move |d: &Rect| {
            Rect::new(at[0] + (d.x - origin.x) / ppu, at[1] + (d.y - origin.y) / ppu, d.w / ppu, d.h / ppu)
        };
        self.displays.iter().map(place).collect()
    }

    pub fn to_desk(&self, [x, y]: [f64; 2]) -> [f64; 2] {
        let origin = bounds(&self.displays);
        [self.at[0] + (x - origin.x) / self.ppu, self.at[1] + (y - origin.y) / self.ppu]
    }

    pub fn from_desk(&self, [x, y]: [f64; 2]) -> [f64; 2] {
        let origin = bounds(&self.displays);
        [origin.x + (x - self.at[0]) * self.ppu, origin.y + (y - self.at[1]) * self.ppu]
    }

    /// The nearest point that is on one of its displays.
    pub fn onto_displays(&self, point: [f64; 2]) -> [f64; 2] {
        if self.displays.iter().any(|d| d.contains(point)) || self.displays.is_empty() {
            return point;
        }
        let nearest = self.displays.iter().min_by(|a, b| a.distance(point).total_cmp(&b.distance(point)));
        nearest.map_or(point, |display| display.clamp(point))
    }

    pub(crate) fn on_display(&self, point: [f64; 2]) -> bool {
        self.displays.iter().any(|d| d.contains(point))
    }
}

/// How far in from the edge a crossing cursor lands, in points.
pub const INSET: f64 = 2.0;

/// What a move of the cursor does.
#[derive(Clone, Debug, PartialEq)]
pub enum Step {
    /// It stays on the same computer, here (in that computer's units).
    Stay([f64; 2]),
    /// It goes over to computer `to`, arriving there (in that computer's units).
    Cross { to: usize, at: [f64; 2] },
}

/// Moves the cursor of computer `on` from `from` by `delta` (both in its units). `corner` keeps
/// that many points at each screen corner from switching computers, for hot corners.
pub fn step(desk: &[Computer], on: usize, from: [f64; 2], delta: [f64; 2], corner: f64) -> Step {
    let Some(computer) = desk.get(on) else { return Step::Stay(from) };
    let to = [from[0] + delta[0], from[1] + delta[1]];
    if computer.on_display(to) {
        return Step::Stay(to);
    }
    let edge = crossed(computer, from, to);
    if let Some(edge) = edge
        && let Some(cross) = beyond(desk, on, computer.onto_displays(to), edge, corner)
    {
        return cross;
    }
    // Nothing there: slide along the edge of the screen it was on.
    let home = computer.displays.iter().find(|d| d.contains(from)).copied();
    Step::Stay(home.map_or_else(|| computer.onto_displays(to), |d| d.clamp(to)))
}

/// Whether a cursor resting at `at` (on computer `on`, in its units) and moving by `delta` is
/// pushing against an edge with another computer behind it. For the computer whose own
/// cursor is clamped by the system, where `at` never leaves the screen.
pub fn push(desk: &[Computer], on: usize, at: [f64; 2], delta: [f64; 2], corner: f64) -> Option<Step> {
    let computer = desk.get(on)?;
    let (at, edges) = pushed(computer, at, delta);
    edges.into_iter().find_map(|edge| beyond(desk, on, at, edge, corner))
}

/// Whether a cursor resting at `at` and moving by `delta` is pushing out against the outer
/// edge of this computer's screens, whatever is beyond.
pub fn pushing(computer: &Computer, at: [f64; 2], delta: [f64; 2]) -> bool {
    delta != [0.0, 0.0] && !pushed(computer, at, delta).1.is_empty()
}

/// The outer edges a cursor at `at` moving by `delta` pushes against, most pushed first, and
/// where it rests.
fn pushed(computer: &Computer, at: [f64; 2], delta: [f64; 2]) -> ([f64; 2], Vec<Edge>) {
    let display = computer.displays.iter().find(|d| d.contains(at)).copied().unwrap_or_else(|| {
        let onto = computer.onto_displays(at);
        computer.displays.iter().find(|d| d.contains(onto)).copied().unwrap_or(bounds(&computer.displays))
    });
    let at = display.clamp(at);
    let mut edges = Vec::new();
    if at[0] <= display.x && delta[0] <= 0.0 {
        edges.push((Edge::Left, -delta[0]));
    }
    if at[0] >= display.right() - 1.0 && delta[0] >= 0.0 {
        edges.push((Edge::Right, delta[0]));
    }
    if at[1] <= display.y && delta[1] <= 0.0 {
        edges.push((Edge::Top, -delta[1]));
    }
    if at[1] >= display.bottom() - 1.0 && delta[1] >= 0.0 {
        edges.push((Edge::Bottom, delta[1]));
    }
    // Moving along an edge (no push into it) only counts when nothing else is pushed.
    edges.sort_by(|a, b| b.1.total_cmp(&a.1));
    let edges = edges
        .into_iter()
        .map(|(edge, _)| edge)
        .filter(|edge| {
            let [sx, sy] = edge.step();
            // Another of its own displays is there.
            !computer.on_display([at[0] + sx, at[1] + sy])
        })
        .collect();
    (at, edges)
}

/// Which edge of the display holding `from` the move to `to` went through.
fn crossed(computer: &Computer, from: [f64; 2], to: [f64; 2]) -> Option<Edge> {
    let display = computer.displays.iter().find(|d| d.contains(from)).copied()?;
    let mut best: Option<(Edge, f64)> = None;
    let mut consider = |edge: Edge, by: f64| {
        if by > 0.0 && best.is_none_or(|(_, most)| by > most) {
            best = Some((edge, by));
        }
    };
    consider(Edge::Left, display.x - to[0]);
    consider(Edge::Right, to[0] - (display.right() - 1.0));
    consider(Edge::Top, display.y - to[1]);
    consider(Edge::Bottom, to[1] - (display.bottom() - 1.0));
    best.map(|(edge, _)| edge)
}

/// The computer past `edge` of computer `on` at the point `at` (its units), and where the
/// cursor lands on it.
pub(crate) fn beyond(desk: &[Computer], on: usize, at: [f64; 2], edge: Edge, corner: f64) -> Option<Step> {
    let computer = desk.get(on)?;
    if corner > 0.0 && near_corner(computer, at, corner) {
        return None;
    }
    let point = computer.to_desk(at);
    // Measured from the screen the cursor is on, not the whole computer.
    let from = computer
        .desk_displays()
        .into_iter()
        .min_by(|a, b| a.distance(point).total_cmp(&b.distance(point)))
        .unwrap_or_else(|| computer.rect());
    let along = if edge.horizontal() { point[1] } else { point[0] };
    let span = |d: &Rect| if edge.horizontal() { (d.y, d.bottom()) } else { (d.x, d.right()) };
    let covers = |d: &Rect| {
        let (start, end) = span(d);
        along >= start - 0.5 && along < end + 0.5
    };
    // The nearest computer that way with a screen right across from the cursor. Only where
    // screens face each other, as macOS does between its own displays: then every crossing
    // has a way straight back to the same spot.
    let mut best: Option<(usize, f64)> = None;
    for (index, other) in desk.iter().enumerate() {
        if index == on {
            continue;
        }
        for rect in other.desk_displays().iter().filter(|d| covers(d)) {
            let gap = match edge {
                Edge::Right => rect.x - from.right(),
                Edge::Left => from.x - rect.right(),
                Edge::Bottom => rect.y - from.bottom(),
                Edge::Top => from.y - rect.bottom(),
            };
            // Allow a small overlap, from hand-placed rectangles.
            if gap < -1.0 {
                continue;
            }
            if best.is_none_or(|(_, nearest)| gap < nearest) {
                best = Some((index, gap));
            }
        }
    }
    let (to, _) = best?;
    let target = &desk[to];
    // Of its screens across from the cursor, the one nearest the edge being crossed: with an
    // external screen beside or above, the cursor lands on the screen that faces it.
    let facing = target.desk_displays().into_iter().filter(covers).min_by(|a, b| {
        let key = |d: &Rect| match edge {
            Edge::Right => d.x,
            Edge::Left => -d.right(),
            Edge::Bottom => d.y,
            Edge::Top => -d.bottom(),
        };
        key(a).total_cmp(&key(b))
    })?;
    let unit = 1.0 / target.ppu;
    let (start, end) = span(&facing);
    let along = along.clamp(start, (end - unit).max(start));
    // A little way in from the edge, so a wiggle of the hand doesn't send it straight back.
    let depth = if edge.horizontal() { facing.w } else { facing.h };
    let inset = INSET.min(depth / 2.0);
    let landing = match edge {
        Edge::Right => [facing.x + inset, along],
        Edge::Left => [facing.right() - unit - inset, along],
        Edge::Bottom => [along, facing.y + inset],
        Edge::Top => [along, facing.bottom() - unit - inset],
    };
    let at = target.onto_displays(target.from_desk(landing));
    Some(Step::Cross { to, at })
}

fn near_corner(computer: &Computer, at: [f64; 2], corner: f64) -> bool {
    let reach = corner * computer.ppu;
    computer.displays.iter().filter(|d| d.contains(at) || d.distance(at) < 1.5).any(|d| {
        let near_x = at[0] - d.x < reach || d.right() - at[0] <= reach;
        let near_y = at[1] - d.y < reach || d.bottom() - at[1] <= reach;
        near_x && near_y
    })
}

/// Where computer `moving` ends up when dropped at `at`: its edges snap to its neighbours'
/// edges, and it never overlaps another.
pub fn snap(desk: &[Computer], moving: usize, at: [f64; 2], reach: f64) -> [f64; 2] {
    let Some(computer) = desk.get(moving) else { return at };
    let size = computer.rect();
    let others: Vec<Rect> = desk.iter().enumerate().filter(|(i, _)| *i != moving).map(|(_, c)| c.rect()).collect();
    let mut x = at[0];
    let mut y = at[1];
    let snap_axis = |value: f64, length: f64, lines: &mut dyn Iterator<Item = f64>| {
        let mut best: Option<(f64, f64)> = None;
        for line in lines {
            for candidate in [line, line - length] {
                let distance = (candidate - value).abs();
                if distance <= reach && best.is_none_or(|(_, d)| distance < d) {
                    best = Some((candidate, distance));
                }
            }
        }
        best.map_or(value, |(candidate, _)| candidate)
    };
    x = snap_axis(x, size.w, &mut others.iter().flat_map(|r| [r.x, r.right()]));
    y = snap_axis(y, size.h, &mut others.iter().flat_map(|r| [r.y, r.bottom()]));
    // Push out of any overlap, along whichever way is shortest.
    for _ in 0..8 {
        let rect = Rect::new(x, y, size.w, size.h);
        let Some(other) = others.iter().find(|o| o.overlaps(&rect)) else { break };
        let moves = [
            (other.right() - rect.x, 0.0),
            (other.x - rect.right(), 0.0),
            (0.0, other.bottom() - rect.y),
            (0.0, other.y - rect.bottom()),
        ];
        let (dx, dy) = moves
            .into_iter()
            .min_by(|a, b| (a.0.abs() + a.1.abs()).total_cmp(&(b.0.abs() + b.1.abs())))
            .unwrap_or((0.0, 0.0));
        x += dx;
        y += dy;
    }
    [x, y]
}

/// A free spot for a newly added computer: to the right of everything already on the desk,
/// top-aligned with the one placed furthest right.
pub fn free_spot(desk: &[Computer]) -> [f64; 2] {
    let rightmost = desk.iter().map(|c| c.rect()).max_by(|a, b| a.right().total_cmp(&b.right()));
    rightmost.map_or([0.0, 0.0], |rect| [rect.right(), rect.y])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mac() -> Computer {
        Computer::new("mac", vec![Rect::new(0.0, 0.0, 1512.0, 982.0)], 1.0, [0.0, 0.0])
    }

    fn pc(at: [f64; 2]) -> Computer {
        // 2560x1440 at 150%: 1706.7 x 960 points.
        Computer::new("pc", vec![Rect::new(0.0, 0.0, 2560.0, 1440.0)], 1.5, at)
    }

    #[test]
    fn crossing_right_lands_on_the_left_edge_at_the_same_height() {
        let desk = [mac(), pc([1512.0, 0.0])];
        let step = step(&desk, 0, [1511.0, 480.0], [5.0, 0.0], 0.0);
        let Step::Cross { to, at } = step else { panic!("{step:?}") };
        assert_eq!(to, 1);
        assert_eq!(at[0], 3.0, "2 points in from the edge is 3 pixels at 150%");
        assert!((at[1] - 720.0).abs() < 0.01, "480 points down is 720 pixels at 150%: {at:?}");
    }

    #[test]
    fn a_screen_below_one_display_is_reached_even_when_another_reaches_further_down() {
        // A PC showing a Mac's external screen as its left display, its own screen lower and to
        // the right; the MacBook's screen sits under the left display. The PC as a whole reaches
        // below the MacBook's top, but the display the cursor is on doesn't.
        let pc = Computer::new(
            "pc",
            vec![Rect::new(0.0, 0.0, 1920.0, 1080.0), Rect::new(1920.0, 300.0, 1920.0, 1080.0)],
            1.0,
            [0.0, 0.0],
        );
        let macbook = Computer::new("macbook", vec![Rect::new(0.0, 0.0, 1512.0, 982.0)], 1.0, [100.0, 1080.0]);
        let desk = [pc, macbook];
        let Some(Step::Cross { to, at }) = push(&desk, 0, [500.0, 1079.0], [0.0, 5.0], 0.0) else { panic!() };
        assert_eq!(to, 1);
        assert_eq!(at, [400.0, 2.0]);
    }

    #[test]
    fn coming_back_lands_on_the_right_edge() {
        let desk = [mac(), pc([1512.0, 0.0])];
        let Step::Cross { to, at } = step(&desk, 1, [0.0, 720.0], [-3.0, 0.0], 0.0) else { panic!() };
        assert_eq!(to, 0);
        assert_eq!(at, [1509.0, 480.0]);
    }

    #[test]
    fn a_wiggle_after_crossing_does_not_cross_back() {
        let desk = [mac(), pc([1512.0, 0.0])];
        let Step::Cross { to: 1, at } = step(&desk, 0, [1511.0, 480.0], [5.0, 0.0], 0.0) else { panic!() };
        // A pixel or two back toward the Mac stays on the PC.
        assert!(matches!(step(&desk, 1, at, [-2.0, 1.0], 0.0), Step::Stay(_)));
        // Moving on back does cross, to where it came from.
        let Step::Cross { to: 0, at: back } = step(&desk, 1, at, [-5.0, 0.0], 0.0) else { panic!() };
        assert_eq!(back, [1509.0, 480.0]);
    }

    #[test]
    fn no_neighbour_slides_along_the_edge() {
        let desk = [mac(), pc([1512.0, 0.0])];
        assert_eq!(step(&desk, 0, [10.0, 10.0], [-50.0, 5.0], 0.0), Step::Stay([0.0, 15.0]));
        // Below the PC's 960 points there's nothing to the right of the Mac.
        assert_eq!(step(&desk, 0, [1511.0, 970.0], [4.0, 0.0], 0.0), Step::Stay([1511.0, 970.0]));
    }

    #[test]
    fn a_second_display_of_the_same_computer_comes_first() {
        let two = Computer::new(
            "mac",
            vec![Rect::new(0.0, 0.0, 1000.0, 800.0), Rect::new(1000.0, 0.0, 800.0, 600.0)],
            1.0,
            [0.0, 0.0],
        );
        let desk = [two, pc([1800.0, 0.0])];
        assert_eq!(step(&desk, 0, [999.0, 100.0], [3.0, 0.0], 0.0), Step::Stay([1002.0, 100.0]));
        assert!(push(&desk, 0, [999.0, 100.0], [3.0, 0.0], 0.0).is_none());
        // Below the second display there's a gap, but the PC is still the next thing that way.
        assert!(matches!(push(&desk, 0, [999.0, 700.0], [3.0, 0.0], 0.0), Some(Step::Cross { to: 1, .. })));
        assert!(matches!(push(&desk, 0, [1799.0, 100.0], [3.0, 0.0], 0.0), Some(Step::Cross { to: 1, .. })));
    }

    #[test]
    fn pushing_a_clamped_cursor_crosses() {
        let desk = [mac(), pc([1512.0, 100.0])];
        let Some(Step::Cross { to: 1, at }) = push(&desk, 0, [1511.0, 500.0], [0.0, 0.0], 0.0) else { panic!() };
        assert!((at[1] - 600.0).abs() < 0.01, "{at:?}");
        // Above the PC (it starts 100 points down) there is nothing.
        assert!(push(&desk, 0, [1511.0, 50.0], [2.0, 0.0], 0.0).is_none());
        // Moving away from the edge never crosses.
        assert!(push(&desk, 0, [1511.0, 500.0], [-2.0, 0.0], 0.0).is_none());
    }

    #[test]
    fn corners_are_kept_for_hot_corners() {
        let desk = [mac(), pc([1512.0, 0.0])];
        assert!(push(&desk, 0, [1511.0, 3.0], [2.0, 0.0], 10.0).is_none());
        assert!(push(&desk, 0, [1511.0, 30.0], [2.0, 0.0], 10.0).is_some());
    }

    #[test]
    fn computers_above_and_below() {
        let desk = [mac(), pc([0.0, -960.0])];
        let Step::Cross { to: 1, at } = step(&desk, 0, [100.0, 0.0], [0.0, -2.0], 0.0) else { panic!() };
        assert!((at[0] - 150.0).abs() < 0.01 && (at[1] - 1436.0).abs() < 0.01, "{at:?}");
    }

    #[test]
    fn snapping_lines_up_edges_and_avoids_overlap() {
        let desk = [mac(), pc([3000.0, 0.0])];
        assert_eq!(snap(&desk, 1, [1520.0, 6.0], 16.0), [1512.0, 0.0]);
        // Dropped on top of the Mac: pushed out the shortest way.
        let at = snap(&desk, 1, [1400.0, 300.0], 16.0);
        let moved = Computer { at, ..pc(at) };
        assert!(!moved.rect().overlaps(&desk[0].rect()), "{at:?}");
    }

    /// A PC with its main screen and a smaller external to its left, raised up; the Mac to the
    /// left of both.
    fn stepped() -> [Computer; 2] {
        let pc = Computer::new(
            "pc",
            vec![Rect::new(0.0, 0.0, 2560.0, 1440.0), Rect::new(-1920.0, -600.0, 1920.0, 1080.0)],
            1.5,
            [1512.0, -400.0],
        );
        [mac(), pc]
    }

    #[test]
    fn the_cursor_lands_on_the_screen_that_faces_it() {
        let desk = stepped();
        // Low on the Mac, only the PC's main screen is across: it lands there, not on the
        // external's bottom corner.
        let Some(Step::Cross { to: 1, at }) = push(&desk, 0, [1511.0, 900.0], [3.0, 0.0], 0.0) else { panic!() };
        assert_eq!(at[0], 3.0, "just inside the main screen's left edge: {at:?}");
        // High on the Mac, the external is across.
        let Some(Step::Cross { to: 1, at }) = push(&desk, 0, [1511.0, 100.0], [3.0, 0.0], 0.0) else { panic!() };
        assert_eq!(at[0], -1917.0, "just inside the external's left edge: {at:?}");
    }

    #[test]
    fn every_crossing_comes_back_to_the_same_spot() {
        let desk = stepped();
        let mut crossed = 0;
        for y in 0..982 {
            let start = [1511.0, f64::from(y)];
            let Some(Step::Cross { to: 1, at }) = push(&desk, 0, start, [2.0, 0.0], 0.0) else { continue };
            crossed += 1;
            let Step::Cross { to: 0, at: back } = step(&desk, 1, at, [-4.0, 0.0], 0.0) else {
                panic!("no way back from {at:?} (left the Mac at {start:?})")
            };
            assert!((back[1] - start[1]).abs() < 1.0, "left at {start:?}, came back at {back:?}");
            assert_eq!(back[0], 1509.0);
        }
        // The PC's screens reach 960 points down; below that the Mac's edge has nothing across.
        assert_eq!(crossed, 961);
    }

    #[test]
    fn free_spot_is_to_the_right() {
        assert_eq!(free_spot(&[mac()]), [1512.0, 0.0]);
        assert_eq!(free_spot(&[]), [0.0, 0.0]);
    }

    #[test]
    fn desk_and_native_round_trip() {
        let computer = Computer::new(
            "pc",
            vec![Rect::new(-1920.0, 0.0, 1920.0, 1080.0), Rect::new(0.0, 0.0, 2560.0, 1440.0)],
            1.25,
            [40.0, -10.0],
        );
        let point = [-300.0, 700.0];
        let back = computer.from_desk(computer.to_desk(point));
        assert!((back[0] - point[0]).abs() < 1e-9 && (back[1] - point[1]).abs() < 1e-9);
        assert_eq!(computer.desk_displays()[1].x, 40.0 + 1920.0 / 1.25);
    }
}
