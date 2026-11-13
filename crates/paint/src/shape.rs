//! Outlines and meshes for the notch and other rounded shapes.

use std::f32::consts::{FRAC_PI_2, PI};

pub use crate::wave::Wave;

use egui::{Color32, Mesh, Pos2, Rect, Vec2, pos2, vec2};

use crate::spring::{lerp, smoothstep};

/// A rounded rect that can flare out into ears at the top, like the notch does, and ripple along the bottom.
#[derive(Clone, Copy, Debug)]
pub struct Blob {
    pub rect: Rect,
    pub top_radius: f32,
    pub bottom_radius: f32,
    pub ear: f32,
    pub bleed_top: bool, // runs 2 points past the top so no seam shows at the screen edge
    pub wave: Option<Wave>,
}

impl Blob {
    pub fn bounds(&self) -> Rect {
        let depth = self.wave.map_or(0.0, |wave| wave.deepest());
        let rect = self.rect.expand2(vec2(self.ear.max(0.0), 0.0));
        Rect::from_min_max(rect.min, pos2(rect.max.x, rect.max.y + depth))
    }

    /// How far down the straight sides run before the bottom corners start to round off.
    pub fn sides_end(&self) -> f32 {
        self.rect.bottom() - Corners::of(self).bottom_extent
    }

    /// Where the right (or left) side of the outline crosses `y`, and the unit direction the
    /// outline runs on downward from there. Above the bottom corners the side counts as straight.
    pub fn side_at(&self, y: f32, right: bool) -> (Pos2, Vec2) {
        let corners = Corners::of(self);
        let (extent, power) = (corners.bottom_extent, 2.0 / corners.bottom_exp);
        let (edge, inward) = if right { (self.rect.right(), -1.0) } else { (self.rect.left(), 1.0) };
        let start = self.rect.bottom() - extent;
        if extent <= 0.01 || y <= start {
            return (pos2(edge, y), Vec2::Y);
        }
        let down = ((y - start) / extent).min(1.0);
        let (s, c) = down.powf(1.0 / power).asin().sin_cos();
        // cos(pi/2) comes out a hair below zero in f32, and powf of a negative number is NaN.
        let c = c.max(0.0);
        let x = edge + inward * extent * (1.0 - c.powf(power));
        let along = vec2(inward * s.powf(2.0 - power), c.powf(2.0 - power)).normalized();
        (pos2(x, y.min(self.rect.bottom())), along)
    }
}

struct Corners {
    ear: f32,
    top_extent: f32,
    top_exp: f32,
    bottom_extent: f32,
    bottom_exp: f32,
}

impl Corners {
    fn of(blob: &Blob) -> Self {
        let (w, h) = (blob.rect.width().max(0.0), blob.rect.height().max(0.0));
        let ear = blob.ear.clamp(0.0, h * 0.5);
        let (top_extent, top_exp) = continuous_corner(blob.top_radius, 0.5 * w.min(h));
        let bottom_room = (h - ear.max(top_extent)).min(0.5 * w);
        let (bottom_extent, bottom_exp) = continuous_corner(blob.bottom_radius, bottom_room);
        Self { ear, top_extent, top_exp, bottom_extent, bottom_exp }
    }
}

/// How many radii from the edge a corner starts to round off.
pub const CORNER_REACH: f32 = 1.32;

/// A superellipse corner like the ones macOS draws. It starts `CORNER_REACH` radii from the edge and
/// gets squarer when there's room.
fn continuous_corner(radius: f32, available: f32) -> (f32, f32) {
    let extent = (radius * CORNER_REACH).clamp(0.0, available.max(0.0));
    let slack = if available > 0.0 { (available - extent) / available } else { 0.0 };
    let exponent = lerp(2.0, 3.1, smoothstep(0.0, 0.3, slack));
    (extent, exponent)
}

fn segments_for(extent: f32, pixels_per_point: f32) -> usize {
    ((extent * pixels_per_point / 1.6) as usize).clamp(6, 48)
}

