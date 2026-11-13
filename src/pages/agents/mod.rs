//! The Agents page: the Claude Code and Codex runs going now, and the ones that finished recently.

mod empty;
mod finished;
mod layout;
mod message;
mod recent;
mod running;
mod unfold;
mod words;
mod wrap;

use std::collections::HashMap;

use egui::{Rect, vec2};
use notchbase_sdk::now_ms;

use super::PageCtx;
use super::settings::{ROW_H, SECTION_H, card, card_of, section};
use crate::assistant::{Finished, Provider};
use crate::bridge::{Action, AgentsView, RunRow};
use crate::ui::agent_mark::{self, Spin};
use crate::ui::canvas::fade;
use crate::ui::motion::Clock;
use crate::ui::rows::{self, Rows};
use unfold::Unfold;
use wrap::Wraps;

const LIVE_FROM: f32 = 0.5;
const RECENT: &str = "Recent";

type RunKey = (Provider, String);

#[derive(Default)]
pub struct AgentsPage {
    clock: Clock,
    spins: HashMap<RunKey, Spin>,
    running: bool,
    unfold: Unfold,
    wraps: Wraps,
    live: Rows<RunKey, RunRow>,
    done: Rows<u64, Finished>,
}

pub fn next_label_change(view: &AgentsView, now_ms: u64) -> Option<u64> {
    view.recent.iter().map(|done| now_ms + words::ago_changes_in(now_ms.saturating_sub(done.ended_ms))).min()
}

impl AgentsPage {
    pub fn estimate() -> f32 {
        layout::height(0, 0)
    }

    pub fn height(&self, view: &AgentsView) -> f32 {
        if self.live.is_empty() && self.done.is_empty() {
            let extra = self.unfold.total(view.recent.iter().map(|done| done.id));
            return layout::height(view.running.len(), view.recent.len()) + extra;
        }
        let live: f32 = self.live.iter().map(|(_, _, shown)| shown * ROW_H).sum();
        let section = self.done.most_shown() * SECTION_H;
        let done: f32 = self.done.iter().map(|(id, _, shown)| shown * (ROW_H + self.unfold.extra(*id))).sum();
        live + section + done
    }

    pub fn is_animating(&self) -> bool {
        self.running || self.unfold.is_animating() || self.live.is_animating() || self.done.is_animating()
    }

    pub fn pin_open(&mut self, id: u64) {
        self.unfold.pin(id);
    }

    pub fn ui(&mut self, ctx: &mut PageCtx<'_>, view: &AgentsView, assistant: Option<Provider>) -> Vec<Action> {
        self.clock.advance(ctx.dt);
        self.running = !view.running.is_empty();
        let mut top = ctx.rect.top();
        let empty = view.running.is_empty() && view.recent.is_empty();
        let animate = ctx.alpha > LIVE_FROM && !empty && !(self.live.is_empty() && self.done.is_empty());
        let runs = view.running.iter().map(|row| ((row.run.provider, row.run.session.clone()), row.clone()));
        self.live.follow(runs, animate);
        self.done.follow(view.recent.iter().map(|done| (done.id, done.clone())), animate);
        self.live.step(ctx.dt);
        self.done.step(ctx.dt);
        if empty {
            let rows = card(ctx, top, 1);
            empty::paint(ctx, rows[0], assistant);
            return Vec::new();
        }
        let now = now_ms();
        let live = ctx.alpha > LIVE_FROM;
        let mut actions = Vec::new();
        let runs: Vec<(RunKey, RunRow, f32)> =
            self.live.iter().map(|(key, row, shown)| (key.clone(), row.clone(), shown)).collect();
        self.spins.retain(|key, _| runs.iter().any(|(run, _, _)| run == key));
        if !runs.is_empty() {
            let heights: Vec<f32> = runs.iter().map(|(_, _, shown)| ROW_H * shown).collect();
            let rects = card_of(ctx, top, &heights);
            for ((key, row, shown), rect) in runs.iter().zip(rects) {
                let spin = self.spins.entry(key.clone()).or_default();
                spin.follow(agent_mark::mood(row.run.waiting, row.run.doing), ctx.dt);
                let full = Rect::from_min_size(rect.min, vec2(rect.width(), ROW_H));
                let mark = ctx.canvas.mark();
                actions.extend(running::paint(ctx, full, row, (*spin, now), live && *shown > 0.99));
                rows::reveal(ctx.canvas, mark, rect, *shown);
            }
            top += heights.iter().sum::<f32>();
        }
        if !self.done.is_empty() {
            let shown = self.done.most_shown();
            let mark = ctx.canvas.mark();
            section(ctx, top, RECENT);
            for shape in ctx.canvas.since(mark) {
                fade(shape, rows::alpha(shown));
            }
            self.recent(ctx, top + SECTION_H * shown, now, live);
        }
        actions
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::assistant::Run;
    use crate::pages::harness::Harness;

    fn run(session: &str) -> RunRow {
        let run = Run {
            provider: Provider::Claude,
            session: session.into(),
            project: session.into(),
            since_ms: now_ms(),
            waiting: false,
            doing: None,
            window: None,
        };
        RunRow { run, prompt: None }
    }

    fn finished(id: u64) -> Finished {
        Finished {
            id,
            provider: Provider::Claude,
            project: "b".into(),
            message: "Done".into(),
            took: Duration::from_secs(90),
            ended_ms: now_ms(),
        }
    }

    fn show(harness: &mut Harness, page: &mut AgentsPage, view: &AgentsView, frames: usize) {
        for _ in 0..frames {
            harness.frame(Vec::new(), |ctx| page.ui(ctx, view, Some(Provider::Claude)));
        }
    }

    #[test]
    fn finished_run_rises_into_recent() {
        let (mut harness, mut page) = (Harness::new(), AgentsPage::default());
        let two = AgentsView { running: vec![run("a"), run("b")], recent: Vec::new() };
        show(&mut harness, &mut page, &two, 3);
        assert_eq!(page.height(&two), 2.0 * ROW_H, "rows already there when the page opens are simply there");
        let after = AgentsView { running: vec![run("a")], recent: vec![finished(1)] };
        show(&mut harness, &mut page, &after, 2);
        let between = page.height(&after);
        assert!(between > 2.0 * ROW_H && between < layout::height(1, 1) - 1.0, "{between}");
        assert!(page.is_animating());
        show(&mut harness, &mut page, &after, 60);
        assert_eq!(page.height(&after), layout::height(1, 1));
    }

    #[test]
    fn last_run_shows_empty_at_once() {
        let (mut harness, mut page) = (Harness::new(), AgentsPage::default());
        let one = AgentsView { running: vec![run("a")], recent: Vec::new() };
        show(&mut harness, &mut page, &one, 3);
        let none = AgentsView::default();
        show(&mut harness, &mut page, &none, 1);
        assert_eq!(page.height(&none), layout::height(0, 0));
        assert!(!page.is_animating());
    }
}
