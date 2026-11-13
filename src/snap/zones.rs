//! Where windows go: the zones a window can take, the biggest free space on a screen, and a tiling of every window.

use egui::{Rect, pos2, vec2};

/// A screen at least this many times wider than tall is ultrawide.
const ULTRAWIDE: f32 = 2.0;
/// The centered zone takes this share of the screen each way.
const CENTER_SHARE: f32 = 0.7;
/// Free space smaller than this, wide by tall, isn't worth offering.
const MIN_FREE: (f32, f32) = (240.0, 160.0);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Zone {
    Full,
    Left,
    Right,
    #[allow(dead_code)]
    Top,
    Bottom,
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
    LeftThird,
    CenterThird,
    RightThird,
    Center,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Choice {
    Auto,
    Fill,
    Zone(Zone),
}

impl Choice {
    pub fn label(self) -> &'static str {
        match self {
            Choice::Auto => "Tidy every window",
            Choice::Fill => "Fill the free space",
            Choice::Zone(zone) => zone.label(),
        }
    }
}

impl Zone {
    pub const ALL: [Zone; 13] = [
        Zone::Full,
        Zone::Left,
        Zone::Right,
        Zone::Top,
        Zone::Bottom,
        Zone::TopLeft,
        Zone::TopRight,
        Zone::BottomLeft,
        Zone::BottomRight,
        Zone::LeftThird,
        Zone::CenterThird,
        Zone::RightThird,
        Zone::Center,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Zone::Full => "Fill the screen",
            Zone::Left => "Left half",
            Zone::Right => "Right half",
            Zone::Top => "Top half",
            Zone::Bottom => "Bottom half",
            Zone::TopLeft => "Top left",
            Zone::TopRight => "Top right",
            Zone::BottomLeft => "Bottom left",
            Zone::BottomRight => "Bottom right",
            Zone::LeftThird => "Left third",
            Zone::CenterThird => "Center third",
            Zone::RightThird => "Right third",
            Zone::Center => "Center",
        }
    }

    /// The zone as a share of the screen, from 0 to 1.
    pub fn share(self) -> Rect {
        let unit = |x: f32, y: f32, w: f32, h: f32| Rect::from_min_size(pos2(x, y), vec2(w, h));
        let third = 1.0 / 3.0;
        match self {
            Zone::Full => unit(0.0, 0.0, 1.0, 1.0),
            Zone::Left => unit(0.0, 0.0, 0.5, 1.0),
            Zone::Right => unit(0.5, 0.0, 0.5, 1.0),
            Zone::Top => unit(0.0, 0.0, 1.0, 0.5),
            Zone::Bottom => unit(0.0, 0.5, 1.0, 0.5),
            Zone::TopLeft => unit(0.0, 0.0, 0.5, 0.5),
            Zone::TopRight => unit(0.5, 0.0, 0.5, 0.5),
            Zone::BottomLeft => unit(0.0, 0.5, 0.5, 0.5),
            Zone::BottomRight => unit(0.5, 0.5, 0.5, 0.5),
            Zone::LeftThird => unit(0.0, 0.0, third, 1.0),
            Zone::CenterThird => unit(third, 0.0, third, 1.0),
            Zone::RightThird => unit(2.0 * third, 0.0, third, 1.0),
            Zone::Center => {
                let margin = (1.0 - CENTER_SHARE) * 0.5;
                unit(margin, margin, CENTER_SHARE, CENTER_SHARE)
            }
        }
    }

    fn cells(self) -> Option<(usize, usize, [usize; 2], [usize; 2])> {
        Some(match self {
            Zone::Full => (1, 1, [0, 1], [0, 1]),
            Zone::Left => (2, 1, [0, 1], [0, 1]),
            Zone::Right => (2, 1, [1, 2], [0, 1]),
            Zone::Top => (1, 2, [0, 1], [0, 1]),
            Zone::Bottom => (1, 2, [0, 1], [1, 2]),
            Zone::TopLeft => (2, 2, [0, 1], [0, 1]),
            Zone::TopRight => (2, 2, [1, 2], [0, 1]),
            Zone::BottomLeft => (2, 2, [0, 1], [1, 2]),
            Zone::BottomRight => (2, 2, [1, 2], [1, 2]),
            Zone::LeftThird => (3, 1, [0, 1], [0, 1]),
            Zone::CenterThird => (3, 1, [1, 2], [0, 1]),
            Zone::RightThird => (3, 1, [2, 3], [0, 1]),
            Zone::Center => return None,
        })
    }

    /// The zone on a work area, with `gap` at the edges and between windows. Grid zones line up with `tile`'s cells.
    pub fn rect(self, work: Rect, gap: f32) -> Rect {
        let field = work.shrink(gap);
        let Some((columns, rows, [c0, c1], [r0, r1])) = self.cells() else {
            let share = self.share();
            return Rect::from_min_size(
                field.min + vec2(field.width() * share.left(), field.height() * share.top()),
                vec2(field.width() * share.width(), field.height() * share.height()),
            );
        };
        let cell_w = (field.width() - gap * (columns as f32 - 1.0)) / columns as f32;
        let cell_h = (field.height() - gap * (rows as f32 - 1.0)) / rows as f32;
        let along = |start: f32, cell: f32, from: usize, to: usize| {
            (start + from as f32 * (cell + gap), start + to as f32 * (cell + gap) - gap)
        };
        let (left, right) = along(field.left(), cell_w, c0, c1);
        let (top, bottom) = along(field.top(), cell_h, r0, r1);
        Rect::from_min_max(pos2(left, top), pos2(right, bottom))
    }
}

