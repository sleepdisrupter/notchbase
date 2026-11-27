//! The Find view's rows: the switch, what the index holds, the system key and clearing what Find has learned.

use std::time::{Duration, Instant};

use egui::{Align2, Id, Rect, Sense, pos2, vec2};

use super::rows::{INSET, PILL_H, Row, Toggle, paint_text, pill_width, toggled};
use super::search::Said;
use crate::bridge::{Action, FindState, FindView, SystemKey};
use crate::pages::PageCtx;
use crate::ui::color::{GREEN, label};
use crate::ui::glass::{self, Glass};
use crate::ui::glyph::{self, Glyph};
use crate::ui::{controls, font};

const CONFIRMED_FOR: Duration = Duration::from_millis(1200);
const CHECK: f32 = 11.0;
const GROUP: usize = 3;
const THOUSANDS: char = ',';
const FIND_TITLE: &str = "Find files";
const OFF_DETAIL: &str = "Search every file and folder from the notch";
const STARTING_DETAIL: &str = "Looking through your files";
const LEARNED_TITLE: &str = "What Find learned";
#[cfg(target_os = "macos")]
const LEARNED_DETAIL: &str = "Stays on this Mac";
#[cfg(not(target_os = "macos"))]
const LEARNED_DETAIL: &str = "Stays on this PC";
const CLEAR: &str = "Clear";
#[cfg(target_os = "macos")]
const KEY_TITLE: &str = "Replace Spotlight";
#[cfg(target_os = "macos")]
const KEY_DETAIL: &str = "\u{2318}Space opens Find instead";
#[cfg(target_os = "macos")]
const KEY_BLOCKED: &str = "Allow notchbase in Accessibility settings";
#[cfg(not(target_os = "macos"))]
const KEY_TITLE: &str = "Replace the Start menu";
#[cfg(not(target_os = "macos"))]
const KEY_DETAIL: &str = "The Windows key opens Find instead";
#[cfg(not(target_os = "macos"))]
const KEY_BLOCKED: &str = "Couldn't watch the Windows key";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum FindRow {
    Files,
    Key,
    Learned,
}

pub(super) const ROWS: &[FindRow] = &[FindRow::Files, FindRow::Key, FindRow::Learned];

impl FindRow {
    pub(super) fn said(self, view: &FindView) -> Said {
        let (title, detail, words) = match self {
            FindRow::Files => (FIND_TITLE, detail(view), "search index spotlight files folders"),
            FindRow::Key => (KEY_TITLE, key_detail(view.system_key).to_owned(), "shortcut keyboard spotlight start"),
            FindRow::Learned => (LEARNED_TITLE, LEARNED_DETAIL.to_owned(), "history forget clear reset privacy"),
        };
        Said { title: title.to_owned(), detail, words }
    }

    pub(super) fn paint(self, ctx: &mut PageCtx<'_>, rect: Rect, view: &FindView, live: bool) -> Option<Action> {
        match self {
            FindRow::Files => {
                let on = view.state != FindState::Off;
                let row = Row { rect, title: FIND_TITLE, detail: &detail(view) };
                let toggle = Toggle { id: Id::new("notchbase.settings.find"), on, enabled: live };
                toggled(ctx, &row, &toggle).then_some(Action::SetFindEnabled(!on))
            }
            FindRow::Key => {
                let taken = view.system_key != SystemKey::Off;
                let row = Row { rect, title: KEY_TITLE, detail: key_detail(view.system_key) };
                let toggle = Toggle { id: Id::new("notchbase.settings.find.key"), on: taken, enabled: live };
                toggled(ctx, &row, &toggle).then_some(Action::SetFindSystemKey(!taken))
            }
            FindRow::Learned => {
                let id = Id::new("notchbase.settings.find.learned");
                let (clear, cleared) = clear_pill(ctx, rect.right() - INSET, rect.center().y, id, live);
                paint_text(ctx, &Row { rect, title: LEARNED_TITLE, detail: LEARNED_DETAIL }, clear.left());
                cleared.then_some(Action::FindForget)
            }
        }
    }
}

/// The tile's line: off, or how many items the index holds.
pub(super) fn summary(view: &FindView) -> String {
    match view.state {
        FindState::Off => "Off".to_owned(),
        _ => format!("On \u{b7} {}", detail(view).to_lowercase()),
    }
}

fn detail(view: &FindView) -> String {
    match (view.state, view.entries) {
        (FindState::Off, _) => OFF_DETAIL.to_owned(),
        (_, 0) => STARTING_DETAIL.to_owned(),
        (_, entries) => format!("{} items", grouped(entries)),
    }
}

fn key_detail(key: SystemKey) -> &'static str {
    if key == SystemKey::Blocked { KEY_BLOCKED } else { KEY_DETAIL }
}

