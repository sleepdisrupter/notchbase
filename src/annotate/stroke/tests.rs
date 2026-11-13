use egui::pos2;

use super::smooth::{FLATNESS, HALFWAY, between};
use super::*;
use crate::ui::color;

fn stroke(width: f32, samples: &[Pos2]) -> Stroke {
    Stroke::from_samples(color::RED, width, samples).expect("a stroke")
}

fn farthest_from(points: &[Pos2], curve: impl Fn(f32) -> Pos2) -> f32 {
    let to_the_line =
        |spot: Pos2| points.windows(2).map(|pair| distance(spot, pair[0], pair[1])).fold(f32::MAX, f32::min);
    (0..=1000).map(|step| to_the_line(curve(step as f32 / 1000.0))).fold(0.0, f32::max)
}

fn distance(spot: Pos2, from: Pos2, to: Pos2) -> f32 {
    let along = to - from;
    let travelled = ((spot - from).dot(along) / along.length_sq()).clamp(0.0, 1.0);
    spot.distance(from + along * travelled)
}

#[test]
fn single_tap_is_one_point() {
    let tap = stroke(8.0, &[pos2(40.0, 30.0)]);
    assert_eq!(tap.points(), [pos2(40.0, 30.0)]);
    assert_eq!(tap.runs().map(|(_, points)| points.to_vec()).collect::<Vec<_>>(), [[pos2(40.0, 30.0)]]);
    assert_eq!((tap.color(), tap.width()), (color::RED, 8.0));
}

#[test]
fn no_samples_make_no_stroke() {
    assert_eq!(Stroke::from_samples(color::RED, 8.0, &[]), None);
    assert_eq!(Stroke::from_samples(color::RED, 8.0, &[pos2(f32::NAN, 1.0), pos2(1.0, f32::INFINITY)]), None);
}

#[test]
fn resting_pointer_adds_no_points() {
    let resting = [pos2(10.0, 10.0), pos2(10.0, 10.0), pos2(10.01, 10.0), pos2(10.0, 10.01)];
    assert_eq!(stroke(4.0, &resting).points(), [pos2(10.0, 10.0)]);
    let moving = stroke(4.0, &[pos2(0.0, 0.0), pos2(0.0, 0.0), pos2(9.0, 0.0), pos2(9.0, 0.0), pos2(9.0, 7.0)]);
    assert!(moving.points().windows(2).all(|pair| pair[0] != pair[1]));
}

#[test]
fn bend_stays_within_a_tenth() {
    for reach in [3.0, 40.0, 100.0, 900.0] {
        let drawn = stroke(6.0, &[pos2(0.0, 0.0), pos2(reach, 0.0), pos2(reach, reach)]);
        let (from, corner, to) = (pos2(reach / 2.0, 0.0), pos2(reach, 0.0), pos2(reach, reach / 2.0));
        let curve = |t: f32| between(between(from, corner, t), between(corner, to, t), t);
        let strayed = farthest_from(drawn.points(), curve);
        assert!(strayed <= FLATNESS + 1.0e-3, "{strayed} px off a bend {reach} px wide");
        assert!(drawn.points().len() < 12 + (reach.sqrt() * 2.0) as usize, "{} points", drawn.points().len());
    }
}

#[test]
fn straight_drag_cut_only_at_samples() {
    let line = stroke(6.0, &[pos2(0.0, 0.0), pos2(300.0, 0.0)]);
    assert_eq!(line.points(), [pos2(0.0, 0.0), pos2(150.0, 0.0), pos2(300.0, 0.0)]);
    let slanted = stroke(6.0, &[pos2(0.0, 0.0), pos2(100.0, 50.0), pos2(200.0, 100.0), pos2(900.0, 450.0)]);
    assert_eq!(slanted.points().len(), 5);
}

#[test]
fn thick_line_no_more_segments() {
    let jagged = [pos2(0.0, 0.0), pos2(180.0, 20.0), pos2(190.0, 240.0), pos2(20.0, 200.0), pos2(400.0, 0.0)];
    let (thin, thick) = (stroke(2.0, &jagged), stroke(THICKEST, &jagged));
    assert_eq!(thin.points(), thick.points());
    assert!((20..120).contains(&thin.points().len()), "{}", thin.points().len());
}

