//! The installer's pages and what they hand back to the app.

mod confirm;
mod finish;
mod welcome;
mod working;

use egui::{Rect, pos2, vec2};
use notchbase_paint::canvas::Canvas;
use notchbase_paint::controls;

use crate::install::{SHORTCUTS, Setting};
use crate::theme;

pub use confirm::confirm;
pub use finish::{done, failed};
pub use welcome::{Welcome, welcome};
pub use working::{Working, working};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Task {
    Install,
    Remove,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Intent {
    Toggle(Setting),
    Install,
    AskRemove,
    Remove,
    Back,
    Retry,
    Close,
}

pub struct Scene<'a> {
    pub ui: &'a egui::Ui,
    pub rect: Rect,
    pub live: bool,
}

pub fn settings() -> Vec<(Setting, &'static str, &'static str)> {
    let all = [
        (Setting::Login, "Open at login", "Start notchbase when you sign in"),
        (Setting::Updates, "Automatic updates", "Install new versions when the notch is idle"),
        (Setting::Assistant, "Claude Code and Codex", "Questions and progress in the notch"),
        (Setting::Desktop, "Desktop shortcut", "Put notchbase on your desktop"),
    ];
    all.into_iter().filter(|(setting, ..)| *setting != Setting::Desktop || SHORTCUTS).collect()
}

pub fn rows() -> usize {
    settings().len()
}

fn section(scene: &Scene<'_>, canvas: &mut Canvas, title: &str) {
    let y = scene.rect.top() + theme::section_top() + theme::SECTION_H * 0.5;
    controls::section_header(canvas, pos2(scene.rect.left() + theme::PAD + theme::INSET, y), title);
}

fn card_rect(scene: &Scene<'_>, rows: usize) -> Rect {
    Rect::from_min_size(
        pos2(scene.rect.left() + theme::PAD, scene.rect.top() + theme::card_top()),
        vec2(scene.rect.width() - 2.0 * theme::PAD, rows as f32 * theme::ROW_H),
    )
}

fn footer_row(scene: &Scene<'_>) -> Rect {
    Rect::from_min_size(
        pos2(scene.rect.left() + theme::PAD, scene.rect.top() + theme::footer_top(rows())),
        vec2(scene.rect.width() - 2.0 * theme::PAD, theme::BUTTON_H),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn desktop_shortcut_only_on_windows() {
        let listed: Vec<Setting> = settings().iter().map(|(setting, ..)| *setting).collect();
        assert_eq!(listed.contains(&Setting::Desktop), SHORTCUTS);
        assert_eq!(listed[..3], [Setting::Login, Setting::Updates, Setting::Assistant]);
        assert_eq!(rows(), listed.len());
    }

    #[test]
    fn settings_have_title_and_detail() {
        assert!(settings().iter().all(|(_, title, detail)| !title.is_empty() && !detail.is_empty()));
    }

    #[test]
    fn card_and_footer_fit_the_padding() {
        let ctx = egui::Context::default();
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            let rect = Rect::from_min_size(pos2(0.0, 0.0), vec2(theme::WIDTH, theme::height(rows())));
            let scene = Scene { ui, rect, live: true };
            let width = theme::WIDTH - 2.0 * theme::PAD;
            let card = Rect::from_min_size(pos2(theme::PAD, theme::card_top()), vec2(width, 2.0 * theme::ROW_H));
            assert_eq!(card_rect(&scene, 2), card);
            let footer = footer_row(&scene);
            assert_eq!((footer.left(), footer.width()), (theme::PAD, width));
            assert_eq!((footer.top(), footer.height()), (theme::footer_top(rows()), theme::BUTTON_H));
            assert_eq!(footer.bottom() + theme::BOTTOM, rect.bottom());
        });
        output.textures_delta.clear();
    }
}