/// The biggest rectangle of the work area that no window covers, if it's big enough to use.
pub fn free_space(work: Rect, gap: f32, taken: &[Rect]) -> Option<Rect> {
    let field = work.shrink(gap);
    let blocks: Vec<Rect> = taken.iter().map(|rect| rect.expand(gap)).filter(|rect| rect.intersects(field)).collect();
    let mut xs = vec![field.left(), field.right()];
    for block in &blocks {
        xs.extend([block.left(), block.right()].into_iter().filter(|x| *x > field.left() && *x < field.right()));
    }
    xs.sort_by(f32::total_cmp);
    xs.dedup_by(|a, b| (*a - *b).abs() < 0.5);
    let mut best: Option<Rect> = None;
    for (i, left) in xs.iter().enumerate() {
        for right in &xs[i + 1..] {
            let mut spans: Vec<(f32, f32)> = blocks
                .iter()
                .filter(|block| block.left() < *right - 0.5 && block.right() > *left + 0.5)
                .map(|block| (block.top().max(field.top()), block.bottom().min(field.bottom())))
                .collect();
            spans.sort_by(|a, b| a.0.total_cmp(&b.0));
            let mut from = field.top();
            spans.push((field.bottom(), field.bottom()));
            for (top, bottom) in spans {
                if top - from > 0.5 {
                    let found = Rect::from_min_max(pos2(*left, from), pos2(*right, top));
                    if best.is_none_or(|b| found.area() > b.area()) {
                        best = Some(found);
                    }
                }
                from = from.max(bottom);
            }
        }
    }
    best.filter(|rect| rect.width() >= MIN_FREE.0 && rect.height() >= MIN_FREE.1)
}

pub fn is_ultrawide(screen: Rect) -> bool {
    screen.height() > 0.0 && screen.width() / screen.height() >= ULTRAWIDE
}

/// Places `count` windows on a work area: side by side for two, a main window and a stack for three, and a grid
/// for more. An ultrawide screen keeps up to four in one row.
pub fn tile(count: usize, work: Rect, gap: f32) -> Vec<Rect> {
    if count == 0 {
        return Vec::new();
    }
    let field = work.shrink(gap);
    let cells = |columns: usize, rows: usize| grid(field, columns, rows, gap);
    let ultrawide = is_ultrawide(work);
    match count {
        1 => vec![field],
        2 => cells(2, 1),
        3 if ultrawide => cells(3, 1),
        3 => main_and_stack(field, gap, 2),
        4 if ultrawide => cells(4, 1),
        4 => cells(2, 2),
        _ => {
            let aspect = if ultrawide { 2.4 } else { work.width() / work.height().max(1.0) };
            let columns = ((count as f32 * aspect).sqrt().round() as usize).clamp(1, count);
            let rows = count.div_ceil(columns);
            let mut rects = cells(columns, rows);
            rects.truncate(count);
            widen_last_row(&mut rects, field, gap, columns, rows, count);
            rects
        }
    }
}

