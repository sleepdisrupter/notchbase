use egui::{Color32, Pos2, Rect, pos2, vec2};

use super::size::SizeControl;
use super::swatches::Swatches;
use super::tools::Tools;
use super::*;
use crate::annotate::Brush;
use crate::ui::color;
use crate::ui::snapshot::Camera;

const SCALE: f32 = 2.0;
const BACKDROP: Color32 = PANE.tint;
const SLOT: f32 = 28.0;
const BEAD_EDGE: f32 = 9.0;
const JUST_OUTSIDE: f32 = 1.0;
const CLEARLY_LIGHTER: i32 = 24;
const PEARLY: i32 = 500;
const OFF_THE_GLYPH: egui::Vec2 = vec2(7.0, 7.0);
const SQUIGGLE_DIP: egui::Vec2 = vec2(0.0, 3.6);
const JUST_OVER_THE_CAP: egui::Vec2 = vec2(7.0, -11.5);
const SETTLING_FRAMES: usize = 120;
const DT: f32 = 1.0 / 60.0;

fn slots() -> [Rect; 5] {
    std::array::from_fn(|index| Rect::from_min_size(pos2(index as f32 * SLOT, 0.0), vec2(SLOT, SLOT)))
}

fn shot(paint: impl Fn(&mut Canvas)) -> image::RgbImage {
    let size = vec2(5.0 * SLOT, SLOT);
    Camera::new(size, SCALE).shoot(Vec::new(), |ui| {
        ui.painter().rect_filled(Rect::from_min_size(Pos2::ZERO, size), 0.0, BACKDROP);
        let mut canvas = Canvas::new(ui.painter());
        paint(&mut canvas);
        canvas.paint(ui.painter(), Pos2::ZERO, 1.0, vec2(0.0, 0.0), 1.0);
    })
}

fn lift(shot: &image::RgbImage, at: Pos2) -> i32 {
    let pixel = shot.get_pixel((at.x * SCALE) as u32, (at.y * SCALE) as u32).0;
    let brightness = |rgb: [u8; 3]| rgb.into_iter().map(i32::from).sum::<i32>();
    brightness(pixel) - brightness([BACKDROP.r(), BACKDROP.g(), BACKDROP.b()])
}

#[test]
fn only_ink_swatch_has_light_edge() {
    let slots = slots();
    let shot = shot(|canvas| Swatches::new(Brush::new(color::GREEN, 5.0)).paint(canvas, &slots));
    let beside = |index: usize| slots[index].center() + vec2(0.0, BEAD_EDGE + JUST_OUTSIDE);
    assert!(lift(&shot, beside(4)) >= CLEARLY_LIGHTER, "{}", lift(&shot, beside(4)));
    assert_eq!(lift(&shot, beside(0)), 0, "garnet stands out by itself");
    assert_eq!(lift(&shot, slots[4].center() + vec2(0.0, BEAD_EDGE + 4.0)), 0, "the edge is thin");
    assert!(lift(&shot, slots[4].center()) < 0, "the bead itself stays ink");
}

#[test]
fn thin_ink_brush_still_visible() {
    let [well, _, slider, ..] = slots();
    let thin = Brush::new(color::INK, Brush::THINNEST);
    let shot = shot(|canvas| SizeControl::default().paint(canvas, well, slider, thin));
    let beside = well.center() + vec2(0.0, Brush::THINNEST / 2.0 + JUST_OUTSIDE);
    assert!(lift(&shot, beside) >= CLEARLY_LIGHTER, "{}", lift(&shot, beside));
    assert!(lift(&shot, well.center()) < 0, "the dot itself stays ink");
    let jade = Brush::new(color::GREEN, Brush::THINNEST);
    let shot = shot_of_jade(well, slider, jade);
    assert_eq!(lift(&shot, beside), 0);
}

fn shot_of_jade(well: Rect, slider: Rect, brush: Brush) -> image::RgbImage {
    shot(|canvas| SizeControl::default().paint(canvas, well, slider, brush))
}