fn push_corner(out: &mut Vec<Pos2>, center: Pos2, extent: f32, exponent: f32, a0: f32, a1: f32, segments: usize) {
    let power = 2.0 / exponent;
    for i in 0..=segments {
        let t = lerp(a0, a1, i as f32 / segments as f32);
        let (s, c) = t.sin_cos();
        let x = c.signum() * c.abs().powf(power);
        let y = s.signum() * s.abs().powf(power);
        out.push(center + vec2(x, y) * extent);
    }
}

fn push_arc(out: &mut Vec<Pos2>, center: Pos2, radius: f32, a0: f32, a1: f32, segments: usize) {
    for i in 0..=segments {
        let t = lerp(a0, a1, i as f32 / segments as f32);
        out.push(center + vec2(t.cos(), t.sin()) * radius);
    }
}

/// The outline as points, clockwise from the top center.
pub fn outline(blob: &Blob, pixels_per_point: f32) -> Vec<Pos2> {
    let r = blob.rect;
    let (left, top, right, bottom) = (r.left(), r.top(), r.right(), r.bottom());
    let cx = r.center().x;
    let bleed = if blob.bleed_top { 2.0 } else { 0.0 };
    let Corners { ear, top_extent, top_exp, bottom_extent, bottom_exp } = Corners::of(blob);

    let mut pts = Vec::with_capacity(160);
    pts.push(pos2(cx, top - bleed));

    if ear > 0.01 {
        pts.push(pos2(right + ear, top - bleed));
        push_arc(&mut pts, pos2(right + ear, top + ear), ear, -FRAC_PI_2, -PI, segments_for(ear, pixels_per_point));
    } else if top_extent > 0.01 {
        let c = pos2(right - top_extent, top + top_extent);
        push_corner(&mut pts, c, top_extent, top_exp, -FRAC_PI_2, 0.0, segments_for(top_extent, pixels_per_point));
    } else {
        pts.push(pos2(right, top - bleed));
    }

    let segs = segments_for(bottom_extent, pixels_per_point);
    if bottom_extent > 0.01 {
        let c = pos2(right - bottom_extent, bottom - bottom_extent);
        push_corner(&mut pts, c, bottom_extent, bottom_exp, 0.0, FRAC_PI_2, segs);
        if let Some(wave) = &blob.wave {
            push_wave(&mut pts, wave, right - bottom_extent, left + bottom_extent, bottom, pixels_per_point);
        }
        let c = pos2(left + bottom_extent, bottom - bottom_extent);
        push_corner(&mut pts, c, bottom_extent, bottom_exp, FRAC_PI_2, PI, segs);
    } else {
        pts.push(pos2(right, bottom));
        if let Some(wave) = &blob.wave {
            push_wave(&mut pts, wave, right, left, bottom, pixels_per_point);
        }
        pts.push(pos2(left, bottom));
    }

    if ear > 0.01 {
        push_arc(&mut pts, pos2(left - ear, top + ear), ear, 0.0, -FRAC_PI_2, segments_for(ear, pixels_per_point));
        pts.push(pos2(left - ear, top - bleed));
    } else if top_extent > 0.01 {
        let c = pos2(left + top_extent, top + top_extent);
        push_corner(&mut pts, c, top_extent, top_exp, PI, 1.5 * PI, segments_for(top_extent, pixels_per_point));
    } else {
        pts.push(pos2(left, top - bleed));
    }

    pts.dedup_by(|a, b| a.distance_sq(*b) < 1e-6);
    while pts.len() > 1 && pts[0].distance_sq(pts[pts.len() - 1]) < 1e-6 {
        pts.pop();
    }
    pts
}

fn push_wave(out: &mut Vec<Pos2>, wave: &Wave, from: f32, to: f32, bottom: f32, pixels_per_point: f32) {
    let span = from - to;
    if span <= 1.0 {
        return;
    }
    let steps = ((span * pixels_per_point / 2.5) as usize).clamp(8, 200);
    for i in 1..steps {
        let t = i as f32 / steps as f32;
        out.push(pos2(from - span * t, bottom + wave.depth_at(1.0 - t)));
    }
}

