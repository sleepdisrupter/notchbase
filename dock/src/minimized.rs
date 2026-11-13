//! Minimized windows, which the dock shows as tiles between the separator and the folders, like the Dock. Watched on
//! a thread of its own: the window list is cheap and says when an app's hidden windows change; only then is that app
//! asked through Accessibility which of them are minimized.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use crate::windows::{self, Win};

#[derive(Clone, Debug, PartialEq)]
pub struct Minimized {
    pub pid: i32,
    pub win: Win,
}

/// Every app is asked again this often anyway, in case a window went down without the list changing.
const SWEEP: Duration = Duration::from_secs(4);

#[derive(Default)]
pub struct Watch {
    /// Each app's windows that are off screen, as last seen.
    hidden: HashMap<i32, Vec<u32>>,
    /// Each app's minimized windows, as last asked.
    minimized: HashMap<i32, Vec<Win>>,
    /// Window ids in the order they went down, so the newest is last, nearest the folders.
    order: Vec<u32>,
    swept: Option<Instant>,
}

impl Watch {
    /// The minimized windows now, oldest first.
    pub fn poll(&mut self) -> Vec<Minimized> {
        let me = std::process::id() as i32;
        let mut hidden: HashMap<i32, Vec<u32>> = HashMap::new();
        for (pid, id, on_screen) in windows::every_window() {
            if pid == me {
                continue;
            }
            let ids = hidden.entry(pid).or_default();
            if !on_screen {
                ids.push(id);
            }
        }
        let sweep = self.swept.is_none_or(|at| at.elapsed() >= SWEEP);
        if sweep {
            self.swept = Some(Instant::now());
        }
        self.minimized.retain(|pid, _| hidden.contains_key(pid));
        for (pid, ids) in &mut hidden {
            ids.sort_unstable();
            let changed = self.hidden.get(pid) != Some(ids);
            if ids.is_empty() {
                self.minimized.remove(pid);
            } else if changed || sweep {
                self.minimized.insert(*pid, windows::minimized_of(*pid));
            }
        }
        self.hidden = hidden;

        let all: Vec<Minimized> = self
            .minimized
            .iter()
            .flat_map(|(pid, wins)| wins.iter().map(|w| Minimized { pid: *pid, win: w.clone() }))
            .collect();
        self.order.retain(|id| all.iter().any(|m| m.win.id == *id));
        for m in &all {
            if !self.order.contains(&m.win.id) {
                self.order.push(m.win.id);
            }
        }
        let mut all = all;
        all.sort_by_key(|m| self.order.iter().position(|id| *id == m.win.id));
        all
    }
}
