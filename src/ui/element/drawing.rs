//! Paints a tool's canvas node: shapes in the tool's own units, scaled to fit, and pointer events sent back in them.

use egui::epaint::RectShape;
use egui::{Align2, Color32, CornerRadius, Id, Pos2, Rect, Sense, Stroke, Vec2, pos2, vec2};
use notchbase_sdk::protocol::{Anchor, CanvasNode, Color, Paint, Pointer, PointerKind, Shape, UiValue, Weight};

use super::Scene;
use super::style::{glyph, rgba};
use crate::ui::canvas::fade;
use crate::ui::glass::{self, Glass};
use crate::ui::shape::{self, Blob};
use crate::ui::{font, glyph as glyphs, scroll};

const ROUND_STEPS: usize = 48;
const FULL_UV: Rect = Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0));
const MISSING: f32 = 0.1;

#[derive(Clone, Copy, Debug, PartialEq)]
struct View {
    origin: Pos2,
    scale: f32,
}

impl View {
    fn point(&self, [x, y]: [f32; 2]) -> Pos2 {
        self.origin + vec2(x, y) * self.scale
    }

    fn rect(&self, [x, y, width, height]: [f32; 4]) -> Rect {
        Rect::from_min_size(self.point([x, y]), vec2(width.max(0.0), height.max(0.0)) * self.scale)
    }

    fn local(&self, at: Pos2) -> [f32; 2] {
        let within = (at - self.origin) / self.scale;
        [within.x, within.y]
    }

    fn inside(&self, offset: [f32; 2], scale: f32) -> Self {
        Self { origin: self.point(offset), scale: self.scale * scale }
    }
}

pub(super) fn size(canvas: &CanvasNode, max: f32) -> Vec2 {
    let size = vec2(canvas.width.max(1.0), canvas.height.max(1.0));
    if canvas.stretch { size * (max.max(0.0) / size.x) } else { size }
}

pub(super) fn paint(scene: &mut Scene, canvas: &CanvasNode, rect: Rect, id: Id) {
    let view = View { origin: rect.min, scale: rect.width() / canvas.width.max(1.0) };
    listen(scene, canvas, rect, view, id);
    let mark = scene.canvas.mark();
    for shape in &canvas.shapes {
        draw(scene, shape, view);
    }
    scene.canvas.clip(mark, rect);
}

