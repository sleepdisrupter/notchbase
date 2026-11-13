use egui::{Event, Key, Modifiers, Pos2, pos2, vec2};

use super::super::bench::{Bench, button};
use super::super::picture::Picture;
use super::{IMAGE, WINDOW, across, bench, shown};
use crate::annotate::composite::composite;
use crate::annotate::testing::{as_image, solid};
use crate::annotate::{Brush, Outcome, SMALLEST_WINDOW, Stroke};

const SWING: f32 = 60.0;

fn first_line(bench: &Bench) -> Stroke {
    bench.editor.job().marks.first().and_then(|mark| mark.line().cloned()).expect("a line")
}

#[test]
fn drag_draws_exactly_one_line() {
    let mut bench = bench();
    let outcomes = bench.drag(&across(&bench));
    assert_eq!(outcomes, [Outcome::Changed(1)]);
    let job = bench.editor.job();
    assert_eq!((job.revision, job.marks.len()), (1, 1));
    assert_eq!(first_line(&bench).color(), Brush::DEFAULT_COLOR);
    assert!(bench.editor.pen.is_none());
}

#[test]
fn line_as_wide_as_brush_at_any_zoom() {
    let mut exact = bench();
    exact.drag(&across(&exact));
    assert_eq!(exact.fit().scale(), 0.5);
    assert_eq!(first_line(&exact).width(), Brush::DEFAULT_SIZE * 2.0);
    let mut shrunk = Bench::new(solid(3200, 1600, [250, 250, 250]), Brush::default(), WINDOW);
    shrunk.drag(&across(&shrunk));
    let width = first_line(&shrunk).width();
    assert!((width * shrunk.fit().scale() - Brush::DEFAULT_SIZE).abs() < 1e-3 && width > Brush::DEFAULT_SIZE * 2.0);
}

#[test]
fn press_outside_picture_does_nothing() {
    let mut bench = bench();
    let [_, middle, end] = across(&bench);
    let beside = bench.fit().rect().left_center() - vec2(12.0, 0.0);
    assert!(bench.layout().stage.contains(beside));
    let outcomes = bench.drag(&[beside, middle, end]);
    assert_eq!(outcomes, []);
    assert_eq!((bench.editor.job().revision, bench.editor.job().marks.len()), (0, 0));
}

#[test]
fn every_sample_joins_the_line() {
    let mut bench = bench();
    let picture = bench.fit().rect();
    let start = picture.center();
    let (up, down) = (start + vec2(60.0, -70.0), start + vec2(120.0, 70.0));
    bench.press(start);
    let burst = [up, down, start + vec2(180.0, 0.0)].map(Event::PointerMoved).to_vec();
    assert_eq!(bench.frame(burst), []);
    bench.release(start + vec2(180.0, 0.0));
    let reached = first_line(&bench).bounds();
    let level = bench.fit().to_image(start).y;
    assert!(reached.top() < level - SWING && reached.bottom() > level + SWING, "{reached:?}");
}

#[test]
fn line_stops_at_picture_edge() {
    let mut bench = bench();
    let picture = bench.fit().rect();
    let outside = picture.right_bottom() + vec2(300.0, 200.0);
    bench.drag(&[picture.center(), outside]);
    let stroke = first_line(&bench);
    assert_eq!(stroke.points().last(), Some(&pos2(IMAGE[0] as f32, IMAGE[1] as f32)));
    let sides = egui::Rect::from_min_size(Pos2::ZERO, vec2(IMAGE[0] as f32, IMAGE[1] as f32));
    assert!(stroke.points().iter().all(|point| sides.contains(*point)));
}

#[test]
fn screen_matches_clipboard() {
    let mut bench = bench();
    let [from, middle, to] = across(&bench);
    bench.drag(&[from, middle, to]);
    bench.key(Key::Num3, Modifiers::NONE);
    bench.drag(&[pos2(to.x, from.y), middle, pos2(from.x, to.y)]);
    let on_clipboard = |bench: &Bench| {
        let job = bench.editor.job();
        as_image(&composite(&job.base, &job.marks))
    };
    assert_eq!(bench.editor.job().marks.len(), 2);
    assert!(shown(&bench) == on_clipboard(&bench) && shown(&bench) != as_image(&bench.editor.job().base));
    bench.key(Key::Z, Modifiers::COMMAND);
    assert!(shown(&bench) == on_clipboard(&bench));
    bench.key(Key::Z, Modifiers::COMMAND);
    assert!(shown(&bench) == as_image(&bench.editor.job().base));
    bench.key(Key::Z, Modifiers::COMMAND | Modifiers::SHIFT);
    assert!(shown(&bench) == on_clipboard(&bench));
}

#[test]
fn escape_drops_line_in_progress() {
    let mut bench = bench();
    let [from, middle, _] = across(&bench);
    bench.press(from);
    bench.hover(middle);
    assert!(bench.editor.pen.is_some());
    assert_eq!(bench.key(Key::Escape, Modifiers::NONE), []);
    assert!(bench.editor.pen.is_none());
    assert_eq!(bench.release(middle), []);
    assert_eq!(bench.editor.job().revision, 0);
}

#[test]
fn picture_clears_toolbar_and_title() {
    for image in [[40, 30], IMAGE, [5120, 2880], [300, 4000], [6000, 200]] {
        let bench = Bench::new(solid(image[0], image[1], [20, 30, 40]), Brush::default(), SMALLEST_WINDOW);
        let (layout, picture) = (bench.layout(), bench.fit().rect());
        assert!(layout.stage.expand(0.5).contains_rect(picture), "{image:?}: {picture:?}");
        assert!(!picture.intersects(layout.toolbar) && !picture.intersects(layout.title), "{image:?}");
        assert!(layout.window.contains_rect(layout.toolbar));
        assert!(layout.swatches.iter().all(|swatch| !swatch.intersects(picture)));
    }
}

#[test]
fn oversized_screenshot_is_explained() {
    let mut bench = Bench::new(solid(16_385, 2, [9, 9, 9]), Brush::default(), WINDOW);
    assert!(matches!(bench.editor.picture, Picture::TooLarge(16_384)));
    let outcomes = bench.drag(&across(&bench));
    assert_eq!(outcomes, []);
    assert_eq!(bench.key(Key::Escape, Modifiers::NONE), [Outcome::Close]);
}

#[test]
fn unseen_release_ends_the_line() {
    let mut bench = bench();
    let [from, middle, _] = across(&bench);
    bench.press(from);
    bench.hover(middle);
    assert_eq!(bench.frame(vec![button(middle, false)]), [Outcome::Changed(1)]);
    bench.press(from);
    assert!(bench.editor.pen.is_some());
}
