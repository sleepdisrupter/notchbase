//! Paints the nodes that only show something: text, icons, progress bars, rings, dots, badges, images and timers.

use egui::epaint::RectShape;
use egui::{Align2, Color32, CornerRadius, Id, Pos2, Rect, Shape, Stroke, pos2, vec2};
use notchbase_sdk::protocol::{Align, Node, TextNode, Tone};

use crate::ui::color::white;
use crate::ui::glass::{self, Glass};
use crate::ui::{controls, glyph as glyphs, list};

use super::measure::{PROGRESS_H, timer_label};
use super::style::{glyph, ink, rgba, text_font, tone};
use super::{Scene, shortcut};

const RING_TRACK: f32 = 0.18;
const RING_STEPS: f32 = 96.0;
const ROUND_CAPS: f32 = 3.0;
const IMAGE_EDGE: f32 = 0.16;
const FULL_UV: Rect = Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0));

pub(super) fn paint(scene: &mut Scene, node: &Node, rect: Rect, id: Id) {
    match node {
        Node::Text(text) => paint_text(scene, text, rect),
        Node::Icon { icon, size, tone: t } => glyphs::paint(scene.canvas, glyph(*icon), rect.center(), *size, tone(*t)),
        Node::Divider => {
            list::hairline(scene.canvas, Rect::from_center_size(rect.center(), vec2(rect.width(), 1.0)));
        }
        Node::Keycaps { keys } => {
            shortcut::place_keycaps(scene.canvas, rect, keys);
        }
        Node::Progress { value, tone: t } => progress(scene, rect, *value, *t),
        Node::Ring { value, size, tone: t, span, stroke, center } => {
            let share = span.map_or(*value, |span| span.left_at(scene.env.now_ms));
            scene.state.counting |= span.is_some();
            let width = stroke.unwrap_or_else(|| (*size * 0.15).clamp(1.5, 3.0));
            ring(scene, rect.center(), (*size - width) * 0.5, share, *t, width);
            if let Some(center) = center {
                let fits = scene.measure().size(center, *size - 2.0 * width);
                scene.paint(center, Rect::from_center_size(rect.center(), fits), id.with("center"));
            }
        }
        Node::Swatch { color, size } => swatch(scene, rect.center(), *size * 0.5, rgba(*color)),
        Node::Dot { color, size } => scene.canvas.circle(rect.center(), *size * 0.5, ink(*color)),
        Node::Badge { icon, size, tone: t } => {
            let fill = tone(*t);
            scene.canvas.circle(rect.center(), *size * 0.5, fill);
            glyphs::paint(scene.canvas, glyph(*icon), rect.center(), *size * 0.5, crate::ui::color::ink_on(fill));
        }
        Node::Image { source, radius, .. } => image(scene, source, rect, *radius),
        Node::Timer { at, mode, style, tone: t } => {
            let label = timer_label(*at, *mode, scene.env.now_ms);
            scene.canvas.text(rect.left_center(), Align2::LEFT_CENTER, label, text_font(*style), tone(*t));
        }
        _ => {}
    }
}

fn paint_text(scene: &mut Scene, text: &TextNode, rect: Rect) {
    let font = text_font(text.style);
    let color = tone(text.tone);
    if text.wrap {
        let mut job = egui::text::LayoutJob::simple(text.text.clone(), font, color, rect.width().max(1.0));
        job.halign = match text.align {
            Align::Start => egui::Align::LEFT,
            Align::Center => egui::Align::Center,
            Align::End => egui::Align::RIGHT,
        };
        let galley = scene.ui.painter().layout_job(job);
        let x = match text.align {
            Align::Start => rect.left(),
            Align::Center => rect.center().x,
            Align::End => rect.right(),
        };
        scene.canvas.add(Shape::galley(pos2(x, rect.top()), galley, color));
        return;
    }
    let shown = controls::truncate(scene.canvas, &text.text, font.clone(), rect.width() + 0.5);
    let (at, anchor) = match text.align {
        Align::Start => (rect.left_center(), Align2::LEFT_CENTER),
        Align::Center => (rect.center(), Align2::CENTER_CENTER),
        Align::End => (rect.right_center(), Align2::RIGHT_CENTER),
    };
    scene.canvas.text(at, anchor, shown, font, color);
}

