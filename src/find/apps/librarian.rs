//! The librarian thread. It reads what the system says about the apps the query thread found into a `Book`.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::{Duration, Instant};

use crate::find::APPS_THREAD;
use crate::find::key::Key;
use crate::find::shared::{Asked, Order, Shared};

use super::{Listed, Read, Used};

const USE_ASKED_EVERY: Duration = Duration::from_secs(60);

#[derive(Debug)]
pub struct Roster {
    pub apps: Vec<Listed>,
    pub fresh: bool,
}

pub struct Librarian {
    rosters: Sender<Roster>,
}

pub(in crate::find) struct Sources {
    pub settle: Box<dyn FnMut() + Send>,
    pub read: Box<dyn FnMut(&Listed) -> Read + Send>,
    pub used: Box<dyn FnMut() -> Option<HashMap<Key, Used>> + Send>,
    pub every: Box<dyn FnMut() -> Vec<PathBuf> + Send>,
    pub adopt: Box<dyn FnMut(Vec<PathBuf>) + Send>,
}

impl Sources {
    fn system(orders: Sender<Order>) -> Self {
        let settle = || super::settle(super::own_menu().as_deref());
        let adopt = move |apps: Vec<PathBuf>| drop(orders.send(Order::Adopt(apps)));
        Self {
            settle: Box::new(settle),
            read: Box::new(super::read),
            used: Box::new(super::used),
            every: Box::new(super::every),
            adopt: Box::new(adopt),
        }
    }
}

impl Librarian {
    pub fn start(shared: Arc<Shared>, asks: Sender<Asked>, orders: Sender<Order>) -> Option<Self> {
        Self::reading(shared, asks, Sources::system(orders))
    }

    pub(in crate::find) fn reading(shared: Arc<Shared>, asks: Sender<Asked>, mut sources: Sources) -> Option<Self> {
        let (rosters, inbox) = mpsc::channel();
        let run = move || {
            (sources.settle)();
            Desk { shared, asks, sources, asked_at: None }.run(&inbox);
        };
        thread::Builder::new().name(APPS_THREAD.into()).spawn(run).ok().map(|_| Self { rosters })
    }

    pub fn file(&self, roster: Roster) {
        let _ = self.rosters.send(roster);
    }
}

struct Desk {
    shared: Arc<Shared>,
    asks: Sender<Asked>,
    sources: Sources,
    asked_at: Option<Instant>,
}

impl Desk {
    fn run(mut self, inbox: &Receiver<Roster>) {
        while let Ok(first) = inbox.recv() {
            let roster = newest(first, inbox);
            self.revise(&roster, Instant::now());
        }
    }

    fn revise(&mut self, roster: &Roster, now: Instant) {
        let due = self.asked_at.is_none_or(|at| roster.fresh && now.saturating_duration_since(at) >= USE_ASKED_EVERY);
        let used = if due { (self.sources.used)() } else { None };
        if due {
            self.asked_at = Some(now);
            let known = (self.sources.every)();
            if !known.is_empty() {
                (self.sources.adopt)(known);
            }
        }
        let held = self.shared.book();
        let revised = held.revised(&roster.apps, used.as_ref(), &mut self.sources.read);
        if revised != *held {
            self.shared.shelve(Arc::new(revised));
            let _ = self.asks.send(Asked::Refresh);
        }
    }
}

