use egui::epaint::{Primitive, Vertex};
use egui::{Pos2, vec2};

use super::super::bench::Bench;
use super::super::picture::Picture;
use super::{IMAGE, WINDOW, across, bench};
use crate::annotate::Brush;
use crate::annotate::testing::solid;

const CORNER_REACH: f32 = 0.5;
const INK_REACH: f32 = 5.0;

fn drawn(bench: &mut Bench) -> Vec<Vertex> {
    let Picture::Shown(texture) = &bench.editor.picture else { panic!("a picture on screen") };
    let picture = texture.id();
    let meshes = bench.tessellated().into_iter().filter_map(|clipped| match clipped.primitive {
        Primitive::Mesh(mesh) if mesh.texture_id == picture => Some(mesh.vertices),
        _ => None,
    });
    meshes.flatten().collect()
}

#[test]
fn picture_fills_its_corners() {
    for image in [[20, 20], [3000, 40], [40, 3000], IMAGE] {
        let mut bench = Bench::new(solid(image[0], image[1], [200, 40, 40]), Brush::default(), WINDOW);
        let (rect, drawn) = (bench.fit().rect(), drawn(&mut bench));
        for corner in [rect.left_top(), rect.right_top(), rect.left_bottom(), rect.right_bottom()] {
            let nearest = drawn.iter().map(|vertex| vertex.pos.distance(corner)).fold(f32::MAX, f32::min);
            assert!(nearest < CORNER_REACH, "{image:?}: nothing is drawn within {nearest} pt of {corner:?}");
        }
    }
}

#[test]
fn ring_shows_only_where_it_draws() {
    let mut bench = bench();
    let [_, middle, end] = across(&bench);
    let edge: Pos2 = bench.fit().rect().left_center();
    for outside in [1.0, 3.0, 4.5, 6.0] {
        bench.hover(edge - vec2(outside, 0.0));
        assert_eq!(bench.editor.ring.target, 0.0, "{outside} pt outside the picture");
        assert_eq!(bench.drag(&[edge - vec2(outside, 0.0), middle, end]), []);
    }
    bench.hover(edge + vec2(1.0, 0.0));
    assert_eq!(bench.editor.ring.target, 1.0, "just inside the picture");
    assert_eq!(bench.drag(&[edge + vec2(1.0, 0.0), middle, end]).len(), 1);
    bench.hover(edge - vec2(INK_REACH, 0.0));
    assert_eq!(bench.editor.ring.target, 0.0);
}