fn grid(field: Rect, columns: usize, rows: usize, gap: f32) -> Vec<Rect> {
    let cell_w = (field.width() - gap * (columns as f32 - 1.0)) / columns as f32;
    let cell_h = (field.height() - gap * (rows as f32 - 1.0)) / rows as f32;
    let mut rects = Vec::with_capacity(columns * rows);
    for row in 0..rows {
        for column in 0..columns {
            let min = pos2(field.left() + column as f32 * (cell_w + gap), field.top() + row as f32 * (cell_h + gap));
            rects.push(Rect::from_min_size(min, vec2(cell_w, cell_h)));
        }
    }
    rects
}

fn main_and_stack(field: Rect, gap: f32, stacked: usize) -> Vec<Rect> {
    let column_w = (field.width() - gap) * 0.5;
    let stack_h = (field.height() - gap * (stacked as f32 - 1.0)) / stacked as f32;
    let mut rects = vec![Rect::from_min_size(field.min, vec2(column_w, field.height()))];
    for i in 0..stacked {
        let min = pos2(field.left() + column_w + gap, field.top() + i as f32 * (stack_h + gap));
        rects.push(Rect::from_min_size(min, vec2(column_w, stack_h)));
    }
    rects
}

/// Spreads a short last row across the whole width, so it leaves no hole.
fn widen_last_row(rects: &mut [Rect], field: Rect, gap: f32, columns: usize, rows: usize, count: usize) {
    let in_last = count - (rows - 1) * columns;
    if in_last == columns || in_last == 0 {
        return;
    }
    let first = (rows - 1) * columns;
    let band = grid(field, in_last, rows, gap);
    for (rect, cell) in rects[first..].iter_mut().zip(band[(rows - 1) * in_last..].iter()) {
        *rect = *cell;
    }
}

/// Gives each window, in order, the free slot nearest to it, so windows don't travel far.
pub fn assign(rects: &[Rect], wanted: &[Rect]) -> Vec<usize> {
    let mut taken = vec![false; rects.len()];
    let mut order = Vec::with_capacity(wanted.len());
    for current in wanted {
        let nearest = rects
            .iter()
            .enumerate()
            .filter(|(i, _)| !taken[*i])
            .min_by(|a, b| distance(a.1, current).total_cmp(&distance(b.1, current)))
            .map(|(i, _)| i);
        match nearest {
            Some(i) => {
                taken[i] = true;
                order.push(i);
            }
            None => break,
        }
    }
    order
}

fn distance(a: &Rect, b: &Rect) -> f32 {
    a.center().distance(b.center())
}

#[cfg(test)]
mod tests {
    use super::*;

    const LAPTOP: Rect = Rect::from_min_max(pos2(0.0, 38.0), pos2(1512.0, 900.0));
    const WIDE: Rect = Rect::from_min_max(pos2(0.0, 0.0), pos2(3440.0, 1440.0));

    fn overlap(a: Rect, b: Rect) -> f32 {
        let w = (a.right().min(b.right()) - a.left().max(b.left())).max(0.0);
        let h = (a.bottom().min(b.bottom()) - a.top().max(b.top())).max(0.0);
        w * h
    }

    #[test]
    fn halves_leave_one_gap_everywhere() {
        let left = Zone::Left.rect(LAPTOP, 8.0);
        let right = Zone::Right.rect(LAPTOP, 8.0);
        assert_eq!(left.left(), 8.0);
        assert_eq!(right.right(), 1512.0 - 8.0);
        assert!((right.left() - left.right() - 8.0).abs() < 1e-3);
        assert_eq!((left.top(), left.bottom()), (LAPTOP.top() + 8.0, LAPTOP.bottom() - 8.0));
    }

