//! Runs: one turn of one session, from the prompt you send to the assistant stopping. Live runs say what they're
//! doing or that they wait on you, and the last eight finished turns are kept for three hours.

use std::collections::{HashMap, VecDeque};
use std::time::Duration;

use super::prompt::Provider;

/// A run nobody hears from this long is dropped. A long Bash call can be quiet for 10 minutes.
const STALE_MS: u64 = 15 * 60 * 1000;
/// How long a session's end is remembered, so a late hook can't bring the run back.
const FORGET_MS: u64 = 60 * 60 * 1000;
const KEEP_FINISHED: usize = 8;
const RECENT_MS: u64 = 3 * 60 * 60 * 1000;

#[derive(Clone, Debug, PartialEq)]
pub struct Run {
    pub provider: Provider,
    pub session: String,
    pub project: String,
    pub since_ms: u64,
    pub waiting: bool,
    pub doing: Option<&'static str>,
    pub window: Option<u32>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Finished {
    pub id: u64,
    pub provider: Provider,
    pub project: String,
    pub message: String,
    pub took: Duration,
    pub ended_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Beat {
    Started,
    Busy,
    Doing(&'static str),
    Waiting,
    Ended(String),
    Idle,
    Gone,
}

pub(super) struct Whose<'a> {
    pub(super) provider: Provider,
    pub(super) session: &'a str,
    pub(super) project: &'a str,
    pub(super) window: Option<u32>,
}

struct Track {
    run: Run,
    heard_ms: u64,
}

#[derive(Default)]
pub(super) struct Runs {
    live: Vec<Track>,
    /// When each session's last run ended. A hook that started before that is too late to count.
    ended: HashMap<(Provider, String), u64>,
    finished: VecDeque<Finished>,
    next_id: u64,
}

/// The word the live activity shows for a tool.
pub(super) fn verb(tool: &str) -> &'static str {
    match tool {
        "Bash" | "BashOutput" | "PowerShell" | "shell" | "exec_command" | "local_shell" => "Running",
        "Edit" | "MultiEdit" | "Write" | "NotebookEdit" | "apply_patch" => "Editing",
        "Read" | "NotebookRead" => "Reading",
        "Grep" | "Glob" | "LS" | "ToolSearch" => "Searching",
        "WebFetch" | "WebSearch" => "Browsing",
        "Task" | "Agent" => "Delegating",
        "TodoWrite" => "Planning",
        tool if tool.starts_with("mcp__") => "Using tools",
        _ => "Working",
    }
}

impl Runs {
    pub(super) fn live(&self) -> Vec<Run> {
        self.live.iter().map(|track| track.run.clone()).collect()
    }

    pub(super) fn finished(&self) -> Vec<Finished> {
        self.finished.iter().cloned().collect()
    }

    pub(super) fn beat(&mut self, whose: &Whose<'_>, beat: Beat, at: u64) -> bool {
        let key = (whose.provider, whose.session.to_owned());
        // A hook that started before the run ended is late, like a PostToolUse racing the Stop.
        if self.ended.get(&key).is_some_and(|ended| *ended >= at) {
            return false;
        }
        let found = self.find(whose);
        let changed = match (beat, found) {
            (Beat::Started, Some(index)) => self.restart(index, at),
            (beat @ (Beat::Started | Beat::Busy | Beat::Doing(_) | Beat::Waiting), None) => {
                self.begin(whose, at);
                let index = self.live.len() - 1;
                self.hear(index, at, &beat);
                true
            }
            (beat @ (Beat::Busy | Beat::Doing(_) | Beat::Waiting), Some(index)) => self.hear(index, at, &beat),
            (Beat::Ended(message), found) => {
                self.ended.insert(key, at);
                found.is_some_and(|index| self.end(index, at, Some(message)))
            }
            (Beat::Idle | Beat::Gone, found) => {
                self.ended.insert(key, at);
                found.is_some_and(|index| self.end(index, at, None))
            }
        };
        self.place(whose) || changed
    }

