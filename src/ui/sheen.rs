//! A soft slanted band of light clipped to a rounded rectangle, for a shine that sweeps across a tile.

use egui::{Color32, Mesh, Pos2, Rect, pos2};

const STRIP: f32 = 1.5;

#[derive(Clone, Copy, Debug)]
pub struct Band {
    pub center_x: f32,
    pub half_width: f32,
    pub slant: f32,
    pub color: Color32,
}

pub fn clipped_band(rect: Rect, radius: f32, band: Band) -> Mesh {
    let radius = radius.clamp(0.0, 0.5 * rect.width().min(rect.height()));
    let columns = ((rect.width() / STRIP).ceil() as usize).max(1);
    let mut mesh = Mesh::default();
    for i in 0..=columns {
        let x = rect.left() + rect.width() * i as f32 / columns as f32;
        let (top, bottom) = vertical_span(rect, radius, x);
        for y in [top, bottom] {
            let at = pos2(x, y);
            mesh.colored_vertex(at, band.color.gamma_multiply(intensity(band, at, rect.center().y)));
        }
        if i > 0 {
            let base = 2 * i as u32;
            mesh.add_triangle(base - 2, base - 1, base);
            mesh.add_triangle(base - 1, base + 1, base);
        }
    }
    mesh
}

fn vertical_span(rect: Rect, radius: f32, x: f32) -> (f32, f32) {
    let dx = (rect.left() + radius - x).max(x - (rect.right() - radius)).clamp(0.0, radius);
    let inset = radius - (radius * radius - dx * dx).max(0.0).sqrt();
    (rect.top() + inset, rect.bottom() - inset)
}

fn intensity(band: Band, at: Pos2, middle_y: f32) -> f32 {
    let offset = (at.x + band.slant * (at.y - middle_y) - band.center_x) / band.half_width.max(1e-3);
    (-offset * offset).exp()
}

#[cfg(test)]
mod tests {
    use egui::vec2;

    use super::*;

    #[test]
    fn corners_pinch_the_band() {
        let rect = Rect::from_min_size(Pos2::ZERO, vec2(40.0, 40.0));
        assert_eq!(vertical_span(rect, 10.0, 20.0), (0.0, 40.0));
        assert_eq!(vertical_span(rect, 10.0, 0.0), (10.0, 30.0));
        let (top, bottom) = vertical_span(rect, 10.0, 40.0);
        assert!((top - 10.0).abs() < 1e-4 && (bottom - 30.0).abs() < 1e-4);
    }

    #[test]
    fn band_is_brightest_at_center() {
        let band = Band { center_x: 20.0, half_width: 6.0, slant: 0.0, color: Color32::WHITE };
        let mesh = clipped_band(Rect::from_min_size(Pos2::ZERO, vec2(40.0, 20.0)), 4.0, band);
        let brightest = mesh.vertices.iter().max_by_key(|v| v.color.a()).unwrap();
        assert!((brightest.pos.x - 20.0).abs() <= STRIP);
        assert_eq!(mesh.indices.len() % 3, 0);
    }
}
