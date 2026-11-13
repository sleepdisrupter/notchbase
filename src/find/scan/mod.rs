//! The scanner. Jobs name folders to list, a small pool of threads lists them, and each `Listing` goes back to the
//! writer as an order. The pool starts when there is work and goes away when the queue runs dry.

mod lanes;
mod pace;
mod plan;
mod rules;
mod tally;
#[cfg(test)]
mod tests;
mod worker;

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::Sender;
use std::thread;

use super::os::{self, Raw};
use super::shared::{Order, Shared};
use lanes::Lanes;
use worker::{Lister, Pool};

pub use pace::Pace;
pub use plan::*;

/// One folder to list. A deep job queues the folders inside it up to the `reach` tier, and a live job goes
/// in the live lane.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Job {
    pub dir: PathBuf,
    pub tier: u8,
    pub deep: bool,
    pub reach: u8,
    pub ticket: u32,
    pub live: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Listing {
    pub dir: PathBuf,
    pub tier: u8,
    pub inside_package: bool,
    pub names: Vec<u8>,
    pub rows: Vec<Raw>,
    pub complete: bool,
    pub deep: bool,
    pub reach: u8,
    pub ticket: u32,
}

/// What the writer asks of the scanner. The writer tests record these calls instead of listing anything.
pub trait Submit: Send + Sync {
    fn submit(&self, jobs: Vec<Job>);
    fn landed(&self, rows: usize);
    fn follow(&self, plan: Arc<Plan>, level: u8);
    fn pace(&self, pace: Pace);
}

pub struct Scanner {
    pool: Arc<Pool>,
}

impl Scanner {
    pub fn new(shared: Arc<Shared>, sink: Sender<Order>) -> Self {
        Self::listing_with(shared, sink, Arc::new(os::list))
    }

    #[cfg(test)]
    pub fn busy(&self) -> bool {
        self.pool.lanes.busy()
    }

    fn listing_with(shared: Arc<Shared>, sink: Sender<Order>, list: Lister) -> Self {
        let lanes = Lanes::new(sink.clone());
        Self { pool: Arc::new(Pool { shared, sink, lanes, list }) }
    }
}

impl Submit for Scanner {
    fn submit(&self, jobs: Vec<Job>) {
        let Some(epoch) = self.pool.lanes.submit(jobs) else { return };
        let pool = self.pool.clone();
        let named = thread::Builder::new().name(super::SCAN_THREAD.to_owned());
        if named.spawn(move || worker::run(&pool, epoch)).is_err() {
            self.pool.lanes.ended(epoch);
        }
    }

    fn landed(&self, rows: usize) {
        self.pool.lanes.landed(rows);
    }

    fn follow(&self, plan: Arc<Plan>, level: u8) {
        self.pool.lanes.follow(plan, level);
    }

    fn pace(&self, pace: Pace) {
        self.pool.lanes.pace(pace);
    }
}
