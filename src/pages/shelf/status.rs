//! The status line above the tiles: what is selected, work in progress, or a notice.

use egui::{Align2, Color32, Id, Rect, Sense, pos2, vec2};

use super::{Feed, ShelfPage, human_bytes};
use crate::bridge::Action;
use crate::pages::PageCtx;
use crate::shelf::{Item, Notice, Op};
use crate::ui::canvas::{Canvas, fade};
use crate::ui::color::{self, label, secondary_label};
use crate::ui::glyph::{self, Glyph};
use crate::ui::spring::lerp;
use crate::ui::{controls, font};

const LINK_GAP: f32 = 14.0;
const LINK_SLOP: f32 = 4.0;
const TEXT_ROOM: f32 = 8.0;
const SLIDE: f32 = 3.0;
const LIVE_FROM: f32 = 0.6;

#[derive(Clone, Debug, PartialEq)]
pub(super) enum Status {
    Summary { picked: bool, text: String },
    Busy(Op, String),
    Notice(Notice),
}

struct Links<'a> {
    ui: &'a egui::Ui,
    canvas: &'a mut Canvas,
    line: Rect,
    right: f32,
    live: bool,
}

impl Links<'_> {
    fn link(&mut self, title: &str, id: Id) -> bool {
        let width = self.canvas.text_width(title, font::callout());
        let text = Rect::from_min_max(pos2(self.right - width, self.line.top()), pos2(self.right, self.line.bottom()));
        self.right = text.left() - LINK_GAP;
        let response = self.live.then(|| self.ui.interact(text.expand(LINK_SLOP), id, Sense::click()));
        let hover = controls::animate(self.ui, id.with("hover"), response.as_ref().is_some_and(|r| r.hovered()));
        let tint = color::mix(secondary_label(), label(), hover);
        self.canvas.text(text.right_center(), Align2::RIGHT_CENTER, title, font::callout(), tint);
        response.is_some_and(|r| r.clicked())
    }

    fn lead(&mut self, text: &str, tint: Color32) {
        let room = self.right - self.line.left() - TEXT_ROOM;
        let text = controls::truncate(self.canvas, text, font::callout(), room);
        let at = pos2(self.line.left() + 2.0, self.line.center().y);
        self.canvas.text(at, Align2::LEFT_CENTER, text, font::callout(), tint);
    }
}

impl ShelfPage {
    pub(super) fn status_ui(&mut self, ctx: &mut PageCtx<'_>, line: Rect, feed: &Feed, actions: &mut Vec<Action>) {
        let state = self.status_state(feed);
        let shown = self.status.show(Some(&state), ctx.dt);
        if let Some((previous, gone)) = self.status.previous() {
            self.paint_status(ctx, line, feed, &previous, (gone, false), &mut Vec::new());
        }
        self.paint_status(ctx, line, feed, &state, (shown, true), actions);
    }

    pub(super) fn status_leaves(&mut self, ctx: &mut PageCtx<'_>, line: Rect, feed: &Feed) {
        self.status.show(None, ctx.dt);
        if let Some((previous, gone)) = self.status.previous() {
            self.paint_status(ctx, line, feed, &previous, (gone, false), &mut Vec::new());
        }
    }

    fn status_state(&self, feed: &Feed) -> Status {
        let Feed { items, busy, notice } = *feed;
        match (busy, notice) {
            (Some(busy), _) => Status::Busy(busy.op, scope_text(items, &self.chosen(items))),
            (None, Some(notice)) => Status::Notice(notice.clone()),
            (None, None) => {
                let chosen = self.chosen(items);
                Status::Summary { picked: !chosen.is_empty(), text: scope_text(items, &chosen) }
            }
        }
    }

