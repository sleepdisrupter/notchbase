//! The lister threads. Each takes a job, lists the folder, queues what is inside and sends the rows on in batches.

use std::io::{self, ErrorKind};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::sync::mpsc::Sender;
use std::thread;
use std::time::{Duration, Instant};

use crate::find::os::{self, Listed};
use crate::find::shared::{Order, Shared};

use super::lanes::{self, Lanes, Next, Queued, Taken};
use super::{Job, Listing, Pace, Plan, Verdict};

pub(super) const BATCH_ROWS: usize = 4_096;
const FLUSH_AFTER: Duration = Duration::from_millis(50);
const SEARCH_NAP: Duration = Duration::from_millis(2);
const SEARCH_WAIT: Duration = Duration::from_millis(40);
const END_OF_NAME: u8 = 0;

pub(super) type Lister = Arc<dyn Fn(&Path, [u64; 2], &mut Listed) -> io::Result<()> + Send + Sync>;

pub(super) struct Pool {
    pub shared: Arc<Shared>,
    pub sink: Sender<Order>,
    pub lanes: Lanes,
    pub list: Lister,
}

struct Worker<'a> {
    pool: &'a Pool,
    sink: Sender<Order>,
    index: usize,
    epoch: u64,
    scratch: Listed,
    batch: Vec<Listing>,
    listed: Vec<lanes::Listed>,
    rows: usize,
    since: Option<Instant>,
}

struct Ending<'a>(&'a Lanes, u64);

impl Drop for Ending<'_> {
    fn drop(&mut self) {
        self.0.ended(self.1);
    }
}

pub(super) fn run(pool: &Pool, epoch: u64) {
    let _ending = Ending(&pool.lanes, epoch);
    thread::scope(|scope| {
        let mut helpers = 0;
        Worker::new(pool, 0, epoch).work(|wanted| {
            while helpers + 1 < Pace::MOST_WORKERS && wanted > helpers {
                helpers += 1;
                let index = helpers;
                let named = thread::Builder::new().name(format!("{}-{index}", crate::find::SCAN_THREAD));
                let _ = named.spawn_scoped(scope, move || Worker::new(pool, index, epoch).work(|_| {}));
            }
        });
    });
}

impl<'a> Worker<'a> {
    fn new(pool: &'a Pool, index: usize, epoch: u64) -> Self {
        Self {
            pool,
            sink: pool.sink.clone(),
            index,
            epoch,
            scratch: Listed::default(),
            batch: Vec::new(),
            listed: Vec::new(),
            rows: 0,
            since: None,
        }
    }

    fn work(mut self, mut grow: impl FnMut(usize)) {
        os::be_gentle();
        loop {
            let mut next = self.pool.lanes.next(self.index, self.epoch, false);
            if matches!(next, Next::Idle) {
                self.flush();
                next = self.pool.lanes.next(self.index, self.epoch, true);
            }
            let Next::Job(taken) = next else { break };
            grow(self.pool.lanes.backlog().min(taken.pace.workers().saturating_sub(1)));
            self.yield_to_a_search();
            let (started, pace) = (Instant::now(), taken.pace);
            self.list(*taken);
            if self.rows >= BATCH_ROWS || self.since.is_some_and(|since| since.elapsed() >= FLUSH_AFTER) {
                self.flush();
            }
            let rest = pace.rest(started.elapsed());
            if !rest.is_zero() {
                thread::sleep(rest);
            }
        }
        self.flush();
    }

    fn list(&mut self, taken: Taken) {
        let Taken { job, plan, level, .. } = taken;
        let read = catch_unwind(AssertUnwindSafe(|| self.read(&job, &plan, level)));
        let (listing, children) = read.unwrap_or_else(|_| {
            eprintln!("notchbase: Find gave up on a folder it could not list");
            self.scratch = Listed::default();
            (None, Vec::new())
        });
        if let Some(listing) = listing {
            self.rows += listing.rows.len();
            self.since.get_or_insert_with(Instant::now);
            self.batch.push(listing);
        }
        self.listed.push(lanes::Listed { job, children });
    }

    fn read(&mut self, job: &Job, plan: &Plan, level: u8) -> (Option<Listing>, Vec<Queued>) {
        let complete = match (self.pool.list)(&job.dir, plan.devices(&job.dir), &mut self.scratch) {
            Ok(()) => true,
            Err(refusal) if missing(&refusal) => return (None, Vec::new()),
            Err(_) => false,
        };
        let (listing, children) = classify(job, plan, level, &self.scratch, complete);
        (Some(listing), children)
    }

    fn flush(&mut self) {
        if self.listed.is_empty() {
            return;
        }
        if !self.batch.is_empty() {
            let _ = self.sink.send(Order::Listed(std::mem::take(&mut self.batch)));
        }
        self.pool.lanes.settle(std::mem::take(&mut self.listed), std::mem::take(&mut self.rows));
        self.since = None;
    }

    // A search should feel instant and the scan can wait, so a worker holds off for up to 40 ms while one runs.
    fn yield_to_a_search(&self) {
        let mut waited = Duration::ZERO;
        while self.pool.shared.searching.load(Ordering::Relaxed) && waited < SEARCH_WAIT {
            thread::sleep(SEARCH_NAP);
            waited += SEARCH_NAP;
        }
    }
}

pub(super) fn classify(job: &Job, plan: &Plan, level: u8, listed: &Listed, complete: bool) -> (Listing, Vec<Queued>) {
    let depth = job.dir.components().count();
    let mut listing = Listing {
        dir: job.dir.clone(),
        tier: job.tier,
        inside_package: plan.packaged(&job.dir),
        names: Vec::with_capacity(listed.names.len()),
        rows: Vec::with_capacity(listed.rows.len()),
        complete,
        deep: job.deep,
        reach: job.reach,
        ticket: job.ticket,
    };
    let mut children = Vec::new();
    for (name, row) in listed.names.split(|byte| *byte == END_OF_NAME).zip(&listed.rows) {
        let text = String::from_utf8_lossy(name);
        let verdict = plan.verdict(&job.dir, job.tier, &text, row.bits, depth);
        if verdict == Verdict::Skip {
            continue;
        }
        listing.names.extend_from_slice(name);
        listing.names.push(END_OF_NAME);
        listing.rows.push(*row);
        let Verdict::Descend(tier) = verdict else { continue };
        let dir = job.dir.join(&*text);
        if job.deep && tier <= job.reach && !plan.held_back(level, &dir, tier) {
            let (rank, ahead) = (plan.rank(&dir), plan.ahead(&dir));
            let child = Job { dir, tier, deep: true, reach: job.reach, ticket: job.ticket, live: job.live };
            children.push(Queued { job: child, rank, ahead });
        }
    }
    (listing, children)
}

fn missing(refusal: &io::Error) -> bool {
    matches!(refusal.kind(), ErrorKind::NotFound | ErrorKind::NotADirectory)
}
