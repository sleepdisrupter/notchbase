mod access;
mod changes;
mod idle;
mod launch;
mod listings;
mod off;
mod upkeep;
mod world;

use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use notchbase_sdk::protocol::Power;

use super::Writer;
use super::system::{Inputs, System};
use crate::config::FindConfig;
use crate::find::context::Stamp;
use crate::find::model::EntryId;
use crate::find::os::{self, Raw, Root, raw, root};
use crate::find::scan::{Job, Listing, Pace, Plan, Submit};
use crate::find::shared::{Asked, Order, Shared, lock};
use crate::find::tests::Scratch;
use crate::find::usage::Usage;
use crate::find::watch::{Burst, Change, Event};

pub(in crate::find) use world::{State, World};

pub(super) const START: Stamp = 1_760_000_000;
pub(super) const VOLUME: [u8; 16] = [7; 16];
const END_OF_NAME: u8 = 0;

#[derive(Default)]
pub(in crate::find) struct Recording {
    pub jobs: Mutex<Vec<Job>>,
    pub landed: Mutex<usize>,
    pub levels: Mutex<Vec<u8>>,
    pub plans: Mutex<Vec<Arc<Plan>>>,
    pub paces: Mutex<Vec<Pace>>,
    world: Mutex<Option<World>>,
}

impl Recording {
    pub(in crate::find) fn take(&self) -> Vec<Job> {
        std::mem::take(&mut *lock(&self.jobs))
    }
}

impl Submit for Recording {
    fn submit(&self, jobs: Vec<Job>) {
        if let Some(world) = &*lock(&self.world) {
            jobs.iter().for_each(|job| world.note(format!("scan {} deep {}", job.dir.display(), job.deep)));
        }
        lock(&self.jobs).extend(jobs);
    }

    fn landed(&self, rows: usize) {
        *lock(&self.landed) += rows;
    }

    fn follow(&self, plan: Arc<Plan>, level: u8) {
        lock(&self.levels).push(level);
        lock(&self.plans).push(plan);
    }

    fn pace(&self, pace: Pace) {
        lock(&self.paces).push(pace);
    }
}

pub(super) struct Rig {
    pub writer: Writer,
    pub world: World,
    pub scanner: Arc<Recording>,
    pub disk: Scratch,
    pub store: Scratch,
    pub asks: Receiver<Asked>,
    pub _orders: Receiver<Order>,
}

impl Rig {
    pub(super) fn new(name: &str) -> Self {
        Self::with(name, FindConfig::default(), Scratch::new(&format!("{name}-store")))
    }

    pub(super) fn with(name: &str, config: FindConfig, store: Scratch) -> Self {
        let disk = Scratch::new(name);
        let found = Root { path: disk.dir().to_path_buf(), volume: VOLUME, devices: [0, 0], flags: root::BOOT };
        let state = State {
            at: Instant::now(),
            stamp: START,
            power: Power { level: Some(80), plugged: true, charging: false },
            roots: vec![found],
            full: true,
            cursor: 0,
            keeps: true,
            watches: true,
            guarded: Vec::new(),
            apps: Vec::new(),
            log: Vec::new(),
        };
        let world = World(Arc::new(Mutex::new(state)));
        Self::assemble(world, config, disk, store)
    }

    pub(super) fn again(self) -> Self {
        let Rig { world, disk, store, writer, .. } = self;
        let config = writer.config.clone();
        drop(writer);
        world.state().log.clear();
        Self::assemble(world, config, disk, store)
    }

    fn assemble(world: World, config: FindConfig, disk: Scratch, store: Scratch) -> Self {
        let shared = Arc::new(Shared::new(Usage::load(None, START), Arc::new(|| {})));
        let scanner = Arc::new(Recording { world: Mutex::new(Some(world.clone())), ..Recording::default() });
        let ((sink, orders), (ask, asks)) = (mpsc::channel(), mpsc::channel());
        let inputs = Inputs {
            scanner: scanner.clone(),
            sink,
            asks: Some(ask),
            dir: Some(store.dir().to_path_buf()),
            home: Some(disk.dir().to_path_buf()),
            config,
            ram_bytes: 16 << 30,
            writes: true,
        };
        let writer = Writer::on(shared, inputs, Box::new(world.clone()));
        Self { writer, world, scanner, disk, store, asks, _orders: orders }
    }

    pub(super) fn root(&self) -> PathBuf {
        self.disk.dir().to_path_buf()
    }

    pub(super) fn at(&self, path: &str) -> PathBuf {
        path.split('/').filter(|part| !part.is_empty()).fold(self.root(), |dir, part| dir.join(part))
    }

    pub(super) fn find(&self, path: &str) -> Option<EntryId> {
        self.writer.shared.reading().resolve(&self.at(path))
    }

    pub(super) fn listed(&mut self, listings: Vec<Listing>) {
        self.writer.obey(Order::Listed(listings));
    }

    pub(super) fn finish(&mut self, jobs: &[Job]) {
        jobs.iter().for_each(|job| self.writer.obey(Order::Finished(job.ticket)));
    }

    pub(super) fn events(&mut self, changes: Vec<(Change, u64)>) {
        let events = changes.into_iter().map(|(change, id)| Event { change, id }).collect();
        self.writer.obey(Order::Changes(Burst { events, caught_up: false }));
    }

    pub(super) fn settle(&mut self) {
        self.world.pass(Duration::from_millis(250));
        self.writer.tick();
    }

    pub(super) fn cursor(&self) -> u64 {
        self.writer.ledger.safe(self.writer.coalescer.oldest())
    }

    pub(super) fn log(&self) -> Vec<String> {
        self.world.state().log.clone()
    }
}

pub(super) fn file(name: &str, modified: u32) -> (&str, u8, u32) {
    (name, 0, modified)
}

pub(super) fn folder(name: &str) -> (&str, u8, u32) {
    (name, raw::DIR, 0)
}

pub(super) fn listing(dir: &Path, entries: &[(&str, u8, u32)], ticket: u32) -> Listing {
    Listing {
        dir: dir.to_path_buf(),
        tier: 0,
        inside_package: false,
        names: entries.iter().flat_map(|(name, _, _)| name.bytes().chain([END_OF_NAME])).collect(),
        rows: entries
            .iter()
            .map(|(_, bits, modified)| Raw { modified: *modified, created: 1, file_id: 0, bits: *bits })
            .collect(),
        complete: true,
        deep: true,
        reach: 3,
        ticket,
    }
}
