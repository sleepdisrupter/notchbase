//! The Assistant view: which assistant the notch follows, and whether its questions and progress show in the notch.

use egui::{Id, Rect, pos2};

use super::rows::{BACK_H, CHOICE, INSET, ROW_H, Row, Toggle, back, card, paint_text, toggled};
use crate::assistant::Provider;
use crate::bridge::{Action, PanelData};
use crate::pages::PageCtx;
use crate::ui::metrics::GAP;
use crate::ui::segmented::segmented;

const ROWS: usize = 3;
const OFF: &str = "Off";
const QUESTIONS: &str = "Questions in the notch";
const PROGRESS: &str = "Progress in the notch";

pub(super) enum Outcome {
    Stay,
    Back,
}

pub(super) fn height() -> f32 {
    BACK_H + GAP + ROWS as f32 * ROW_H
}

pub(super) fn ui(ctx: &mut PageCtx<'_>, data: &PanelData, actions: &mut Vec<Action>) -> Outcome {
    let live = ctx.alpha > super::rows::INTERACTIVE_FROM;
    let mut top = ctx.rect.top();
    let back = back(ctx, top, "Settings", live);
    top += BACK_H + GAP;
    let rows = card(ctx, top, ROWS);
    let [choice, questions, progress] = rows[..] else { return Outcome::Stay };
    actions.extend(assistant_choice(ctx, choice, data.assistant, live));

    let has_assistant = data.assistant.is_some();
    let row = Row { rect: questions, title: QUESTIONS, detail: questions_detail(data.assistant) };
    let on = data.questions_in_notch && has_assistant;
    let toggle = Toggle { id: Id::new("notchbase.settings.questions"), on, enabled: live && has_assistant };
    if toggled(ctx, &row, &toggle) {
        actions.push(Action::SetQuestionsInNotch(!data.questions_in_notch));
    }

    let row = Row { rect: progress, title: PROGRESS, detail: progress_detail(data.assistant) };
    let on = data.progress_in_notch && has_assistant;
    let toggle = Toggle { id: Id::new("notchbase.settings.progress"), on, enabled: live && has_assistant };
    if toggled(ctx, &row, &toggle) {
        actions.push(Action::SetProgressInNotch(!data.progress_in_notch));
    }
    if back { Outcome::Back } else { Outcome::Stay }
}

pub(super) fn summary(assistant: Option<Provider>, questions: bool, progress: bool) -> String {
    let Some(provider) = assistant else { return OFF.to_owned() };
    let shows = match (questions, progress) {
        (true, true) => "questions and progress",
        (true, false) => "questions",
        (false, true) => "progress",
        (false, false) => "usage only",
    };
    format!("{} \u{b7} {shows}", provider.title())
}

fn assistant_choice(ctx: &mut PageCtx<'_>, rect: Rect, current: Option<Provider>, live: bool) -> Option<Action> {
    let row = Row { rect, title: "Assistant", detail: "Usage and questions" };
    let choice = Rect::from_center_size(pos2(rect.right() - INSET - CHOICE.x * 0.5, rect.center().y), CHOICE);
    paint_text(ctx, &row, choice.left());
    let choices = assistant_choices();
    let titles = choices.map(|choice| choice.map_or(OFF, Provider::title));
    let selected = choices.iter().position(|choice| *choice == current).unwrap_or(0);
    let id = Id::new("notchbase.settings.assistant");
    segmented(ctx.ui, ctx.canvas, choice, id, &titles, selected, live).map(|index| Action::SetAssistant(choices[index]))
}

fn assistant_choices() -> [Option<Provider>; 3] {
    [Some(Provider::Claude), Some(Provider::Codex), None]
}

fn questions_detail(assistant: Option<Provider>) -> &'static str {
    match assistant {
        Some(Provider::Claude) => "Answer Claude Code's prompts and questions here",
        Some(Provider::Codex) => "Answer Codex's permission prompts here",
        None => "Choose an assistant to use this",
    }
}

fn progress_detail(assistant: Option<Provider>) -> &'static str {
    match assistant {
        Some(Provider::Claude) => "Shows while Claude Code works, and when it's done",
        Some(Provider::Codex) => "Shows while Codex works, and when it's done",
        None => "Choose an assistant to use this",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nav_row_says_what_it_does() {
        assert_eq!(summary(Some(Provider::Claude), true, true), "Claude \u{b7} questions and progress");
        assert_eq!(summary(Some(Provider::Codex), true, false), "ChatGPT \u{b7} questions");
        assert_eq!(summary(Some(Provider::Claude), false, true), "Claude \u{b7} progress");
        assert_eq!(summary(Some(Provider::Codex), false, false), "ChatGPT \u{b7} usage only");
        assert_eq!(summary(None, true, true), "Off");
    }

    #[test]
    fn view_is_back_and_one_card() {
        assert_eq!(height(), BACK_H + GAP + 3.0 * ROW_H);
    }
}
