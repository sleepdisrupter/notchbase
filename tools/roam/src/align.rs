//! Lining the screens up. While the desk is being arranged, every screen on it, on every
//! computer, shows a guide over everything: a frame in its own colour, its name, and along each
//! edge a band in the colour of the screen the cursor goes on to from there. Near an edge, the
//! spot the cursor leaves from and the spot it comes in at on the next screen are marked, so the
//! two can be lined up by eye, in Roam and in the system's own display settings together.
//!
//! The computer being arranged works it all out from its desk, the same one the cursor follows,
//! and tells the others what to show on each of their screens: they only draw.

use crate::layout::{Computer, Edge, Rect, Step, beyond};

#[cfg(target_os = "macos")]
mod mac;
#[cfg(windows)]
mod windows;

/// One colour for each screen, in order: the editor colours its screens the same way.
pub const PALETTE: [[u8; 3]; 8] = [
    [64, 140, 255],
    [52, 199, 89],
    [255, 149, 0],
    [255, 55, 135],
    [175, 82, 222],
    [48, 186, 210],
    [255, 204, 0],
    [255, 69, 58],
];

/// How near an edge, in points, the cursor shows where it would cross.
pub const REACH: f64 = 240.0;

/// The most bands a screen's guide carries, all edges together.
pub const MOST_BANDS: usize = 12;

pub fn colour(index: u8) -> [u8; 3] {
    PALETTE[usize::from(index) % PALETTE.len()]
}

/// A stretch of a screen's edge where the cursor goes on to another screen.
#[derive(Clone, Debug, PartialEq)]
pub struct Band {
    pub edge: Edge,
    /// Where it starts and ends along the edge, 0 to 1 (left to right, or top to bottom).
    pub from: f32,
    pub to: f32,
    /// The colour of the screen it goes to.
    pub colour: u8,
    /// That screen, in a few words.
    pub label: String,
}

/// What one screen shows while the screens are lined up.
#[derive(Clone, Debug, PartialEq)]
pub struct Guide {
    /// Which of its computer's screens, by its place in that computer's list.
    pub display: u8,
    pub colour: u8,
    pub title: String,
    pub detail: String,
    pub bands: Vec<Band>,
}

/// A guide on one of this computer's screens, at `rect` in its own units.
#[derive(Clone, Debug, PartialEq)]
pub struct Shown {
    pub rect: Rect,
    pub guide: Guide,
}

/// Where the cursor would cross: the spot it leaves from on its screen's edge, or the one it
/// comes in at on the next.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mark {
    pub display: u8,
    pub edge: Edge,
    /// Along that edge, 0 to 1.
    pub along: f32,
    /// Comes in here, rather than leaves.
    pub arrives: bool,
}

/// A mark on one of this computer's screens, at `rect` in its own units.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Marked {
    pub rect: Rect,
    pub mark: Mark,
}

/// How one screen of a computer on the desk is named and coloured.
#[derive(Clone, Debug, PartialEq)]
pub struct Screen {
    /// Its place in its computer's own list.
    pub display: u8,
    pub colour: u8,
    pub title: String,
    pub detail: String,
    /// How the screens beside it name it.
    pub label: String,
}

/// A stretch of an edge, 0 to 1 along it, from which the cursor goes to display `display` of
/// computer `computer` on the desk.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Span {
    pub from: f64,
    pub to: f64,
    pub computer: usize,
    pub display: usize,
}

fn along_of(edge: Edge, rect: &Rect) -> (f64, f64) {
    if edge.horizontal() { (rect.y, rect.bottom()) } else { (rect.x, rect.right()) }
}

/// The last spot on `display`'s `edge` at `along` (in its computer's units): where a cursor
/// pushing against that edge rests.
fn edge_spot(display: &Rect, edge: Edge, along: f64) -> [f64; 2] {
    match edge {
        Edge::Left => [display.x, along],
        Edge::Right => [display.right() - 1.0, along],
        Edge::Top => [along, display.y],
        Edge::Bottom => [along, display.bottom() - 1.0],
    }
}

