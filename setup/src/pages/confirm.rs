//! Asks before uninstalling.

use notchbase_paint::canvas::Canvas;
use notchbase_paint::color;
use notchbase_paint::glyph::Glyph;
use notchbase_paint::list::Lines;

use crate::widgets::{self, Choice};

use super::{Intent, Scene, card_rect, footer_row, section};

pub fn confirm(scene: &Scene<'_>, canvas: &mut Canvas) -> Option<Intent> {
    section(scene, canvas, "Uninstall");
    let rect = card_rect(scene, 1);
    widgets::card(canvas, rect, 1);
    let text = Lines::new("Uninstall notchbase?", "It quits and leaves Start. Your shelf and settings stay");
    widgets::badge_row(canvas, rect, color::ORANGE, Glyph::Exclamation, &text);
    let cancel = Choice { key: "cancel", title: "Cancel", primary: false, enabled: true };
    let uninstall = Choice { key: "uninstall", title: "Uninstall", primary: true, enabled: true };
    match widgets::footer(scene.ui, canvas, footer_row(scene), &[cancel, uninstall], scene.live) {
        Some(0) => Some(Intent::Back),
        Some(_) => Some(Intent::Remove),
        None => None,
    }
}
