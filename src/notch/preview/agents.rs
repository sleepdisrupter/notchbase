//! Sample agent runs for the Agents page previews.

use std::time::Duration;

use notchbase_sdk::now_ms;

use crate::assistant::{Finished, Provider, Run};
use crate::bridge::{AgentsView, RunRow};
use crate::notch::Notch;

use super::Preview;

const MINUTE_MS: u64 = 60_000;
const EDITING_FOR_MS: u64 = 3 * MINUTE_MS + 12_000;
const THINKING_FOR_MS: u64 = 42_000;
const WAITING_FOR_MS: u64 = MINUTE_MS + 5_000;
const ROCKET_TOOK: Duration = Duration::from_secs(2 * 60 + 31);
const ROCKET_ENDED_MS: u64 = 4 * MINUTE_MS;
const DOCS_TOOK: Duration = Duration::from_secs(48);
const DOCS_ENDED_MS: u64 = 26 * MINUTE_MS;
const ROCKET_ID: u64 = 2;
const ROCKET_MESSAGE: &str = "Shipped the fix, tests green.\nThe crash came from a window handle that went stale after the display changed, so the notch now asks for a fresh one on every monitor event, and a new test covers unplugging the second screen mid-animation.\nI also tidied the release notes and bumped the version for tonight's build.";
const PERMISSION: &str = "Run this command?";
const PROMPT_ID: u64 = 1;
const NO_SUCH_PROCESS: u32 = u32::MAX;
const MANY_RECENT: [(&str, &str, u64); 8] = [
    ("rocket", "Shipped the fix, tests green.", 4),
    ("docs", "Rewrote the setup guide.", 12),
    ("api", "Added paging to the list endpoint.", 19),
    ("website", "Swapped the hero image.", 33),
    ("billing", "Fixed the rounding in invoices.", 47),
    ("mobile", "Bumped the SDK and rebuilt.", 58),
    ("infra", "Raised the worker memory limit.", 75),
    ("design", "Exported the new icon set.", 96),
];

impl Notch {
    pub fn previewed_agents(&self) -> Option<AgentsView> {
        let now = now_ms();
        match self.preview? {
            Preview::Agents | Preview::AgentsExpanded => Some(AgentsView {
                running: vec![
                    row(run("notchbase", now - EDITING_FOR_MS, false, Some("Editing")), None),
                    row(run("website", now - THINKING_FOR_MS, false, None), None),
                ],
                recent: vec![
                    finished(ROCKET_ID, "rocket", ROCKET_MESSAGE, ROCKET_TOOK, now - ROCKET_ENDED_MS),
                    finished(1, "docs", "", DOCS_TOOK, now - DOCS_ENDED_MS),
                ],
            }),
            Preview::AgentsWaiting => Some(AgentsView {
                running: vec![row(
                    run("notchbase", now - WAITING_FOR_MS, true, Some("Running")),
                    Some((PROMPT_ID, PERMISSION.to_owned())),
                )],
                recent: Vec::new(),
            }),
            Preview::AgentsEmpty => Some(AgentsView::default()),
            Preview::AgentsMany => Some(many(now)),
            _ => None,
        }
    }

    pub(super) fn open_first_agent_row(&mut self) {
        self.pages.open_agent_row(ROCKET_ID);
    }
}

fn many(now: u64) -> AgentsView {
    AgentsView {
        running: vec![
            row(run("notchbase", now - EDITING_FOR_MS, false, Some("Editing")), None),
            row(run("website", now - THINKING_FOR_MS, false, None), None),
            row(run("api", now - WAITING_FOR_MS, false, Some("Running")), None),
        ],
        recent: MANY_RECENT
            .iter()
            .zip(1..)
            .map(|((project, message, ago), id)| finished(id, project, message, DOCS_TOOK, now - ago * MINUTE_MS))
            .collect(),
    }
}

fn run(project: &str, since_ms: u64, waiting: bool, doing: Option<&'static str>) -> Run {
    Run {
        provider: Provider::Claude,
        session: project.to_owned(),
        project: project.to_owned(),
        since_ms,
        waiting,
        doing,
        window: Some(NO_SUCH_PROCESS),
    }
}

fn row(run: Run, prompt: Option<(u64, String)>) -> RunRow {
    RunRow { run, prompt }
}

fn finished(id: u64, project: &str, message: &str, took: Duration, ended_ms: u64) -> Finished {
    Finished {
        id,
        provider: Provider::Claude,
        project: project.to_owned(),
        message: message.to_owned(),
        took,
        ended_ms,
    }
}

#[cfg(test)]
mod tests {
    use egui::{Event, Pos2, Rect, pos2, vec2};

    use super::*;
    use crate::bridge::{Action, Page};
    use crate::notch::tests::fixtures::Rig;
    use crate::platform::Style;

    const FIRST_ROW_H: f32 = 50.0;