fn progress(scene: &mut Scene, rect: Rect, value: f32, t: Tone) {
    let track = Rect::from_center_size(rect.center(), vec2(rect.width(), PROGRESS_H));
    glass::paint(scene.canvas, track, PROGRESS_H * 0.5, Glass::CONTROL);
    let share = value.clamp(0.0, 1.0);
    if share > 0.0 {
        let fill = Rect::from_min_size(track.min, vec2((track.width() * share).max(PROGRESS_H), PROGRESS_H));
        scene.canvas.rect(fill, PROGRESS_H * 0.5, tone(t));
    }
}

fn ring(scene: &mut Scene, center: Pos2, radius: f32, value: f32, t: Tone, width: f32) {
    scene.canvas.ring(center, radius, Stroke::new(width, white(RING_TRACK)));
    let share = value.clamp(0.0, 1.0);
    if share <= 0.0 {
        return;
    }
    let start = -std::f32::consts::FRAC_PI_2;
    let steps = (share * RING_STEPS).ceil().max(1.0) as usize;
    let arc: Vec<Pos2> = (0..=steps)
        .map(|i| {
            let angle = start + std::f32::consts::TAU * share * i as f32 / steps as f32;
            center + vec2(angle.cos(), angle.sin()) * radius
        })
        .collect();
    if width > ROUND_CAPS {
        for end in [arc.first(), arc.last()].into_iter().flatten() {
            scene.canvas.circle(*end, width * 0.5, tone(t));
        }
    }
    scene.canvas.add(Shape::line(arc, Stroke::new(width, tone(t))));
}

fn swatch(scene: &mut Scene, center: Pos2, radius: f32, color: Color32) {
    scene.canvas.circle(center, (radius - 0.75).max(0.0), color);
    glass::ring(scene.canvas, center, (radius - 1.5).max(0.0), radius, Glass::CONTROL);
}

fn image(scene: &mut Scene, source: &notchbase_sdk::protocol::ImageSource, rect: Rect, radius: f32) {
    let corner = CornerRadius::same(radius.round().clamp(0.0, 255.0) as u8);
    match scene.textures.get(scene.ui.ctx(), scene.env.images, source) {
        Some(texture) => {
            scene.canvas.add(RectShape::filled(rect, corner, Color32::WHITE).with_texture(texture, FULL_UV));
        }
        None => scene.canvas.rect(rect, radius, white(0.1)),
    }
    scene.canvas.rect_stroke(rect, radius, Stroke::new(1.0, white(IMAGE_EDGE)));
}

#[cfg(test)]
mod tests {
    use egui::ColorImage;
    use notchbase_sdk::protocol::{Color, Icon, ImageId, ImageSource, Span, TextStyle, TimerMode};

    use super::super::fixture::{Painted, Stage};
    use super::*;

    const TEAL: Color = Color { r: 0, g: 128, b: 128, a: 255 };

    fn area(left: f32, top: f32, width: f32, height: f32) -> Rect {
        Rect::from_min_size(pos2(left, top), vec2(width, height))
    }

    fn filled(painted: &Painted, color: Color32) -> Vec<Rect> {
        painted.rects().iter().filter(|shape| shape.fill == color).map(|shape| shape.rect).collect()
    }

    fn progress_fill(value: f32) -> Vec<Rect> {
        let node = Node::Progress { value, tone: Tone::Green };
        filled(&Stage::new().paint(&node, area(0.0, 0.0, 200.0, 20.0)), tone(Tone::Green))
    }

    fn ring(value: f32, stroke: Option<f32>) -> Painted {
        let node = Node::Ring { value, size: 40.0, tone: Tone::Green, span: None, stroke, center: None };
        Stage::new().paint(&node, area(0.0, 0.0, 40.0, 40.0))
    }

    fn text(text: &str, align: Align, wrap: bool) -> Node {
        Node::Text(TextNode { text: text.into(), style: TextStyle::Body, tone: Tone::Primary, wrap, align })
    }

    #[test]
    fn progress_fills_its_share() {
        let fill = progress_fill(0.5);
        assert_eq!(fill, [Rect::from_min_size(pos2(0.0, 7.0), vec2(100.0, PROGRESS_H))]);
        assert_eq!(progress_fill(3.0)[0].width(), 200.0, "past full is full");
    }