fn grouped(count: u32) -> String {
    let digits = count.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / GROUP);
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(GROUP) {
            out.push(THOUSANDS);
        }
        out.push(digit);
    }
    out
}

fn clear_pill(ctx: &mut PageCtx<'_>, right: f32, center_y: f32, id: Id, live: bool) -> (Rect, bool) {
    let width = pill_width(ctx.canvas, CLEAR);
    let rect = Rect::from_min_size(pos2(right - width, center_y - PILL_H * 0.5), vec2(width, PILL_H));
    let response = ctx.ui.interact(rect, id, if live { Sense::click() } else { Sense::hover() });
    let clicked = live && response.clicked();
    let left = confirming(ctx, id, clicked);
    if !left.is_zero() {
        ctx.ui.ctx().request_repaint_after(left);
    }
    let done = controls::animate(ctx.ui, id.with("done"), !left.is_zero());
    let hover = controls::animate(ctx.ui, id.with("hover"), live && response.hovered());
    glass::paint(ctx.canvas, rect, PILL_H * 0.5, Glass::lerp(Glass::CONTROL, Glass::TILE_HOVER, hover));
    let words = label().gamma_multiply(1.0 - done);
    ctx.canvas.text(rect.center(), Align2::CENTER_CENTER, CLEAR, font::callout(), words);
    if done > 0.001 {
        glyph::paint(ctx.canvas, Glyph::Checkmark, rect.center(), CHECK, GREEN.gamma_multiply(done));
    }
    (rect, clicked)
}

fn confirming(ctx: &mut PageCtx<'_>, id: Id, clicked: bool) -> Duration {
    let now = ctx.now;
    let since = ctx.ui.data_mut(|memory| {
        if clicked {
            memory.insert_temp(id, now);
        }
        memory.get_temp::<Instant>(id)
    });
    since.map_or(Duration::ZERO, |at| CONFIRMED_FOR.saturating_sub(now.saturating_duration_since(at)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pages::harness::{Harness, PAGE, press, release};

    const ROW: f32 = 50.0;

    fn pressed(id: &str) -> Vec<Action> {
        let mut harness = Harness::new();
        let view = FindView { state: FindState::Ready, system_key: SystemKey::On, ..FindView::default() };
        let row =
            |index: f32| Rect::from_min_size(pos2(PAGE.left(), PAGE.top() + index * ROW), vec2(PAGE.width(), ROW));
        let rects = [row(0.0), row(1.0), row(2.0)];
        let frame = |harness: &mut Harness, events| {
            harness.frame(events, |ctx| {
                ROWS.iter().zip(rects).filter_map(|(each, rect)| each.paint(ctx, rect, &view, true)).collect::<Vec<_>>()
            })
        };
        frame(&mut harness, Vec::new());
        let at = harness.rect_of(Id::new(id)).center();
        let mut actions = frame(&mut harness, press(at));
        actions.extend(frame(&mut harness, release(at)));
        actions
    }

    #[test]
    fn find_switches_turn_off() {
        assert_eq!(pressed("notchbase.settings.find"), [Action::SetFindEnabled(false)]);
        assert_eq!(pressed("notchbase.settings.find.key"), [Action::SetFindSystemKey(false)]);
    }

    #[test]
    fn counts_group_thousands() {
        assert_eq!(grouped(0), "0");
        assert_eq!(grouped(999), "999");
        assert_eq!(grouped(1_000), "1,000");
        assert_eq!(grouped(1_452_310), "1,452,310");
        assert_eq!(grouped(u32::MAX), "4,294,967,295");
    }

    #[test]
    fn find_row_says_index_size() {
        let off = FindView::default();
        assert_eq!(detail(&off), OFF_DETAIL);
        let starting = FindView { state: FindState::Starting, entries: 12, ..FindView::default() };
        assert_eq!(detail(&starting), "12 items");
        let empty = FindView { state: FindState::Ready, ..FindView::default() };
        assert_eq!(detail(&empty), STARTING_DETAIL);
        let ready = FindView { state: FindState::Ready, entries: 1_452_310, ..FindView::default() };
        assert_eq!(detail(&ready), "1,452,310 items");
    }

    #[test]
    fn tile_says_on_or_off() {
        assert_eq!(summary(&FindView::default()), "Off");
        let ready = FindView { state: FindState::Ready, entries: 1_452_310, ..FindView::default() };
        assert_eq!(summary(&ready), "On \u{b7} 1,452,310 items");
    }

    #[test]
    fn key_row_explains_key() {
        assert_eq!((key_detail(SystemKey::Off), key_detail(SystemKey::On)), (KEY_DETAIL, KEY_DETAIL));
        assert_eq!(key_detail(SystemKey::Blocked), KEY_BLOCKED);
        assert!(KEY_DETAIL.ends_with("opens Find instead") && !KEY_TITLE.is_empty());
    }
}