fn edge_normal(a: Pos2, b: Pos2) -> Vec2 {
    let d = (b - a).normalized();
    vec2(d.y, -d.x)
}

pub fn vertex_normals(pts: &[Pos2]) -> Vec<Vec2> {
    let n = pts.len();
    let orientation = if signed_area(pts) >= 0.0 { 1.0 } else { -1.0 };
    (0..n)
        .map(|i| {
            let n0 = edge_normal(pts[(i + n - 1) % n], pts[i]) * orientation;
            let n1 = edge_normal(pts[i], pts[(i + 1) % n]) * orientation;
            joined(n0, n1)
        })
        .collect()
}

pub fn path_normals(path: &[Pos2]) -> Vec<Vec2> {
    let n = path.len();
    (0..n)
        .map(|i| {
            let before = (i > 0).then(|| edge_normal(path[i - 1], path[i]));
            let after = (i + 1 < n).then(|| edge_normal(path[i], path[i + 1]));
            match (before, after) {
                (Some(n0), Some(n1)) => joined(n0, n1),
                (Some(normal), None) | (None, Some(normal)) => normal,
                (None, None) => Vec2::ZERO,
            }
        })
        .collect()
}

/// Joins two edge normals into a miter, capped so sharp corners don't spike.
fn joined(n0: Vec2, n1: Vec2) -> Vec2 {
    let sum = n0 + n1;
    let normal = if sum.length_sq() > 1e-8 { sum.normalized() } else { n0 };
    normal / normal.dot(n0).max(0.35)
}

pub fn signed_area(pts: &[Pos2]) -> f32 {
    let mut a = 0.0;
    for i in 0..pts.len() {
        let (p, q) = (pts[i], pts[(i + 1) % pts.len()]);
        a += p.x * q.y - q.x * p.y;
    }
    0.5 * a
}

pub fn fill_with(blob: &Blob, color: impl Fn(Pos2) -> Color32, pixels_per_point: f32) -> Mesh {
    let pts = outline(blob, pixels_per_point);
    match fan_origin(blob, &pts) {
        Some(origin) => fill_polygon_with(&pts, origin, color, pixels_per_point),
        None => Mesh::default(),
    }
}

pub fn fan_origin(blob: &Blob, pts: &[Pos2]) -> Option<Pos2> {
    match pts.first() {
        Some(&first) if blob.bleed_top => Some(first),
        Some(_) if blob.wave.is_some() => Some(pos2(blob.rect.center().x, blob.rect.top() + 0.5)),
        Some(_) => Some(blob.rect.center()),
        None => None,
    }
}

pub fn fill_polygon(pts: &[Pos2], origin: Pos2, color: Color32, pixels_per_point: f32) -> Mesh {
    fill_polygon_with(pts, origin, |_| color, pixels_per_point)
}

/// Fills a polygon as a fan from `origin`, with a half-pixel feather for anti-aliasing. The polygon must
/// be star-shaped from `origin`.
pub fn fill_polygon_with(
    pts: &[Pos2],
    origin: Pos2,
    color_at: impl Fn(Pos2) -> Color32,
    pixels_per_point: f32,
) -> Mesh {
    let n = pts.len();
    let mut mesh = Mesh::default();
    if n < 3 {
        return mesh;
    }

    let half_feather = 0.5 / pixels_per_point;
    mesh.reserve_vertices(2 * n + 1);
    mesh.reserve_triangles(3 * n);

    for (&p, normal) in pts.iter().zip(vertex_normals(pts)) {
        mesh.colored_vertex(p - normal * half_feather, color_at(p));
        mesh.colored_vertex(p + normal * half_feather, Color32::TRANSPARENT);
    }

    let n = n as u32;
    let center = 2 * n;
    mesh.colored_vertex(origin, color_at(origin));
    for i in 0..n {
        let j = (i + 1) % n;
        mesh.add_triangle(center, 2 * i, 2 * j);
        mesh.add_triangle(2 * i, 2 * i + 1, 2 * j + 1);
        mesh.add_triangle(2 * i, 2 * j + 1, 2 * j);
    }
    mesh
}