fn draw(scene: &mut Scene, shape: &Shape, view: View) {
    let ppp = scene.canvas.pixels_per_point();
    match shape {
        Shape::Rect { rect, radius, fill, stroke } => {
            let (rect, radius) = (view.rect(*rect), radius.max(0.0) * view.scale);
            match fill {
                Some(Paint::Solid(color)) => scene.canvas.rect(rect, radius, rgba(*color)),
                Some(paint) => filled(scene, &rounded(rect, radius, ppp), rect, *paint),
                None => {}
            }
            if let Some(stroke) = stroke {
                scene.canvas.rect_stroke(rect, radius, Stroke::new(stroke.width * view.scale, rgba(stroke.color)));
            }
        }
        Shape::Ellipse { center, radius: [across, down], fill, stroke } => {
            let (center, radius) = (view.point(*center), vec2(across.max(0.0), down.max(0.0)) * view.scale);
            match fill {
                Some(Paint::Solid(color)) => {
                    scene.canvas.add(egui::Shape::ellipse_filled(center, radius, rgba(*color)))
                }
                Some(paint) => {
                    filled(scene, &round(center, radius), Rect::from_center_size(center, radius * 2.0), *paint)
                }
                None => {}
            }
            if let Some(stroke) = stroke {
                let stroke = Stroke::new(stroke.width * view.scale, rgba(stroke.color));
                scene.canvas.add(egui::Shape::ellipse_stroke(center, radius, stroke));
            }
        }
        Shape::Line { points, stroke } => {
            let points: Vec<Pos2> = points.iter().map(|point| view.point(*point)).collect();
            scene.canvas.add(egui::Shape::line(points, Stroke::new(stroke.width * view.scale, rgba(stroke.color))));
        }
        Shape::Polygon { points, fill, stroke } => {
            let points: Vec<Pos2> = points.iter().map(|point| view.point(*point)).collect();
            if let Some(paint) = fill {
                let bounds = Rect::from_points(&points);
                match convex(&points) {
                    true => filled(scene, &points, bounds, *paint),
                    false => scene.canvas.add(egui::Shape::mesh(carved(&points, |at| shade(*paint, bounds, at)))),
                }
            }
            if let Some(stroke) = stroke {
                let stroke = Stroke::new(stroke.width * view.scale, rgba(stroke.color));
                scene.canvas.add(egui::Shape::closed_line(points, stroke));
            }
        }
        Shape::Text { at, text, size, weight, color, anchor, width } => {
            let font = face(*weight, size * view.scale);
            let (at, anchor, color) = (view.point(*at), align(*anchor), rgba(*color));
            match width {
                Some(width) => {
                    let galley = scene.canvas.wrapped(text.as_str(), font, color, width * view.scale);
                    let placed = anchor.anchor_size(at, galley.size());
                    scene.canvas.add(egui::Shape::galley(placed.min, galley, color));
                }
                None => {
                    scene.canvas.text(at, anchor, text.as_str(), font, color);
                }
            }
        }
        Shape::Icon { icon, center, size, color } => {
            glyphs::paint(scene.canvas, glyph(*icon), view.point(*center), size * view.scale, rgba(*color));
        }
        Shape::Run { at, parts, gap, anchor } => {
            let sizes: Vec<Vec2> = parts.iter().map(|part| extent(scene, part, view)).collect();
            let shown = sizes.iter().filter(|size| size.x > 0.0).count().saturating_sub(1) as f32;
            let whole = vec2(
                sizes.iter().map(|size| size.x).sum::<f32>() + shown * gap * view.scale,
                sizes.iter().map(|size| size.y).fold(0.0, f32::max),
            );
            let placed = align(*anchor).anchor_size(view.point(*at), whole);
            let mut x = placed.left();
            for (part, size) in parts.iter().zip(sizes).filter(|(_, size)| size.x > 0.0) {
                let middle = pos2(x + size.x * 0.5, placed.center().y);
                draw(scene, &centered(part), View { origin: middle, scale: view.scale });
                x += size.x + gap * view.scale;
            }
        }
        Shape::Image { source, rect, radius } => {
            let (rect, radius) = (view.rect(*rect), radius.max(0.0) * view.scale);
            match scene.textures.get(scene.ui.ctx(), scene.env.images, source) {
                Some(texture) => {
                    let corner = CornerRadius::same(radius.round().clamp(0.0, 255.0) as u8);
                    scene.canvas.add(RectShape::filled(rect, corner, Color32::WHITE).with_texture(texture, FULL_UV));
                }
                None => scene.canvas.rect(rect, radius, crate::ui::color::white(MISSING)),
            }
        }
        Shape::Glass { rect, radius } => {
            glass::paint(scene.canvas, view.rect(*rect), radius.max(0.0) * view.scale, Glass::TILE);
        }
        Shape::Shadow { rect, radius, blur, color } => {
            let blur = (blur.max(0.0) * view.scale).round().min(255.0) as u8;
            let shadow = egui::Shadow { offset: [0, 0], blur, spread: 0, color: rgba(*color) };
            let radius = (radius.max(0.0) * view.scale).min(255.0);
            scene.canvas.add(shadow.as_shape(view.rect(*rect), radius));
        }
        Shape::Group { shapes, opacity, offset, scale, clip } => {
            let mark = scene.canvas.mark();
            let inner = view.inside(*offset, scale.max(0.0));
            for shape in shapes {
                draw(scene, shape, inner);
            }
            if *opacity < 1.0 {
                for shape in scene.canvas.since(mark) {
                    fade(shape, opacity.clamp(0.0, 1.0));
                }
            }
            if let Some(clip) = clip {
                scene.canvas.clip(mark, inner.rect(*clip));
            }
        }
    }
}

fn extent(scene: &Scene, part: &Shape, view: View) -> Vec2 {
    match part {
        Shape::Text { text, size, weight, .. } => {
            scene.canvas.text_size(text.as_str(), face(*weight, size * view.scale))
        }
        Shape::Icon { size, .. } => Vec2::splat(size * view.scale),
        _ => Vec2::ZERO,
    }
}

fn centered(part: &Shape) -> Shape {
    match part.clone() {
        Shape::Text { text, size, weight, color, .. } => {
            Shape::Text { at: [0.0, 0.0], text, size, weight, color, anchor: Anchor::Center, width: None }
        }
        Shape::Icon { icon, size, color, .. } => Shape::Icon { icon, center: [0.0, 0.0], size, color },
        other => other,
    }
}

fn face(weight: Weight, size: f32) -> egui::FontId {
    match weight {
        Weight::Regular => font::regular(size),
        Weight::Semibold => font::semibold(size),
        Weight::Mono => font::mono(size),
    }
}

