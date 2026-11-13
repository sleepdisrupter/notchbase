//! The welcome walkthrough: a hello, a step for each permission, and a last card.

mod copy;
mod layout;
#[cfg(test)]
mod tests;

use egui::{Align2, Color32, Id, Rect, Shape, pos2, vec2};

use self::copy::{
    ALL_SET, COME_BACK, CONTINUE, DONE, GET_STARTED, LATER_IN_SETTINGS, NOT_NOW, NOTHING_MISSING, OPEN_ANY_TIME, SKIP,
    WELCOME, WELCOME_READY, WELCOME_WHY, hint, primary, words,
};
use self::layout::{
    BUTTON_H, BUTTON_MIN, BUTTON_SPACING, CURRENT_DOT, DOT, DOT_SPACING, MARK, WELL, WHY_ROWS, balanced, buttons_top,
    centered, column, dots_top, hint_font, hint_top, line, title_font, title_top, why_font, why_top,
};
use super::PageCtx;
use crate::bridge::{Action, StepView, WelcomeStage, WelcomeView};
use crate::system::permissions::Standing;
use crate::ui::buttons::{self, Button, Tone};
use crate::ui::canvas::{self, Canvas};
use crate::ui::color::{self, label, secondary_label, tertiary_label};
use crate::ui::font;
use crate::ui::glass::{self, Glass};
use crate::ui::glyph::{self, Glyph};
use crate::ui::spring::{lerp, smoothstep};

const LIVE_FROM: f32 = 0.5;
const GRANT_SECS: f32 = 0.3;
const CHECK_FROM: f32 = 0.6;
const LABEL_GONE_BY: f32 = 0.45;
const LABEL_SHOWN_FROM: f32 = 0.4;
const REVEAL_SECS: f32 = 0.3;
const CLICKABLE_FROM: f32 = 0.6;
const RISE: f32 = 8.0;
const PRIMARY_ID: &str = "notchbase.welcome.primary";
const SECONDARY_ID: &str = "notchbase.welcome.secondary";

pub struct WelcomePage {
    shown: Option<WelcomeStage>,
    reveal: f32,
    done: Option<bool>,
    pinned: Option<WelcomeView>,
}

impl Default for WelcomePage {
    fn default() -> Self {
        Self { shown: None, reveal: 1.0, done: None, pinned: None }
    }
}

struct Card<'a> {
    glyph: Glyph,
    tint: Color32,
    title: &'a str,
    why: &'a str,
    hint: Option<&'a str>,
    checked: f32,
}

#[derive(Clone, Copy, PartialEq)]
struct Choice {
    primary: &'static str,
    secondary: Option<&'static str>,
}

impl WelcomePage {
    pub fn pin(&mut self, view: WelcomeView) {
        self.pinned = Some(view);
    }

    pub fn height(canvas: &Canvas) -> f32 {
        layout::height(canvas)
    }

    pub fn is_animating(&self) -> bool {
        self.reveal < 1.0
    }

    pub fn take_done(&mut self) -> Option<bool> {
        self.done.take()
    }

    pub fn rest(&mut self) {
        *self = Self { pinned: self.pinned.take(), ..Self::default() };
    }

    pub fn ui(&mut self, ctx: &mut PageCtx<'_>, data: &WelcomeView) -> Vec<Action> {
        let pinned = self.pinned.take();
        let actions = self.shown_ui(ctx, pinned.as_ref().unwrap_or(data));
        self.pinned = pinned;
        actions
    }

    fn shown_ui(&mut self, ctx: &mut PageCtx<'_>, view: &WelcomeView) -> Vec<Action> {
        self.follow(view.stage, ctx.dt);
        let live = ctx.alpha >= LIVE_FROM && self.reveal >= CLICKABLE_FROM;
        let mark = ctx.canvas.mark();
        let actions = match view.stage {
            WelcomeStage::Hello => hello(ctx, view, live),
            WelcomeStage::Step(_) => view.step().map(|step| step_ui(ctx, view, step, live)).unwrap_or_default(),
            WelcomeStage::AllSet => all_set(ctx, view, live),
        };
        let eased = 1.0 - (1.0 - self.reveal).powi(3);
        for shape in ctx.canvas.since(mark) {
            canvas::fade(shape, eased);
            shape.translate(vec2(0.0, RISE * (1.0 - eased)));
        }
        if actions.contains(&Action::WelcomeDone) {
            self.done = Some(!view.first_run);
        }
        actions
    }

    fn follow(&mut self, stage: WelcomeStage, dt: f32) {
        if self.shown.replace(stage).is_some_and(|shown| shown != stage) {
            self.reveal = 0.0;
        }
        self.reveal = (self.reveal + dt / REVEAL_SECS).min(1.0);
    }
}

