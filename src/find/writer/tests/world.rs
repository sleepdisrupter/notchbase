use std::io;
use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use notchbase_sdk::protocol::Power;

use crate::config::FindConfig;
use crate::find::context::Stamp;
use crate::find::os::{self, Raw, Root};
use crate::find::scan::{Fences, Plan};
use crate::find::shared::{Order, lock};
use crate::find::watch::Watcher;
use crate::find::writer::system::{Moment, System};

pub(in crate::find) struct State {
    pub at: Instant,
    pub stamp: Stamp,
    pub power: Power,
    pub roots: Vec<Root>,
    pub full: bool,
    pub cursor: u64,
    pub keeps: bool,
    pub watches: bool,
    pub guarded: Vec<PathBuf>,
    pub apps: Vec<PathBuf>,
    pub log: Vec<String>,
}

#[derive(Clone)]
pub(in crate::find) struct World(pub(in crate::find) Arc<Mutex<State>>);

impl World {
    pub(in crate::find) fn state(&self) -> MutexGuard<'_, State> {
        lock(&self.0)
    }

    pub(in crate::find) fn pass(&self, time: Duration) {
        let mut state = self.state();
        state.at += time;
        state.stamp += time.as_secs() as Stamp;
    }

    pub(in crate::find) fn note(&self, line: String) {
        self.state().log.push(line);
    }
}

impl System for World {
    fn now(&self) -> Moment {
        let state = self.state();
        Moment { at: state.at, stamp: state.stamp }
    }

    fn power(&self) -> Power {
        self.state().power
    }

    fn roots(&self, _volumes: bool) -> Vec<Root> {
        self.state().roots.clone()
    }

    fn guarded(&self, _home: &Path) -> Vec<PathBuf> {
        self.state().guarded.clone()
    }

    fn full_access(&self) -> bool {
        self.state().full
    }

    fn plan(&self, config: &FindConfig, roots: &[Root], home: Option<&Path>, data: Option<&Path>, full: bool) -> Plan {
        let state = self.state();
        let fences = Fences { guarded: state.guarded.clone(), apps: state.apps.clone(), ..Fences::default() };
        Plan::fenced(config, roots, home, data, full, fences)
    }

    fn stat(&self, path: &Path) -> io::Result<Option<Raw>> {
        os::stat(path)
    }

    fn cursor_now(&self) -> u64 {
        self.state().cursor
    }

    fn keeps_cursor(&self) -> bool {
        self.state().keeps
    }

    fn watch(&self, roots: &[Root], since: Option<u64>, _sink: Sender<Order>) -> Option<Watcher> {
        let watching: Vec<String> = roots.iter().map(|found| found.path.display().to_string()).collect();
        self.note(format!("watch {watching:?} since {since:?}"));
        let stopped = self.clone();
        self.state().watches.then(|| Watcher::new(move || stopped.note("unwatch".into())))
    }
}