    pub(super) fn answered(&mut self, provider: Provider, session: &str) -> bool {
        let Some(track) =
            self.live.iter_mut().find(|track| track.run.provider == provider && track.run.session == session)
        else {
            return false;
        };
        std::mem::replace(&mut track.run.waiting, false)
    }

    pub(super) fn expire(&mut self, now_ms: u64) -> bool {
        self.ended.retain(|_, ended| now_ms.saturating_sub(*ended) < FORGET_MS);
        let before = (self.live.len(), self.finished.len());
        self.live.retain(|track| now_ms.saturating_sub(track.heard_ms) < STALE_MS);
        self.finished.retain(|done| now_ms.saturating_sub(done.ended_ms) < RECENT_MS);
        (self.live.len(), self.finished.len()) != before
    }

    pub(super) fn expire_gone(&mut self, now_ms: u64, alive: impl Fn(u32) -> bool) -> Vec<(Provider, String)> {
        let (gone, kept): (Vec<Track>, Vec<Track>) = std::mem::take(&mut self.live)
            .into_iter()
            .partition(|track| track.run.window.is_some_and(|pid| !alive(pid)));
        self.live = kept;
        let gone: Vec<(Provider, String)> =
            gone.into_iter().map(|track| (track.run.provider, track.run.session)).collect();
        for key in &gone {
            self.ended.insert(key.clone(), now_ms);
        }
        gone
    }

    pub(super) fn window_of(&self, provider: Provider, session: &str) -> Option<u32> {
        self.live.iter().find(|track| track.run.provider == provider && track.run.session == session)?.run.window
    }

    pub(super) fn drop_provider(&mut self, provider: Provider) -> bool {
        let before = self.live.len();
        self.live.retain(|track| track.run.provider != provider);
        self.live.len() != before
    }

    fn find(&self, whose: &Whose<'_>) -> Option<usize> {
        self.live.iter().position(|track| track.run.provider == whose.provider && track.run.session == whose.session)
    }

    /// Keeps the latest window any hook reported, so one that found none never erases it.
    fn place(&mut self, whose: &Whose<'_>) -> bool {
        let (Some(window), Some(index)) = (whose.window, self.find(whose)) else {
            return false;
        };
        self.live[index].run.window.replace(window) != Some(window)
    }

    fn begin(&mut self, whose: &Whose<'_>, at: u64) {
        let run = Run {
            provider: whose.provider,
            session: whose.session.to_owned(),
            project: whose.project.to_owned(),
            since_ms: at,
            waiting: false,
            doing: None,
            window: None,
        };
        self.live.push(Track { run, heard_ms: at });
    }

    fn restart(&mut self, index: usize, at: u64) -> bool {
        let track = &mut self.live[index];
        if at <= track.run.since_ms {
            return false;
        }
        track.run.since_ms = at;
        track.run.waiting = false;
        track.run.doing = None;
        track.heard_ms = track.heard_ms.max(at);
        true
    }

    fn hear(&mut self, index: usize, at: u64, beat: &Beat) -> bool {
        let track = &mut self.live[index];
        track.heard_ms = track.heard_ms.max(at);
        if at < track.run.since_ms {
            return false;
        }
        let next = match beat {
            Beat::Waiting => (true, track.run.doing),
            Beat::Doing(verb) => (false, Some(*verb)),
            _ => (false, None),
        };
        if (track.run.waiting, track.run.doing) == next {
            return false;
        }
        (track.run.waiting, track.run.doing) = next;
        true
    }

    fn end(&mut self, index: usize, at: u64, finished: Option<String>) -> bool {
        if at < self.live[index].run.since_ms {
            return false;
        }
        let Track { run, .. } = self.live.remove(index);
        if let Some(message) = finished {
            self.next_id += 1;
            let took = Duration::from_millis(at - run.since_ms);
            let Run { provider, project, .. } = run;
            let done = Finished { id: self.next_id, provider, project, message, took, ended_ms: at };
            self.finished.push_back(done);
            if self.finished.len() > KEEP_FINISHED {
                self.finished.pop_front();
            }
        }
        true
    }
}
