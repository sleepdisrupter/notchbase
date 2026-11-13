//! Meshes that cut holes in the window. They carry texture ids the renderer looks for, and their alpha is
//! subtracted from the window instead of drawn.

mod pane;

use egui::epaint::Mesh;
use egui::{Color32, Rect, Shape, TextureId};

pub use pane::{Pane, SMOKE, panes};

// Picked from the top of the user range so they never meet a real texture.
pub const TEXTURE: TextureId = TextureId::User(u64::MAX - 0x6c);
pub const PANE: TextureId = TextureId::User(u64::MAX - 0x6d);

pub fn hole(mut mesh: Mesh) -> Shape {
    mesh.texture_id = TEXTURE;
    Shape::mesh(mesh)
}

pub fn pane(mut mesh: Mesh) -> Shape {
    mesh.texture_id = PANE;
    Shape::mesh(mesh)
}

pub fn erases(texture: TextureId) -> bool {
    texture == TEXTURE || texture == PANE
}

/// A grid mesh over `area` with the strength at each corner from `strength_at`, for holes that fade.
pub fn grid(area: Rect, cell: egui::Vec2, strength_at: impl Fn(egui::Pos2) -> f32) -> Mesh {
    let columns = (area.width() / cell.x.max(1.0)).ceil().max(1.0) as u32;
    let rows = (area.height() / cell.y.max(1.0)).ceil().max(1.0) as u32;
    let mut mesh = Mesh::default();
    for row in 0..=rows {
        for column in 0..=columns {
            let at = area.min
                + egui::vec2(area.width() * column as f32 / columns as f32, area.height() * row as f32 / rows as f32);
            mesh.colored_vertex(at, strength(strength_at(at)));
        }
    }
    let across = columns + 1;
    for row in 0..rows {
        for column in 0..columns {
            let corner = row * across + column;
            mesh.add_triangle(corner, corner + 1, corner + across);
            mesh.add_triangle(corner + 1, corner + across + 1, corner + across);
        }
    }
    mesh
}

/// How strongly a vertex erases. Only the alpha counts.
pub fn strength(alpha: f32) -> Color32 {
    Color32::from_black_alpha((alpha.clamp(0.0, 1.0) * 255.0).round() as u8)
}

#[cfg(test)]
mod tests {
    use egui::{Pos2, pos2, vec2};

    use super::*;

    #[test]
    fn hole_and_pane_retag_the_mesh() {
        let mesh = crate::shape::fill_polygon(
            &[pos2(0.0, 0.0), pos2(4.0, 0.0), pos2(4.0, 4.0)],
            pos2(3.0, 1.0),
            strength(1.0),
            1.0,
        );
        let Shape::Mesh(holed) = hole(mesh.clone()) else { panic!("a mesh") };
        let Shape::Mesh(paned) = pane(mesh.clone()) else { panic!("a mesh") };
        assert_eq!((holed.texture_id, paned.texture_id), (TEXTURE, PANE));
        assert_eq!((holed.vertices.len(), holed.indices.len()), (mesh.vertices.len(), mesh.indices.len()));
        assert_eq!(paned.vertices, mesh.vertices);
    }

    #[test]
    fn only_hole_and_pane_erase() {
        assert!(erases(TEXTURE) && erases(PANE));
        assert!(!erases(TextureId::default()));
        assert!(!erases(TextureId::User(0)));
        assert_ne!(TEXTURE, PANE);
    }

    #[test]
    fn strength_clamps_and_rounds() {
        assert_eq!(strength(0.0), Color32::TRANSPARENT);
        assert_eq!(strength(1.0), Color32::BLACK);
        assert_eq!(strength(0.5).a(), 128);
        assert_eq!(strength(-3.0).a(), 0);
        assert_eq!(strength(7.0).a(), 255);
        assert_eq!(strength(0.25), Color32::from_black_alpha(64));
    }

    #[test]
    fn grid_covers_area_cell_by_cell() {
        let area = Rect::from_min_size(pos2(10.0, 20.0), vec2(10.0, 10.0));
        let fade = grid(area, vec2(5.0, 2.5), |at| (at.y - 20.0) / 10.0);
        assert_eq!((fade.vertices.len(), fade.indices.len()), (3 * 5, 2 * 4 * 6));
        assert_eq!((fade.vertices[0].pos, fade.vertices[14].pos), (pos2(10.0, 20.0), pos2(20.0, 30.0)));
        assert_eq!((fade.vertices[0].color.a(), fade.vertices[14].color.a()), (0, 255));
        assert_eq!(fade.calc_bounds(), area);
    }

    #[test]
    fn grid_keeps_at_least_one_cell() {
        let tiny = Rect::from_min_size(Pos2::ZERO, vec2(0.0, 0.0));
        let single = grid(tiny, vec2(5.0, 5.0), |_| 1.0);
        assert_eq!((single.vertices.len(), single.indices.len()), (4, 6));
        let area = Rect::from_min_size(Pos2::ZERO, vec2(3.0, 2.0));
        let fine = grid(area, vec2(0.0, -4.0), |_| 0.5);
        assert_eq!(fine.vertices.len(), 4 * 3, "a cell is never smaller than a point");
        assert!(fine.vertices.iter().all(|vertex| vertex.color.a() == 128));
    }
}
