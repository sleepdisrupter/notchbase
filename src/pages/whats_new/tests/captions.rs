use std::sync::Arc;

use egui::Galley;

use crate::pages::harness::Harness;
use crate::pages::whats_new::layout::{CAPTION_ROWS, EVEN_SLACK, caption};
use crate::ui::color::secondary_label;
use crate::ui::font;

const ORPHANED: &str = "Press the magnifier in the corner and type to reach any file or folder you have";
const STUBBORN: &str = "Press screenshotting today";
const NEARLY_ALL: f32 = 0.9;
const JUST_SHORT: f32 = 0.97;

struct Wrapped {
    plain: Arc<Galley>,
    even_room: Arc<Galley>,
    caption: Arc<Galley>,
}

fn wrapped(harness: &mut Harness, words: &str, share: f32) -> Wrapped {
    harness.frame(Vec::new(), |ctx| {
        let natural = ctx.canvas.text_width(words, font::subheadline());
        let clamped = |room: f32| ctx.canvas.clamped(words, font::subheadline(), secondary_label(), room, CAPTION_ROWS);
        let room = natural * share;
        Wrapped {
            plain: clamped(room),
            even_room: clamped((natural * 0.5 + EVEN_SLACK).min(room)),
            caption: caption(ctx.canvas, words, room),
        }
    })
}

fn widths(galley: &Galley) -> Vec<f32> {
    galley.rows.iter().map(|row| row.row.size.x).collect()
}

#[test]
fn stranded_words_split_evenly() {
    let shown = wrapped(&mut Harness::new(), ORPHANED, NEARLY_ALL);
    let (plain, even) = (widths(&shown.plain), widths(&shown.caption));
    assert!(!shown.plain.elided && plain.len() == 2, "the sample has to wrap to two lines: {plain:?}");
    assert!(!shown.caption.elided && even.len() == 2, "{even:?}");
    assert!(even[1] > plain[1] && even[0] < plain[0], "{even:?} is no more even than {plain:?}");
    assert!((even[0] - even[1]).abs() < (plain[0] - plain[1]).abs() * 0.5, "{even:?} after {plain:?}");
    assert!(shown.caption.size().x < shown.plain.size().x && shown.caption.size().y == shown.plain.size().y);
}

#[test]
fn long_caption_keeps_ellipsis() {
    let shown = wrapped(&mut Harness::new(), ORPHANED, 1.0 / (CAPTION_ROWS + 1) as f32);
    assert!(shown.plain.elided && shown.caption.elided);
    assert_eq!(shown.caption.rows.len(), CAPTION_ROWS);
    assert_eq!(widths(&shown.caption), widths(&shown.plain));
}

#[test]
fn even_split_never_adds_a_line() {
    let shown = wrapped(&mut Harness::new(), STUBBORN, JUST_SHORT);
    let plain = widths(&shown.plain);
    assert!(!shown.plain.elided && plain.len() == 2, "the sample has to wrap to two lines: {plain:?}");
    assert!(shown.even_room.elided, "the long middle word has to push the even split past its two lines");
    assert_eq!(widths(&shown.caption), plain);
}
