//! What Windows says about its apps: the Start menu shortcuts and how often each one was used.

mod packaged;
mod registry;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use crate::find::key::{Key, key_of};
use crate::poll::lock;

use super::assist::{BRANCHES, EVERYONES_PROGRAMS, OWN_PROGRAMS, counted, placed, rot13};
use super::{Read, Used};

const START_MENU_PROGRAMS: &str = "Microsoft\\Windows\\Start Menu\\Programs";

static PACKAGED: Mutex<Vec<(String, PathBuf)>> = Mutex::new(Vec::new());

pub fn read(_path: &Path) -> Read {
    Read::default()
}

pub fn settle(menu: Option<&Path>) {
    if let Some(menu) = menu {
        *lock(&PACKAGED) = packaged::sync(menu);
    }
}

pub fn every() -> Vec<std::path::PathBuf> {
    Vec::new()
}

pub fn used() -> Option<HashMap<Key, Used>> {
    let folders = menus();
    let packaged = lock(&PACKAGED).clone();
    let mut used: HashMap<Key, Used> = HashMap::new();
    for (name, record) in BRANCHES.into_iter().flat_map(registry::counts) {
        let name = rot13(&name);
        let shortcut = packaged.iter().find(|(id, _)| id.eq_ignore_ascii_case(&name)).map(|(_, path)| path.clone());
        let (Some(path), Some(seen)) = (shortcut.or_else(|| placed(&name, &folders)), counted(&record)) else {
            continue;
        };
        let known = used.entry(key_of(&path)).or_default();
        *known = Used { last: known.last.max(seen.last), times: known.times.saturating_add(seen.times) };
    }
    (!used.is_empty()).then_some(used)
}

fn menus() -> Vec<(&'static str, PathBuf)> {
    let below = |holder: &str| std::env::var_os(holder).map(|holder| PathBuf::from(holder).join(START_MENU_PROGRAMS));
    let named = [(OWN_PROGRAMS, below("APPDATA")), (EVERYONES_PROGRAMS, below("ProgramData"))];
    named.into_iter().filter_map(|(id, folder)| Some((id, folder?))).collect()
}

pub fn opens(_path: &Path) -> Option<String> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "reads how the apps of this machine were used from its registry"]
    fn this_pc_reports_shortcut_use() {
        let used = used().unwrap_or_default();
        println!("{} shortcuts and programs were used", used.len());
        assert!(used.values().all(|used| *used != Used::default()));
    }
}
