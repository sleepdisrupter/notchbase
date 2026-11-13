//! Panes are holes that the Mac blurs and darkens behind. This finds their outlines in a frame for the backdrop.

use std::collections::{BTreeMap, HashSet};

use egui::epaint::{ClippedPrimitive, Mesh, Primitive};
use egui::{Pos2, Rect};

use super::PANE;

/// The darkening layers laid over each pane, as (gray, opacity).
pub const SMOKE: [(f32, f32); 2] = [(0.20, 0.5), (0.25, 0.9)];

#[derive(Clone, Debug, PartialEq)]
pub struct Pane {
    pub outline: Vec<Pos2>,
    pub strength: f32,
}

pub fn panes(primitives: &[ClippedPrimitive]) -> Vec<Pane> {
    let mut panes = Vec::new();
    for clipped in primitives {
        let Primitive::Mesh(mesh) = &clipped.primitive else { continue };
        if mesh.texture_id != PANE {
            continue;
        }
        let strength = mesh.vertices.iter().map(|vertex| vertex.color.a()).max().unwrap_or(0);
        let whole = clipped.clip_rect.contains_rect(mesh.calc_bounds());
        for outline in rims(mesh).into_iter().filter(|_| strength > 0) {
            let outline = if whole { outline } else { trim(outline, clipped.clip_rect) };
            if outline.len() >= 3 {
                panes.push(Pane { outline, strength: f32::from(strength) / 255.0 });
            }
        }
    }
    panes
}

/// The outer edges of a mesh, meaning sides only one triangle uses, chained into loops.
fn rims(mesh: &Mesh) -> Vec<Vec<Pos2>> {
    let sides = |corners: &[u32; 3]| [(corners[0], corners[1]), (corners[1], corners[2]), (corners[2], corners[0])];
    let edges: HashSet<(u32, u32)> = mesh.indices.as_chunks::<3>().0.iter().flat_map(sides).collect();
    let mut onward: BTreeMap<u32, u32> =
        edges.iter().filter(|(from, to)| !edges.contains(&(*to, *from))).copied().collect();
    let mut rims = Vec::new();
    while let Some((&start, _)) = onward.first_key_value() {
        let (mut rim, mut at) = (Vec::new(), start);
        while let Some(to) = onward.remove(&at) {
            rim.push(mesh.vertices[at as usize].pos);
            at = to;
        }
        rims.push(rim);
    }
    rims
}

/// Clips an outline to a rect one side at a time (Sutherland-Hodgman).
fn trim(mut outline: Vec<Pos2>, clip: Rect) -> Vec<Pos2> {
    for (axis, bound, above) in
        [(0, clip.min.x, true), (0, clip.max.x, false), (1, clip.min.y, true), (1, clip.max.y, false)]
    {
        let inside = |point: Pos2| if above { point[axis] >= bound } else { point[axis] <= bound };
        let mut kept = Vec::with_capacity(outline.len() + 2);
        for (index, &to) in outline.iter().enumerate() {
            let from = outline[(index + outline.len() - 1) % outline.len()];
            if inside(from) != inside(to) {
                kept.push(from + (to - from) * ((bound - from[axis]) / (to[axis] - from[axis])));
            }
            if inside(to) {
                kept.push(to);
            }
        }
        outline = kept;
    }
    outline
}

#[cfg(test)]
mod tests {
    use egui::{TextureId, pos2};

    use super::*;
    use crate::erase::{TEXTURE, strength};

    fn square(left: f32, texture_id: TextureId, alpha: f32) -> Mesh {
        let corners = [pos2(left, 2.0), pos2(left + 6.0, 2.0), pos2(left + 6.0, 8.0), pos2(left, 8.0)];
        let fill = crate::shape::fill_polygon_with(&corners, pos2(left + 3.0, 5.0), |_| strength(alpha), 1.0);
        Mesh { texture_id, ..fill }
    }

    fn within(mesh: Mesh, clip_rect: Rect) -> Vec<ClippedPrimitive> {
        vec![ClippedPrimitive { clip_rect, primitive: Primitive::Mesh(mesh) }]
    }

    #[test]
    fn pane_is_its_rim_inside_the_clip() {
        let rim = |left: f32, right: f32| Rect::from_min_max(pos2(left, 1.5), pos2(right, 8.5));
        let whole = panes(&within(square(2.0, PANE, 0.5), Rect::EVERYTHING));
        assert_eq!(whole.len(), 1);
        assert_eq!((whole[0].outline.len(), whole[0].strength), (4, 128.0 / 255.0));
        assert_eq!(Rect::from_points(&whole[0].outline), rim(1.5, 8.5), "the feathered edge is the rim");

        let cut = panes(&within(square(2.0, PANE, 0.5), Rect::from_min_max(pos2(0.0, 0.0), pos2(5.0, 20.0))));
        assert_eq!((cut[0].outline.len(), Rect::from_points(&cut[0].outline)), (4, rim(1.5, 5.0)));
        let gone = panes(&within(square(2.0, PANE, 0.5), Rect::from_min_max(pos2(20.0, 0.0), pos2(30.0, 20.0))));
        assert!(gone.is_empty(), "nothing of it is inside the clip");

        assert!(panes(&within(square(2.0, TEXTURE, 0.5), Rect::EVERYTHING)).is_empty(), "a plain hole is no pane");
        assert!(panes(&within(square(2.0, PANE, 0.0), Rect::EVERYTHING)).is_empty(), "a pane faded out is gone");

        let mut two = square(2.0, PANE, 0.5);
        two.append(square(12.0, PANE, 0.5));
        let both = panes(&within(two, Rect::EVERYTHING));
        let rims: Vec<Rect> = both.iter().map(|pane| Rect::from_points(&pane.outline)).collect();
        assert_eq!(rims, [rim(1.5, 8.5), rim(11.5, 18.5)], "panes drawn in one mesh stay apart");
    }
}
