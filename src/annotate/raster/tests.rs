use std::f32::consts::PI;

use egui::pos2;

use super::capsule::Capsule;
use super::*;
use crate::annotate::testing::{Dice, line, solid, textured, through};
use crate::capture::Screenshot;
use crate::ui::color;

const WHITE: [u8; 3] = [255, 255, 255];
const WIDTHS: [f32; 6] = [0.6, 1.0, 2.5, 9.0, 31.0, 140.0];
const NEARLY_LEVEL: f32 = 1.0e-3;
const LARGE_IMAGE: [usize; 2] = [1000, 1000];
const CORNER_TO_CORNER: [[f32; 2]; 2] = [[10.0, 10.0], [990.0, 990.0]];
const THIN: f32 = 4.0;
const ROOM_FOR_SOFT_EDGES: usize = 4;
const BOX_TIMES_THE_WORK: usize = 30;

fn inked(base: &Screenshot, stroke: &Stroke) -> Screenshot {
    let mut shot = base.clone();
    let size = [shot.width, shot.height];
    ink(&mut shot.rgba, size, PixelRect::of_image(size), stroke);
    shot
}

fn strongest_segment_wins(base: &Screenshot, stroke: &Stroke) -> Vec<u8> {
    let dot = stroke.points().first().map(|&tap| [tap, tap]);
    let segments = dot.into_iter().chain(stroke.points().windows(2).map(|pair| [pair[0], pair[1]]));
    let capsules: Vec<Capsule> = segments.map(|[from, to]| Capsule::new(from, to, stroke.radius())).collect();
    let ink = stroke.color().to_array();
    let mut rgba = base.rgba.clone();
    for (index, pixel) in rgba.as_chunks_mut::<CHANNELS>().0.iter_mut().enumerate() {
        let centre = pos2((index % base.width) as f32 + PIXEL_CENTRE, (index / base.width) as f32 + PIXEL_CENTRE);
        let level = capsules.iter().map(|capsule| capsule.coverage(centre)).max().unwrap_or(0);
        for (channel, ink) in pixel.iter_mut().zip(&ink[..3]) {
            *channel = lerp(*channel, *ink, level);
        }
    }
    rgba
}

fn red_at(shot: &Screenshot, x: usize, y: usize) -> u8 {
    shot.rgba[(y * shot.width + x) * CHANNELS]
}

#[test]
fn tap_is_a_brush_wide_dot() {
    let dot = inked(&solid(40, 40, WHITE), &through(color::INK, 16.0, &[[20.0, 20.0]]));
    let darkness = |x: usize, y: usize| f32::from(u8::MAX - red_at(&dot, x, y)) / f32::from(u8::MAX - color::INK.r());
    let area: f32 = (0..40).flat_map(|y| (0..40).map(move |x| (x, y))).map(|(x, y)| darkness(x, y)).sum();
    assert!((area - PI * 8.0 * 8.0).abs() < 1.5, "{area}");
    assert_eq!((red_at(&dot, 20, 20), red_at(&dot, 15, 15)), (color::INK.r(), color::INK.r()));
    assert_eq!((red_at(&dot, 30, 20), red_at(&dot, 20, 9), red_at(&dot, 13, 13)), (255, 255, 255));
    for (x, y) in [(12, 20), (20, 27), (14, 14), (15, 13)] {
        let soft = red_at(&dot, x, y);
        assert!(soft > color::INK.r() && soft < u8::MAX, "the rim is soft at {x},{y}");
        let mirrored = [red_at(&dot, 39 - x, y), red_at(&dot, x, 39 - y), red_at(&dot, y, x)];
        assert_eq!(mirrored, [soft; 3], "the dot is the same on every side at {x},{y}");
    }
}

#[test]
fn on_grid_crisp_between_rows_soft() {
    let on_grid = inked(&solid(40, 20, WHITE), &line(color::INK, 4.0, [10.0, 10.0], [30.0, 10.0]));
    let column: Vec<u8> = (6..14).map(|y| red_at(&on_grid, 20, y)).collect();
    assert_eq!(column, [255, 255, 14, 14, 14, 14, 255, 255]);
    let between = inked(&solid(40, 20, WHITE), &line(color::INK, 4.0, [10.0, 10.5], [30.0, 10.5]));
    let column: Vec<u8> = (6..15).map(|y| red_at(&between, 20, y)).collect();
    assert_eq!(column, [255, 255, 134, 14, 14, 14, 134, 255, 255]);
}

#[test]
fn line_ends_are_round() {
    let capped = inked(&solid(60, 40, WHITE), &line(color::INK, 20.0, [20.0, 20.0], [40.0, 20.0]));
    assert_eq!((red_at(&capped, 11, 20), red_at(&capped, 48, 20)), (14, 14), "the cap reaches past the end");
    assert_eq!((red_at(&capped, 11, 11), red_at(&capped, 48, 28)), (255, 255), "but its corners are cut");
    assert_eq!((red_at(&capped, 9, 20), red_at(&capped, 50, 20)), (255, 255));
}

#[test]
fn strongest_coverage_wins() {
    let base = textured(96, 80);
    let strokes = [
        through(color::RED, 9.0, &[[8.0, 8.0], [80.0, 20.0], [20.0, 60.0], [70.0, 70.0], [40.3, 5.7]]),
        through(color::GREEN, 1.0, &[[5.5, 70.5], [90.0, 12.0]]),
        through(color::YELLOW, 31.0, &[[48.0, 40.0], [52.0, 43.0], [47.0, 47.0]]),
        through(color::PEARL, 5.0, &[[-20.0, 30.0], [30.0, -12.0], [120.0, 95.0]]),
        through(color::INK, 3.3, &[[60.2, 33.9]]),
    ];
    for stroke in &strokes {
        assert!(inked(&base, stroke).rgba == strongest_segment_wins(&base, stroke), "{:?}", stroke.bounds());
    }
}