/// Where a cursor resting at `at` on `edge` of display `index` of computer `on` goes when pushed
/// through it: which computer, which of its displays, and where (in that computer's units). The
/// system takes it to its own screen beside; Roam to another computer's, as `layout` decides.
fn across(desk: &[Computer], on: usize, index: usize, at: [f64; 2], edge: Edge) -> Option<(usize, usize, [f64; 2])> {
    let computer = desk.get(on)?;
    let [sx, sy] = edge.step();
    let next = [at[0] + sx, at[1] + sy];
    if let Some(own) = computer.displays.iter().enumerate().position(|(i, d)| i != index && d.contains(next)) {
        return Some((on, own, next));
    }
    let Some(Step::Cross { to, at }) = beyond(desk, on, at, edge, 0.0) else { return None };
    let display = desk[to].displays.iter().position(|d| d.contains(at))?;
    Some((to, display, at))
}

/// The stretches of `edge` of display `index` of computer `on` that lead to another screen,
/// in order along it.
pub fn spans(desk: &[Computer], on: usize, index: usize, edge: Edge) -> Vec<Span> {
    let Some(computer) = desk.get(on) else { return Vec::new() };
    let (Some(native), Some(placed)) = (computer.displays.get(index), computer.desk_displays().get(index).copied())
    else {
        return Vec::new();
    };
    let (start, end) = along_of(edge, &placed);
    let length = end - start;
    if length <= 0.0 {
        return Vec::new();
    }
    let mut cuts = vec![start, end];
    for other in desk {
        for rect in other.desk_displays() {
            let (a, b) = along_of(edge, &rect);
            cuts.extend([a, b].into_iter().filter(|v| *v > start && *v < end));
        }
    }
    cuts.sort_by(f64::total_cmp);
    cuts.dedup_by(|a, b| (*a - *b).abs() < 1e-6);
    let (native_start, _) = along_of(edge, native);
    let mut spans: Vec<Span> = Vec::new();
    for pair in cuts.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        if b - a < 0.01 {
            continue;
        }
        let middle = native_start + ((a + b) / 2.0 - start) * computer.ppu;
        let Some((to, display, _)) = across(desk, on, index, edge_spot(native, edge, middle), edge) else { continue };
        let (from, until) = ((a - start) / length, (b - start) / length);
        match spans.last_mut() {
            Some(last) if last.computer == to && last.display == display && (last.to - from).abs() < 1e-9 => {
                last.to = until;
            }
            _ => spans.push(Span { from, to: until, computer: to, display }),
        }
    }
    spans
}

/// Every screen's guide, for each computer on the desk: `screens[c][i]` names display `i` of
/// computer `c`.
pub fn guides(desk: &[Computer], screens: &[Vec<Screen>]) -> Vec<Vec<Guide>> {
    desk.iter()
        .enumerate()
        .map(|(on, computer)| {
            (0..computer.displays.len())
                .filter_map(|index| {
                    let screen = screens.get(on)?.get(index)?;
                    let mut bands = Vec::new();
                    for edge in Edge::ALL {
                        for span in spans(desk, on, index, edge) {
                            let Some(to) = screens.get(span.computer).and_then(|s| s.get(span.display)) else {
                                continue;
                            };
                            bands.push(Band {
                                edge,
                                from: span.from as f32,
                                to: span.to as f32,
                                colour: to.colour,
                                label: to.label.clone(),
                            });
                        }
                    }
                    bands.truncate(MOST_BANDS);
                    Some(Guide {
                        display: screen.display,
                        colour: screen.colour,
                        title: screen.title.clone(),
                        detail: screen.detail.clone(),
                        bands,
                    })
                })
                .collect()
        })
        .collect()
}