/// A band just outside a closed outline. `band_along` does the same for an open path.
pub fn band_around(outline: &[Pos2], width: f32, color_at: impl Fn(Pos2) -> Color32, pixels_per_point: f32) -> Mesh {
    if outline.len() < 3 {
        return Mesh::default();
    }
    band(outline, &vertex_normals(outline), true, width, color_at, pixels_per_point)
}

pub fn band_along(path: &[Pos2], width: f32, color_at: impl Fn(Pos2) -> Color32, pixels_per_point: f32) -> Mesh {
    band(path, &path_normals(path), false, width, color_at, pixels_per_point)
}

fn band(
    pts: &[Pos2],
    normals: &[Vec2],
    closed: bool,
    width: f32,
    color_at: impl Fn(Pos2) -> Color32,
    pixels_per_point: f32,
) -> Mesh {
    let n = pts.len();
    let mut mesh = Mesh::default();
    if n < 2 {
        return mesh;
    }

    let half_feather = 0.5 / pixels_per_point;
    let solid = (width - half_feather).max(0.0);
    mesh.reserve_vertices(3 * n);
    mesh.reserve_triangles(4 * n);

    for (&p, &normal) in pts.iter().zip(normals) {
        let color = color_at(p);
        mesh.colored_vertex(p - normal * half_feather, color);
        mesh.colored_vertex(p + normal * solid, color);
        mesh.colored_vertex(p + normal * (width + half_feather), Color32::TRANSPARENT);
    }

    let spans = if closed { n } else { n - 1 };
    for i in 0..spans {
        let (a, b) = (3 * i as u32, 3 * ((i + 1) % n) as u32);
        mesh.add_triangle(a, b, a + 1);
        mesh.add_triangle(a + 1, b, b + 1);
        mesh.add_triangle(a + 1, b + 1, a + 2);
        mesh.add_triangle(a + 2, b + 1, b + 2);
    }
    mesh
}

#[cfg(test)]
mod tests {
    use super::*;

    fn area(pts: &[Pos2]) -> f32 {
        signed_area(pts)
    }

    #[test]
    fn notch_outline_is_clockwise_fan() {
        let blob = Blob {
            rect: Rect::from_min_size(pos2(100.0, 0.0), vec2(200.0, 32.0)),
            top_radius: 0.0,
            bottom_radius: 9.0,
            ear: 5.0,
            bleed_top: true,
            wave: None,
        };
        let pts = outline(&blob, 2.0);
        assert!(area(&pts) > 0.0);
        for i in 1..pts.len() - 1 {
            let (a, b, c) = (pts[0], pts[i], pts[i + 1]);
            let cross = (b - a).x * (c - a).y - (b - a).y * (c - a).x;
            assert!(cross >= -1e-3, "fan triangle {i} flipped: {cross}");
        }
    }

    #[test]
    fn capsule_ends_are_round() {
        let blob = Blob {
            rect: Rect::from_min_size(pos2(0.0, 6.0), vec2(180.0, 30.0)),
            top_radius: 15.0,
            bottom_radius: 15.0,
            ear: 0.0,
            bleed_top: false,
            wave: None,
        };
        let pts = outline(&blob, 1.0);
        let right_end = pts.iter().map(|p| p.x).fold(f32::MIN, f32::max);
        assert!((right_end - 180.0).abs() < 0.01);
        assert!(area(&pts) > 0.0);
    }

    const CAPSULE_RADIUS: f32 = 18.5;
    const BAND: f32 = 1.6;
    const PIXELS: f32 = 2.0;

