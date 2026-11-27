//! The Assistant view: which assistant the notch follows, and whether its questions and progress show in the notch.

use egui::{Id, Rect, pos2};

use super::rows::{CHOICE, INSET, Row, Toggle, paint_text, toggled};
use super::search::Said;
use crate::assistant::Provider;
use crate::bridge::{Action, PanelData};
use crate::pages::PageCtx;
use crate::ui::segmented::segmented;

const OFF: &str = "Off";
const CHOICE_TITLE: &str = "Assistant";
const CHOICE_DETAIL: &str = "Usage and questions";
const QUESTIONS: &str = "Questions in the notch";
const PROGRESS: &str = "Progress in the notch";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum AssistantRow {
    Choice,
    Questions,
    Progress,
}

pub(super) const ROWS: &[AssistantRow] = &[AssistantRow::Choice, AssistantRow::Questions, AssistantRow::Progress];

impl AssistantRow {
    pub(super) fn said(self, data: &PanelData) -> Said {
        let (title, detail, words) = match self {
            AssistantRow::Choice => (CHOICE_TITLE, CHOICE_DETAIL, "claude codex chatgpt ai agent usage"),
            AssistantRow::Questions => (QUESTIONS, questions_detail(data.assistant), "claude codex prompts permission"),
            AssistantRow::Progress => (PROGRESS, progress_detail(data.assistant), "claude codex working done"),
        };
        Said { title: title.to_owned(), detail: detail.to_owned(), words }
    }

    pub(super) fn paint(self, ctx: &mut PageCtx<'_>, rect: Rect, data: &PanelData, live: bool) -> Option<Action> {
        let has_assistant = data.assistant.is_some();
        match self {
            AssistantRow::Choice => assistant_choice(ctx, rect, data.assistant, live),
            AssistantRow::Questions => {
                let row = Row { rect, title: QUESTIONS, detail: questions_detail(data.assistant) };
                let on = data.questions_in_notch && has_assistant;
                let toggle = Toggle { id: Id::new("notchbase.settings.questions"), on, enabled: live && has_assistant };
                toggled(ctx, &row, &toggle).then_some(Action::SetQuestionsInNotch(!data.questions_in_notch))
            }
            AssistantRow::Progress => {
                let row = Row { rect, title: PROGRESS, detail: progress_detail(data.assistant) };
                let on = data.progress_in_notch && has_assistant;
                let toggle = Toggle { id: Id::new("notchbase.settings.progress"), on, enabled: live && has_assistant };
                toggled(ctx, &row, &toggle).then_some(Action::SetProgressInNotch(!data.progress_in_notch))
            }
        }
    }
}

/// The tile's line: the assistant, and what of it shows when not everything does.
pub(super) fn summary(assistant: Option<Provider>, questions: bool, progress: bool) -> String {
    let Some(provider) = assistant else { return OFF.to_owned() };
    let shows = match (questions, progress) {
        (true, true) => return provider.title().to_owned(),
        (true, false) => "questions only",
        (false, true) => "progress only",
        (false, false) => "usage only",
    };
    format!("{} \u{b7} {shows}", provider.title())
}

fn assistant_choice(ctx: &mut PageCtx<'_>, rect: Rect, current: Option<Provider>, live: bool) -> Option<Action> {
    let row = Row { rect, title: CHOICE_TITLE, detail: CHOICE_DETAIL };
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
    fn tile_says_what_shows() {
        assert_eq!(summary(Some(Provider::Claude), true, true), "Claude");
        assert_eq!(summary(Some(Provider::Codex), true, false), "ChatGPT \u{b7} questions only");
        assert_eq!(summary(Some(Provider::Claude), false, true), "Claude \u{b7} progress only");
        assert_eq!(summary(Some(Provider::Codex), false, false), "ChatGPT \u{b7} usage only");
        assert_eq!(summary(None, true, true), "Off");
    }
}