/// With the cursor at `at` on computer `on` (in its units), where it would leave from and come
/// in at, for each edge within `reach` points that leads somewhere: each mark with the desk
/// computer whose screen it goes on.
pub fn marks(desk: &[Computer], screens: &[Vec<Screen>], on: usize, at: [f64; 2], reach: f64) -> Vec<(usize, Mark)> {
    let Some(computer) = desk.get(on) else { return Vec::new() };
    let at = computer.onto_displays(at);
    let Some(index) = computer.displays.iter().position(|d| d.contains(at)) else { return Vec::new() };
    let display = computer.displays[index];
    let code = |c: usize, i: usize| screens.get(c).and_then(|s| s.get(i)).map(|s| s.display);
    let fraction = |rect: &Rect, edge: Edge, point: [f64; 2]| {
        let (start, end) = along_of(edge, rect);
        let value = if edge.horizontal() { point[1] } else { point[0] };
        ((value - start) / (end - start).max(1e-9)).clamp(0.0, 1.0) as f32
    };
    let mut marks = Vec::new();
    for edge in Edge::ALL {
        let away = match edge {
            Edge::Left => at[0] - display.x,
            Edge::Right => display.right() - 1.0 - at[0],
            Edge::Top => at[1] - display.y,
            Edge::Bottom => display.bottom() - 1.0 - at[1],
        } / computer.ppu;
        if away > reach {
            continue;
        }
        let spot = edge_spot(&display, edge, if edge.horizontal() { at[1] } else { at[0] });
        let Some((to, target, landing)) = across(desk, on, index, spot, edge) else { continue };
        let (Some(from_code), Some(to_code)) = (code(on, index), code(to, target)) else { continue };
        marks.push((on, Mark { display: from_code, edge, along: fraction(&display, edge, spot), arrives: false }));
        let entering = edge.opposite();
        let rect = desk[to].displays[target];
        marks.push((
            to,
            Mark { display: to_code, edge: entering, along: fraction(&rect, entering, landing), arrives: true },
        ));
    }
    marks
}

// ------------------------------------------------------------------------------- drawing

/// Something a guide draws, in its screen's own units from its top left.
#[derive(Clone, Debug, PartialEq)]
pub enum Shape {
    Rect {
        at: [f64; 4],
        fill: [u8; 4],
        radius: f64,
    },
    /// Words centred in the box `at`.
    Text {
        at: [f64; 4],
        text: String,
        size: f64,
        colour: [u8; 4],
        bold: bool,
    },
}

const CARD: [u8; 4] = [24, 24, 28, 242];
const WHITE: [u8; 4] = [255, 255, 255, 255];
const SOFT: [u8; 4] = [210, 210, 216, 255];
const DIM: [u8; 4] = [150, 150, 158, 255];
const INK: [u8; 4] = [16, 16, 18, 255];

fn opaque([r, g, b]: [u8; 3]) -> [u8; 4] {
    [r, g, b, 255]
}

/// Black or white, whichever reads on `background`.
fn ink_on([r, g, b]: [u8; 3]) -> [u8; 4] {
    let light = 0.299 * f64::from(r) + 0.587 * f64::from(g) + 0.114 * f64::from(b);
    if light > 150.0 { INK } else { WHITE }
}

/// About how wide `text` comes out at `size`, for sizing what goes round it.
pub fn width_of(text: &str, size: f64, bold: bool) -> f64 {
    let per = if bold { 0.6 } else { 0.56 };
    text.chars().count() as f64 * size * per
}

/// The unit everything is sized by: a hundredth of the screen's shorter side, so a guide looks
/// the same on every screen whatever its pixels.
fn unit(size: [f64; 2]) -> f64 {
    (size[0].min(size[1]) / 100.0).max(4.0)
}

fn arrow(edge: Edge) -> &'static str {
    match edge {
        Edge::Left => "←",
        Edge::Right => "→",
        Edge::Top => "↑",
        Edge::Bottom => "↓",
    }
}