    #[test]
    fn quarters_tile_without_overlap() {
        let quarters =
            [Zone::TopLeft, Zone::TopRight, Zone::BottomLeft, Zone::BottomRight].map(|z| z.rect(LAPTOP, 8.0));
        for (i, a) in quarters.iter().enumerate() {
            for b in &quarters[i + 1..] {
                assert_eq!(overlap(*a, *b), 0.0);
            }
        }
        let widths: Vec<f32> = quarters.iter().map(|r| r.width()).collect();
        assert!(widths.iter().all(|w| (w - widths[0]).abs() < 1e-3));
    }

    #[test]
    fn thirds_are_equal_and_ordered() {
        let thirds = [Zone::LeftThird, Zone::CenterThird, Zone::RightThird].map(|z| z.rect(WIDE, 8.0));
        assert!(thirds[0].right() < thirds[1].left() && thirds[1].right() < thirds[2].left());
        assert!((thirds[0].width() - thirds[1].width()).abs() < 1e-3);
        assert!((thirds[1].width() - thirds[2].width()).abs() < 1e-3);
    }

    #[test]
    fn tiling_fills_without_overlap() {
        for screen in [LAPTOP, WIDE] {
            for count in 1..=9 {
                let rects = tile(count, screen, 8.0);
                assert_eq!(rects.len(), count, "{count} windows on {screen:?}");
                let field = screen.shrink(8.0);
                for (i, a) in rects.iter().enumerate() {
                    assert!(field.expand(0.01).contains_rect(*a), "{count}: {a:?} leaves the field");
                    assert!(a.width() > 100.0 && a.height() > 100.0);
                    for b in &rects[i + 1..] {
                        assert!(overlap(*a, *b) < 0.01, "{count}: {a:?} overlaps {b:?}");
                    }
                }
            }
        }
    }

    #[test]
    fn three_get_main_and_stack_on_laptop() {
        let rects = tile(3, LAPTOP, 8.0);
        assert_eq!(rects[0].height(), LAPTOP.shrink(8.0).height());
        assert_eq!(rects[1].left(), rects[2].left());
        assert!(rects[1].bottom() < rects[2].top());
    }

    #[test]
    fn short_last_row_is_widened() {
        let rects = tile(5, LAPTOP, 8.0);
        let field = LAPTOP.shrink(8.0);
        let right = rects.iter().map(|r| r.right()).fold(f32::MIN, f32::max);
        assert!((right - field.right()).abs() < 0.01);
        let bottom_row: Vec<&Rect> = rects.iter().filter(|r| r.bottom() > field.bottom() - 1.0).collect();
        let covered: f32 = bottom_row.iter().map(|r| r.width()).sum::<f32>() + 8.0 * (bottom_row.len() as f32 - 1.0);
        assert!((covered - field.width()).abs() < 0.01);
    }

    #[test]
    fn fill_takes_what_a_third_leaves() {
        let third = Zone::LeftThird.rect(WIDE, 8.0);
        let free = free_space(WIDE, 8.0, &[third]).unwrap();
        assert!((free.left() - third.right() - 8.0).abs() < 1e-3);
        assert_eq!((free.right(), free.top(), free.bottom()), (WIDE.right() - 8.0, 8.0, WIDE.bottom() - 8.0));
    }

    #[test]
    fn fill_picks_biggest_gap_no_overlap() {
        let corner = Zone::TopLeft.rect(LAPTOP, 8.0);
        let free = free_space(LAPTOP, 8.0, &[corner]).unwrap();
        assert_eq!(overlap(free, corner), 0.0);
        assert!(free.area() > corner.area() * 2.0);
    }

    #[test]
    fn fill_on_empty_and_full_screens() {
        assert_eq!(free_space(LAPTOP, 8.0, &[]), Some(LAPTOP.shrink(8.0)));
        assert_eq!(free_space(LAPTOP, 8.0, &[Zone::Full.rect(LAPTOP, 8.0)]), None);
    }

    #[test]
    fn windows_keep_the_nearest_slot() {
        let slots = tile(2, LAPTOP, 8.0);
        let on_right = Rect::from_min_size(pos2(1000.0, 100.0), vec2(300.0, 300.0));
        let on_left = Rect::from_min_size(pos2(50.0, 100.0), vec2(300.0, 300.0));
        assert_eq!(assign(&slots, &[on_right, on_left]), [1, 0]);
    }
}
