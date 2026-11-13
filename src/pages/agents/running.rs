//! Paints one run that is going now. A click opens it, or answers it when it waits for you.

use egui::{Id, Rect, Sense, pos2};

use super::layout::{TEXT_GAP, detail, lead_center, title};
use super::words;
use crate::bridge::{Action, RunRow};
use crate::pages::PageCtx;
use crate::pages::settings::{INSET, pill};
use crate::ui::agent_mark::{self, Spin, WAITING};
use crate::ui::color::secondary_label;
use crate::ui::controls;
use crate::ui::glyph::{self, Glyph};

const MARK: f32 = 20.0;
const CHEVRON: f32 = 10.0;
const HIGHLIGHT_INSET: f32 = 3.0;
const OPEN: &str = "Open";

pub(super) fn paint(
    ctx: &mut PageCtx<'_>,
    rect: Rect,
    row: &RunRow,
    (spin, now_ms): (Spin, u64),
    live: bool,
) -> Option<Action> {
    let run = &row.run;
    let key = Id::new(("notchbase.agents.run", run.provider, run.session.as_str()));
    let prompt = row.prompt.as_ref().filter(|_| run.waiting);
    let mut right = rect.right() - INSET;
    let answer = prompt.and_then(|(id, _)| answer_on_click(ctx, rect, key, &mut right, live).then_some(*id));
    let open = run.window.and_then(|pid| open_on_click(ctx, rect, key, &mut right, live).then_some(pid));
    let mood = agent_mark::mood(run.waiting, run.doing);
    agent_mark::mark(ctx.canvas, run.provider, mood, lead_center(rect), MARK, &spin);
    let doing = words::doing(run, prompt.map(|(_, asked)| asked.as_str()), now_ms);
    let tone = if run.waiting { WAITING } else { secondary_label() };
    title(ctx, rect, &run.project, true, right);
    detail(ctx, rect, &doing, tone, right);
    open.map(Action::FocusWindow).or(answer.map(Action::ShowPrompt))
}

fn answer_on_click(ctx: &mut PageCtx<'_>, rect: Rect, key: Id, right: &mut f32, live: bool) -> bool {
    let response = ctx.ui.interact(rect, key, if live { Sense::click() } else { Sense::hover() });
    let hover = controls::animate(ctx.ui, key.with("hover"), live && response.hovered());
    controls::row_highlight(ctx.canvas, rect.shrink(HIGHLIGHT_INSET), hover);
    let at = pos2(*right - CHEVRON * 0.5, rect.center().y);
    glyph::paint(ctx.canvas, Glyph::ChevronRight, at, CHEVRON, secondary_label());
    *right = at.x - CHEVRON * 0.5 - TEXT_GAP;
    live && response.clicked()
}

fn open_on_click(ctx: &mut PageCtx<'_>, rect: Rect, key: Id, right: &mut f32, live: bool) -> bool {
    let (shape, clicked) = pill(ctx, *right, rect.center().y, key.with("open"), OPEN, live);
    *right = shape.left() - TEXT_GAP;
    clicked
}