    fn capsule() -> Blob {
        Blob {
            rect: Rect::from_min_size(pos2(0.0, 10.0), vec2(126.0, 2.0 * CAPSULE_RADIUS)),
            top_radius: CAPSULE_RADIUS,
            bottom_radius: CAPSULE_RADIUS,
            ear: 0.0,
            bleed_top: false,
            wave: None,
        }
    }

    fn beyond_capsule(p: Pos2) -> f32 {
        let rect = capsule().rect;
        let spine = pos2(p.x.clamp(rect.left() + CAPSULE_RADIUS, rect.right() - CAPSULE_RADIUS), rect.center().y);
        p.distance(spine) - CAPSULE_RADIUS
    }

    fn lies_just_outside(band: &Mesh) {
        let half_feather = 0.5 / PIXELS;
        for vertex in &band.vertices {
            let beyond = beyond_capsule(vertex.pos);
            if vertex.color == Color32::TRANSPARENT {
                assert!((beyond - (BAND + half_feather)).abs() < 0.05, "the feathered rim sits {beyond} out");
            } else {
                assert!(beyond > -half_feather - 0.05 && beyond < BAND + 0.05, "the solid band sits {beyond} out");
            }
        }
    }

    #[test]
    fn side_at_follows_the_outline() {
        let notch = Blob {
            rect: Rect::from_min_size(pos2(100.0, 0.0), vec2(297.0, 32.0)),
            top_radius: 0.0,
            bottom_radius: 10.0,
            ear: 5.0,
            bleed_top: true,
            wave: None,
        };
        let opened_tab = Blob {
            rect: Rect::from_min_size(pos2(420.0, 0.0), vec2(135.0, 32.0)),
            bottom_radius: 18.56,
            ear: 4.0,
            ..notch
        };
        for blob in [notch, opened_tab, capsule()] {
            let pts = outline(&blob, PIXELS);
            let (from, to) = (blob.rect.center().y.max(blob.rect.top() + blob.ear), blob.rect.bottom());
            for right in [true, false] {
                for step in 0..=40 {
                    let (p, along) = blob.side_at(lerp(from, to, step as f32 / 40.0), right);
                    let (gap, span) = (0..pts.len())
                        .map(|i| {
                            let (a, b) = (pts[i], pts[(i + 1) % pts.len()]);
                            let t = ((p - a).dot(b - a) / (b - a).length_sq().max(1e-9)).clamp(0.0, 1.0);
                            (p.distance(a + (b - a) * t), (b - a).normalized())
                        })
                        .fold((f32::MAX, Vec2::ZERO), |best, next| if next.0 < best.0 { next } else { best });
                    assert!(gap < 0.05, "{p:?} is {gap} off the outline");
                    assert!(along.y >= 0.0 && along.dot(span).abs() > 0.99, "{along:?} runs across {span:?}");
                }
            }
        }
        let start = notch.sides_end();
        assert_eq!(notch.side_at(start - 3.0, true), (pos2(397.0, start - 3.0), Vec2::Y), "straight above the corner");
        assert!(start < 32.0 - 10.0, "a continuous corner starts rounding above its radius");
    }

    #[test]
    fn band_around_closes_outside() {
        let pts = outline(&capsule(), PIXELS);
        let band = band_around(&pts, BAND, |_| Color32::WHITE, PIXELS);
        lies_just_outside(&band);
        assert_eq!(band.vertices.len(), 3 * pts.len());
        assert_eq!(band.indices.len(), 3 * 4 * pts.len(), "every span is laid, the last one back to the first");
    }

    #[test]
    fn band_along_stops_at_ends() {
        let pts = outline(&capsule(), PIXELS);
        let bottom: Vec<Pos2> = pts.iter().copied().filter(|p| p.y > capsule().rect.center().y + 10.0).collect();
        let band = band_along(&bottom, BAND, |_| Color32::WHITE, PIXELS);
        lies_just_outside(&band);
        assert!(band.vertices.iter().all(|vertex| vertex.pos.y > capsule().rect.center().y), "under the capsule");
        assert_eq!(band.indices.len(), 3 * 4 * (bottom.len() - 1), "no span joins the two ends");
    }
}