fn hello(ctx: &mut PageCtx<'_>, view: &WelcomeView, live: bool) -> Vec<Action> {
    let ready = view.steps.is_empty();
    let card = Card {
        glyph: Glyph::Spark,
        tint: label(),
        title: WELCOME,
        why: if ready { WELCOME_READY } else { WELCOME_WHY },
        hint: None,
        checked: 0.0,
    };
    paint_card(ctx, &card);
    let choice = if ready {
        Choice { primary: CONTINUE, secondary: None }
    } else {
        Choice { primary: GET_STARTED, secondary: Some(SKIP) }
    };
    match choose(ctx, &choice, live) {
        (true, _) => vec![Action::WelcomeStart],
        (_, true) => vec![Action::WelcomeSkip],
        _ => Vec::new(),
    }
}

fn step_ui(ctx: &mut PageCtx<'_>, view: &WelcomeView, step: &StepView, live: bool) -> Vec<Action> {
    let words = words(step.access);
    let granted = step.standing == Standing::Granted;
    let id = Id::new(("notchbase.welcome.asking", step.access));
    let checked = ctx.ui.ctx().animate_bool_with_time(id.with("granted"), granted, GRANT_SECS);
    let choice = Choice { primary: primary(step), secondary: (!granted).then_some(NOT_NOW) };
    let asked: (Choice, Option<&'static str>) = match granted {
        false => {
            ctx.ui.data_mut(|data| data.insert_temp(id, (choice, hint(step))));
            (choice, hint(step))
        }
        true => ctx.ui.data(|data| data.get_temp(id)).unwrap_or((choice, None)),
    };
    let card = Card { glyph: words.glyph, tint: label(), title: words.title, why: words.why, hint: asked.1, checked };
    paint_card(ctx, &card);
    let top = ctx.rect.top() + buttons_top(ctx.canvas);
    let (primary, secondary) = buttons_between(ctx, top, (&asked.0, &choice), checked, live);
    let at = match view.stage {
        WelcomeStage::Step(index) => index,
        _ => 0,
    };
    dots(ctx, view.steps.len(), at);
    match (primary, secondary) {
        (true, _) if granted => vec![Action::WelcomeNext],
        (true, _) => vec![Action::WelcomeAsk(step.access)],
        (_, true) => vec![Action::WelcomeNext],
        _ => Vec::new(),
    }
}

fn all_set(ctx: &mut PageCtx<'_>, view: &WelcomeView, live: bool) -> Vec<Action> {
    let missing = view.steps.iter().any(|step| step.standing != Standing::Granted);
    let why = match (view.first_run, view.steps.is_empty(), missing) {
        (true, true, _) => OPEN_ANY_TIME,
        (true, false, _) => LATER_IN_SETTINGS,
        (false, _, true) => COME_BACK,
        (false, _, false) => NOTHING_MISSING,
    };
    let card = Card { glyph: Glyph::Checkmark, tint: color::GREEN, title: ALL_SET, why, hint: None, checked: 0.0 };
    paint_card(ctx, &card);
    let (done, _) = choose(ctx, &Choice { primary: DONE, secondary: None }, live);
    if done { vec![Action::WelcomeDone] } else { Vec::new() }
}

fn paint_card(ctx: &mut PageCtx<'_>, card: &Card<'_>) {
    let span = column(ctx.rect.width());
    let (top, middle) = (ctx.rect.top(), ctx.rect.center().x);
    let well = Rect::from_center_size(pos2(middle, top + WELL * 0.5), vec2(WELL, WELL));
    glass::paint(ctx.canvas, well, WELL * 0.5, Glass::CONTROL);
    let checked = card.checked.clamp(0.0, 1.0);
    if checked < 1.0 {
        glyph::paint(
            ctx.canvas,
            card.glyph,
            well.center(),
            MARK * lerp(1.0, CHECK_FROM, checked),
            card.tint.gamma_multiply(1.0 - checked),
        );
    }
    if checked > 0.0 {
        let check = color::GREEN.gamma_multiply(checked);
        glyph::paint(ctx.canvas, Glyph::Checkmark, well.center(), MARK * lerp(CHECK_FROM, 1.0, checked), check);
    }
    let title = centered(ctx.canvas, card.title, title_font(), label(), span.span(), 1);
    ctx.canvas.add(Shape::galley(pos2(middle, top + title_top()), title, label()));
    let why = balanced(ctx.canvas, card.why, why_font(), secondary_label(), span.span());
    let room = line(ctx.canvas, why_font()) * WHY_ROWS as f32;
    let why_at = top + why_top(ctx.canvas) + ((room - why.size().y) * 0.5).max(0.0);
    ctx.canvas.add(Shape::galley(pos2(middle, why_at), why, secondary_label()));
    if let Some(hint) = card.hint.filter(|_| card.checked < 1.0) {
        let tint = tertiary_label().gamma_multiply(1.0 - card.checked.clamp(0.0, 1.0));
        let hint = centered(ctx.canvas, hint, hint_font(), tint, span.span(), 1);
        ctx.canvas.add(Shape::galley(pos2(middle, top + hint_top(ctx.canvas)), hint, tint));
    }
}

