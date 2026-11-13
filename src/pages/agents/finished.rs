//! The Recent section: runs that finished, each with the start of its last message.

use egui::{Rect, vec2};

use super::AgentsPage;
use super::layout::message_width;
use super::message::Message;
use super::{recent, words, wrap};
use crate::assistant::Finished;
use crate::pages::PageCtx;
use crate::pages::settings::{ROW_H, card_of};
use crate::ui::rows;

impl AgentsPage {
    pub(super) fn recent(&mut self, ctx: &mut PageCtx<'_>, top: f32, now_ms: u64, live: bool) {
        let recent: Vec<(Finished, f32)> = self.done.iter().map(|(_, done, shown)| (done.clone(), shown)).collect();
        let tall = |done: &Finished| ROW_H + self.unfold.extra(done.id);
        let heights: Vec<f32> = recent.iter().map(|(done, shown)| tall(done) * shown).collect();
        let rects = card_of(ctx, top, &heights);
        let width = rects.first().copied().map_or(0.0, message_width);
        let mut hovered = None;
        let mut fulls = Vec::with_capacity(recent.len());
        for ((done, shown), rect) in recent.iter().zip(rects) {
            let canvas = &*ctx.canvas;
            let wrapped = self.wraps.get(done.id, width, || wrap::measure(canvas, &words::whole(&done.message), width));
            let full = wrapped.extra();
            let settled = *shown > 0.99;
            if live && settled && hovered.is_none() && full > 0.0 && ctx.ui.rect_contains_pointer(rect) {
                hovered = Some(done.id);
            }
            let message =
                Message { first: words::first_line(&done.message), wrapped, extra: self.unfold.extra(done.id) };
            let whole = Rect::from_min_size(rect.min, vec2(rect.width(), ROW_H + message.extra));
            let mark = ctx.canvas.mark();
            recent::paint(ctx, whole, done, now_ms, &message);
            rows::reveal(ctx.canvas, mark, rect, *shown);
            fulls.push((done.id, full));
        }
        let ids: Vec<u64> = recent.iter().map(|(done, _)| done.id).collect();
        self.wraps.keep(&ids, width);
        self.unfold.follow(&fulls, hovered, !live);
        self.unfold.step(ctx.dt);
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::assistant::Provider;
    use crate::bridge::AgentsView;
    use crate::pages::agents::layout;

    const LINE: f32 = 12.0;

    fn done(id: u64) -> Finished {
        Finished {
            id,
            provider: Provider::Claude,
            project: "rocket".into(),
            message: "Shipped".into(),
            took: Duration::from_secs(60),
            ended_ms: 0,
        }
    }

    #[test]
    fn open_rows_add_their_lines() {
        let view = AgentsView { running: Vec::new(), recent: vec![done(1), done(2)] };
        let mut page = AgentsPage::default();
        assert_eq!(page.height(&view), layout::height(0, 2), "no hover, no growth");
        page.unfold.follow(&[(1, 2.0 * LINE), (2, LINE)], Some(1), true);
        assert_eq!(page.height(&view), layout::height(0, 2) + 2.0 * LINE);
        page.unfold.follow(&[(1, 2.0 * LINE), (2, LINE)], Some(2), true);
        assert_eq!(page.height(&view), layout::height(0, 2) + LINE);
        assert!(!page.is_animating());
        page.unfold.follow(&[(1, 2.0 * LINE), (2, LINE)], Some(1), false);
        page.unfold.step(1.0 / 60.0);
        assert!(page.is_animating());
        let between = page.height(&view) - layout::height(0, 2);
        assert!(between > 0.0 && between < 2.0 * LINE, "the height follows the spring: {between}");
    }

    #[test]
    fn next_label_change_is_soonest() {
        let minute = 60_000;
        let at = |ended_ms| Finished { ended_ms, ..done(1) };
        let empty = AgentsView { running: Vec::new(), recent: Vec::new() };
        assert_eq!(crate::pages::agents::next_label_change(&empty, 10 * minute), None);
        let view = AgentsView { running: Vec::new(), recent: vec![at(0), at(4 * minute + 40_000)] };
        let now = 10 * minute + 30_000;
        assert_eq!(crate::pages::agents::next_label_change(&view, now), Some(10 * minute + 40_000));
    }
}
