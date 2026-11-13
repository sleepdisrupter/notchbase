//! Paints one picker overlay: the dim around the selection, a frame with corner ticks, the size label and the hint.

use egui::{Align2, Color32, Mesh, Painter, Pos2, Rect, Shape, Vec2, pos2, vec2};

use super::geometry::PixelRect;
use super::label::SizeLabel;
use crate::capture::overlay::paint::{frame, label_shadow};
use crate::ui::canvas::Canvas;
use crate::ui::glass::{self, Glass};
use crate::ui::{color, font};

const DIM: f32 = 0.28;
const TICK_LENGTH: f32 = 12.0;
/// A whole display selection is framed this far inside, so the frame stays on screen.
const WHOLE_INSET: f32 = 3.0;
const HINT_H: f32 = 36.0;
const HINT_PAD: f32 = 16.0;
const HINT_DOT: f32 = 5.0;
const HINT_GAP: f32 = 9.0;

pub(super) struct Scene {
    pub(super) view: Vec2,
    pub(super) size: [usize; 2],
    pub(super) dt: f32,
    pub(super) opacity: f32,
    pub(super) selection: Option<PixelRect>,
    pub(super) hint: Option<&'static str>,
}

pub(super) fn draw(painter: &Painter, label: &mut SizeLabel, scene: &Scene) {
    let selection = scene.selection.filter(|r| r.width() > 0 && r.height() > 0);
    let hole = selection.map(|r| r.to_points(scene.view, scene.size));
    painter.add(Shape::mesh(dim(Rect::from_min_size(Pos2::ZERO, scene.view), hole, scene.opacity)));

    let (Some(selection), Some(rect)) = (selection, hole) else { return };
    let mut chrome = Canvas::new(painter);
    let whole = selection == PixelRect::full(scene.size);
    let framed = if whole { rect.shrink(WHOLE_INSET) } else { rect };
    chrome.add(Shape::mesh(selection_frame(framed, painter.pixels_per_point())));
    label.paint(&mut chrome, selection, framed, scene.view, scene.dt);
    if let Some(text) = scene.hint {
        hint(&mut chrome, scene.view, text);
    }
    chrome.paint(painter, Pos2::ZERO, 1.0, Vec2::ZERO, scene.opacity);
}

fn hint(canvas: &mut Canvas, view: Vec2, text: &str) {
    let text_width = canvas.text_width(text, font::body());
    let size = vec2(HINT_PAD * 2.0 + HINT_DOT * 2.0 + HINT_GAP + text_width, HINT_H).round();
    let rect = Rect::from_center_size((view * 0.5).to_pos2(), size);
    label_shadow(canvas, rect);
    glass::paint(canvas, rect, rect.height() * 0.5, Glass::tinted_by(Color32::from_gray(48)));
    let dot = pos2(rect.left() + HINT_PAD + HINT_DOT, rect.center().y);
    canvas.circle(dot, HINT_DOT, color::RED);
    let at = pos2(dot.x + HINT_DOT + HINT_GAP, rect.center().y);
    canvas.text(at, Align2::LEFT_CENTER, text, font::body(), color::label());
}

fn dim(screen: Rect, hole: Option<Rect>, opacity: f32) -> Mesh {
    let shade = Color32::from_black_alpha((DIM * opacity * 255.0).round() as u8);
    let mut mesh = Mesh::default();
    for rect in dim_rects(screen, hole) {
        mesh.add_colored_rect(rect, shade);
    }
    mesh
}

fn dim_rects(screen: Rect, hole: Option<Rect>) -> Vec<Rect> {
    let Some(hole) = hole.map(|h| h.intersect(screen)).filter(Rect::is_positive) else {
        return vec![screen];
    };
    [
        Rect::from_min_max(screen.min, pos2(screen.max.x, hole.min.y)),
        Rect::from_min_max(pos2(screen.min.x, hole.max.y), screen.max),
        Rect::from_min_max(pos2(screen.min.x, hole.min.y), pos2(hole.min.x, hole.max.y)),
        Rect::from_min_max(pos2(hole.max.x, hole.min.y), pos2(screen.max.x, hole.max.y)),
    ]
    .into_iter()
    .filter(Rect::is_positive)
    .collect()
}

