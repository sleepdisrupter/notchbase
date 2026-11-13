//! The writer, the only thread that changes the index and the usage model. It takes orders from the scanner, the
//! watcher and the app, sends work to the scanner as errands, and ticks for upkeep in between.

mod access;
mod adopt;
mod apply;
mod changes;
mod errands;
mod idle;
mod launch;
pub mod ledger;
mod orders;
mod reconcile;
mod system;
#[cfg(test)]
pub(in crate::find) mod tests;
mod upkeep;

use std::collections::{HashMap, HashSet};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::time::Instant;

use crate::config::FindConfig;

use super::os::Root;
use super::scan::{Job, Plan, Submit};
use super::shared::{Asked, Order, Shared};
use super::store::Meta;
use super::watch::coalesce::{Coalescer, admitted};
use super::watch::{Event, Watcher};
use errands::{Duty, Errands, Purpose};
use ledger::Ledger;
use system::{Moment, Native};
use upkeep::Timers;

pub use system::{Inputs, System};

pub struct Writer {
    shared: Arc<Shared>,
    scanner: Arc<dyn Submit>,
    system: Box<dyn System>,
    sink: Sender<Order>,
    asks: Option<Sender<Asked>>,
    dir: Option<PathBuf>,
    home: Option<PathBuf>,
    config: FindConfig,
    ram_bytes: u64,
    writes: bool,
    plan: Arc<Plan>,
    meta: Meta,
    roots: Vec<Root>,
    unwatched: HashSet<PathBuf>,
    watcher: Option<Watcher>,
    coalescer: Coalescer,
    ledger: Ledger,
    errands: Errands,
    deferred: Vec<Deferred>,
    rescanned: HashMap<PathBuf, Instant>,
    relisted: HashMap<PathBuf, Instant>,
    keeping: HashSet<PathBuf>,
    reconciles: Vec<PathBuf>,
    full: bool,
    launched: bool,
    timers: Timers,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Work {
    dir: PathBuf,
    deep: bool,
    reach: u8,
}

#[derive(Debug)]
struct Deferred {
    errand: u32,
    due: Instant,
    work: Work,
    live: bool,
}

impl Writer {
    pub fn with(shared: Arc<Shared>, inputs: Inputs) -> Self {
        Self::on(shared, inputs, Box::new(Native))
    }

    pub(in crate::find) fn on(shared: Arc<Shared>, inputs: Inputs, system: Box<dyn System>) -> Self {
        let Inputs { scanner, sink, asks, dir, home, config, ram_bytes, writes } = inputs;
        let plan = Arc::new(system.plan(&config, &[], home.as_deref(), dir.as_deref(), false));
        shared.guard(plan.clone());
        let now = system.now();
        Self {
            shared,
            scanner,
            system,
            sink,
            asks,
            dir,
            home,
            config,
            ram_bytes,
            writes,
            plan,
            meta: Meta::default(),
            roots: Vec::new(),
            unwatched: HashSet::new(),
            watcher: None,
            coalescer: Coalescer::new(),
            ledger: Ledger::seeded(0),
            errands: Errands::default(),
            deferred: Vec::new(),
            rescanned: HashMap::new(),
            relisted: HashMap::new(),
            keeping: HashSet::new(),
            reconciles: Vec::new(),
            full: false,
            launched: false,
            timers: Timers::new(now),
        }
    }

    pub fn run(mut self, inbox: Receiver<Order>) {
        if self.writes && self.config.enabled {
            guarded(|| self.launch());
        }
        loop {
            match inbox.recv_timeout(self.wait()) {
                Ok(order) => guarded(|| self.obey(order)),
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => return,
            }
            if self.tick_due() {
                guarded(|| self.tick());
            }
        }
    }

    pub fn obey(&mut self, order: Order) {
        let now = self.system.now();
        self.timers.last_order = now.at;
        match order {
            Order::Listed(listings) => self.listed(listings, now),
            Order::Finished(ticket) => self.finished(ticket, now),
            Order::Changes(burst) => self.changed(burst, now),
            Order::Unwatched(root) => {
                self.unwatched.insert(root);
            }
            Order::Picked { pick, context } => self.picked(&pick, &context),
            Order::Left { picked, now: at } => self.left(picked, at),
            Order::Seed(used) => self.seed(&used, now),
            Order::Gone(path) => self.look(&path, None, now),
            Order::Adopt(apps) => self.adopt(&apps),
            Order::Follow(config) => self.follow(config, now),
            Order::Forget => self.shared.usage.forget(),
            Order::Access => self.probe(now),
        }
        self.report(now);
    }

    pub fn tick(&mut self) {
        let now = self.system.now();
        self.timers.last_tick = now.at;
        let due = self.coalescer.due(now.at, self.timers.on_battery());
        if !due.is_empty() {
            self.probe(now);
        }
        for Event { change, id } in due {
            if let Some(change) = admitted(change, &self.plan) {
                self.change(Event { change, id }, now);
            }
        }
        self.run_deferred(now);
        self.upkeep(now);
        self.report(now);
    }

    fn errand(&mut self, oldest: Option<u64>, duties: Vec<Duty>, purpose: Purpose, work: Vec<Work>, now: Moment) {
        let errand = self.errands.open(duties, purpose);
        if let Some(oldest) = oldest {
            self.ledger.opened(errand, oldest);
        }
        self.dispatch(errand, work, purpose == Purpose::Live, now);
    }

    fn dispatch(&mut self, errand: u32, work: Vec<Work>, live: bool, now: Moment) {
        if work.is_empty() {
            if let Some(idle) = self.errands.idle(errand) {
                self.land(errand, idle.duties, idle.purpose, now);
            }
            return;
        }
        let jobs: Vec<Job> = work
            .into_iter()
            .filter_map(|Work { dir, deep, reach }| {
                let ticket = self.errands.ticket(errand)?;
                Some(Job { dir, tier: 0, deep, reach, ticket, live })
            })
            .collect();
        self.scanner.submit(jobs);
    }

    fn finished(&mut self, ticket: u32, now: Moment) {
        if let Some((errand, landed)) = self.errands.finished(ticket) {
            self.land(errand, landed.duties, landed.purpose, now);
        }
    }

    fn run_deferred(&mut self, now: Moment) {
        let (ready, waiting) = std::mem::take(&mut self.deferred).into_iter().partition(|work| work.due <= now.at);
        self.deferred = waiting;
        for Deferred { errand, work, live, .. } in ready {
            match live {
                true => self.relisted_now(work.dir.clone(), now),
                false => _ = self.rescanned.insert(work.dir.clone(), now.at),
            }
            self.dispatch(errand, vec![work], live, now);
        }
    }
}

// One bad order should not stop the writer, or the index would go stale until the next launch.
fn guarded(work: impl FnOnce()) {
    if catch_unwind(AssertUnwindSafe(work)).is_err() {
        eprintln!("notchbase: Find skipped an order it could not carry out");
    }
}