fn filled(scene: &mut Scene, outline: &[Pos2], bounds: Rect, paint: Paint) {
    let ppp = scene.canvas.pixels_per_point();
    let middle = outline.iter().fold(Vec2::ZERO, |sum, point| sum + point.to_vec2()) / outline.len().max(1) as f32;
    let color_at = |at: Pos2| match paint {
        Paint::Radial { inner, .. } if at == middle.to_pos2() => rgba(inner),
        paint => shade(paint, bounds, at),
    };
    scene.canvas.add(egui::Shape::mesh(shape::fill_polygon_with(outline, middle.to_pos2(), color_at, ppp)));
}

fn turn(a: Pos2, b: Pos2, c: Pos2) -> f32 {
    (b - a).x * (c - b).y - (b - a).y * (c - b).x
}

fn convex(points: &[Pos2]) -> bool {
    let count = points.len();
    let turns = (0..count).map(|at| turn(points[at], points[(at + 1) % count], points[(at + 2) % count]));
    let (mut left, mut right) = (false, false);
    for turn in turns {
        left |= turn > 0.0;
        right |= turn < 0.0;
    }
    !(left && right)
}

fn ears(points: &[Pos2]) -> Vec<[usize; 3]> {
    let way = if shape::signed_area(points) >= 0.0 { 1.0 } else { -1.0 };
    let mut left: Vec<usize> = (0..points.len()).collect();
    let mut ears = Vec::new();
    while left.len() > 3 {
        let count = left.len();
        let ear = (0..count).find(|&at| {
            let [a, b, c] = [left[(at + count - 1) % count], left[at], left[(at + 1) % count]];
            let corner = [points[a], points[b], points[c]];
            let holds = |point: Pos2| (0..3).all(|side| turn(corner[side], corner[(side + 1) % 3], point) * way > 0.0);
            turn(corner[0], corner[1], corner[2]) * way > 0.0
                && !left.iter().any(|&other| ![a, b, c].contains(&other) && holds(points[other]))
        });
        let at = ear.unwrap_or(0);
        ears.push([left[(at + count - 1) % count], left[at], left[(at + 1) % count]]);
        left.remove(at);
    }
    if let [a, b, c] = left[..] {
        ears.push([a, b, c]);
    }
    ears
}

fn carved(points: &[Pos2], color_at: impl Fn(Pos2) -> Color32) -> egui::Mesh {
    let mut mesh = egui::Mesh::default();
    for point in points {
        mesh.colored_vertex(*point, color_at(*point));
    }
    for [a, b, c] in ears(points) {
        mesh.add_triangle(a as u32, b as u32, c as u32);
    }
    mesh
}

fn shade(paint: Paint, bounds: Rect, at: Pos2) -> Color32 {
    match paint {
        Paint::Solid(color) => rgba(color),
        Paint::Linear { from, to, angle } => {
            let along = vec2(angle.to_radians().cos(), angle.to_radians().sin());
            let reach = (along.x.abs() * bounds.width() + along.y.abs() * bounds.height()) * 0.5;
            let share = if reach > 0.0 { (at - bounds.center()).dot(along) / reach * 0.5 + 0.5 } else { 0.0 };
            blend(from, to, share)
        }
        Paint::Radial { inner, outer } => {
            let away = (at - bounds.center()).length_sq();
            if away < 0.01 { rgba(inner) } else { rgba(outer) }
        }
    }
}

fn blend(from: Color, to: Color, share: f32) -> Color32 {
    let share = share.clamp(0.0, 1.0);
    let mix = |a: u8, b: u8| (f32::from(a) + (f32::from(b) - f32::from(a)) * share).round() as u8;
    Color32::from_rgba_unmultiplied(mix(from.r, to.r), mix(from.g, to.g), mix(from.b, to.b), mix(from.a, to.a))
}

fn rounded(rect: Rect, radius: f32, ppp: f32) -> Vec<Pos2> {
    let radius = radius.min(rect.width().min(rect.height()) * 0.5);
    let blob = Blob { rect, top_radius: radius, bottom_radius: radius, ear: 0.0, bleed_top: false, wave: None };
    shape::outline(&blob, ppp)
}

fn round(center: Pos2, radius: Vec2) -> Vec<Pos2> {
    let at = |step: usize| {
        let angle = std::f32::consts::TAU * step as f32 / ROUND_STEPS as f32;
        center + vec2(angle.cos() * radius.x, angle.sin() * radius.y)
    };
    (0..ROUND_STEPS).map(at).collect()
}

