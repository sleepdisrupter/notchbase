//! Frosted glass: a tinted body with a lit rim. When the glass is see-through it also casts a shadow and
//! lights up near the pointer.

use egui::{Color32, Pos2, Rect, Shape, vec2};

use crate::canvas::Canvas;
use crate::color::{mix, over, white};
use crate::shape;

pub const CLEAR: f32 = 0.94;
const TINT_LEFT: f32 = 0.94;
const FROST: f32 = 0.055;
const FROST_SHEEN: f32 = 0.05;
const HAIRLINE: f32 = 2.2;
const HAIRLINE_AMBIENT: f32 = 0.12;
const HAIRLINE_LIT: f32 = 0.4;
const HAIRLINE_COUNTER: f32 = 0.22;
const HAIRLINE_FOCUS: f32 = 3.0;
const POINTER_GLOW: f32 = 0.11;
const POINTER_GLOW_REACH: f32 = 34.0;
const POINTER_RIM: f32 = 0.3;
const POINTER_RIM_REACH: f32 = 48.0;
const POINTER_GLINT: f32 = 0.4;
const POINTER_GLINT_REACH: f32 = 20.0;
const SWELL: f32 = 1.5;
const SWELL_REACH: f32 = 40.0;
const SWELLS_UP_TO: f32 = 120.0;
const SHADOW: f32 = 0.3;
const THICKNESS_LIGHT: f32 = 0.07;
const THICKNESS_SHADE: f32 = 0.16;
const SEEN_THROUGH: &str = "notchbase.glass.seen-through";

/// How see-through glass is this frame, from 0 (opaque) to 1.
pub fn set_seen_through(ctx: &egui::Context, amount: f32) {
    ctx.data_mut(|data| data.insert_temp(egui::Id::new(SEEN_THROUGH), amount.clamp(0.0, 1.0)));
}