#[test]
fn chosen_tool_sits_on_pearl_thumb() {
    let [brush, text, ..] = slots();
    let with_text = shot(|canvas| Tools::new(Tool::Text).paint(canvas, &[brush, text]));
    assert!(lift(&with_text, text.center() + OFF_THE_GLYPH) >= PEARLY);
    assert!(lift(&with_text, text.center()) < 0, "the letter on the thumb is ink");
    let groove = lift(&with_text, brush.center() + OFF_THE_GLYPH);
    assert!(groove > 0 && groove < PEARLY / 4, "{groove}");
    assert!(lift(&with_text, brush.center() + SQUIGGLE_DIP) >= CLEARLY_LIGHTER, "the squiggle beside it is light");
    let with_brush = shot(|canvas| Tools::new(Tool::Brush).paint(canvas, &[brush, text]));
    assert!(lift(&with_brush, brush.center() + OFF_THE_GLYPH) >= PEARLY);
    assert!(lift(&with_brush, text.center() + OFF_THE_GLYPH) < PEARLY / 4);
    assert!(lift(&with_brush, text.center()) >= CLEARLY_LIGHTER, "the letter beside it is light");
}

#[test]
fn thumb_glides_then_rests() {
    let mut tools = Tools::new(Tool::Brush);
    assert!(tools.is_settled());
    tools.step(Tool::Text, DT);
    assert!(!tools.is_settled());
    (0..SETTLING_FRAMES).for_each(|_| tools.step(Tool::Text, DT));
    assert!(tools.is_settled());
    let [brush, text, ..] = slots();
    let shot = shot(|canvas| tools.paint(canvas, &[brush, text]));
    assert!(lift(&shot, text.center() + OFF_THE_GLYPH) >= PEARLY);
}

#[test]
fn well_shows_dot_or_letter() {
    let [well, _, slider, ..] = slots();
    let jade = Brush::new(color::GREEN, 14.0);
    let inside_the_dot = well.center() + vec2(5.0, 3.0);
    let mut control = SizeControl::default();
    let dotted = shot(|canvas| control.paint(canvas, well, slider, jade));
    assert!(lift(&dotted, inside_the_dot) >= CLEARLY_LIGHTER);
    control.step(Tool::Text, DT);
    assert!(!control.is_settled(), "the dot gives way to the letter on a spring");
    (0..SETTLING_FRAMES).for_each(|_| control.step(Tool::Text, DT));
    assert!(control.is_settled());
    let lettered = shot(|canvas| control.paint(canvas, well, slider, jade));
    assert_eq!(lift(&lettered, inside_the_dot), 0, "the dot is gone");
    assert!(lift(&lettered, well.center()) >= CLEARLY_LIGHTER, "the stem of the letter stands in the middle");
}

#[test]
fn ink_letter_edge_covers_its_cap() {
    let [well, _, slider, ..] = slots();
    let mut control = SizeControl::default();
    (0..SETTLING_FRAMES).for_each(|_| control.step(Tool::Text, DT));
    let lettered = |ink: Color32| shot(|canvas| control.paint(canvas, well, slider, Brush::new(ink, 9.0)));
    let (ink, jade) = (lettered(color::INK), lettered(color::GREEN));
    let over_the_cap = well.center() + JUST_OVER_THE_CAP;
    assert!(lift(&ink, well.center()) < 0, "the stem itself stays ink");
    assert!(lift(&ink, over_the_cap) >= CLEARLY_LIGHTER, "{}", lift(&ink, over_the_cap));
    assert_eq!(lift(&jade, over_the_cap), 0, "a bright letter stands out by itself");
}

#[test]
fn well_letter_grows_until_full() {
    let [well, _, slider, ..] = slots();
    let mut control = SizeControl::default();
    (0..SETTLING_FRAMES).for_each(|_| control.step(Tool::Text, DT));
    let cap_of = |size: f32| {
        let shot = shot(|canvas| control.paint(canvas, well, slider, Brush::new(color::GREEN, size)));
        (0..SLOT as i32)
            .find(|up| lift(&shot, well.center() - vec2(0.0, *up as f32)) == 0)
            .expect("the top of the stem")
    };
    let (thin, default, thick, thickest) = (cap_of(Brush::THINNEST), cap_of(5.0), cap_of(9.0), cap_of(Brush::THICKEST));
    assert!(thin < default && default < thick, "{thin} {default} {thick}");
    assert_eq!(thick, thickest, "past the well's size the letter stays inside it");
    assert!(thickest < (SLOT / 2.0) as i32);
}