fn choose(ctx: &mut PageCtx<'_>, choice: &Choice, live: bool) -> (bool, bool) {
    let top = ctx.rect.top() + buttons_top(ctx.canvas);
    buttons_at(ctx, top, choice, live)
}

fn buttons_between(
    ctx: &mut PageCtx<'_>,
    top: f32,
    (from, to): (&Choice, &Choice),
    t: f32,
    live: bool,
) -> (bool, bool) {
    if t >= 1.0 || from == to {
        return buttons_at(ctx, top, to, live);
    }
    let widths = |choice: &Choice| {
        let first = buttons::width(ctx.canvas, choice.primary, BUTTON_MIN);
        let second = choice.secondary.map(|title| buttons::width(ctx.canvas, title, BUTTON_MIN));
        (first, second, first + second.map_or(0.0, |width| width + BUTTON_SPACING))
    };
    let ((was_first, was_second, was_total), (first, _, total)) = (widths(from), widths(to));
    let left = ctx.rect.center().x - lerp(was_total, total, t) * 0.5;
    let rect = Rect::from_min_size(pos2(left, top), vec2(lerp(was_first, first, t), BUTTON_H));
    let button = Button { rect, id: Id::new(PRIMARY_ID), title: "", tone: Tone::Primary, enabled: true };
    let primary = buttons::button(ctx.ui, ctx.canvas, &button, live && t > 0.5);
    let ink = color::ink_on(color::ACCENT);
    let labels =
        [(from.primary, 1.0 - smoothstep(0.0, LABEL_GONE_BY, t)), (to.primary, smoothstep(LABEL_SHOWN_FROM, 1.0, t))];
    for (title, shown) in labels {
        if shown > 0.01 {
            ctx.canvas.text(rect.center(), Align2::CENTER_CENTER, title, font::headline(), ink.gamma_multiply(shown));
        }
    }
    if let (Some(title), Some(width)) = (from.secondary, was_second) {
        let rect = Rect::from_min_size(pos2(rect.right() + BUTTON_SPACING, top), vec2(width, BUTTON_H));
        let button = Button { rect, id: Id::new(SECONDARY_ID), title, tone: Tone::Plain, enabled: true };
        let mark = ctx.canvas.mark();
        buttons::button(ctx.ui, ctx.canvas, &button, false);
        for shape in ctx.canvas.since(mark) {
            canvas::fade(shape, 1.0 - t);
        }
    }
    (primary, false)
}

fn buttons_at(ctx: &mut PageCtx<'_>, top: f32, choice: &Choice, live: bool) -> (bool, bool) {
    let first = buttons::width(ctx.canvas, choice.primary, BUTTON_MIN);
    let second = choice.secondary.map(|title| buttons::width(ctx.canvas, title, BUTTON_MIN));
    let total = first + second.map_or(0.0, |width| width + BUTTON_SPACING);
    let left = ctx.rect.center().x - total * 0.5;
    let rect = Rect::from_min_size(pos2(left, top), vec2(first, BUTTON_H));
    let button = Button { rect, id: Id::new(PRIMARY_ID), title: choice.primary, tone: Tone::Primary, enabled: true };
    let primary = buttons::button(ctx.ui, ctx.canvas, &button, live);
    let secondary = match (choice.secondary, second) {
        (Some(title), Some(width)) => {
            let rect = Rect::from_min_size(pos2(rect.right() + BUTTON_SPACING, top), vec2(width, BUTTON_H));
            let button = Button { rect, id: Id::new(SECONDARY_ID), title, tone: Tone::Plain, enabled: true };
            buttons::button(ctx.ui, ctx.canvas, &button, live)
        }
        _ => false,
    };
    (primary, secondary)
}

fn dots(ctx: &mut PageCtx<'_>, count: usize, at: usize) {
    if count < 2 {
        return;
    }
    let widths: Vec<f32> = (0..count).map(|index| if index == at { CURRENT_DOT } else { DOT }).collect();
    let total = widths.iter().sum::<f32>() + DOT_SPACING * (count - 1) as f32;
    let mut left = ctx.rect.center().x - total * 0.5;
    let top = ctx.rect.top() + dots_top(ctx.canvas);
    for (index, width) in widths.into_iter().enumerate() {
        let dot = Rect::from_min_size(pos2(left, top), vec2(width, DOT));
        let tint = if index == at { label() } else { tertiary_label() };
        ctx.canvas.rect(dot, DOT * 0.5, tint);
        left += width + DOT_SPACING;
    }
}