fn seen_through(canvas: &Canvas) -> f32 {
    canvas.painter().ctx().data(|data| data.get_temp(egui::Id::new(SEEN_THROUGH))).unwrap_or(0.0)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Glass {
    pub tint: Color32,
    pub sheen: f32,
    pub specular: f32,
    pub edge: f32,
    pub clear: f32,
}

impl Glass {
    pub const TILE: Self = Self::flat(crate::color::SURFACE);
    pub const TILE_HOVER: Self =
        Self { tint: Color32::from_rgb(43, 43, 46), sheen: 0.06, specular: 0.42, edge: 0.60, clear: CLEAR };
    pub const CONTROL: Self =
        Self { tint: Color32::from_rgb(38, 38, 41), sheen: 0.05, specular: 0.28, edge: 0.50, clear: CLEAR };

    pub const fn flat(tint: Color32) -> Self {
        Self { tint, sheen: 0.0, specular: 0.0, edge: 0.0, clear: 0.0 }
    }

    pub fn tinted_by(background: Color32) -> Self {
        let tint = mix(Color32::from_rgb(20, 20, 22), background, 0.28);
        Self { tint: tint.gamma_multiply(0.94), sheen: 0.06, specular: 0.45, edge: 0.55, clear: CLEAR }
    }

    fn is_flat(&self) -> bool {
        self.specular <= 0.0 && self.edge <= 0.0
    }

    pub fn lerp(a: Self, b: Self, t: f32) -> Self {
        Self {
            tint: mix(a.tint, b.tint, t),
            sheen: crate::spring::lerp(a.sheen, b.sheen, t),
            specular: crate::spring::lerp(a.specular, b.specular, t),
            edge: crate::spring::lerp(a.edge, b.edge, t),
            clear: crate::spring::lerp(a.clear, b.clear, t),
        }
    }

    fn rim(&self, normal: egui::Vec2) -> Color32 {
        let facing = facing_light(normal);
        let spec = 0.07 + self.specular * facing.max(0.0).powf(2.2);
        let dark = self.edge * (-facing).max(0.0).powf(1.4);
        over(white(spec), Color32::from_black_alpha((dark * 255.0) as u8))
    }
}

// The light comes from the top left. 1 faces it and -1 faces away.
fn facing_light(normal: egui::Vec2) -> f32 {
    normal.normalized().dot(vec2(-0.42, -1.0).normalized())
}

fn hairline(normal: egui::Vec2) -> f32 {
    let facing = facing_light(normal);
    let lit = HAIRLINE_LIT * facing.max(0.0).powf(HAIRLINE_FOCUS);
    let counter = HAIRLINE_COUNTER * (-facing).max(0.0).powf(HAIRLINE_FOCUS);
    HAIRLINE_AMBIENT + lit + counter
}

/// A band that fades inward from an outline, colored point by point. Rims and hairlines use it.
pub fn rim_mesh(
    pts: &[Pos2],
    normals: &[egui::Vec2],
    width: f32,
    pixels_per_point: f32,
    color_for: impl Fn(Pos2, egui::Vec2) -> Color32,
) -> egui::Mesh {
    let mut mesh = egui::Mesh::default();
    let n = pts.len() as u32;
    if n < 3 {
        return mesh;
    }
    let half_feather = 0.5 / pixels_per_point;
    for (&p, &normal) in pts.iter().zip(normals) {
        mesh.colored_vertex(p + normal * half_feather, Color32::TRANSPARENT);
        mesh.colored_vertex(p - normal * half_feather, color_for(p, normal));
        mesh.colored_vertex(p - normal * width, Color32::TRANSPARENT);
    }
    for i in 0..n {
        let j = (i + 1) % n;
        let (a, b) = (3 * i, 3 * j);
        mesh.add_triangle(a, a + 1, b + 1);
        mesh.add_triangle(a, b + 1, b);
        mesh.add_triangle(a + 1, a + 2, b + 2);
        mesh.add_triangle(a + 1, b + 2, b + 1);
    }
    mesh
}

/// Paints a glass rounded rect. See-through glass also marks a pane for the backdrop, and small pieces swell
/// under the pointer.
pub fn paint(canvas: &mut Canvas, rect: Rect, radius: f32, glass: Glass) {
    let ppp = canvas.pixels_per_point();
    let clear = glass.clear * seen_through(canvas);
    let pointer = canvas.painter().ctx().pointer_latest_pos().filter(|_| clear > 0.004);
    let near = |at: Pos2, reach: f32| {
        pointer.map_or(0.0, |pointer| {
            let away = at.distance(pointer) / reach;
            clear * (-away * away).exp()
        })
    };
    let swell = if rect.width() <= SWELLS_UP_TO { SWELL * near(rect.center(), SWELL_REACH) } else { 0.0 };
    let (rect, radius) = (rect.expand(swell), radius + swell);
    let blob = shape::Blob { rect, top_radius: radius, bottom_radius: radius, ear: 0.0, bleed_top: false, wave: None };
    let pts = shape::outline(&blob, ppp);
    if pts.len() < 3 {
        return;
    }
    let (top, height) = (rect.top(), rect.height().max(1.0));
    if clear > 0.004 {
        let shade = Color32::from_black_alpha((SHADOW * clear * 255.0) as u8);
        let shadow = egui::Shadow { offset: [0, 2], blur: 10, spread: 0, color: shade };
        canvas.add(shadow.as_shape(rect, radius.min(f32::from(u8::MAX))));
        let hole = shape::fill_polygon_with(&pts, rect.center(), |_| crate::erase::strength(clear), ppp);
        canvas.add(crate::erase::pane(hole));
    }
    let tint = glass.tint.gamma_multiply(1.0 - TINT_LEFT * clear);
    let body = shape::fill_polygon_with(
        &pts,
        rect.center(),
        |p| {
            let t = ((p.y - top) / height).clamp(0.0, 1.0);
            let glow = POINTER_GLOW * near(p, POINTER_GLOW_REACH);
            over(white((glass.sheen + FROST_SHEEN * clear) * (1.0 - t) + FROST * clear + glow), tint)
        },
        ppp,
    );
    canvas.add(Shape::mesh(body));
    if glass.is_flat() {
        return;
    }
    let normals = shape::vertex_normals(&pts);
    if clear > 0.004 {
        let depth = (radius * 0.5).clamp(3.0, 6.0);
        canvas.add(Shape::mesh(rim_mesh(&pts, &normals, depth, ppp, |_, n| thickness(n, clear))));
    }
    let width = (radius * 0.35).clamp(1.0, 2.5);
    let width = crate::spring::lerp(width, HAIRLINE, clear);
    canvas.add(Shape::mesh(rim_mesh(&pts, &normals, width, ppp, |p, n| {
        let glint = POINTER_RIM * near(p, POINTER_RIM_REACH) + POINTER_GLINT * near(p, POINTER_GLINT_REACH);
        mix(glass.rim(n), white(hairline(n) + glint), clear)
    })));
}

fn thickness(normal: egui::Vec2, clear: f32) -> Color32 {
    let up = -normal.normalized().y;
    if up > 0.0 {
        white(THICKNESS_LIGHT * up * clear)
    } else {
        Color32::from_black_alpha((THICKNESS_SHADE * -up * clear * 255.0) as u8)
    }
}

/// A glass ring between the `inner` and `outer` radii, with a rim on both edges.
pub fn ring(canvas: &mut Canvas, center: Pos2, inner: f32, outer: f32, glass: Glass) {
    let ppp = canvas.pixels_per_point();
    let segments = ((outer * ppp * 0.9) as usize).clamp(48, 256);
    let dirs: Vec<egui::Vec2> = (0..segments)
        .map(|i| {
            let a = std::f32::consts::TAU * i as f32 / segments as f32;
            vec2(a.cos(), a.sin())
        })
        .collect();

    let half_feather = 0.5 / ppp;
    let (top, height) = (center.y - outer, 2.0 * outer);
    let body_color = |p: Pos2| {
        let t = ((p.y - top) / height).clamp(0.0, 1.0);
        over(white(glass.sheen * (1.0 - t)), glass.tint)
    };
    let mut body = egui::Mesh::default();
    for &d in &dirs {
        let pi = center + d * (inner + half_feather);
        let po = center + d * (outer - half_feather);
        body.colored_vertex(center + d * (inner - half_feather), Color32::TRANSPARENT);
        body.colored_vertex(pi, body_color(pi));
        body.colored_vertex(po, body_color(po));
        body.colored_vertex(center + d * (outer + half_feather), Color32::TRANSPARENT);
    }
    let n = segments as u32;
    for i in 0..n {
        let j = (i + 1) % n;
        for k in 0..3 {
            let (a, b) = (4 * i + k, 4 * j + k);
            body.add_triangle(a, a + 1, b + 1);
            body.add_triangle(a, b + 1, b);
        }
    }
    canvas.add(Shape::mesh(body));
    if glass.is_flat() {
        return;
    }

    let width = ((outer - inner) * 0.45).clamp(1.0, 3.0);
    let outer_pts: Vec<Pos2> = dirs.iter().map(|&d| center + d * outer).collect();
    canvas.add(Shape::mesh(rim_mesh(&outer_pts, &dirs, width, ppp, |_, nrm| glass.rim(nrm))));
    let inner_pts: Vec<Pos2> = dirs.iter().map(|&d| center + d * inner).collect();
    let inward: Vec<egui::Vec2> = dirs.iter().map(|&d| -d).collect();
    canvas.add(Shape::mesh(rim_mesh(&inner_pts, &inward, width, ppp, |_, nrm| glass.rim(nrm))));
}

#[cfg(test)]
mod tests {
    use egui::{LayerId, Painter, pos2};

    use super::*;
    use crate::erase::PANE;

    const SQUARE: [Pos2; 4] = [pos2(0.0, 0.0), pos2(10.0, 0.0), pos2(10.0, 10.0), pos2(0.0, 10.0)];

    fn canvas(ctx: &egui::Context) -> Canvas {
        Canvas::new(&Painter::new(ctx.clone(), LayerId::background(), Rect::EVERYTHING))
    }

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-5
    }

    #[test]
    fn flat_glass_has_no_rim() {
        assert!(Glass::flat(Color32::RED).is_flat());
        assert_eq!(Glass::TILE, Glass::flat(crate::color::SURFACE));
        assert!(!Glass::CONTROL.is_flat() && !Glass::TILE_HOVER.is_flat());
        assert_eq!(Glass::flat(Color32::RED).clear, 0.0);
    }

    #[test]
    fn lerp_runs_between_glasses() {
        let (from, to) = (Glass::CONTROL, Glass::TILE_HOVER);
        assert_eq!(Glass::lerp(from, to, 0.0), from);
        let end = Glass::lerp(from, to, 1.0);
        assert_eq!(end.tint, to.tint);
        assert!(close(end.sheen, to.sheen) && close(end.specular, to.specular) && close(end.edge, to.edge));
        let half = Glass::lerp(from, to, 0.5);
        assert!(close(half.specular, (from.specular + to.specular) * 0.5));
    }

    #[test]
    fn tinted_by_takes_some_color() {
        let warm = Glass::tinted_by(Color32::from_rgb(200, 40, 40));
        let cool = Glass::tinted_by(Color32::from_rgb(40, 40, 200));
        assert!(warm.tint.r() > cool.tint.r() && cool.tint.b() > warm.tint.b());
        assert_eq!((warm.clear, warm.sheen, warm.specular, warm.edge), (CLEAR, 0.06, 0.45, 0.55));
    }

    #[test]
    fn rim_is_lit_from_top_left() {
        let lit = Glass::CONTROL.rim(vec2(-0.42, -1.0));
        let shaded = Glass::CONTROL.rim(vec2(0.42, 1.0));
        assert!(lit.r() > shaded.r(), "{lit:?} {shaded:?}");
        assert!(shaded.a() > 0, "the shaded side darkens");
    }

    #[test]
    fn hairline_follows_the_light() {
        let toward = hairline(vec2(-0.42, -1.0));
        let away = hairline(vec2(0.42, 1.0));
        let across = hairline(vec2(1.0, -0.42));
        assert!(close(toward, HAIRLINE_AMBIENT + HAIRLINE_LIT));
        assert!(close(away, HAIRLINE_AMBIENT + HAIRLINE_COUNTER));
        assert!(close(across, HAIRLINE_AMBIENT));
    }

    #[test]
    fn thickness_lights_top_shades_bottom() {
        assert_eq!(thickness(vec2(0.0, -1.0), 1.0), white(THICKNESS_LIGHT));
        assert_eq!(thickness(vec2(0.0, 1.0), 1.0), Color32::from_black_alpha((THICKNESS_SHADE * 255.0) as u8));
        assert_eq!(thickness(vec2(0.0, 1.0), 0.0).a(), 0);
        assert_eq!(thickness(vec2(0.0, -1.0), 0.0).a(), 0);
    }

    #[test]
    fn rim_mesh_is_three_rings() {
        let normals = crate::shape::vertex_normals(&SQUARE);
        let mesh = rim_mesh(&SQUARE, &normals, 2.0, 1.0, |_, _| Color32::WHITE);
        assert_eq!((mesh.vertices.len(), mesh.indices.len()), (3 * 4, 4 * 4 * 3));
        assert!(mesh.indices.iter().all(|index| (*index as usize) < mesh.vertices.len()));
        assert_eq!(mesh.vertices.iter().filter(|vertex| vertex.color == Color32::WHITE).count(), 4);
        assert!(rim_mesh(&SQUARE[..2], &normals[..2], 2.0, 1.0, |_, _| Color32::WHITE).is_empty());
    }

    #[test]
    fn opaque_glass_is_body_and_rim() {
        let ctx = egui::Context::default();
        let mut flat = canvas(&ctx);
        paint(&mut flat, Rect::from_min_size(pos2(10.0, 10.0), vec2(80.0, 30.0)), 8.0, Glass::TILE);
        assert_eq!(flat.shapes.len(), 1, "a flat glass is just its body");
        let mut control = canvas(&ctx);
        paint(&mut control, Rect::from_min_size(pos2(10.0, 10.0), vec2(80.0, 30.0)), 8.0, Glass::CONTROL);
        assert_eq!(control.shapes.len(), 2, "a body and a rim");
        let mut nothing = canvas(&ctx);
        paint(&mut nothing, Rect::from_min_size(pos2(10.0, 10.0), vec2(0.0, 0.0)), 0.0, Glass::CONTROL);
        assert!(nothing.shapes.is_empty(), "a rect with no area paints nothing");
    }

    #[test]
    fn clear_glass_shadows_and_marks_pane() {
        let ctx = egui::Context::default();
        set_seen_through(&ctx, 7.0);
        let mut clear = canvas(&ctx);
        assert_eq!(seen_through(&clear), 1.0, "the amount is clamped");
        paint(&mut clear, Rect::from_min_size(pos2(10.0, 10.0), vec2(200.0, 30.0)), 8.0, Glass::CONTROL);
        assert_eq!(clear.shapes.len(), 5, "shadow, pane, body, thickness and hairline");
        assert!(matches!(&clear.shapes[1], Shape::Mesh(mesh) if mesh.texture_id == PANE));
        set_seen_through(&ctx, -1.0);
        assert_eq!(seen_through(&canvas(&ctx)), 0.0);
    }

    #[test]
    fn ring_has_body_and_two_rims() {
        let ctx = egui::Context::default();
        let mut flat = canvas(&ctx);
        ring(&mut flat, pos2(50.0, 50.0), 10.0, 20.0, Glass::TILE);
        let [Shape::Mesh(body)] = &flat.shapes[..] else { panic!("one body mesh") };
        assert_eq!(body.vertices.len(), 4 * 48, "small rings use the fewest segments");
        let mut lit = canvas(&ctx);
        ring(&mut lit, pos2(50.0, 50.0), 10.0, 400.0, Glass::CONTROL);
        assert_eq!(lit.shapes.len(), 3);
        let Shape::Mesh(big) = &lit.shapes[0] else { panic!("a mesh") };
        assert_eq!(big.vertices.len(), 4 * 256, "large rings stop at the most segments");
    }
}