fn selection_frame(rect: Rect, ppp: f32) -> Mesh {
    let hair = 1.0 / ppp;
    let line = ppp.round().max(1.0) / ppp;
    let mut mesh = Mesh::default();
    frame(&mut mesh, rect.expand(line + hair), hair, Color32::from_black_alpha(90));
    frame(&mut mesh, rect.expand(line), line, color::white(0.92));
    corner_ticks(&mut mesh, rect, ppp);
    mesh
}

fn corner_ticks(mesh: &mut Mesh, rect: Rect, ppp: f32) {
    let thick = (2.5 * ppp).round().max(2.0) / ppp;
    let outer = rect.expand(thick);
    let length = ((TICK_LENGTH * ppp).round() / ppp).min(outer.width() * 0.5).min(outer.height() * 0.5);
    let color = color::white(0.96);
    for (corner, sx, sy) in [
        (outer.left_top(), 1.0, 1.0),
        (outer.right_top(), -1.0, 1.0),
        (outer.left_bottom(), 1.0, -1.0),
        (outer.right_bottom(), -1.0, -1.0),
    ] {
        let at = |dx: f32, dy: f32| corner + vec2(sx * dx, sy * dy);
        mesh.add_colored_rect(Rect::from_two_pos(at(0.0, 0.0), at(length, thick)), color);
        mesh.add_colored_rect(Rect::from_two_pos(at(0.0, thick), at(thick, length)), color);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn area(rects: &[Rect]) -> f32 {
        rects.iter().map(Rect::area).sum()
    }

    #[test]
    fn dim_covers_all_but_selection() {
        let screen = Rect::from_min_size(Pos2::ZERO, vec2(1536.0, 864.0));
        assert_eq!(dim_rects(screen, None), vec![screen]);
        let holes = [
            Rect::from_min_max(pos2(100.0, 80.0), pos2(400.0, 300.0)),
            Rect::from_min_max(pos2(0.0, 0.0), pos2(200.0, 864.0)),
            Rect::from_min_max(pos2(1400.0, -20.0), pos2(1600.0, 100.0)),
            screen,
        ];
        for hole in holes {
            let rects = dim_rects(screen, Some(hole));
            let visible_hole = hole.intersect(screen);
            assert!((area(&rects) + visible_hole.area() - screen.area()).abs() < 1e-2, "{hole:?}");
            for (i, a) in rects.iter().enumerate() {
                assert!(screen.contains_rect(*a), "{a:?}");
                let inside = a.intersect(visible_hole);
                assert!(!inside.is_positive(), "{a:?} dims the selection {hole:?}");
                for b in &rects[i + 1..] {
                    assert!(!a.intersect(*b).is_positive(), "{a:?} overlaps {b:?}");
                }
            }
        }
        let sliver = Rect::from_min_max(pos2(50.0, 50.0), pos2(50.0, 90.0));
        assert_eq!(dim_rects(screen, Some(sliver)), vec![screen]);
    }

    #[test]
    fn paints_every_region_state_headless() {
        let ctx = egui::Context::default();
        crate::ui::font::install(&ctx);
        let size = [320usize, 200usize];
        let selections = [
            None,
            Some(PixelRect { x0: 10, y0: 10, x1: 10, y1: 40 }),
            Some(PixelRect { x0: 3, y0: 5, x1: 5, y1: 7 }),
            Some(PixelRect { x0: 20, y0: 30, x1: 200, y1: 120 }),
            Some(PixelRect::full(size)),
        ];
        let mut label = SizeLabel::new();
        for ppp in [1.0, 1.5, 2.0] {
            let view = vec2(size[0] as f32 / ppp, size[1] as f32 / ppp);
            for selection in selections {
                label.reset();
                for opacity in [0.3, 1.0] {
                    let mut input = egui::RawInput {
                        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, view)),
                        ..Default::default()
                    };
                    input.viewports.entry(egui::ViewportId::ROOT).or_default().native_pixels_per_point = Some(ppp);
                    let hint = (selection == Some(PixelRect::full(size))).then_some("Click to record");
                    let scene = Scene { view, size, dt: 1.0 / 60.0, opacity, selection, hint };
                    let mut output = ctx.run_ui(input, |ui| draw(ui.painter(), &mut label, &scene));
                    output.textures_delta.clear();
                    let primitives = ctx.tessellate(std::mem::take(&mut output.shapes), output.pixels_per_point);
                    assert!(!primitives.is_empty());
                }
            }
        }
    }
}
