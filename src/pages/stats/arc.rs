//! Paints the progress arc each gauge uses.

use std::f32::consts::{FRAC_PI_2, PI, TAU};

use egui::{Color32, Mesh, Pos2, Shape, Vec2, vec2};

use crate::ui::canvas::Canvas;

const START: f32 = -FRAC_PI_2;
const MIN_FRACTION: f32 = 0.005;
const CAP_STEPS: u32 = 10;

pub(super) fn progress_arc(canvas: &mut Canvas, center: Pos2, radius: f32, width: f32, fraction: f32, color: Color32) {
    if fraction < MIN_FRACTION {
        return;
    }
    let ppp = canvas.pixels_per_point();
    let band = Band { center, radius, half: width * 0.5, feather: 0.5 / ppp, color };
    let sweep = TAU * fraction.min(1.0);
    let closed = sweep >= TAU - 2.0 * band.half / radius;
    let segments = ((sweep * (radius + band.half) * ppp / 2.5).ceil() as usize).clamp(12, 360);
    let mesh = if closed { band.circle(segments) } else { band.arc(sweep, segments) };
    canvas.add(Shape::mesh(mesh));
}

struct Band {
    center: Pos2,
    radius: f32,
    half: f32,
    feather: f32,
    color: Color32,
}

impl Band {
    fn circle(&self, segments: usize) -> Mesh {
        let mut mesh = Mesh::default();
        for i in 0..segments {
            self.column(&mut mesh, direction(START + TAU * i as f32 / segments as f32));
        }
        for i in 0..segments as u32 {
            join(&mut mesh, 4 * i, 4 * ((i + 1) % segments as u32));
        }
        mesh
    }

    fn arc(&self, sweep: f32, segments: usize) -> Mesh {
        let mut mesh = Mesh::default();
        for i in 0..=segments {
            self.column(&mut mesh, direction(START + sweep * i as f32 / segments as f32));
        }
        for i in 0..segments as u32 {
            join(&mut mesh, 4 * i, 4 * (i + 1));
        }
        let end = START + sweep;
        self.round_cap(&mut mesh, end, tangent(end), 4 * segments as u32);
        self.round_cap(&mut mesh, START, -tangent(START), 0);
        mesh
    }

    fn column(&self, mesh: &mut Mesh, dir: Vec2) {
        let (outer, inner) = (self.radius + self.half, self.radius - self.half);
        mesh.colored_vertex(self.center + dir * (outer + self.feather), Color32::TRANSPARENT);
        mesh.colored_vertex(self.center + dir * (outer - self.feather), self.color);
        mesh.colored_vertex(self.center + dir * (inner + self.feather), self.color);
        mesh.colored_vertex(self.center + dir * (inner - self.feather), Color32::TRANSPARENT);
    }

    fn round_cap(&self, mesh: &mut Mesh, angle: f32, out: Vec2, column: u32) {
        let radial = direction(angle);
        let tip = self.center + radial * self.radius;
        let hub = mesh.vertices.len() as u32;
        mesh.colored_vertex(tip, self.color);
        let (mut solid, mut edge) = (column + 1, column);
        for k in 1..=CAP_STEPS {
            let (next_solid, next_edge) = if k == CAP_STEPS {
                (column + 2, column + 3)
            } else {
                let phi = PI * k as f32 / CAP_STEPS as f32;
                let d = radial * phi.cos() + out * phi.sin();
                let s = mesh.vertices.len() as u32;
                mesh.colored_vertex(tip + d * (self.half - self.feather), self.color);
                mesh.colored_vertex(tip + d * (self.half + self.feather), Color32::TRANSPARENT);
                (s, s + 1)
            };
            mesh.add_triangle(hub, solid, next_solid);
            mesh.add_triangle(solid, edge, next_edge);
            mesh.add_triangle(solid, next_edge, next_solid);
            (solid, edge) = (next_solid, next_edge);
        }
    }
}

fn join(mesh: &mut Mesh, a: u32, b: u32) {
    for k in 0..3 {
        mesh.add_triangle(a + k, a + k + 1, b + k + 1);
        mesh.add_triangle(a + k, b + k + 1, b + k);
    }
}

fn direction(angle: f32) -> Vec2 {
    vec2(angle.cos(), angle.sin())
}

fn tangent(angle: f32) -> Vec2 {
    vec2(-angle.sin(), angle.cos())
}