    #[test]
    fn agent_previews_pin_sample_runs() {
        let busy = Notch::with_preview(Style::Island, None, Some(Preview::Agents));
        assert_eq!(busy.pager.page, Page::Agents);
        let view = busy.previewed_agents().unwrap();
        assert_eq!((view.running.len(), view.recent.len()), (2, 2));
        assert_eq!(view.running[0].run.doing, Some("Editing"));
        assert!(view.running.iter().all(|row| row.run.window == Some(NO_SUCH_PROCESS)), "every run can be opened");
        let waiting = Notch::with_preview(Style::Island, None, Some(Preview::AgentsWaiting)).previewed_agents();
        assert_eq!(waiting.unwrap().running[0].prompt, Some((PROMPT_ID, PERMISSION.to_owned())));
        let empty = Notch::with_preview(Style::Island, None, Some(Preview::AgentsEmpty)).previewed_agents();
        assert_eq!(empty, Some(AgentsView::default()));
        let many = Notch::with_preview(Style::Island, None, Some(Preview::AgentsMany)).previewed_agents().unwrap();
        assert_eq!((many.running.len(), many.recent.len()), (3, 8));
        assert_eq!(Notch::with_preview(Style::Island, None, Some(Preview::Stats)).previewed_agents(), None);
    }

    fn click(preview: Preview, at: impl Fn(Rect) -> Pos2) -> Vec<Action> {
        let rig = Rig::new();
        let mut n = Notch::with_preview(Style::Island, None, Some(preview));
        for _ in 0..3 {
            rig.frame(&mut n, &[], Vec::new());
        }
        let page = n.content_rect(Page::Agents);
        let first = Rect::from_min_size(page.min, vec2(page.width(), FIRST_ROW_H));
        let at = at(first);
        let button = |pressed| egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        rig.frame(&mut n, &[], vec![egui::Event::PointerMoved(at)]);
        rig.frame(&mut n, &[], vec![button(true)]);
        rig.frame(&mut n, &[], vec![button(false)])
    }

    #[test]
    fn open_brings_running_agent_up() {
        let actions = click(Preview::Agents, |row| pos2(row.right() - 30.0, row.center().y));
        assert_eq!(actions, [Action::FocusWindow(NO_SUCH_PROCESS)]);
        let elsewhere = click(Preview::Agents, |row| pos2(row.left() + 80.0, row.center().y));
        assert!(elsewhere.is_empty(), "a row that waits on nothing has nothing to answer");
    }

    #[test]
    fn waiting_row_answers_on_click() {
        let answer = click(Preview::AgentsWaiting, |row| pos2(row.left() + 80.0, row.center().y));
        assert_eq!(answer, [Action::ShowPrompt(PROMPT_ID)]);
        let open = click(Preview::AgentsWaiting, |row| pos2(row.right() - 60.0, row.center().y));
        assert_eq!(open, [Action::FocusWindow(NO_SUCH_PROCESS)]);
    }

    const SECTION_H: f32 = 26.0;
    const AT_REST: f32 = 4.0 * FIRST_ROW_H + SECTION_H;

    fn frames(rig: &Rig, n: &mut Notch, count: usize, events: Vec<Event>) {
        rig.frame(n, &[], events);
        for _ in 1..count {
            rig.frame(n, &[], Vec::new());
        }
    }

    fn height(n: &Notch) -> f32 {
        n.content_rect(Page::Agents).height()
    }

    fn row_center(n: &Notch, index: usize, above: f32) -> Pos2 {
        let page = n.content_rect(Page::Agents);
        pos2(
            page.left() + 80.0,
            page.top() + 2.0 * FIRST_ROW_H + SECTION_H + above + (index as f32 + 0.5) * FIRST_ROW_H,
        )
    }

    #[test]
    fn hover_unfolds_long_message() {
        let rig = Rig::new();
        let mut n = Notch::with_preview(Style::Island, None, Some(Preview::Agents));
        frames(&rig, &mut n, 3, Vec::new());
        assert_eq!(height(&n), AT_REST);
        let rocket = row_center(&n, 0, 0.0);
        frames(&rig, &mut n, 2, vec![Event::PointerMoved(rocket)]);
        let rising = height(&n);
        assert!(rising > AT_REST, "the row starts to grow");
        frames(&rig, &mut n, 90, Vec::new());
        let open = height(&n) - AT_REST;
        assert!(open > rising - AT_REST, "and keeps growing with the spring");
        assert!((55.0..85.0).contains(&open), "five more caption lines: {open}");
        let docs = row_center(&n, 1, open);
        frames(&rig, &mut n, 90, vec![Event::PointerMoved(docs)]);
        assert_eq!(height(&n), AT_REST, "a row without a long message does not grow, and the other folds");
        let back = row_center(&n, 0, 0.0);
        frames(&rig, &mut n, 2, vec![Event::PointerMoved(back)]);
        frames(&rig, &mut n, 90, vec![Event::PointerGone]);
        assert_eq!(height(&n), AT_REST);
    }

    #[test]
    fn expanded_holds_first_row_open() {
        let rig = Rig::new();
        let mut n = Notch::with_preview(Style::Island, None, Some(Preview::AgentsExpanded));
        frames(&rig, &mut n, 3, Vec::new());
        let open = height(&n) - AT_REST;
        assert!((55.0..85.0).contains(&open), "six caption lines under the title: {open}");
        frames(&rig, &mut n, 30, vec![Event::PointerGone]);
        assert_eq!(height(&n) - AT_REST, open, "pinned open without a pointer");
        assert_eq!(n.previewed_agents().map(|view| view.recent[0].id), Some(ROCKET_ID));
    }
}
