//! The query thread. It answers the newest question, refreshes the answer while Find shows, and hands the apps it
//! meets to the librarian.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

use super::apps::{Librarian, Roster};
use super::key::Key;
use super::query::{self, Scratch, Stop};
use super::shared::{Ask, Asked, Shared, lock};
use super::view::{Answer, FindRow};

const REFRESH_EVERY: Duration = Duration::from_millis(500);

#[derive(Debug, Default)]
struct Sifted {
    ask: Option<Ask>,
    refresh: bool,
    apps: bool,
    stop: bool,
}

pub struct Asker {
    shared: Arc<Shared>,
    scratch: Scratch,
    frozen: Option<Vec<FindRow>>,
    typed: Option<Ask>,
    shown: Vec<FindRow>,
    refreshed: Option<Instant>,
    librarian: Option<Librarian>,
    catalogued: Option<u64>,
    visited: bool,
}

impl Asker {
    pub fn new(shared: Arc<Shared>, home: Option<PathBuf>) -> Self {
        Self {
            shared,
            scratch: Scratch::new(home),
            frozen: None,
            typed: None,
            shown: Vec::new(),
            refreshed: None,
            librarian: None,
            catalogued: None,
            visited: false,
        }
    }

    pub fn cataloguing(self, librarian: Option<Librarian>) -> Self {
        Self { librarian, ..self }
    }

    pub fn run(mut self, inbox: Receiver<Asked>) {
        let mut due: Option<Instant> = None;
        loop {
            let waited = match due {
                Some(at) => inbox.recv_timeout(at.saturating_duration_since(Instant::now())),
                None => inbox.recv().map_err(|_| RecvTimeoutError::Disconnected),
            };
            let Sifted { ask, refresh, apps, stop } = match waited {
                Ok(first) => self.sift(first, &inbox),
                Err(RecvTimeoutError::Timeout) => Sifted::default(),
                Err(RecvTimeoutError::Disconnected) => return,
            };
            if stop {
                return;
            }
            let now = Instant::now();
            due = match ask {
                Some(ask) => {
                    self.answer(&ask);
                    self.refreshed = Some(now);
                    None
                }
                None => due.or_else(|| refresh.then(|| self.refresh_due(now))),
            };
            if apps {
                self.meet_the_apps();
            }
            self.catalogue();
            if due.is_some_and(|at| at <= now) {
                self.refresh();
                (self.refreshed, due) = (Some(now), None);
            }
        }
    }

    pub fn answer(&mut self, ask: &Ask) {
        let listed = query::asks_nothing(ask);
        let thawed = self.frozen.clone().filter(|_| listed);
        let rows =
            |rows| Answer { generation: ask.generation, query: ask.text.clone(), rows, reading: None, more: false };
        let Some(answer) = thawed.map(rows).or_else(|| self.look(ask)) else { return };
        if listed && self.frozen.is_none() {
            *lock(&self.shared.predicted) =
                answer.rows.iter().filter(|row| row.predicted).map(|row| Key(row.key)).collect();
            self.frozen = Some(answer.rows.clone());
        }
        self.typed = (!listed).then(|| ask.clone());
        self.shown = if listed { Vec::new() } else { answer.rows.clone() };
        self.post(answer);
    }

    fn refresh(&mut self) {
        let Some(ask) = self.typed.clone().filter(|_| self.shared.visible.load(Ordering::Acquire)) else { return };
        if let Some(answer) = self.look(&ask).filter(|answer| answer.rows != self.shown) {
            self.shown = answer.rows.clone();
            self.post(answer);
        }
    }

    fn refresh_due(&self, now: Instant) -> Instant {
        self.refreshed.map_or(now, |last| (last + REFRESH_EVERY).max(now))
    }

    fn sift(&mut self, first: Asked, inbox: &Receiver<Asked>) -> Sifted {
        let mut sifted = Sifted::default();
        for asked in std::iter::once(first).chain(inbox.try_iter()) {
            match asked {
                Asked::Query(ask) => sifted.ask = Some(ask),
                Asked::Shown => {
                    (self.frozen, self.typed, self.visited) = (None, None, true);
                    self.scratch.launchers.forget();
                }
                Asked::Refresh => sifted.refresh = true,
                Asked::Apps => sifted.apps = true,
                Asked::Stop => sifted.stop = true,
            }
        }
        sifted
    }

    pub fn meet_the_apps(&mut self) {
        let (shared, scratch) = (&self.shared, &mut self.scratch);
        let stop = Stop { latest: &shared.latest, mine: u64::MAX };
        let met = catch_unwind(AssertUnwindSafe(|| query::warm(&shared.reading(), scratch, &stop)));
        if met.is_err() {
            eprintln!("notchbase: Find could not gather the apps and will try again");
        }
    }

    pub fn catalogue(&mut self) {
        let Some(librarian) = &self.librarian else { return };
        let edition = self.scratch.launchers.edition();
        if !self.scratch.launchers.gathered() || (self.catalogued == Some(edition) && !self.visited) {
            return;
        }
        let apps = self.scratch.launchers.roster(&self.shared.reading());
        librarian.file(Roster { apps, fresh: std::mem::take(&mut self.visited) });
        self.catalogued = Some(edition);
    }

    fn look(&mut self, ask: &Ask) -> Option<Answer> {
        self.scratch.book = self.shared.book();
        self.scratch.everything = self.shared.everything.load(Ordering::Acquire);
        let (shared, scratch) = (&self.shared, &mut self.scratch);
        let stop = Stop { latest: &shared.latest, mine: ask.generation };
        shared.searching.store(true, Ordering::Release);
        let looked =
            catch_unwind(AssertUnwindSafe(|| query::run(&shared.reading(), &shared.usage, ask, scratch, &stop)));
        shared.searching.store(false, Ordering::Release);
        looked.unwrap_or_else(|_| {
            eprintln!("notchbase: a search for files failed and was dropped");
            None
        })
    }

    fn post(&self, answer: Answer) {
        *lock(&self.shared.answer) = Some(answer);
        self.shared.publish();
    }
}

#[cfg(test)]
mod tests;
