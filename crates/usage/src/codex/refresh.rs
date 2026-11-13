//! One refresh: find the Codex home, scan its newest logs, and place the windows in the 5-hour and weekly slots.

use std::env;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::SystemTime;

use super::scan::{self, Cache, Scan};
use super::state::{Outcome, Shared};
use super::{home, lock, slots};

const UNREADABLE: &str = "Couldn't read Codex usage";

pub(super) fn refresh(shared: &Mutex<Shared>) {
    let mut cache = std::mem::take(&mut lock(shared).cache);
    let outcome = check(home::locate(|name| env::var_os(name)), &mut cache, SystemTime::now());
    let mut shared = lock(shared);
    shared.cache = cache;
    shared.apply(outcome);
}

fn check(home: Option<PathBuf>, cache: &mut Cache, now: SystemTime) -> Outcome {
    let Some(home) = home else {
        return Outcome::Empty;
    };
    match scan::newest(&home, cache) {
        Scan::Found(snapshot) => Outcome::Ready(slots::usage(&snapshot, now)),
        Scan::Empty => Outcome::Empty,
        Scan::Failed => Outcome::Failed(UNREADABLE.to_owned()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codex::fixture::{Scratch, T0, at, stamp, token_count};
    use crate::{PlanUsage, UsageWindow};

    const HOUR: u64 = 3600;

    #[test]
    fn no_home_means_signed_out() {
        assert_eq!(check(None, &mut Cache::default(), at(T0)), Outcome::Empty);
    }

    #[test]
    fn unlistable_sessions_is_an_error() {
        let dir = Scratch::new();
        dir.write("sessions", &["not a folder".into()], at(T0));
        let outcome = check(Some(dir.path().to_path_buf()), &mut Cache::default(), at(T0));
        assert_eq!(outcome, Outcome::Failed(UNREADABLE.into()));
    }

    #[test]
    fn rollout_becomes_plan_usage() {
        let dir = Scratch::new();
        let limits = format!(
            r#"{{"limit_id": "codex", "primary": {{"used_percent": 74.0, "window_minutes": 300, "resets_at": {five}}},
                "secondary": {{"used_percent": 40.0, "window_minutes": 10080, "resets_at": {week}}}, "plan_type": "plus"}}"#,
            five = T0 + HOUR,
            week = T0 + 100 * HOUR,
        );
        let premium = r#"{"limit_id": "premium", "primary": null, "secondary": null}"#;
        let lines = [token_count(&stamp(T0), &limits), token_count(&stamp(T0 + 5), premium)];
        dir.write("sessions/2026/09/21/rollout-2026-09-21T14-00-00-a.jsonl", &lines, at(T0 + 5));
        let home = Some(dir.path().to_path_buf());
        let expected = |five_hour| {
            Outcome::Ready(PlanUsage::Ready {
                five_hour,
                weekly: Some(UsageWindow {
                    used: 0.4,
                    resets_at: Some(at(T0 + 100 * HOUR)),
                    estimated: false,
                    minutes: Some(10_080),
                }),
                fetched_at: at(T0),
            })
        };
        let live = UsageWindow { used: 0.74, resets_at: Some(at(T0 + HOUR)), estimated: false, minutes: Some(300) };
        let mut cache = Cache::default();
        assert_eq!(check(home.clone(), &mut cache, at(T0 + 60)), expected(Some(live)));
        let reset = UsageWindow { used: 0.0, resets_at: None, estimated: true, minutes: Some(300) };
        assert_eq!(check(home, &mut cache, at(T0 + 2 * HOUR)), expected(Some(reset)), "cached snapshots still age");
    }
}
