//! The agents' live activity in the closed notch, and the notice when a run finishes.

use std::time::{Duration, Instant};

use notchbase_sdk::now_ms;

use crate::app::App;
use crate::app::config::assistant_of;
use crate::assistant::{Finished, Run};
use crate::bridge::{AgentLive, AgentsView, Mood, Page, ToolSlot};
use crate::notch::Notch;
use crate::notch::live::agents;
use crate::pages::agents::next_label_change;
use crate::platform;

const SHOW_AFTER: Duration = Duration::from_secs(10);
const AGENTS: ToolSlot = ToolSlot(u16::MAX - 1);
const LATE_MS: u64 = 50;

#[derive(Default)]
pub(in crate::app) struct Agents {
    shown: Option<AgentLive>,
    finished_seen: u64,
    labels_due: Option<u64>,
}

impl Agents {
    pub(in crate::app) fn live(&self) -> Option<AgentLive> {
        self.shown.clone()
    }
}

impl App {
    pub(in crate::app) fn focus_window(&mut self, pid: u32) {
        if platform::focus_window(pid) {
            self.notches.windows_mut().for_each(|(_, nw)| nw.forget_hover_app());
            self.update_notch(Notch::collapse);
        }
    }

    pub(in crate::app) fn sync_agents(&mut self) {
        self.sync_agents_view();
        let on = self.shows_progress();
        let now = now_ms();
        let settled: Vec<Run> =
            self.inbox.runs().into_iter().filter(|run| on && worked(run, now) >= SHOW_AFTER).collect();
        for finished in fresh(self.inbox.finished(), &mut self.agents.finished_seen) {
            if on && finished.took >= SHOW_AFTER {
                let notice = agents::done(&finished);
                self.update_notch(|notch| notch.show_notice(AGENTS, notice.clone()));
            }
        }
        let shown = summary(&settled);
        if shown == self.agents.shown {
            return;
        }
        self.agents.shown = shown;
        self.show_activities();
    }

    pub(in crate::app) fn agents_wakeup(&self, now: Instant) -> Option<Instant> {
        if !self.shows_progress() {
            return None;
        }
        let at = now_ms();
        self.inbox
            .runs()
            .iter()
            .map(|run| worked(run, at))
            .filter(|worked| *worked < SHOW_AFTER)
            .map(|worked| now + (SHOW_AFTER - worked))
            .min()
    }

    pub(in crate::app) fn tick_recent(&mut self) -> bool {
        let shown = self.visible_page() == Some(Page::Agents);
        let (due, next) = label_tick(self.agents.labels_due, shown, &self.panel.agents, now_ms());
        self.agents.labels_due = next;
        due
    }

    pub(in crate::app) fn recent_wakeup(&self, now: Instant) -> Option<Instant> {
        let due = self.agents.labels_due?;
        Some(now + Duration::from_millis(due.saturating_sub(now_ms()) + LATE_MS))
    }

    pub(in crate::app) fn agents_busy(&self) -> bool {
        !self.inbox.runs().is_empty()
    }

    fn shows_progress(&self) -> bool {
        let config = self.config.config();
        assistant_of(config).is_some() && config.assistant.progress_in_notch
    }
}

fn summary(runs: &[Run]) -> Option<AgentLive> {
    let oldest = runs.iter().min_by_key(|run| run.since_ms)?;
    let newest = runs.iter().max_by_key(|run| run.since_ms)?;
    let mood = if runs.iter().any(|run| run.waiting) {
        Mood::Waiting
    } else {
        newest.doing.or_else(|| runs.iter().find_map(|run| run.doing)).map_or(Mood::Thinking, Mood::Doing)
    };
    Some(AgentLive { provider: oldest.provider, mood, since_ms: oldest.since_ms, count: runs.len() })
}

fn label_tick(due: Option<u64>, shown: bool, view: &AgentsView, now_ms: u64) -> (bool, Option<u64>) {
    let passed = shown && due.is_some_and(|due| now_ms >= due);
    let next = shown.then(|| next_label_change(view, now_ms)).flatten();
    (passed, next)
}

fn worked(run: &Run, now_ms: u64) -> Duration {
    Duration::from_millis(now_ms.saturating_sub(run.since_ms))
}

fn fresh(finished: Vec<Finished>, seen: &mut u64) -> Vec<Finished> {
    let after = *seen;
    *seen = finished.iter().map(|done| done.id).max().unwrap_or(after).max(after);
    finished.into_iter().filter(|done| done.id > after).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assistant::Provider;

    fn finished(id: u64) -> Finished {
        Finished {
            id,
            provider: Provider::Claude,
            project: "p".into(),
            message: String::new(),
            took: Duration::from_secs(30),
            ended_ms: 0,
        }
    }

    #[test]
    fn finished_run_announced_once() {
        let mut seen = 0;
        assert_eq!(fresh(vec![finished(1), finished(2)], &mut seen).len(), 2);
        assert!(fresh(vec![finished(1), finished(2)], &mut seen).is_empty());
        let later = fresh(vec![finished(2), finished(3)], &mut seen);
        assert_eq!((later.len(), later[0].id, seen), (1, 3, 3));
    }

    fn run(since_ms: u64, waiting: bool, doing: Option<&'static str>) -> Run {
        Run {
            provider: Provider::Codex,
            session: "s".into(),
            project: "p".into(),
            since_ms,
            waiting,
            doing,
            window: None,
        }
    }

    #[test]
    fn open_page_wakes_for_ago_label() {
        let minute = 60_000;
        let view = AgentsView { running: Vec::new(), recent: vec![Finished { ended_ms: minute, ..finished(1) }] };
        let now = 5 * minute + 20_000;
        let (passed, due) = label_tick(None, true, &view, now);
        assert_eq!((passed, due), (false, Some(6 * minute)));
        assert_eq!(label_tick(due, true, &view, now + 1_000), (false, due));
        assert_eq!(label_tick(due, true, &view, 6 * minute), (true, Some(7 * minute)));
        assert_eq!(label_tick(due, false, &view, 6 * minute), (false, None), "a hidden page never wakes");
        let empty = AgentsView { running: Vec::new(), recent: Vec::new() };
        assert_eq!(label_tick(None, true, &empty, now), (false, None), "nothing recent, nothing to tick");
    }

    #[test]
    fn run_counts_from_turn_start() {
        let run = run(1_000, false, None);
        assert_eq!(worked(&run, 13_000), Duration::from_secs(12));
        assert_eq!(worked(&run, 500), Duration::ZERO);
    }

    #[test]
    fn runs_share_one_mood() {
        assert_eq!(summary(&[]), None);
        let thinking = summary(&[run(5, false, None)]).unwrap();
        assert_eq!((thinking.mood, thinking.count), (Mood::Thinking, 1));
        let doing = summary(&[run(9, false, None), run(3, false, Some("Editing"))]).unwrap();
        assert_eq!((doing.mood, doing.since_ms, doing.count), (Mood::Doing("Editing"), 3, 2));
        let waiting = summary(&[run(9, true, None), run(3, false, Some("Editing"))]).unwrap();
        assert_eq!(waiting.mood, Mood::Waiting);
    }
}
