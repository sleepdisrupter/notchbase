//! Starts the writer thread, and starts it again when an order finds it gone.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use crate::config::FindConfig;

use super::context;
use super::recents::recently_used;
use super::scan::Scanner;
use super::shared::{Asked, Order, Shared, lock};
use super::weights::{SEED_AGAIN_AFTER, SEED_MOST, SEED_WITHIN};
use super::writer::{Inputs, Writer};
use super::{RECENTS_THREAD, Setup, WRITER_THREAD};

pub(super) struct Crew {
    dir: Option<PathBuf>,
    home: Option<PathBuf>,
    config: Mutex<FindConfig>,
    ram_bytes: u64,
    writes: bool,
    asks: Sender<Asked>,
    seeded: Mutex<Option<Instant>>,
    seeding: Arc<AtomicBool>,
}

impl Crew {
    pub(super) fn new(setup: Setup, asks: Sender<Asked>) -> Self {
        let Setup { dir, home, config, ram_bytes, writes } = setup;
        let config = Mutex::new(config);
        let (seeded, seeding) = (Mutex::new(None), Arc::new(AtomicBool::new(false)));
        Self { dir, home, config, ram_bytes, writes, asks, seeded, seeding }
    }

    pub(super) fn writer(&self, shared: &Arc<Shared>, orders: &Sender<Order>) -> Writer {
        let inputs = Inputs {
            scanner: Arc::new(Scanner::new(shared.clone(), orders.clone())),
            sink: orders.clone(),
            asks: Some(self.asks.clone()),
            dir: self.dir.clone(),
            home: self.home.clone(),
            config: lock(&self.config).clone(),
            ram_bytes: self.ram_bytes,
            writes: self.writes,
        };
        Writer::with(shared.clone(), inputs)
    }

    pub(super) fn respawn(&self, shared: &Arc<Shared>) -> Option<Sender<Order>> {
        eprintln!("notchbase: Find's writer stopped and is started again");
        let (orders, inbox) = mpsc::channel();
        let writer = self.writer(shared, &orders);
        thread::Builder::new().name(WRITER_THREAD.into()).spawn(move || writer.run(inbox)).ok().map(|_| orders)
    }

    pub(super) fn follow(&self, config: &FindConfig) {
        *lock(&self.config) = config.clone();
    }

    pub(super) fn seed(&self, orders: Sender<Order>) {
        let config = lock(&self.config).clone();
        if !(self.writes && config.enabled && config.learn && config.recents) {
            return;
        }
        let mut seeded = lock(&self.seeded);
        let again = Duration::from_secs(SEED_AGAIN_AFTER.into());
        if seeded.is_some_and(|at| at.elapsed() < again) || self.seeding.swap(true, Ordering::AcqRel) {
            return;
        }
        *seeded = Some(Instant::now());
        let seeding = self.seeding.clone();
        let run = move || {
            let used = recently_used(context::now().saturating_sub(SEED_WITHIN), SEED_MOST);
            if !used.is_empty() {
                let _ = orders.send(Order::Seed(used));
            }
            seeding.store(false, Ordering::Release);
        };
        if thread::Builder::new().name(RECENTS_THREAD.into()).spawn(run).is_err() {
            self.seeding.store(false, Ordering::Release);
        }
    }
}