/// A pill of words just in from `edge`: its side facing that edge at `at`, centred along it,
/// filled and inked in the two colours given, and kept inside a `screen`-sized screen.
fn pill(
    shapes: &mut Vec<Shape>,
    edge: Edge,
    at: [f64; 2],
    text: String,
    size: f64,
    (fill, colour): ([u8; 4], [u8; 4]),
    screen: [f64; 2],
) {
    let w = width_of(&text, size, true) + size * 1.6;
    let h = size * 1.9;
    let (x, y) = match edge {
        Edge::Left => (at[0], at[1] - h / 2.0),
        Edge::Right => (at[0] - w, at[1] - h / 2.0),
        Edge::Top => (at[0] - w / 2.0, at[1]),
        Edge::Bottom => (at[0] - w / 2.0, at[1] - h),
    };
    let x = x.clamp(0.0, (screen[0] - w).max(0.0));
    let y = y.clamp(0.0, (screen[1] - h).max(0.0));
    shapes.push(Shape::Rect { at: [x, y, w, h], fill, radius: h / 2.0 });
    shapes.push(Shape::Text { at: [x, y, w, h], text, size, colour, bold: true });
}

/// The rectangle of `edge` from `from` to `to` (0 to 1), `thick` deep into a `size` screen.
fn strip(size: [f64; 2], edge: Edge, from: f64, to: f64, thick: f64) -> [f64; 4] {
    let [w, h] = size;
    match edge {
        Edge::Left => [0.0, from * h, thick, (to - from) * h],
        Edge::Right => [w - thick, from * h, thick, (to - from) * h],
        Edge::Top => [from * w, 0.0, (to - from) * w, thick],
        Edge::Bottom => [from * w, h - thick, (to - from) * w, thick],
    }
}

/// The point `along` (0 to 1) `edge`, `inward` in from it.
fn inside(size: [f64; 2], edge: Edge, along: f64, inward: f64) -> [f64; 2] {
    let [w, h] = size;
    match edge {
        Edge::Left => [inward, along * h],
        Edge::Right => [w - inward, along * h],
        Edge::Top => [along * w, inward],
        Edge::Bottom => [along * w, h - inward],
    }
}

/// What a `size` screen shows for its guide: the frame, the bands and their labels, and its
/// card with its name.
pub fn scene(size: [f64; 2], guide: &Guide) -> Vec<Shape> {
    let u = unit(size);
    let [w, h] = size;
    let own = colour(guide.colour);
    let mut shapes = Vec::new();
    // The frame, all round, in the screen's own colour.
    let frame = u * 0.6;
    for edge in Edge::ALL {
        shapes.push(Shape::Rect { at: strip(size, edge, 0.0, 1.0, frame), fill: opaque(own), radius: 0.0 });
    }
    // Where the cursor goes on from, in the colour of where it goes.
    let band = u * 1.5;
    for b in &guide.bands {
        let (from, to) = (f64::from(b.from), f64::from(b.to));
        shapes.push(Shape::Rect {
            at: strip(size, b.edge, from, to, band),
            fill: opaque(colour(b.colour)),
            radius: 0.0,
        });
    }
    // The card, low on the left, out of the way of the system's settings window.
    let title_size = u * 4.2;
    let detail_size = u * 2.1;
    let hint_size = u * 1.5;
    let hint = "Lining up screens · turn off in Roam".to_owned();
    let inner = width_of(&guide.title, title_size, true)
        .max(width_of(&guide.detail, detail_size, false))
        .max(width_of(&hint, hint_size, false));
    let card_w = (inner + u * 6.0).min(w - u * 8.0).max(u * 20.0);
    let card_h = u * 2.0 + title_size * 1.3 + detail_size * 1.3 + hint_size * 1.4 + u * 2.4;
    let (cx, cy) = (u * 5.0, h - u * 5.0 - card_h);
    shapes.push(Shape::Rect { at: [cx, cy, card_w, card_h], fill: CARD, radius: u * 1.6 });
    shapes.push(Shape::Rect { at: [cx, cy, card_w, u * 1.0], fill: opaque(own), radius: u * 0.5 });
    let mut y = cy + u * 2.0;
    shapes.push(Shape::Text {
        at: [cx, y, card_w, title_size * 1.3],
        text: guide.title.clone(),
        size: title_size,
        colour: WHITE,
        bold: true,
    });
    y += title_size * 1.3;
    shapes.push(Shape::Text {
        at: [cx, y, card_w, detail_size * 1.3],
        text: guide.detail.clone(),
        size: detail_size,
        colour: SOFT,
        bold: false,
    });
    y += detail_size * 1.3 + u * 0.4;
    shapes.push(Shape::Text {
        at: [cx, y, card_w, hint_size * 1.4],
        text: hint,
        size: hint_size,
        colour: DIM,
        bold: false,
    });
    // Each band's label, just in from its middle.
    let label_size = u * 1.8;
    for b in &guide.bands {
        let middle = f64::from(b.from + b.to) / 2.0;
        let at = inside(size, b.edge, middle, band + u * 1.2);
        let words = format!("{}  {}", arrow(b.edge), b.label);
        let fill = opaque(colour(b.colour));
        pill(&mut shapes, b.edge, at, words, label_size, (fill, ink_on(colour(b.colour))), size);
    }
    shapes
}

