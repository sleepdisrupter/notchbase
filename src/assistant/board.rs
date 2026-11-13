//! The board of open prompts and the runs beside them. Hook events add, settle and retire prompts, a held prompt keeps
//! its hook waiting for an answer, and every change bumps the version.

use std::sync::mpsc::{self, Receiver, Sender};
use std::time::{Duration, Instant};

use super::prompt::{Prompt, PromptKind, Provider, Reply, fits};
use super::runs::{Beat, Finished, Run, Runs, Whose};
use super::timing::{ATTENTION_TTL, FINISHED_TTL, NOTE_TTL};
use super::wire::{Answer, Event, Signal};

pub(super) struct Held {
    pub(super) id: u64,
    pub(super) answers: Receiver<Answer>,
}

struct Entry {
    prompt: Prompt,
    agent: Option<String>,
    started_ms: u64,
    key: Option<u64>,
    expires: Option<Instant>,
    answer: Option<Sender<Answer>>,
}

impl Entry {
    fn held(&self) -> bool {
        self.answer.is_some()
    }

    /// Attention and finished prompts that nothing waits on, which a newer attention replaces.
    fn lingers(&self) -> bool {
        !self.held() && matches!(self.prompt.kind, PromptKind::Attention { .. } | PromptKind::Finished { .. })
    }

    fn release(self) {
        if let Some(answer) = self.answer {
            let _ = answer.send(Answer::Release);
        }
    }
}

struct Origin {
    provider: Provider,
    session: String,
    agent: Option<String>,
    agent_type: Option<String>,
    project: String,
    started_ms: u64,
    window: Option<u32>,
}

impl Origin {
    fn same_session(&self, entry: &Entry) -> bool {
        entry.prompt.provider == self.provider && entry.prompt.session == self.session
    }

    fn same_agent(&self, entry: &Entry) -> bool {
        self.same_session(entry) && entry.agent == self.agent
    }

    /// A hook only clears prompts from hooks that started no later than it, so a late one can't clear newer prompts.
    fn precedes(&self, entry: &Entry) -> bool {
        entry.started_ms <= self.started_ms
    }
}

#[derive(Default)]
pub(super) struct Board {
    entries: Vec<Entry>,
    next_id: u64,
    version: u64,
    muted: Vec<Provider>,
    runs: Runs,
    /// Sessions that just lost a prompt. Their runs stop waiting once nothing is left to answer.
    vacated: Vec<(Provider, String)>,
}

impl Board {
    pub(super) fn version(&self) -> u64 {
        self.version
    }

    pub(super) fn prompts(&self) -> Vec<Prompt> {
        self.entries.iter().map(|entry| entry.prompt.clone()).collect()
    }

    pub(super) fn runs(&self) -> Vec<Run> {
        self.runs.live()
    }

    pub(super) fn finished(&self) -> Vec<Finished> {
        self.runs.finished()
    }

    pub(super) fn enabled(&self, provider: Provider) -> bool {
        !self.muted.contains(&provider)
    }

    pub(super) fn set_enabled(&mut self, provider: Provider, enabled: bool) {
        if enabled {
            self.muted.retain(|muted| *muted != provider);
        } else if self.enabled(provider) {
            self.muted.push(provider);
            self.retire(|entry| entry.prompt.provider == provider);
            if self.runs.drop_provider(provider) {
                self.version += 1;
            }
            self.unwait();
        }
    }

    pub(super) fn apply(&mut self, event: Event, now: Instant) -> Option<Held> {
        if !self.enabled(event.provider) {
            return None;
        }
        let (origin, signal) = split(event);
        self.track(&origin, &signal);
        let held = self.signal(&origin, signal, now);
        self.unwait();
        held
    }

