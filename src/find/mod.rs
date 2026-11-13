//! Find, the file search behind the Find page. It keeps an index of every file in memory, kept current by the
//! system's change notifications, and learns from what you open. `Find` is the app's handle on it: questions go in
//! and answers come back with a wake. wiki/Find.md has the threads, the launch and the file formats.

mod apps;
mod asking;
pub mod context;
mod crew;
mod guess;
pub mod icons;
mod key;
mod kind;
mod model;
mod os;
mod query;
mod rank;
mod recents;
mod scan;
mod shared;
mod store;
#[cfg(test)]
pub(crate) mod tests;
mod usage;
pub mod view;
mod watch;
mod weights;
mod writer;

use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::mpsc::{self, Receiver, SendError, Sender};
use std::sync::{Arc, Mutex};
use std::thread;

use crate::config::FindConfig;
use crate::poll::Wake;

pub use apps::opens;

#[cfg(target_os = "macos")]
pub fn full_disk_access() -> bool {
    os::full_access()
}

use apps::Librarian;
use asking::Asker;
use context::Context;
use crew::Crew;
pub use key::{Key, key_of};
use shared::{Ask, Asked, Order, Shared, lock};
use usage::Usage;
use view::{Answer, Phase, Pick, Status};
use writer::Writer;

pub const WRITER_THREAD: &str = "notchbase-find";
pub const QUERY_THREAD: &str = "notchbase-find-query";
pub const SCAN_THREAD: &str = "notchbase-find-scan";
#[cfg_attr(not(windows), expect(dead_code))]
pub const WATCH_THREAD: &str = "notchbase-find-watch";
pub const ICONS_THREAD: &str = "notchbase-find-icons";
pub const RECENTS_THREAD: &str = "notchbase-find-recents";
pub const APPS_THREAD: &str = "notchbase-find-apps";

pub struct Setup {
    pub dir: Option<PathBuf>,
    pub home: Option<PathBuf>,
    pub config: FindConfig,
    pub ram_bytes: u64,
    pub writes: bool,
}

pub struct Find {
    shared: Arc<Shared>,
    orders: Mutex<Option<Sender<Order>>>,
    asks: Option<Sender<Asked>>,
    visit: Mutex<Visit>,
    crew: Option<Crew>,
}

impl Drop for Find {
    fn drop(&mut self) {
        self.put(Asked::Stop);
    }
}

#[derive(Default)]
struct Visit {
    context: Context,
    picked: Option<Key>,
}

impl Find {
    pub fn start(wake: Wake, setup: Setup) -> Self {
        let (find, writer, inbox, asker, questions) = Self::assemble(setup, wake);
        let writing = thread::Builder::new().name(WRITER_THREAD.into()).spawn(move || writer.run(inbox));
        let asking = thread::Builder::new().name(QUERY_THREAD.into()).spawn(move || asker.run(questions));
        if writing.is_err() || asking.is_err() {
            eprintln!("notchbase: Find could not start its threads");
        }
        find.seed();
        find
    }

    pub fn folder() -> Option<PathBuf> {
        store::dir()
    }

    #[cfg(test)]
    pub fn idle(wake: Wake) -> Self {
        let shared = Arc::new(Shared::new(Usage::load(None, context::now()), wake));
        Self { shared, orders: Mutex::new(None), asks: None, visit: Mutex::default(), crew: None }
    }

    pub fn ask(&self, text: &str) -> u64 {
        let generation = self.shared.latest.fetch_add(1, Ordering::AcqRel) + 1;
        let context = lock(&self.visit).context.clone();
        self.put(Asked::Query(Ask { generation, text: Arc::from(text), context }));
        generation
    }

    pub fn shown(&self, context: Context) {
        self.shared.visible.store(true, Ordering::Release);
        *lock(&self.visit) = Visit { context, picked: None };
        self.put(Asked::Shown);
        self.seed();
    }

    pub fn left(&self) {
        self.shared.visible.store(false, Ordering::Release);
        let picked = lock(&self.visit).picked.take();
        self.send(Order::Left { picked, now: context::now() });
    }

    pub fn picked(&self, pick: &Pick, context: Context) {
        lock(&self.visit).picked = Some(key_of(&pick.path));
        self.send(Order::Picked { pick: pick.clone(), context });
    }

    pub fn reachable(&self, path: &Path) -> bool {
        self.shared.reachable(path)
    }

    #[cfg(test)]
    pub fn fence(&self, guarded: Vec<PathBuf>) {
        let fences = scan::Fences { guarded, ..scan::Fences::default() };
        let plan = scan::Plan::fenced(&FindConfig::default(), &[], None, None, false, fences);
        self.shared.guard(Arc::new(plan));
    }

    pub fn gone(&self, path: &Path) {
        self.send(Order::Gone(path.to_path_buf()));
    }

    pub fn follow(&self, config: &FindConfig) {
        self.shared.everything.store(config.hidden, Ordering::Release);
        if let Some(crew) = &self.crew {
            crew.follow(config);
        }
        self.send(Order::Follow(config.clone()));
        self.seed();
    }

    pub fn forget(&self) {
        self.send(Order::Forget);
    }

    pub fn recheck_access(&self) {
        self.send(Order::Access);
    }

    pub fn version(&self) -> u64 {
        self.shared.version.load(Ordering::Acquire)
    }

    pub fn take(&self) -> Option<Answer> {
        lock(&self.shared.answer).take()
    }

    pub fn status(&self) -> Status {
        *lock(&self.shared.status)
    }

    #[cfg(test)]
    pub fn post(&self, answer: Option<Answer>, status: Status) {
        *lock(&self.shared.answer) = answer;
        *lock(&self.shared.status) = status;
        self.shared.publish();
    }

    fn assemble(setup: Setup, wake: Wake) -> (Self, Writer, Receiver<Order>, Asker, Receiver<Asked>) {
        let learned = setup.dir.as_ref().filter(|_| setup.writes).map(|dir| dir.join(store::USAGE_FILE));
        let usage = if setup.config.learn {
            Usage::load(learned.as_deref(), context::now())
        } else {
            Usage::forgotten(learned.as_deref(), context::now())
        };
        let shared = Arc::new(Shared::new(usage, wake));
        shared.everything.store(setup.config.hidden, Ordering::Release);
        lock(&shared.status).phase = Phase::Loading;
        let (asks, questions) = mpsc::channel();
        let (orders, inbox) = mpsc::channel();
        let librarian = setup.writes.then(|| Librarian::start(shared.clone(), asks.clone(), orders.clone())).flatten();
        let asker = Asker::new(shared.clone(), setup.home.clone()).cataloguing(librarian);
        let crew = Crew::new(setup, asks.clone());
        let writer = crew.writer(&shared, &orders);
        let find = Self {
            shared,
            orders: Mutex::new(Some(orders)),
            asks: Some(asks),
            visit: Mutex::default(),
            crew: Some(crew),
        };
        (find, writer, inbox, asker, questions)
    }

    fn send(&self, order: Order) {
        let mut orders = lock(&self.orders);
        let Some(sender) = orders.as_ref() else { return };
        let Err(SendError(order)) = sender.send(order) else { return };
        *orders = self.crew.as_ref().and_then(|crew| crew.respawn(&self.shared));
        if let Some(sender) = orders.as_ref() {
            let _ = sender.send(order);
        }
    }

    fn put(&self, asked: Asked) {
        if let Some(asks) = &self.asks {
            let _ = asks.send(asked);
        }
    }

    fn seed(&self) {
        let Some(crew) = &self.crew else { return };
        if let Some(sender) = lock(&self.orders).clone() {
            crew.seed(sender);
        }
    }
}