/// What a `size` screen shows for the cursor's marks on it.
pub fn mark_scene(size: [f64; 2], marks: &[Mark]) -> Vec<Shape> {
    let u = unit(size);
    let [w, h] = size;
    let mut shapes = Vec::new();
    for mark in marks {
        let along = f64::from(mark.along);
        let length = u * 9.0;
        let thick = (u * 0.45).max(2.0);
        let outline = (u * 0.2).max(1.0);
        // A line straight in from the edge at the very spot, with a dark edge so it shows on
        // anything, and a wider tick on the edge itself.
        let (line, tick) = match mark.edge {
            Edge::Left => ([0.0, along * h - thick / 2.0, length, thick], [0.0, along * h - u * 1.5, u * 1.6, u * 3.0]),
            Edge::Right => (
                [w - length, along * h - thick / 2.0, length, thick],
                [w - u * 1.6, along * h - u * 1.5, u * 1.6, u * 3.0],
            ),
            Edge::Top => ([along * w - thick / 2.0, 0.0, thick, length], [along * w - u * 1.5, 0.0, u * 3.0, u * 1.6]),
            Edge::Bottom => (
                [along * w - thick / 2.0, h - length, thick, length],
                [along * w - u * 1.5, h - u * 1.6, u * 3.0, u * 1.6],
            ),
        };
        for [x, y, rw, rh] in [line, tick] {
            shapes.push(Shape::Rect {
                at: [x - outline, y - outline, rw + outline * 2.0, rh + outline * 2.0],
                fill: INK,
                radius: 0.0,
            });
        }
        shapes.push(Shape::Rect { at: line, fill: WHITE, radius: 0.0 });
        shapes.push(Shape::Rect { at: tick, fill: WHITE, radius: 0.0 });
        let words = if mark.arrives { "Cursor comes in here" } else { "Cursor leaves here" };
        let at = inside(size, mark.edge, along, length + u * 0.6);
        pill(&mut shapes, mark.edge, at, words.to_owned(), u * 1.6, (WHITE, INK), size);
    }
    shapes
}

/// The box round everything in `shapes`, to redraw only that.
pub fn bounds_of(shapes: &[Shape]) -> Option<[f64; 4]> {
    let mut all = shapes.iter().map(|s| match s {
        Shape::Rect { at, .. } | Shape::Text { at, .. } => *at,
    });
    let first = all.next()?;
    let [mut left, mut top, mut right, mut bottom] = [first[0], first[1], first[0] + first[2], first[1] + first[3]];
    for [x, y, w, h] in all {
        left = left.min(x);
        top = top.min(y);
        right = right.max(x + w);
        bottom = bottom.max(y + h);
    }
    Some([left, top, right - left, bottom - top])
}

/// Puts `shown` over this computer's screens, in place of what was there: none takes the
/// guides away.
#[cfg(any(target_os = "macos", windows))]
pub fn show_here(shown: Vec<Shown>) {
    #[cfg(target_os = "macos")]
    mac::show(shown);
    #[cfg(windows)]
    windows::show(shown);
}

/// Puts the cursor's marks over this computer's screens, in place of the ones there.
#[cfg(any(target_os = "macos", windows))]
pub fn mark_here(marks: Vec<Marked>) {
    #[cfg(target_os = "macos")]
    mac::mark(marks);
    #[cfg(windows)]
    windows::mark(marks);
}

#[cfg(test)]
mod tests;
