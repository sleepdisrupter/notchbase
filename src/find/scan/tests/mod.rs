mod apps;
mod disk;
mod lanes;
mod plan;
mod pool;
mod verdicts;

use std::collections::{HashMap, HashSet};
use std::io::{self, ErrorKind};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::config::FindConfig;
use crate::find::os::{Listed, Raw, Root, raw, root};
use crate::find::shared::{Order, Shared, lock};
use crate::find::usage::Usage;

use super::worker::Lister;
use super::{Fences, Job, Listing, Plan, Scanner};

pub(super) const DISK: &str = "/disk";
pub(super) const HOME: &str = "/disk/Users/me";
pub(super) const DIR: u8 = raw::DIR;
const PATIENCE: Duration = Duration::from_secs(30);
const END_OF_NAME: u8 = 0;

pub(super) type Folder<'a> = (&'a str, &'a [(&'a str, u8)]);

pub(super) struct Tree {
    folders: HashMap<PathBuf, Vec<(String, u8)>>,
    known: HashSet<PathBuf>,
    locked: Vec<PathBuf>,
    opened: Mutex<Vec<PathBuf>>,
}

impl Tree {
    pub(super) fn new(folders: &[Folder<'_>]) -> Self {
        let mut tree =
            Self { folders: HashMap::new(), known: HashSet::new(), locked: Vec::new(), opened: Mutex::default() };
        for (dir, entries) in folders {
            tree.known.insert(dir.into());
            tree.known
                .extend(entries.iter().filter(|(_, bits)| bits & DIR != 0).map(|(name, _)| Path::new(dir).join(name)));
            tree.folders.insert(dir.into(), entries.iter().map(|(name, bits)| ((*name).to_owned(), *bits)).collect());
        }
        tree
    }

    pub(super) fn locked(mut self, dir: &str) -> Self {
        self.locked.push(dir.into());
        self
    }

    pub(super) fn without(mut self, dir: &str) -> Self {
        self.known.remove(Path::new(dir));
        self
    }

    pub(super) fn opened(&self) -> Vec<PathBuf> {
        let mut opened = lock(&self.opened).clone();
        opened.sort();
        opened
    }

    pub(super) fn lister(self: &Arc<Self>) -> Lister {
        let tree = self.clone();
        Arc::new(move |dir, _devices, into| tree.list(dir, into))
    }

    fn list(&self, dir: &Path, into: &mut Listed) -> io::Result<()> {
        into.names.clear();
        into.rows.clear();
        lock(&self.opened).push(dir.to_path_buf());
        if self.locked.iter().any(|locked| locked == dir) {
            return Err(ErrorKind::PermissionDenied.into());
        }
        if !self.known.contains(dir) {
            return Err(ErrorKind::NotFound.into());
        }
        for (name, bits) in self.folders.get(dir).into_iter().flatten() {
            into.names.extend(name.bytes().chain([END_OF_NAME]));
            into.rows.push(Raw { modified: 20, created: 10, file_id: 0, bits: *bits });
        }
        Ok(())
    }
}

pub(super) struct Rig {
    pub scanner: Scanner,
    pub inbox: Receiver<Order>,
    pub tree: Arc<Tree>,
}

impl Rig {
    pub(super) fn new(tree: Tree) -> Self {
        let (sink, inbox) = mpsc::channel();
        let shared = Arc::new(Shared::new(Usage::load(None, 0), Arc::new(|| {})));
        let tree = Arc::new(tree);
        Self { scanner: Scanner::listing_with(shared, sink, tree.lister()), inbox, tree }
    }

    pub(super) fn finish(&self, tickets: &[u32]) -> Vec<Order> {
        finish(&self.inbox, tickets)
    }
}

pub(super) fn finish(inbox: &Receiver<Order>, tickets: &[u32]) -> Vec<Order> {
    let (mut waiting, mut orders): (HashSet<u32>, Vec<Order>) = (tickets.iter().copied().collect(), Vec::new());
    while !waiting.is_empty() {
        let order = inbox.recv_timeout(PATIENCE).expect("every ticket finishes");
        if let Order::Finished(ticket) = &order {
            assert!(waiting.remove(ticket), "ticket {ticket} finished twice or was never asked for");
        }
        orders.push(order);
    }
    orders
}

pub(super) fn listings(orders: &[Order]) -> Vec<&Listing> {
    let batches = orders.iter().filter_map(|order| if let Order::Listed(batch) = order { Some(batch) } else { None });
    batches.flatten().collect()
}

pub(super) fn names(listing: &Listing) -> Vec<String> {
    let names = listing.names.split(|byte| *byte == END_OF_NAME);
    names.zip(&listing.rows).map(|(name, _)| String::from_utf8_lossy(name).into_owned()).collect()
}

pub(super) fn listing_of<'a>(orders: &'a [Order], dir: &str) -> &'a Listing {
    let found = listings(orders).into_iter().find(|listing| listing.dir == Path::new(dir));
    found.unwrap_or_else(|| panic!("{dir} was listed"))
}

pub(super) fn deep(dir: &str, ticket: u32) -> Job {
    Job { dir: dir.into(), tier: 0, deep: true, reach: 3, ticket, live: false }
}

pub(super) fn boot(path: &str) -> Root {
    Root { path: path.into(), volume: [0; 16], devices: [1, 1], flags: root::BOOT }
}

pub(super) fn removable(path: &str) -> Root {
    Root { path: path.into(), volume: [0; 16], devices: [2, 2], flags: root::REMOVABLE }
}

pub(super) fn fences() -> Fences {
    let paths = |below: &str, names: &[&str]| names.iter().map(|name| Path::new(below).join(name)).collect();
    Fences {
        sealed: paths(DISK, &["System", "usr", "bin", "Volumes"]),
        open: paths(DISK, &["System/Applications", "usr/local"]),
        guarded: paths(HOME, &["Desktop", "Documents", "Downloads", "Library/Containers"]),
        apps: paths(DISK, &["Applications", "System/Applications", "Users/me/Applications"]),
    }
}

pub(super) fn plan(full_access: bool) -> Plan {
    planned(&FindConfig::default(), full_access)
}

pub(super) fn planned(config: &FindConfig, full_access: bool) -> Plan {
    let roots = [boot(DISK), removable("/disk/Volumes/Work")];
    let data = Path::new(HOME).join("Library/Application Support/notchbase/find");
    Plan::fenced(config, &roots, Some(Path::new(HOME)), Some(&data), full_access, fences())
}