fn newest(first: Roster, inbox: &Receiver<Roster>) -> Roster {
    inbox.try_iter().fold(first, |held, next| Roster { fresh: held.fresh || next.fresh, ..next })
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    use super::*;
    use crate::find::key::key_of;
    use crate::find::usage::Usage;

    const SOON: Duration = Duration::from_secs(5);

    fn listed(path: &str) -> Listed {
        Listed { key: key_of(Path::new(path)), path: path.into(), modified: 1 }
    }

    type Adopted = Arc<std::sync::Mutex<Vec<PathBuf>>>;

    fn desk(asked: &Arc<AtomicUsize>) -> (Desk, Receiver<Asked>) {
        let (desk, refreshes, _) = desk_adopting(asked);
        (desk, refreshes)
    }

    fn desk_adopting(asked: &Arc<AtomicUsize>) -> (Desk, Receiver<Asked>, Adopted) {
        let shared = Arc::new(Shared::new(Usage::load(None, 0), Arc::new(|| {})));
        let (asks, refreshes) = mpsc::channel();
        let counter = asked.clone();
        let adopted = Adopted::default();
        let taken = adopted.clone();
        let sources = Sources {
            settle: Box::new(|| {}),
            read: Box::new(|app| Read {
                title: Some(format!("shown {}", app.path.display())),
                backstage: false,
                unlisted: false,
            }),
            used: Box::new(move || {
                let times = counter.fetch_add(1, Ordering::Relaxed) as u32 + 1;
                Some(HashMap::from([(key_of(Path::new("/Applications/Maps.app")), Used { last: 9, times })]))
            }),
            every: Box::new(|| vec![PathBuf::from("/System/Library/CoreServices/Dock.app")]),
            adopt: Box::new(move |apps| taken.lock().unwrap().extend(apps)),
        };
        (Desk { shared, asks, sources, asked_at: None }, refreshes, adopted)
    }

    #[test]
    fn every_known_app_handed_over() {
        let asked = Arc::new(AtomicUsize::new(0));
        let (mut desk, _refreshes, adopted) = desk_adopting(&asked);
        let roster = |fresh: bool| Roster { apps: vec![listed("/Applications/Maps.app")], fresh };
        let start = Instant::now();
        desk.revise(&roster(false), start);
        let dock = PathBuf::from("/System/Library/CoreServices/Dock.app");
        assert_eq!(adopted.lock().unwrap().as_slice(), std::slice::from_ref(&dock), "with the first book");
        desk.revise(&roster(true), start + Duration::from_secs(5));
        assert_eq!(adopted.lock().unwrap().len(), 1, "not again within the minute");
        desk.revise(&roster(true), start + USE_ASKED_EVERY);
        assert_eq!(*adopted.lock().unwrap(), [dock.clone(), dock], "and again with the next asking");
    }

    #[test]
    fn roster_becomes_book() {
        let asked = Arc::new(AtomicUsize::new(0));
        let (mut desk, refreshes) = desk(&asked);
        let roster = Roster { apps: vec![listed("/Applications/Maps.app")], fresh: false };
        let start = Instant::now();
        desk.revise(&roster, start);
        let book = desk.shared.book();
        let card = book.card(roster.apps[0].key).expect("the app was read");
        assert_eq!((&*card.title, card.used.times), ("shown /Applications/Maps.app", 1), "the first book asks for use");
        assert!(matches!(refreshes.try_recv(), Ok(Asked::Refresh)));
        desk.revise(&roster, start);
        assert!(refreshes.try_recv().is_err(), "the same roster changes nothing");
        assert_eq!(asked.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn use_asked_at_most_each_minute() {
        let asked = Arc::new(AtomicUsize::new(0));
        let (mut desk, _refreshes) = desk(&asked);
        let roster = |fresh: bool| Roster { apps: vec![listed("/Applications/Maps.app")], fresh };
        let start = Instant::now();
        desk.revise(&roster(false), start);
        desk.revise(&roster(true), start + Duration::from_secs(5));
        desk.revise(&roster(false), start + USE_ASKED_EVERY * 2);
        assert_eq!(asked.load(Ordering::Relaxed), 1, "too soon, then not fresh");
        desk.revise(&roster(true), start + USE_ASKED_EVERY * 2);
        assert_eq!(asked.load(Ordering::Relaxed), 2);
        assert_eq!(desk.shared.book().card(listed("/Applications/Maps.app").key).map(|card| card.used.times), Some(2));
    }

    #[test]
    fn newest_roster_of_burst_read() {
        let (rosters, inbox) = mpsc::channel();
        for (path, fresh) in [("/Applications/B.app", true), ("/Applications/C.app", false)] {
            rosters.send(Roster { apps: vec![listed(path)], fresh }).unwrap();
        }
        let kept = newest(Roster { apps: vec![listed("/Applications/A.app")], fresh: false }, &inbox);
        assert_eq!((kept.apps, kept.fresh), (vec![listed("/Applications/C.app")], true));
    }

    #[test]
    fn reads_on_own_thread() {
        let shared = Arc::new(Shared::new(Usage::load(None, 0), Arc::new(|| {})));
        let (asks, refreshes) = mpsc::channel();
        let sources = Sources {
            settle: Box::new(|| {}),
            read: Box::new(|_| Read { title: Some("Maps".into()), backstage: true, unlisted: false }),
            used: Box::new(|| None),
            every: Box::new(Vec::new),
            adopt: Box::new(|_| {}),
        };
        let librarian = Librarian::reading(shared.clone(), asks, sources).expect("a thread");
        let app = listed("/Applications/Maps.app");
        librarian.file(Roster { apps: vec![app.clone()], fresh: true });
        assert!(matches!(refreshes.recv_timeout(SOON), Ok(Asked::Refresh)));
        let book = shared.book();
        assert_eq!(book.card(app.key).map(|card| (&*card.title, card.backstage)), Some(("Maps", true)));
    }
}
