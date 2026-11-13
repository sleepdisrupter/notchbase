//! Finds the newest usage in recent logs: at most 8 files and 8 MiB a pass, each file cached by size and modified time.

use std::path::Path;

use super::rollouts::{self, Rollout};
use super::snapshot::Snapshot;
use super::tail;

const READ_FILES: usize = 8;
const READ_BUDGET: u64 = 4 * tail::MAX_BYTES;

#[derive(Debug, PartialEq)]
pub(super) enum Scan {
    Found(Snapshot),
    Empty,
    Failed,
}

#[derive(Default)]
pub(super) struct Cache {
    entries: Vec<Entry>,
}

struct Entry {
    rollout: Rollout,
    found: Option<Snapshot>,
}

impl Cache {
    fn take(&mut self, rollout: &Rollout) -> Option<Entry> {
        let index = self.entries.iter().position(|entry| entry.rollout.path == rollout.path)?;
        Some(self.entries.swap_remove(index))
    }
}

pub(super) fn newest(home: &Path, cache: &mut Cache) -> Scan {
    let rollouts = match rollouts::recent(home) {
        Ok(rollouts) => rollouts,
        Err(err) => {
            eprintln!("notchbase: couldn't list Codex sessions in {}: {err}", home.display());
            return Scan::Failed;
        }
    };
    let mut pass = Pass { budget: READ_BUDGET, best: None, failed: false, kept: Vec::new() };
    for rollout in rollouts.into_iter().take(READ_FILES) {
        // Files come newest first, so one written before our best reading can't beat it, nor can the rest.
        let older = pass.best.as_ref().is_some_and(|best| best.at >= rollout.modified);
        if older || !pass.visit(rollout, cache) {
            break;
        }
    }
    cache.entries = pass.kept;
    match (pass.best, pass.failed) {
        (Some(snapshot), _) => Scan::Found(snapshot),
        (None, true) => Scan::Failed,
        (None, false) => Scan::Empty,
    }
}

struct Pass {
    budget: u64,
    best: Option<Snapshot>,
    failed: bool,
    kept: Vec<Entry>,
}

impl Pass {
    fn visit(&mut self, rollout: Rollout, cache: &mut Cache) -> bool {
        let cached = cache.take(&rollout);
        if let Some(entry) = cached.as_ref().filter(|entry| entry.rollout == rollout) {
            self.offer(entry.found.as_ref());
            self.kept.extend(cached);
            return true;
        }
        // Not enough budget left for a whole tail, so stop here and keep the old entry.
        if self.budget < tail::MAX_BYTES {
            self.kept.extend(cached);
            return false;
        }
        match tail::last_snapshot(&rollout.path, rollout.len, rollout.modified) {
            Ok(tail) => {
                self.budget = self.budget.saturating_sub(tail.read);
                self.offer(tail.found.as_ref());
                self.kept.push(Entry { rollout, found: tail.found });
            }
            Err(err) => {
                eprintln!("notchbase: couldn't read Codex usage from {}: {err}", rollout.path.display());
                self.failed = true;
                if let Some(stale) = cached {
                    self.offer(stale.found.as_ref());
                    self.kept.push(stale);
                }
            }
        }
        true
    }

    fn offer(&mut self, found: Option<&Snapshot>) {
        if let Some(found) = found.filter(|found| self.best.as_ref().is_none_or(|best| found.at > best.at)) {
            self.best = Some(found.clone());
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use crate::codex::fixture::{Scratch, T0, at, stamp, token_count};

    fn line(percent: u32, secs: u64) -> String {
        token_count(&stamp(secs), &format!(r#"{{"primary": {{"used_percent": {percent}, "window_minutes": 300}}}}"#))
    }

    fn percent(scan: Scan) -> Option<f64> {
        match scan {
            Scan::Found(snapshot) => snapshot.primary.map(|window| window.percent),
            _ => None,
        }
    }

    const A: &str = "sessions/2026/09/21/rollout-2026-09-21T08-00-00-a.jsonl";
    const B: &str = "sessions/2026/09/21/rollout-2026-09-21T09-00-00-b.jsonl";
    const OLD: &str = "archived_sessions/rollout-2026-09-10T09-00-00-c.jsonl";

    #[test]
    fn no_limits_anywhere_is_empty() {
        let dir = Scratch::new();
        let mut cache = Cache::default();
        assert_eq!(newest(&dir.path().join("missing"), &mut cache), Scan::Empty);
        assert_eq!(newest(dir.path(), &mut cache), Scan::Empty);
        dir.write(A, &["{not json".into(), r#"{"type":"session_meta","payload":{}}"#.into()], at(T0));
        assert_eq!(newest(dir.path(), &mut cache), Scan::Empty);
    }

    #[test]
    fn newest_snapshot_across_files_wins() {
        let dir = Scratch::new();
        dir.write(OLD, &[line(90, T0 - 86_400)], at(T0 - 86_400));
        dir.write(A, &[line(10, T0 + 50), r#"{"type":"turn_context"}"#.into()], at(T0 + 300));
        dir.write(B, &[line(20, T0 + 100), line(25, T0 + 200)], at(T0 + 200));
        let mut cache = Cache::default();
        assert_eq!(
            percent(newest(dir.path(), &mut cache)),
            Some(25.0),
            "B's snapshot is newer though A was touched later"
        );
    }

    #[test]
    fn fresh_session_falls_back_to_older() {
        let dir = Scratch::new();
        dir.write(OLD, &[line(40, T0)], at(T0));
        dir.write(B, &[r#"{"type":"session_meta","payload":{}}"#.into()], at(T0 + 500));
        assert_eq!(percent(newest(dir.path(), &mut Cache::default())), Some(40.0));
    }

    #[test]
    fn unchanged_files_are_not_read_again() {
        let dir = Scratch::new();
        let path = dir.write(A, &[line(10, T0)], at(T0 + 10));
        let mut cache = Cache::default();
        assert_eq!(percent(newest(dir.path(), &mut cache)), Some(10.0));
        dir.write(A, &[line(77, T0)], at(T0 + 10));
        assert_eq!(fs::metadata(&path).unwrap().len(), line(10, T0).len() as u64 + 1);
        assert_eq!(percent(newest(dir.path(), &mut cache)), Some(10.0), "same length and time: cached");
        dir.write(A, &[line(77, T0), line(78, T0 + 20)], at(T0 + 20));
        assert_eq!(percent(newest(dir.path(), &mut cache)), Some(78.0));
        assert_eq!(percent(newest(dir.path(), &mut Cache::default())), Some(78.0));
    }

    #[test]
    fn cache_forgets_files_out_of_window() {
        let dir = Scratch::new();
        let path = dir.write(A, &[line(10, T0)], at(T0));
        let mut cache = Cache::default();
        newest(dir.path(), &mut cache);
        assert_eq!(cache.entries.len(), 1);
        fs::remove_file(path).unwrap();
        assert_eq!(newest(dir.path(), &mut cache), Scan::Empty);
        assert!(cache.entries.is_empty());
    }
}