    #[test]
    fn started_progress_is_never_thinner() {
        assert_eq!(progress_fill(0.01)[0].width(), PROGRESS_H);
    }

    #[test]
    fn empty_progress_is_only_track() {
        assert!(progress_fill(0.0).is_empty());
        assert!(progress_fill(-1.0).is_empty());
    }

    #[test]
    fn ring_draws_clockwise_from_top() {
        let painted = ring(0.25, Some(4.0));
        let arc = &painted.lines()[0];
        assert_eq!(arc.points.len(), 25);
        let (first, last) = (arc.points[0], arc.points[24]);
        assert!((first - pos2(20.0, 2.0)).length() < 1e-4, "{first:?}");
        assert!((last - pos2(38.0, 20.0)).length() < 1e-4, "{last:?}");
        assert_eq!(ring(1.0, Some(4.0)).lines()[0].points.len(), 97);
    }

    #[test]
    fn only_thick_rings_round_their_ends() {
        let thick = ring(0.25, Some(4.0));
        let caps: Vec<_> =
            thick.circles().iter().filter(|circle| circle.fill == tone(Tone::Green)).map(|c| c.radius).collect();
        assert_eq!(caps, [2.0, 2.0]);
        let thin = ring(0.25, Some(ROUND_CAPS));
        assert!(thin.circles().iter().all(|circle| circle.fill != tone(Tone::Green)));
        let natural = ring(0.25, None);
        assert_eq!(natural.circles().len(), 1, "a 40 point ring is drawn three points wide");
    }

    #[test]
    fn empty_ring_is_only_track() {
        let painted = ring(0.0, Some(4.0));
        assert!(painted.lines().is_empty());
        let track = painted.circles();
        assert_eq!(track.len(), 1);
        assert_eq!((track[0].center, track[0].radius), (pos2(20.0, 20.0), 18.0));
        assert_eq!(track[0].stroke, Stroke::new(4.0, white(RING_TRACK)));
    }

    #[test]
    fn countdown_ring_shows_time_left() {
        let mut stage = Stage::new();
        stage.now_ms = 45_000;
        let node = Node::Ring {
            value: 1.0,
            size: 40.0,
            tone: Tone::Green,
            span: Some(Span { start: 0, end: 60_000 }),
            stroke: Some(4.0),
            center: None,
        };
        let painted = stage.paint(&node, area(0.0, 0.0, 40.0, 40.0));
        assert_eq!(painted.lines()[0].points.len(), 25, "a quarter is left");
    }

    #[test]
    fn ring_centres_its_inner_node() {
        let center = Node::Dot { color: TEAL, size: 8.0 };
        let node = Node::Ring {
            value: 0.0,
            size: 40.0,
            tone: Tone::Green,
            span: None,
            stroke: Some(4.0),
            center: Some(Box::new(center)),
        };
        let painted = Stage::new().paint(&node, area(10.0, 10.0, 40.0, 40.0));
        let dot =
            painted.circles().into_iter().find(|circle| circle.fill == ink(TEAL)).map(|dot| (dot.center, dot.radius));
        assert_eq!(dot, Some((pos2(30.0, 30.0), 4.0)));
    }

    #[test]
    fn divider_is_one_faint_hairline() {
        let painted = Stage::new().paint(&Node::Divider, area(10.0, 0.0, 100.0, 9.0));
        let lines = filled(&painted, white(list::HAIRLINE));
        assert_eq!(lines, [Rect::from_center_size(pos2(60.0, 4.5), vec2(100.0, 1.0))]);
    }

    #[test]
    fn dot_is_a_disc_in_its_colour() {
        let painted = Stage::new().paint(&Node::Dot { color: TEAL, size: 12.0 }, area(0.0, 0.0, 12.0, 12.0));
        let dots: Vec<_> = painted.circles().iter().map(|circle| (circle.center, circle.radius, circle.fill)).collect();
        assert_eq!(dots, [(pos2(6.0, 6.0), 6.0, ink(TEAL))]);
    }

