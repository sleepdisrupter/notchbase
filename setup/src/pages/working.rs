//! The progress page while it installs or removes.

use egui::{Align2, Id, Rect, pos2, vec2};
use notchbase_paint::canvas::Canvas;
use notchbase_paint::color::label;
use notchbase_paint::controls::Slider;
use notchbase_paint::list::{self, Lines};
use notchbase_paint::{font, metrics};
use notchbase_release::Version;

use crate::install::Stage;
use crate::theme;
use crate::widgets;

use super::{Scene, Task, card_rect, section};

const MB: f64 = 1024.0 * 1024.0;
const PERCENT_W: f32 = 44.0;

pub struct Working<'a> {
    pub task: Task,
    pub stage: &'a Stage,
    pub version: Option<Version>,
    pub size: Option<u64>,
    pub shown: f32,
}

pub fn working(scene: &Scene<'_>, canvas: &mut Canvas, working: &Working<'_>) {
    let installing = working.task == Task::Install;
    section(scene, canvas, if installing { "Installing" } else { "Uninstalling" });
    let rect = card_rect(scene, 2);
    widgets::card(canvas, rect, 1);
    let version = working.version.map(|version| version.to_string()).unwrap_or_default();
    let size = working.size.map(|size| format!("{:.1} MB", size as f64 / MB)).unwrap_or_default();
    let (title, detail) = match working.stage {
        Stage::Removing => ("Removing notchbase".to_owned(), "Quitting it and taking it out of Start".to_owned()),
        Stage::Downloading => (format!("Downloading notchbase {version}"), size),
        Stage::Installing => ("Installing".to_owned(), "Putting notchbase in place".to_owned()),
        _ if installing => ("Starting notchbase".to_owned(), "Look at the top of your screen".to_owned()),
        _ => ("Tidying up".to_owned(), String::new()),
    };
    let top = rect.top() + theme::ROW_H * 0.5 + 4.0;
    let percent_x = rect.right() - theme::INSET;
    let text = Lines::new(&title, &detail);
    list::two_lines(canvas, rect.left() + theme::INSET, percent_x - PERCENT_W, top, &text);
    let percent = format!("{}%", (working.shown * 100.0).round() as u32);
    canvas.text(pos2(percent_x, top), Align2::RIGHT_CENTER, percent, font::value(), label());
    let bar = Rect::from_min_size(
        pos2(rect.left() + theme::INSET, rect.bottom() - theme::INSET - metrics::SLIDER),
        vec2(rect.width() - 2.0 * theme::INSET, metrics::SLIDER),
    );
    let slider =
        Slider { value: working.shown, level: None, leading: None, dimmed: 0.0, enabled: false, thickness: None };
    slider.show(scene.ui, canvas, bar, Id::new("setup.progress"));
}
