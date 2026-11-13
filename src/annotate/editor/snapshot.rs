use egui::{Color32, Pos2, Vec2, pos2, vec2};

use super::bench::{Bench, SCALE};
use crate::annotate::{Brush, CHROME, sample, window_size};
use crate::ui::color;
use crate::ui::snapshot::folder;

const MONITOR: Vec2 = vec2(1512.0, 982.0);
const LIGHTS_IN_THE_TITLE_BAR: bool = cfg!(target_os = "macos");
const TITLE_BAR: f32 = 32.0;
const LIGHTS: [(f32, Color32); 3] = [
    (16.0, Color32::from_rgb(255, 95, 87)),
    (39.0, Color32::from_rgb(254, 188, 46)),
    (62.0, Color32::from_rgb(40, 200, 64)),
];
const LIGHT: f32 = 7.0;
const LABEL: &str = "Ship this one";
const BEING_TYPED: &str = "Words go on";
const SECOND_LINE: &str = "the picture";

fn bench(image: [usize; 2], brush: Brush) -> Bench {
    let size = window_size(image, SCALE, MONITOR, CHROME);
    let mut bench = Bench::new(sample(image), brush, size);
    if LIGHTS_IN_THE_TITLE_BAR {
        bench.editor.sits_under_a_title_bar(TITLE_BAR);
    }
    bench
}

fn arc(center: Pos2, reach: Vec2, turns: f32) -> Vec<Pos2> {
    (0..=48)
        .map(|step| {
            let angle = std::f32::consts::TAU * turns * step as f32 / 48.0;
            center + vec2(reach.x * angle.cos(), reach.y * angle.sin())
        })
        .collect()
}

fn save(bench: &mut Bench, pointer: Pos2, name: &str) {
    let editor = &mut bench.editor;
    let shot = bench.camera.shoot(vec![egui::Event::PointerMoved(pointer)], |ui| {
        editor.ui(ui, 1.0 / 60.0);
        if LIGHTS_IN_THE_TITLE_BAR {
            for (x, light) in LIGHTS {
                ui.painter().circle_filled(pos2(x, TITLE_BAR / 2.0), LIGHT, light);
            }
        }
    });
    shot.save(folder().join(name)).expect("a written snapshot");
}

#[test]
#[ignore = "writes PNGs of the annotator to target/snapshots; run it by name to look at a change"]
fn snapshots() {
    let mut large = bench([1600, 1000], Brush::default());
    let stage = large.fit().rect();
    large.drag(&arc(stage.center() - vec2(150.0, 60.0), vec2(120.0, 46.0), 1.04));
    large.key(egui::Key::Num2, egui::Modifiers::NONE);
    let underline: Vec<Pos2> =
        (0..=20).map(|step| stage.left_bottom() + vec2(180.0 + step as f32 * 14.0, -90.0)).collect();
    large.drag(&underline);
    large.editor.copied(large.editor.job().revision);
    large.settle();
    large.hover(stage.center() + vec2(140.0, 30.0));
    large.settle();
    save(&mut large, stage.center() + vec2(140.0, 30.0), "annotate.png");
    large.key(egui::Key::Num5, egui::Modifiers::NONE);
    (0..7).for_each(|_| drop(large.key(egui::Key::CloseBracket, egui::Modifiers::NONE)));
    let stroke = arc(stage.center() + vec2(170.0, 40.0), vec2(110.0, 70.0), 0.6);
    large.press(stroke[0]);
    stroke.iter().for_each(|at| drop(large.hover(*at)));
    large.settle();
    save(&mut large, stroke[stroke.len() - 1], "annotate-drawing.png");

    let mut small = bench([600, 300], Brush::new(color::GREEN, 14.0));
    let stage = small.fit().rect();
    small.drag(&arc(stage.center(), vec2(90.0, 40.0), 0.55));
    small.settle();
    let away = small.layout().toolbar.center_top() - vec2(0.0, 12.0);
    small.hover(away);
    small.settle();
    save(&mut small, away, "annotate-small.png");
    small.frost(false);
    save(&mut small, away, "annotate-plain.png");

    let mut words = bench([1600, 1000], Brush::default());
    let stage = words.fit().rect();
    words.key(egui::Key::T, egui::Modifiers::NONE);
    words.click(stage.left_top() + vec2(440.0, 100.0));
    words.type_text(LABEL);
    words.key(egui::Key::Enter, egui::Modifiers::NONE);
    words.editor.copied(words.editor.job().revision);
    words.key(egui::Key::Num3, egui::Modifiers::NONE);
    (0..4).for_each(|_| drop(words.key(egui::Key::CloseBracket, egui::Modifiers::NONE)));
    words.click(stage.center() + vec2(-150.0, 40.0));
    words.type_text(BEING_TYPED);
    words.key(egui::Key::Enter, egui::Modifiers::SHIFT);
    words.type_text(SECOND_LINE);
    words.settle();
    let beside = stage.center() + vec2(210.0, -60.0);
    words.hover(beside);
    words.settle();
    save(&mut words, beside, "annotate-text.png");
}
