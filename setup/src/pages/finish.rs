//! The last page, once it worked or failed.

use notchbase_paint::canvas::Canvas;
use notchbase_paint::color;
use notchbase_paint::glyph::Glyph;
use notchbase_paint::list::Lines;

use crate::widgets::{self, Choice};

use super::{Intent, Scene, Task, card_rect, footer_row, section};

pub fn done(scene: &Scene<'_>, canvas: &mut Canvas, task: Task) -> Option<Intent> {
    section(scene, canvas, "Done");
    let rect = card_rect(scene, 1);
    widgets::card(canvas, rect, 1);
    let (title, detail) = match task {
        Task::Install => ("notchbase is installed", "It\u{2019}s running at the top of your screen"),
        Task::Remove => ("notchbase is uninstalled", "Your shelf and settings stay put in case you come back"),
    };
    let text = Lines::new(title, detail);
    widgets::badge_row(canvas, rect, color::GREEN, Glyph::Checkmark, &text);
    let done = Choice { key: "done", title: "Done", primary: true, enabled: true };
    widgets::footer(scene.ui, canvas, footer_row(scene), &[done], scene.live).map(|_| Intent::Close)
}

pub fn failed(scene: &Scene<'_>, canvas: &mut Canvas, task: Task, problem: &str) -> Option<Intent> {
    section(scene, canvas, "Something went wrong");
    let rect = card_rect(scene, 1);
    widgets::card(canvas, rect, 1);
    let title = match task {
        Task::Install => "Couldn\u{2019}t install notchbase",
        Task::Remove => "Couldn\u{2019}t uninstall notchbase",
    };
    let text = Lines::new(title, problem);
    widgets::badge_row(canvas, rect, color::RED, Glyph::Exclamation, &text);
    let close = Choice { key: "close", title: "Close", primary: false, enabled: true };
    let retry = Choice { key: "retry", title: "Try Again", primary: true, enabled: true };
    match widgets::footer(scene.ui, canvas, footer_row(scene), &[close, retry], scene.live) {
        Some(0) => Some(Intent::Close),
        Some(_) => Some(Intent::Retry),
        None => None,
    }
}