    fn signal(&mut self, origin: &Origin, signal: Signal, now: Instant) -> Option<Held> {
        match signal {
            Signal::Ask { key, prompt } => return Some(self.hold(origin, key, prompt, now)),
            Signal::Note { key, prompt } => self.show(origin, Some(key), prompt, now, NOTE_TTL),
            Signal::Attention { message } => self.attend(origin, message, now),
            Signal::ToolDone { key } => self.settle(origin, key),
            Signal::TurnStarted => self.retire(|entry| origin.same_agent(entry) && origin.precedes(entry)),
            Signal::TurnEnded { message } => {
                self.retire(|entry| origin.same_agent(entry) && origin.precedes(entry));
                self.show(origin, None, PromptKind::Finished { message }, now, FINISHED_TTL);
            }
            Signal::Idle | Signal::ToolStarted { .. } => {}
            Signal::SessionEnded => self.retire(|entry| origin.same_session(entry) && origin.precedes(entry)),
        }
        None
    }

    pub(super) fn reply(&mut self, id: u64, reply: Reply, now_ms: u64) {
        let Some(index) = self.entries.iter().position(|entry| entry.prompt.id == id) else {
            return;
        };
        let entry = self.entries.remove(index);
        self.version += 1;
        if interrupts(&entry.prompt, &reply) {
            self.stop_run(&entry.prompt, now_ms);
        }
        // Answering in the assistant hands the question over, so the run still waits on you.
        if reply != Reply::InApp {
            self.vacated.push((entry.prompt.provider, entry.prompt.session.clone()));
        }
        if let Some(answer) = entry.answer {
            let _ = answer.send(answer_for(&entry.prompt.kind, reply));
        }
        self.unwait();
    }

    pub(super) fn forget(&mut self, id: u64) {
        self.retire(|entry| entry.prompt.id == id);
        self.unwait();
    }

    pub(super) fn sweep(&mut self, now: Instant) {
        self.retire(|entry| entry.expires.is_some_and(|expires| expires <= now));
        self.unwait();
    }

    pub(super) fn expire_runs(&mut self, now_ms: u64) {
        if self.runs.expire(now_ms) {
            self.version += 1;
        }
    }

    pub(super) fn expire_gone(&mut self, now_ms: u64, alive: impl Fn(u32) -> bool) {
        let gone = self.runs.expire_gone(now_ms, alive);
        if gone.is_empty() {
            return;
        }
        self.version += 1;
        self.retire(|entry| gone.iter().any(|(provider, session)| in_session(entry, *provider, session)));
        self.unwait();
    }

    pub(super) fn window_of(&self, id: u64) -> Option<u32> {
        let prompt = &self.entries.iter().find(|entry| entry.prompt.id == id)?.prompt;
        self.runs.window_of(prompt.provider, &prompt.session)
    }

    pub(super) fn release_all(&mut self) {
        self.retire(|_| true);
        self.unwait();
    }

    fn unwait(&mut self) {
        for (provider, session) in std::mem::take(&mut self.vacated) {
            let asking = self.entries.iter().any(|entry| {
                entry.prompt.provider == provider
                    && entry.prompt.session == session
                    && !matches!(entry.prompt.kind, PromptKind::Finished { .. })
            });
            if !asking && self.runs.answered(provider, &session) {
                self.version += 1;
            }
        }
    }

    fn stop_run(&mut self, prompt: &Prompt, now_ms: u64) {
        let whose =
            Whose { provider: prompt.provider, session: &prompt.session, project: &prompt.project, window: None };
        if self.runs.beat(&whose, Beat::Idle, now_ms) {
            self.version += 1;
        }
    }

    fn track(&mut self, origin: &Origin, signal: &Signal) {
        let beat = match signal {
            Signal::Ask { .. } | Signal::Note { .. } | Signal::Attention { .. } => Beat::Waiting,
            Signal::ToolStarted { tool } => Beat::Doing(super::runs::verb(tool)),
            Signal::ToolDone { .. } => Beat::Busy,
            Signal::TurnStarted => Beat::Started,
            Signal::TurnEnded { message } => Beat::Ended(message.clone()),
            Signal::Idle => Beat::Idle,
            Signal::SessionEnded => Beat::Gone,
        };
        let whose = Whose {
            provider: origin.provider,
            session: &origin.session,
            project: &origin.project,
            window: origin.window,
        };
        if self.runs.beat(&whose, beat, origin.started_ms) {
            self.version += 1;
        }
    }