    #[test]
    fn swatch_shows_exact_colour_in_ring() {
        let painted = Stage::new().paint(&Node::Swatch { color: TEAL, size: 20.0 }, area(0.0, 0.0, 20.0, 20.0));
        let swatch = painted.circles()[0];
        assert_eq!((swatch.center, swatch.radius, swatch.fill), (pos2(10.0, 10.0), 9.25, rgba(TEAL)));
        assert!(painted.shapes.len() > 1, "the ring is painted over it");
    }

    #[test]
    fn badge_is_a_disc_with_a_glyph() {
        let node = Node::Badge { icon: Icon::Speaker, size: 24.0, tone: Tone::Red };
        let painted = Stage::new().paint(&node, area(0.0, 0.0, 24.0, 24.0));
        let disc = painted.circles()[0];
        assert_eq!((disc.center, disc.radius, disc.fill), (pos2(12.0, 12.0), 12.0, tone(Tone::Red)));
        assert!(painted.shapes.len() > 1);
    }

    #[test]
    fn missing_image_holds_its_place() {
        let node = Node::Image { source: ImageSource::Capture(ImageId(99)), width: 40.0, height: 30.0, radius: 6.0 };
        let rect = area(0.0, 0.0, 40.0, 30.0);
        let painted = Stage::new().paint(&node, rect);
        assert_eq!(filled(&painted, white(0.1)), [rect]);
        let edge = painted.rects().iter().find(|shape| shape.stroke.width > 0.0).map(|shape| shape.stroke);
        assert_eq!(edge, Some(Stroke::new(1.0, white(IMAGE_EDGE))));
    }

    #[test]
    fn loaded_image_fills_its_rect() {
        let mut stage = Stage::new();
        stage.images.show_sample(ColorImage::filled([2, 2], Color32::RED));
        let node = Node::Image { source: ImageSource::Capture(ImageId(0)), width: 40.0, height: 30.0, radius: 6.0 };
        let rect = area(0.0, 0.0, 40.0, 30.0);
        let painted = stage.paint(&node, rect);
        let textured = painted.rects().iter().find(|shape| shape.brush.is_some()).map(|shape| (shape.rect, shape.fill));
        assert_eq!(textured, Some((rect, Color32::WHITE)));
        assert!(filled(&painted, white(0.1)).is_empty());
    }

    #[test]
    fn text_sits_at_its_alignment() {
        let rect = area(10.0, 0.0, 300.0, 20.0);
        let placed = |align| Stage::new().paint(&text("Hello", align, false), rect).texts()[0].0;
        assert_eq!(placed(Align::Start).left(), 10.0);
        assert!((placed(Align::Center).center().x - 160.0).abs() < 1e-3);
        assert!((placed(Align::End).right() - 310.0).abs() < 1e-3);
        assert!((placed(Align::Start).center().y - 10.0).abs() < 1e-3);
    }

    #[test]
    fn long_text_ends_in_an_ellipsis() {
        let long = "A sentence far too long to fit inside a narrow little box";
        let painted = Stage::new().paint(&text(long, Align::Start, false), area(0.0, 0.0, 80.0, 20.0));
        let (rect, shown) = &painted.texts()[0];
        assert!(shown.ends_with('\u{2026}'), "{shown}");
        assert!(rect.width() <= 80.5, "{rect:?}");
    }

    #[test]
    fn wrapped_text_breaks_from_top() {
        let long = "A sentence far too long to fit inside a narrow little box";
        let painted = Stage::new().paint(&text(long, Align::Start, true), area(0.0, 40.0, 80.0, 200.0));
        let (rect, shown) = &painted.texts()[0];
        assert_eq!(shown, long);
        assert_eq!(rect.top(), 40.0);
        assert!(rect.height() > 2.0 * crate::ui::font::body().size, "{rect:?}");
    }

    #[test]
    fn timer_reads_from_the_left() {
        let mut stage = Stage::new();
        stage.now_ms = 9_001;
        let node = Node::Timer { at: 10_000, mode: TimerMode::Remaining, style: TextStyle::Body, tone: Tone::Primary };
        let painted = stage.paint(&node, area(5.0, 0.0, 100.0, 20.0));
        let (rect, shown) = &painted.texts()[0];
        assert_eq!(shown, "0:01");
        assert_eq!(rect.left(), 5.0);
    }
}