#[test]
fn any_slant_matches_the_slow_way() {
    let (base, mut dice) = (textured(96, 80), Dice(8));
    for round in 0..240 {
        let (from, width) = (dice.spot([96, 80]), WIDTHS[dice.below(6) as usize]);
        let to = match round % 3 {
            0 => [from[0] + 20.0 + dice.below(70) as f32, from[1] + dice.below(9) as f32 * NEARLY_LEVEL],
            1 => [from[0] + dice.below(9) as f32 * NEARLY_LEVEL, from[1] - 20.0 - dice.below(70) as f32],
            _ => dice.spot([96, 80]),
        };
        let stroke = line(color::RED, width, from, to);
        assert!(inked(&base, &stroke).rgba == strongest_segment_wins(&base, &stroke), "{width} px {from:?} {to:?}");
    }
}

#[test]
fn retracing_adds_no_ink() {
    let base = textured(96, 48);
    let once = inked(&base, &through(color::RED, 7.0, &[[10.0, 20.5], [80.0, 20.5]]));
    let there_and_back = [[10.0, 20.5], [80.0, 20.5], [10.0, 20.5], [80.0, 20.5]];
    let thrice = inked(&base, &through(color::RED, 7.0, &there_and_back));
    assert!(once.rgba == thrice.rgba);
    assert!(once.rgba != base.rgba);
}

#[test]
fn ink_mixes_in_bytes_spares_alpha() {
    assert_eq!((lerp(255, 236, 0), lerp(255, 236, 255)), (255, 236));
    assert_eq!((lerp(0, 255, 128), lerp(255, 0, 128), lerp(255, 236, 128)), (128, 127, 245));
    assert_eq!((lerp(0, 255, 1), lerp(0, 255, 254), lerp(200, 200, 77)), (1, 254, 200));
    let soft = inked(&solid(40, 20, WHITE), &line(color::RED, 4.0, [10.0, 10.5], [30.0, 10.5]));
    let at = (8 * 40 + 20) * CHANNELS;
    assert_eq!(soft.rgba[at..at + CHANNELS], [245, 169, 172, 255]);
    assert!(soft.rgba.as_chunks::<CHANNELS>().0.iter().all(|pixel| pixel[3] == u8::MAX));
}

#[test]
fn clip_limits_the_ink() {
    let base = textured(96, 80);
    let stroke = through(color::RED, 11.0, &[[8.0, 8.0], [80.0, 20.0], [20.0, 60.0], [70.0, 70.0]]);
    let whole = inked(&base, &stroke);
    let clip = PixelRect { x: 30, y: 10, width: 25, height: 44 };
    let mut clipped = base.clone();
    ink(&mut clipped.rgba, [96, 80], clip, &stroke);
    for (index, pixel) in clipped.rgba.as_chunks::<CHANNELS>().0.iter().enumerate() {
        let (x, y) = (index % 96, index / 96);
        let inside = (clip.x..clip.right()).contains(&x) && (clip.y..clip.bottom()).contains(&y);
        let expected = if inside { &whole } else { &base };
        assert_eq!(pixel[..], expected.rgba[index * CHANNELS..(index + 1) * CHANNELS], "{x},{y}");
    }
    assert!(clipped.rgba != base.rgba && clipped.rgba != whole.rgba);
}

#[test]
fn nothing_outside_bounds_touched() {
    let base = textured(200, 120);
    let stroke = through(color::GREEN, 6.0, &[[40.0, 30.0], [70.0, 60.0], [100.0, 40.0]]);
    let drawn = inked(&base, &stroke);
    let bounds = PixelRect::covering(stroke.bounds(), [200, 120]);
    for (index, (after, before)) in drawn.rgba.iter().zip(&base.rgba).enumerate() {
        let (x, y) = (index / CHANNELS % 200, index / CHANNELS / 200);
        let inside = (bounds.x..bounds.right()).contains(&x) && (bounds.y..bounds.bottom()).contains(&y);
        assert!(inside || after == before, "{x},{y}");
    }
}

#[test]
fn work_follows_the_line() {
    let diagonal = through(color::RED, THIN, &CORNER_TO_CORNER);
    let mut mask = Mask::new(PixelRect::covering(diagonal.bounds(), LARGE_IMAGE));
    diagonal.runs().for_each(|(_, points)| mask.trace(points, diagonal.radius()));
    let touched: usize = mask.touched.iter().map(ExactSizeIterator::len).sum();
    let [from, to] = CORNER_TO_CORNER.map(|[x, y]| pos2(x, y));
    let length_times_width = (from.distance(to) * THIN) as usize;
    assert!(touched < length_times_width * ROOM_FOR_SOFT_EDGES, "{touched} pixels for a line of {length_times_width}");
    assert!(touched * BOX_TIMES_THE_WORK < mask.coverage.len());
}

#[test]
fn stray_stroke_or_buffer_untouched() {
    let base = textured(32, 32);
    let outside = line(color::RED, 6.0, [-40.0, -40.0], [-10.0, -20.0]);
    assert!(inked(&base, &outside).rgba == base.rgba);
    let mut short = vec![9u8; 100];
    ink(&mut short, [32, 32], PixelRect::of_image([32, 32]), &line(color::RED, 6.0, [0.0, 0.0], [31.0, 31.0]));
    assert_eq!(short, vec![9u8; 100]);
    let mut empty = Vec::new();
    ink(&mut empty, [0, 0], PixelRect::EMPTY, &line(color::RED, 6.0, [0.0, 0.0], [31.0, 31.0]));
    assert!(empty.is_empty());
}