    fn paint_status(
        &mut self,
        ctx: &mut PageCtx<'_>,
        line: Rect,
        feed: &Feed,
        state: &Status,
        (shown, current): (f32, bool),
        actions: &mut Vec<Action>,
    ) {
        if shown <= 0.001 {
            return;
        }
        let mark = ctx.canvas.mark();
        let live = current && super::interactive(ctx) && shown > LIVE_FROM;
        let mut links = Links { ui: ctx.ui, canvas: ctx.canvas, line, right: line.right(), live };
        match state {
            Status::Busy(_, text) => links.lead(text, secondary_label()),
            Status::Notice(notice) => paint_notice(links.canvas, line, notice),
            Status::Summary { picked, text } => self.summary_status(&mut links, feed.items, (*picked, text), actions),
        }
        let slide = vec2(0.0, lerp(SLIDE, 0.0, shown));
        for shape in ctx.canvas.since(mark) {
            shape.translate(slide);
            fade(shape, shown);
        }
    }

    fn chosen<'a>(&self, items: &'a [Item]) -> Vec<&'a Item> {
        items.iter().filter(|item| self.selected.contains(&item.id)).collect()
    }

    fn summary_status(
        &mut self,
        links: &mut Links,
        items: &[Item],
        (picked, text): (bool, &str),
        actions: &mut Vec<Action>,
    ) {
        let selected = self.chosen(items);
        if !picked {
            if links.link("Clear", Id::new("notchbase.shelf.clear")) {
                actions.push(Action::ShelfClear);
            }
            if items.len() > 1 && links.link("Select All", Id::new("notchbase.shelf.all")) {
                self.selected = items.iter().map(|item| item.id).collect();
            }
        } else {
            if links.link("Remove", Id::new("notchbase.shelf.remove")) {
                actions.extend(selected.iter().map(|item| Action::ShelfRemove(item.id)));
                self.selected.clear();
            }
            if links.link("Deselect", Id::new("notchbase.shelf.deselect")) {
                self.selected.clear();
            }
        }
        links.lead(text, secondary_label());
    }
}

fn paint_notice(canvas: &mut Canvas, line: Rect, notice: &Notice) {
    let glyph_at = pos2(line.left() + 6.0, line.center().y);
    if notice.ok {
        glyph::paint(canvas, Glyph::Checkmark, glyph_at, 11.0, color::GREEN);
    } else {
        glyph::paint(canvas, Glyph::Exclamation, glyph_at, 12.0, color::ORANGE);
    }
    let left = glyph_at.x + 11.0;
    let text = controls::truncate(canvas, &notice.text, font::callout(), line.right() - left);
    canvas.text(pos2(left, line.center().y), Align2::LEFT_CENTER, text, font::callout(), label());
}

fn scope_text(items: &[Item], selected: &[&Item]) -> String {
    let bytes = |items: &mut dyn Iterator<Item = &Item>| items.filter_map(|item| item.bytes).sum::<u64>();
    let (count, total) = match selected {
        [] => (items.len(), bytes(&mut items.iter())),
        some => (some.len(), bytes(&mut some.iter().copied())),
    };
    let head = match (selected.is_empty(), count) {
        (true, 1) => "1 file".to_owned(),
        (true, count) => format!("All {count} files"),
        (false, count) => format!("{count} of {} selected", items.len()),
    };
    if total > 0 { format!("{head} \u{b7} {}", human_bytes(total)) } else { head }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::time::Instant;

    use super::*;
    use crate::shelf::{ItemId, Kind, Origin};

    fn item(id: u64, bytes: u64) -> Item {
        Item {
            id: ItemId(id),
            path: PathBuf::from(format!("{id}.png")),
            name: format!("{id}.png"),
            kind: Kind::Image,
            ext: "PNG".into(),
            bytes: Some(bytes),
            pages: None,
            seconds: None,
            dims: None,
            thumb: None,
            thumb_rev: 0,
            added: Instant::now(),
            origin: Origin::Dropped,
        }
    }

    #[test]
    fn scope_says_what_actions_touch() {
        let items = [item(1, 1_000), item(2, 2_000), item(3, 0)];
        assert_eq!(scope_text(&items, &[]), "All 3 files \u{b7} 3.0 KB");
        assert_eq!(scope_text(&items[..1], &[]), "1 file \u{b7} 1.0 KB");
        assert_eq!(scope_text(&items, &[&items[1]]), "1 of 3 selected \u{b7} 2.0 KB");
        assert_eq!(scope_text(&items, &[&items[2]]), "1 of 3 selected");
    }
}