#[test]
fn starts_and_ends_with_the_pointer() {
    let samples = [pos2(3.0, 4.0), pos2(50.0, 9.0), pos2(80.0, 70.0), pos2(20.0, 90.0)];
    let drawn = stroke(6.0, &samples);
    assert_eq!(drawn.points().first(), samples.first());
    assert_eq!(drawn.points().last(), samples.last());
}

#[test]
fn corner_is_rounded_not_kinked() {
    let corner = [pos2(0.0, 0.0), pos2(100.0, 0.0), pos2(100.0, 100.0)];
    let drawn = stroke(6.0, &corner);
    assert!(!drawn.points().contains(&corner[1]));
    let nearest = drawn.points().iter().map(|point| point.distance(corner[1])).fold(f32::MAX, f32::min);
    assert!((10.0..30.0).contains(&nearest), "{nearest}");
    let sharpest_turn = drawn
        .points()
        .windows(3)
        .map(|trio| (trio[1] - trio[0]).normalized().dot((trio[2] - trio[1]).normalized()))
        .fold(1.0, f32::min);
    assert!(sharpest_turn > 0.99, "{sharpest_turn}");
}

#[test]
fn curve_stays_inside_the_trace() {
    let samples = [pos2(0.0, 0.0), pos2(60.0, 10.0), pos2(90.0, 80.0), pos2(30.0, 120.0), pos2(0.0, 60.0)];
    let drawn = stroke(4.0, &samples);
    let hull = Rect::from_points(&samples).expand(1.0e-3);
    assert!(drawn.points().iter().all(|point| hull.contains(*point)));
    for pair in samples.windows(2) {
        let midpoint = between(pair[0], pair[1], HALFWAY);
        assert!(drawn.points().contains(&midpoint), "the curve passes through {midpoint:?}");
    }
}

#[test]
fn bounds_hold_line_and_soft_edge() {
    let drawn = stroke(10.0, &[pos2(20.0, 30.0), pos2(60.0, 30.0)]);
    assert_eq!(drawn.bounds(), Rect::from_min_max(pos2(14.0, 24.0), pos2(66.0, 36.0)));
    assert_eq!(stroke(4.0, &[pos2(9.0, 9.0)]).bounds(), Rect::from_min_max(pos2(6.0, 6.0), pos2(12.0, 12.0)));
}

#[test]
fn impossible_width_made_drawable() {
    let tap = [pos2(1.0, 1.0)];
    assert_eq!(stroke(0.0, &tap).width(), THINNEST);
    assert_eq!(stroke(f32::NAN, &tap).width(), THINNEST);
    assert_eq!(stroke(f32::INFINITY, &tap).width(), THICKEST);
    assert_eq!(stroke(-3.0, &tap).radius(), THINNEST / 2.0);
}

#[test]
fn stroke_colour_is_opaque() {
    let glassy = Color32::from_rgba_premultiplied(9, 8, 7, 100);
    let drawn = Stroke::from_samples(glassy, 4.0, &[pos2(1.0, 1.0)]).expect("a stroke");
    assert_eq!(drawn.color(), Color32::from_rgb(9, 8, 7));
}

#[test]
fn runs_share_their_end_points() {
    let zigzag = [[0.0, 0.0], [90.0, 40.0], [200.0, 10.0], [260.0, 80.0], [120.0, 120.0], [20.0, 90.0]];
    let drawn = stroke(6.0, &zigzag.map(|[x, y]| pos2(x, y)));
    let runs: Vec<(Rect, &[Pos2])> = drawn.runs().collect();
    assert!(runs.len() > 5);
    assert_eq!(runs[0].1.first(), drawn.points().first());
    assert_eq!(runs[runs.len() - 1].1.last(), drawn.points().last());
    assert!(runs.windows(2).all(|pair| pair[0].1.last() == pair[1].1.first()));
    let walked: usize = runs.iter().map(|(_, points)| points.len() - 1).sum();
    assert_eq!(walked, drawn.points().len() - 1, "every segment is walked exactly once");
    for (reach, points) in &runs {
        assert!(points.len() <= RUN + 1);
        assert!(points.iter().all(|point| reach.shrink(3.0 + ANTI_ALIAS - 1.0e-3).contains(*point)));
        assert!(drawn.bounds().contains_rect(*reach));
    }
}
