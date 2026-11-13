//! Builds the Agents page from the assistants' runs and prompts.

use crate::app::App;
use crate::assistant::{Finished, Prompt, PromptKind, Run};
use crate::bridge::{AgentsView, RunRow};

use super::title_of;

const RECENT: usize = 8;
const SAMPLED: u64 = u64::MAX;

impl App {
    pub(super) fn sync_agents_view(&mut self) {
        if self.agents_seen == SAMPLED {
            return;
        }
        if let Some(sample) = self.notches.iter().find_map(|nw| nw.notch.previewed_agents()) {
            self.panel.agents = sample;
            self.agents_seen = SAMPLED;
            self.redraw_notch();
            return;
        }
        let version = self.inbox.version();
        if version == self.agents_seen {
            return;
        }
        self.agents_seen = version;
        let view = agents_view(self.inbox.runs(), self.inbox.finished(), &self.inbox.prompts());
        if view != self.panel.agents {
            self.panel.agents = view;
            self.redraw_notch();
        }
    }
}

fn agents_view(runs: Vec<Run>, finished: Vec<Finished>, prompts: &[Prompt]) -> AgentsView {
    let mut running: Vec<RunRow> = runs
        .into_iter()
        .map(|run| {
            let prompt = waiting_prompt(&run, prompts);
            RunRow { run, prompt }
        })
        .collect();
    running.sort_by_key(|row| std::cmp::Reverse(row.run.since_ms));
    let mut recent = finished;
    recent.sort_by(|a, b| b.ended_ms.cmp(&a.ended_ms).then(b.id.cmp(&a.id)));
    recent.truncate(RECENT);
    AgentsView { running, recent }
}

fn waiting_prompt(run: &Run, prompts: &[Prompt]) -> Option<(u64, String)> {
    prompts
        .iter()
        .filter(|prompt| prompt.provider == run.provider && prompt.session == run.session)
        .find(|prompt| !matches!(prompt.kind, PromptKind::Finished { .. }))
        .map(|prompt| (prompt.id, title_of(prompt)))
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::*;
    use crate::assistant::Provider;

    fn run(session: &str, since_ms: u64, waiting: bool) -> Run {
        Run {
            provider: Provider::Claude,
            session: session.into(),
            project: session.into(),
            since_ms,
            waiting,
            doing: None,
            window: None,
        }
    }

    fn prompt(id: u64, session: &str, kind: PromptKind) -> Prompt {
        Prompt {
            id,
            provider: Provider::Claude,
            session: session.into(),
            project: session.into(),
            agent: None,
            kind,
            at: Instant::now(),
        }
    }

    fn finished(id: u64, ended_ms: u64) -> Finished {
        Finished {
            id,
            provider: Provider::Claude,
            project: "p".into(),
            message: String::new(),
            took: Duration::from_secs(30),
            ended_ms,
        }
    }

    #[test]
    fn newest_turn_first() {
        let view = agents_view(vec![run("old", 1_000, false), run("new", 9_000, false)], Vec::new(), &[]);
        let order: Vec<&str> = view.running.iter().map(|row| row.run.session.as_str()).collect();
        assert_eq!(order, ["new", "old"]);
        let ended: Vec<Finished> = (1..=10).map(|id| finished(id, id * 1_000)).collect();
        let view = agents_view(Vec::new(), ended, &[]);
        let ids: Vec<u64> = view.recent.iter().map(|done| done.id).collect();
        assert_eq!(ids, [10, 9, 8, 7, 6, 5, 4, 3]);
    }

    #[test]
    fn waiting_run_has_its_prompt() {
        let bash = PromptKind::Permission { tool: "Bash".into(), detail: "ls".into(), always: false };
        let prompts = [
            prompt(1, "other", PromptKind::Attention { message: "Elsewhere".into() }),
            prompt(2, "app", PromptKind::Finished { message: "Done".into() }),
            prompt(3, "app", bash),
        ];
        let view = agents_view(vec![run("app", 1_000, true), run("idle", 2_000, false)], Vec::new(), &prompts);
        assert_eq!(view.running[1].prompt, Some((3, "Run this command?".into())));
        assert_eq!(view.running[0].prompt, None);
    }
}