    fn hold(&mut self, origin: &Origin, key: u64, kind: PromptKind, now: Instant) -> Held {
        let (sender, answers) = mpsc::channel();
        let mut entry = self.entry(origin, Some(key), kind, now);
        entry.answer = Some(sender);
        let id = entry.prompt.id;
        self.push(entry);
        Held { id, answers }
    }

    fn show(&mut self, origin: &Origin, key: Option<u64>, kind: PromptKind, now: Instant, ttl: Duration) {
        let mut entry = self.entry(origin, key, kind, now);
        entry.expires = Some(now + ttl);
        self.push(entry);
    }

    fn attend(&mut self, origin: &Origin, message: String, now: Instant) {
        // A held prompt already says this session needs you.
        if self.entries.iter().any(|entry| entry.held() && origin.same_session(entry)) {
            return;
        }
        self.retire(|entry| entry.lingers() && origin.same_session(entry));
        self.show(origin, None, PromptKind::Attention { message }, now, ATTENTION_TTL);
    }

    fn settle(&mut self, origin: &Origin, key: u64) {
        let found = self
            .entries
            .iter()
            .position(|entry| entry.key == Some(key) && origin.same_agent(entry) && origin.precedes(entry));
        if let Some(index) = found {
            self.version += 1;
            let entry = self.entries.remove(index);
            self.vacated.push((entry.prompt.provider, entry.prompt.session.clone()));
            entry.release();
        }
    }

    fn entry(&mut self, origin: &Origin, key: Option<u64>, kind: PromptKind, now: Instant) -> Entry {
        self.next_id += 1;
        let prompt = Prompt {
            id: self.next_id,
            provider: origin.provider,
            session: origin.session.clone(),
            project: origin.project.clone(),
            agent: origin.agent_type.clone(),
            kind,
            at: now,
        };
        Entry { prompt, agent: origin.agent.clone(), started_ms: origin.started_ms, key, expires: None, answer: None }
    }

    fn push(&mut self, entry: Entry) {
        self.entries.push(entry);
        self.version += 1;
    }

    fn retire(&mut self, doomed: impl Fn(&Entry) -> bool) {
        let (gone, kept): (Vec<Entry>, Vec<Entry>) = std::mem::take(&mut self.entries).into_iter().partition(doomed);
        self.entries = kept;
        if !gone.is_empty() {
            self.version += 1;
        }
        for entry in gone {
            self.vacated.push((entry.prompt.provider, entry.prompt.session.clone()));
            entry.release();
        }
    }
}

fn in_session(entry: &Entry, provider: Provider, session: &str) -> bool {
    entry.prompt.provider == provider && entry.prompt.session == session
}

fn split(event: Event) -> (Origin, Signal) {
    let Event { provider, session, agent, agent_type, cwd, started_ms, window, signal } = event;
    let origin = Origin { provider, session, agent, agent_type, project: project(&cwd), started_ms, window };
    (origin, signal)
}

fn project(cwd: &str) -> String {
    cwd.rsplit(['/', '\\']).find(|part| !part.is_empty()).unwrap_or(cwd).to_owned()
}

/// Denying a Claude permission interrupts the turn, and Claude sends no Stop after that.
fn interrupts(prompt: &Prompt, reply: &Reply) -> bool {
    *reply == Reply::Deny && prompt.provider == Provider::Claude && matches!(prompt.kind, PromptKind::Permission { .. })
}

/// Turns a reply into the hook's answer. Anything that doesn't fit the prompt releases it.
fn answer_for(kind: &PromptKind, reply: Reply) -> Answer {
    match (kind, reply) {
        (PromptKind::Permission { always: true, .. }, Reply::AllowAlways) => Answer::Always,
        (PromptKind::Permission { .. }, Reply::Allow | Reply::AllowAlways) => Answer::Allow,
        (PromptKind::Permission { .. }, Reply::Deny) => Answer::Deny,
        (PromptKind::Questions { questions, answerable: true }, Reply::Answers(picks)) if fits(questions, &picks) => {
            Answer::Answers { picks }
        }
        _ => Answer::Release,
    }
}
