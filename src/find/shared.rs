//! What the Find threads share: the live index and the usage model behind their locks, the latest answer, and flags.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError, RwLock, RwLockReadGuard, RwLockWriteGuard};

use crate::config::FindConfig;
use crate::poll::Wake;

use super::apps::Book;
use super::context::{Context, Stamp};
use super::key::Key;
use super::model::Live;
use super::scan::{Listing, Plan};
use super::usage::{Usage, Used};
use super::view::{Answer, Pick, Status};
use super::watch::Burst;

pub(in crate::find) struct Shared {
    pub live: RwLock<Live>,
    pub usage: Usage,
    pub latest: AtomicU64,
    pub searching: AtomicBool,
    pub visible: AtomicBool,
    pub everything: AtomicBool,
    pub version: AtomicU64,
    pub answer: Mutex<Option<Answer>>,
    pub predicted: Mutex<Vec<Key>>,
    pub status: Mutex<Status>,
    pub gate: RwLock<Arc<Plan>>,
    pub book: RwLock<Arc<Book>>,
    pub wake: Wake,
}

impl Shared {
    pub fn new(usage: Usage, wake: Wake) -> Self {
        Self {
            live: RwLock::new(Live::empty()),
            usage,
            latest: AtomicU64::new(0),
            searching: AtomicBool::new(false),
            visible: AtomicBool::new(false),
            everything: AtomicBool::new(false),
            version: AtomicU64::new(0),
            answer: Mutex::new(None),
            predicted: Mutex::new(Vec::new()),
            status: Mutex::new(Status::default()),
            gate: RwLock::new(Arc::new(Plan::resting())),
            book: RwLock::new(Arc::new(Book::default())),
            wake,
        }
    }

    pub fn publish(&self) {
        self.version.fetch_add(1, Ordering::AcqRel);
        (self.wake)();
    }

    pub fn reading(&self) -> RwLockReadGuard<'_, Live> {
        self.live.read().unwrap_or_else(PoisonError::into_inner)
    }

    pub fn writing(&self) -> RwLockWriteGuard<'_, Live> {
        self.live.write().unwrap_or_else(PoisonError::into_inner)
    }

    pub fn guard(&self, plan: Arc<Plan>) {
        *self.gate.write().unwrap_or_else(PoisonError::into_inner) = plan;
    }

    pub fn reachable(&self, path: &Path) -> bool {
        !self.gate.read().unwrap_or_else(PoisonError::into_inner).guarded(path)
    }

    pub fn book(&self) -> Arc<Book> {
        self.book.read().unwrap_or_else(PoisonError::into_inner).clone()
    }

    pub fn shelve(&self, book: Arc<Book>) {
        *self.book.write().unwrap_or_else(PoisonError::into_inner) = book;
    }
}

pub(in crate::find) use crate::poll::lock;

#[derive(Debug)]
pub(in crate::find) enum Order {
    Listed(Vec<Listing>),
    Finished(u32),
    #[cfg_attr(not(any(test, target_os = "macos", windows)), expect(dead_code))]
    Changes(Burst),
    #[cfg_attr(not(any(test, windows)), expect(dead_code))]
    Unwatched(PathBuf),
    Picked {
        pick: Pick,
        context: Context,
    },
    Left {
        picked: Option<Key>,
        now: Stamp,
    },
    Seed(Vec<Used>),
    Gone(PathBuf),
    Adopt(Vec<PathBuf>),
    Follow(FindConfig),
    Forget,
    Access,
}

#[derive(Debug)]
pub(in crate::find) enum Asked {
    Query(Ask),
    Shown,
    Refresh,
    Apps,
    Stop,
}

#[derive(Clone, Debug)]
pub(in crate::find) struct Ask {
    pub generation: u64,
    pub text: Arc<str>,
    pub context: Context,
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicUsize;

    use super::*;

    fn counted() -> (Arc<AtomicUsize>, Shared) {
        let wakes = Arc::new(AtomicUsize::new(0));
        let counter = wakes.clone();
        let wake: Wake = Arc::new(move || {
            counter.fetch_add(1, Ordering::Relaxed);
        });
        (wakes, Shared::new(Usage::load(None, 0), wake))
    }

    #[test]
    fn publish_raises_version() {
        let (wakes, shared) = counted();
        assert_eq!((shared.version.load(Ordering::Acquire), wakes.load(Ordering::Relaxed)), (0, 0));
        shared.publish();
        shared.publish();
        assert_eq!((shared.version.load(Ordering::Acquire), wakes.load(Ordering::Relaxed)), (2, 2));
    }

    #[test]
    fn fresh_service_is_empty() {
        let (_, shared) = counted();
        assert_eq!(shared.reading().len(), 0);
        assert!(lock(&shared.answer).is_none() && lock(&shared.predicted).is_empty());
        assert!(!shared.visible.load(Ordering::Relaxed) && !shared.searching.load(Ordering::Relaxed));
    }

    #[test]
    fn poisoned_lock_still_opens() {
        let (_, shared) = counted();
        let shared = Arc::new(shared);
        let poisoner = shared.clone();
        let crashed = std::thread::spawn(move || {
            let _held = (lock(&poisoner.status), poisoner.writing());
            panic!("a worker fell over while holding the locks");
        })
        .join();
        assert!(crashed.is_err());
        assert_eq!(lock(&shared.status).entries, 0);
        assert_eq!(shared.reading().len(), 0);
    }
}
