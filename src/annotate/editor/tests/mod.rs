mod controls;
mod drawing;
mod keys;
mod picture;
mod repeats;
mod typing;

use egui::{Key, Modifiers, Pos2, Vec2, vec2};

use super::bench::Bench;
use super::picture::Picture;
use crate::annotate::Brush;
use crate::annotate::testing::textured;

const WINDOW: Vec2 = vec2(900.0, 640.0);
const IMAGE: [usize; 2] = [800, 400];
const CLOSERS: [(Key, Modifiers); 3] =
    [(Key::Escape, Modifiers::NONE), (Key::Enter, Modifiers::NONE), (Key::W, Modifiers::COMMAND)];

fn bench() -> Bench {
    Bench::new(textured(IMAGE[0], IMAGE[1]), Brush::default(), WINDOW)
}

fn across(bench: &Bench) -> [Pos2; 3] {
    let picture = bench.fit().rect();
    [picture.left_top() + vec2(40.0, 40.0), picture.center(), picture.right_bottom() - vec2(40.0, 40.0)]
}

fn shown(bench: &Bench) -> egui::ColorImage {
    let Picture::Shown(texture) = &bench.editor.picture else { panic!("a picture on screen") };
    bench.camera.texture(texture.id()).expect("an uploaded picture").clone()
}