fn align(anchor: Anchor) -> Align2 {
    match anchor {
        Anchor::TopLeft => Align2::LEFT_TOP,
        Anchor::Top => Align2::CENTER_TOP,
        Anchor::TopRight => Align2::RIGHT_TOP,
        Anchor::Left => Align2::LEFT_CENTER,
        Anchor::Center => Align2::CENTER_CENTER,
        Anchor::Right => Align2::RIGHT_CENTER,
        Anchor::BottomLeft => Align2::LEFT_BOTTOM,
        Anchor::Bottom => Align2::CENTER_BOTTOM,
        Anchor::BottomRight => Align2::RIGHT_BOTTOM,
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Touch {
    over: bool,
    held: bool,
}

fn listen(scene: &mut Scene, canvas: &CanvasNode, rect: Rect, view: View, id: Id) {
    if !scene.live || (canvas.on_pointer.is_none() && canvas.on_scroll.is_none() && canvas.on_menu.is_none()) {
        return;
    }
    let sense =
        if canvas.on_pointer.is_some() || canvas.on_menu.is_some() { Sense::click_and_drag() } else { Sense::hover() };
    let response = scene.ui.interact(rect, id, sense);
    let at = response.interact_pointer_pos().or_else(|| response.hover_pos()).map(|at| view.local(at));
    let memory = id.with("touch");
    let before = scene.ui.ctx().data(|data| data.get_temp::<Touch>(memory)).unwrap_or_default();
    let (pressed, released, moved) = scene.ui.input(|input| {
        (input.pointer.primary_pressed(), input.pointer.primary_released(), input.pointer.delta() != Vec2::ZERO)
    });
    let dragging = response.dragged() || response.is_pointer_button_down_on();
    let over = response.hovered() || dragging;
    let held = (before.held || (pressed && response.is_pointer_button_down_on())) && !released;
    let tell = |scene: &mut Scene, kind: PointerKind, delta: Vec2| {
        if let Some(at) = at {
            let delta = delta / view.scale;
            scene.emit(canvas.on_pointer, UiValue::Pointer(Pointer::moved(kind, at, [delta.x, delta.y])));
        }
    };
    if pressed && response.is_pointer_button_down_on() {
        tell(scene, PointerKind::Press, Vec2::ZERO);
    } else if response.dragged() && response.drag_delta() != Vec2::ZERO {
        tell(scene, PointerKind::Drag, response.drag_delta());
    } else if over && !dragging && (moved || !before.over) {
        tell(scene, PointerKind::Hover, Vec2::ZERO);
    }
    if before.held && released {
        tell(scene, PointerKind::Release, Vec2::ZERO);
    }
    if response.clicked() {
        tell(scene, PointerKind::Click, Vec2::ZERO);
    }
    if let Some([x, y]) = at.filter(|_| canvas.on_menu.is_some() && response.secondary_clicked()) {
        scene.emit(canvas.on_menu, UiValue::Pointer(Pointer::at(PointerKind::Press, x, y)));
    }
    if before.over && !over {
        let gone = Pointer::at(PointerKind::Leave, -1.0, -1.0);
        scene.emit(canvas.on_pointer, UiValue::Pointer(gone));
    }
    if canvas.on_scroll.is_some() && response.hovered() {
        let wheel = scroll::read(scene.ui);
        let turned = vec2(wheel.x, wheel.y) * scroll::POINTS_PER_NOTCH / view.scale;
        if let Some(at) = at.filter(|_| scroll::take(scene.ui, turned.length_sq()) != 0.0) {
            let scrolled = Pointer::moved(PointerKind::Scroll, at, [turned.x, turned.y]);
            scene.emit(canvas.on_scroll, UiValue::Pointer(scrolled));
        }
    }
    let now = Touch { over, held };
    if now != before {
        scene.ui.ctx().data_mut(|data| data.insert_temp(memory, now));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn points<const N: usize>(points: [(f32, f32); N]) -> Vec<Pos2> {
        points.map(|(x, y)| pos2(x, y)).to_vec()
    }

    fn covered(points: &[Pos2]) -> f32 {
        let area = |[a, b, c]: [usize; 3]| turn(points[a], points[b], points[c]).abs() * 0.5;
        ears(points).into_iter().map(area).sum()
    }

    #[test]
    fn canvas_keeps_size_or_stretches() {
        let canvas = CanvasNode { width: 170.0, height: 90.0, ..CanvasNode::default() };
        assert_eq!(size(&canvas, 340.0), vec2(170.0, 90.0));
        assert_eq!(size(&CanvasNode { stretch: true, ..canvas }, 340.0), vec2(340.0, 180.0));
    }

    #[test]
    fn group_moves_and_scales_its_shapes() {
        let view = View { origin: pos2(100.0, 50.0), scale: 2.0 };
        let inner = view.inside([10.0, 5.0], 0.5);
        assert_eq!(inner, View { origin: pos2(120.0, 60.0), scale: 1.0 });
        assert_eq!(inner.point([4.0, 4.0]), pos2(124.0, 64.0));
        assert_eq!(view.rect([1.0, 2.0, 10.0, -3.0]), Rect::from_min_size(pos2(102.0, 54.0), vec2(20.0, 0.0)));
        assert_eq!(view.local(pos2(140.0, 70.0)), [20.0, 10.0]);
    }

    #[test]
    fn concave_shape_fills_only_inside() {
        let arrow = points([(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (5.0, 4.0), (0.0, 10.0)]);
        assert!(!convex(&arrow));
        assert!(convex(&points([(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)])));
        assert_eq!(ears(&arrow).len(), 3);
        assert!((covered(&arrow) - shape::signed_area(&arrow).abs()).abs() < 1e-3, "{}", covered(&arrow));
        let backwards: Vec<Pos2> = arrow.iter().rev().copied().collect();
        assert!((covered(&backwards) - covered(&arrow)).abs() < 1e-3, "either winding works");
        let comb = points([
            (0.0, 0.0),
            (9.0, 0.0),
            (9.0, 6.0),
            (7.0, 6.0),
            (7.0, 2.0),
            (5.0, 2.0),
            (5.0, 6.0),
            (3.0, 6.0),
            (3.0, 2.0),
            (2.0, 2.0),
            (2.0, 6.0),
            (0.0, 6.0),
        ]);
        assert!((covered(&comb) - shape::signed_area(&comb).abs()).abs() < 1e-3);
        assert_eq!(carved(&comb, |_| Color32::WHITE).indices.len(), 3 * (comb.len() - 2));
    }

    #[test]
    fn run_parts_recentre_and_keep_look() {
        let label = Shape::Text {
            at: [90.0, 40.0],
            text: "pm".into(),
            size: 10.0,
            weight: Weight::Semibold,
            color: Color::rgb(1, 2, 3),
            anchor: Anchor::BottomRight,
            width: Some(20.0),
        };
        let Shape::Text { at, size, weight, color, anchor, width, .. } = centered(&label) else { panic!() };
        assert_eq!((at, anchor, width), ([0.0, 0.0], Anchor::Center, None));
        assert_eq!((size, weight, color), (10.0, Weight::Semibold, Color::rgb(1, 2, 3)));
        let glyph = Shape::Icon {
            icon: notchbase_sdk::protocol::Icon::Clock,
            center: [5.0, 5.0],
            size: 12.0,
            color: Color::rgb(9, 9, 9),
        };
        assert!(matches!(centered(&glyph), Shape::Icon { center: [0.0, 0.0], size: 12.0, .. }));
        assert_eq!((face(Weight::Mono, 11.0), face(Weight::Regular, 11.0)), (font::mono(11.0), font::regular(11.0)));
    }

    #[test]
    fn gradient_runs_edge_to_edge() {
        let bounds = Rect::from_min_max(pos2(0.0, 0.0), pos2(100.0, 40.0));
        let (black, white) = (Color::rgb(0, 0, 0), Color::rgb(255, 255, 255));
        let across = Paint::Linear { from: black, to: white, angle: 0.0 };
        assert_eq!(shade(across, bounds, pos2(0.0, 20.0)), Color32::BLACK);
        assert_eq!(shade(across, bounds, pos2(100.0, 5.0)), Color32::WHITE);
        assert_eq!(shade(across, bounds, pos2(50.0, 20.0)), Color32::from_rgb(128, 128, 128));
        let down = Paint::Linear { from: black, to: white, angle: 90.0 };
        assert_eq!(shade(down, bounds, pos2(80.0, 40.0)), Color32::WHITE);
        let glow = Paint::Radial { inner: white, outer: black };
        assert_eq!(
            [shade(glow, bounds, bounds.center()), shade(glow, bounds, pos2(0.0, 0.0))],
            [Color32::WHITE, Color32::BLACK]
        );
    }
}
