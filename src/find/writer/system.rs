//! The `System` trait, everything the writer asks of the machine: the clock, power, roots, access, stat and the
//! watcher. `Native` is the real one, and the tests swap in a world of their own.

use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::Sender;
use std::time::Instant;

use notchbase_sdk::protocol::Power;

use crate::config::FindConfig;
use crate::find::context::{self, Stamp};
use crate::find::os::{self, Raw, Root};
use crate::find::scan::{Plan, Submit};
use crate::find::shared::{Asked, Order};
use crate::find::watch::Watcher;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Moment {
    pub at: Instant,
    pub stamp: Stamp,
}

pub trait System: Send {
    fn now(&self) -> Moment;
    fn power(&self) -> Power;
    fn roots(&self, volumes: bool) -> Vec<Root>;
    fn guarded(&self, home: &Path) -> Vec<PathBuf>;
    fn full_access(&self) -> bool;
    fn plan(&self, config: &FindConfig, roots: &[Root], home: Option<&Path>, data: Option<&Path>, full: bool) -> Plan;
    fn stat(&self, path: &Path) -> io::Result<Option<Raw>>;
    fn cursor_now(&self) -> u64;
    fn keeps_cursor(&self) -> bool;
    fn watch(&self, roots: &[Root], since: Option<u64>, sink: Sender<Order>) -> Option<Watcher>;
}

pub struct Native;

impl System for Native {
    fn now(&self) -> Moment {
        Moment { at: Instant::now(), stamp: context::now() }
    }

    fn power(&self) -> Power {
        crate::system::power::read()
    }

    fn roots(&self, volumes: bool) -> Vec<Root> {
        os::roots(volumes)
    }

    fn guarded(&self, home: &Path) -> Vec<PathBuf> {
        os::guarded(home)
    }

    fn full_access(&self) -> bool {
        os::full_access()
    }

    fn plan(&self, config: &FindConfig, roots: &[Root], home: Option<&Path>, data: Option<&Path>, full: bool) -> Plan {
        Plan::new(config, roots, home, data, full)
    }

    fn stat(&self, path: &Path) -> io::Result<Option<Raw>> {
        os::stat(path)
    }

    fn cursor_now(&self) -> u64 {
        os::cursor_now()
    }

    fn keeps_cursor(&self) -> bool {
        os::KEEPS_CURSOR
    }

    fn watch(&self, roots: &[Root], since: Option<u64>, sink: Sender<Order>) -> Option<Watcher> {
        os::watch(roots, since, sink)
    }
}

pub struct Inputs {
    pub scanner: Arc<dyn Submit>,
    pub sink: Sender<Order>,
    pub asks: Option<Sender<Asked>>,
    pub dir: Option<PathBuf>,
    pub home: Option<PathBuf>,
    pub config: FindConfig,
    pub ram_bytes: u64,
    pub writes: bool,
}
