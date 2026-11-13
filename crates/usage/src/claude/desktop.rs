//! Reads the Claude app's plan-usage-history.json. The newest file wins, and reset times are guessed from where the
//! current window started in the samples, so these windows are estimates.

use std::fs::{self, Metadata};
use std::io;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde_json::Value;

use super::json::number;
use super::{PlanUsage, UsageWindow};

const FILE_NAME: &str = "plan-usage-history.json";
const PACKAGE_PREFIX: &str = "Claude_";
const MAX_BYTES: u64 = 8 * 1024 * 1024;
const SCHEMA_VERSION: u64 = 2;
const READ_ATTEMPTS: usize = 3;
const RETRY_PAUSE: Duration = Duration::from_millis(150);
const FIVE_HOURS: Duration = Duration::from_secs(5 * 3600);
const WEEK: Duration = Duration::from_secs(7 * 24 * 3600);
const IDLE_PERCENT: f64 = 0.5;
const RESET_DROP_PERCENT: f64 = 3.0;
const MAX_RESET_GAP_MS: i64 = 20 * 60 * 1000;

#[derive(Clone, Debug, PartialEq)]
pub(super) struct HistoryUsage {
    five_hour: Option<UsageWindow>,
    weekly: Option<UsageWindow>,
    pub(super) at: SystemTime,
}

impl From<HistoryUsage> for PlanUsage {
    fn from(usage: HistoryUsage) -> Self {
        PlanUsage::Ready { five_hour: usage.five_hour, weekly: usage.weekly, fetched_at: usage.at }
    }
}

#[derive(Debug, PartialEq)]
pub(super) enum History {
    Found(HistoryUsage),
    Unreadable,
    Missing,
}

enum Located {
    Found(PathBuf, Metadata),
    Blocked(PathBuf, io::Error),
    Absent,
}

pub(super) struct Cache {
    path: PathBuf,
    modified: Option<SystemTime>,
    len: u64,
    usage: HistoryUsage,
}

impl Cache {
    fn is_current(&self, path: &Path, len: u64, modified: Option<SystemTime>) -> bool {
        self.path == path && self.len == len && self.modified.is_some() && self.modified == modified
    }
}

pub(super) fn load(cache: &mut Option<Cache>) -> History {
    let mut failure = None;
    for attempt in 0..READ_ATTEMPTS {
        if attempt > 0 {
            thread::sleep(RETRY_PAUSE);
        }
        let (path, meta) = match locate() {
            Located::Found(path, meta) => (path, meta),
            Located::Absent if cache.is_none() => return History::Missing,
            // We had it a moment ago, so it's likely being rewritten.
            Located::Absent => continue,
            Located::Blocked(path, err) => {
                failure = Some((path, err.to_string()));
                continue;
            }
        };
        let (len, modified) = (meta.len(), meta.modified().ok());
        if let Some(c) = cache.as_ref().filter(|c| c.is_current(&path, len, modified)) {
            return History::Found(c.usage.clone());
        }
        match read(&path, len) {
            Ok(usage) => {
                *cache = Some(Cache { path, modified, len, usage: usage.clone() });
                return History::Found(usage);
            }
            Err(reason) => failure = Some((path, reason)),
        }
    }
    if let Some((path, reason)) = failure {
        eprintln!("notchbase: couldn't read Claude usage from {}: {reason}", path.display());
    }
    cache.as_ref().map_or(History::Unreadable, |c| History::Found(c.usage.clone()))
}

fn locate() -> Located {
    let mut newest: Option<(PathBuf, Metadata)> = None;
    let mut blocked = None;
    for path in candidates() {
        match fs::metadata(&path) {
            Ok(meta) if meta.is_file() => {
                if newest.as_ref().is_none_or(|(_, best)| modified(&meta) > modified(best)) {
                    newest = Some((path, meta));
                }
            }
            Ok(_) => {}
            Err(err) if err.kind() == io::ErrorKind::NotFound => {}
            Err(err) => blocked = blocked.or(Some((path, err))),
        }
    }
    match (newest, blocked) {
        (Some((path, meta)), _) => Located::Found(path, meta),
        (None, Some((path, err))) => Located::Blocked(path, err),
        (None, None) => Located::Absent,
    }
}

fn modified(meta: &Metadata) -> Option<SystemTime> {
    meta.modified().ok()
}

fn candidates() -> Vec<PathBuf> {
    let env = |name: &str| std::env::var_os(name).filter(|v| !v.is_empty()).map(PathBuf::from);
    let mut dirs = Vec::new();
    if cfg!(windows) {
        dirs.extend(env("APPDATA").map(|d| d.join("Claude")));
        dirs.extend(env("LOCALAPPDATA").map(|d| packaged(&d.join("Packages"))).unwrap_or_default());
    } else if cfg!(target_os = "macos") {
        if let Some(home) = env("HOME") {
            let support = home.join("Library").join("Application Support");
            dirs.push(support.join("Claude"));
            dirs.push(support.join("Claude-3p"));
        }
    } else {
        let config = env("XDG_CONFIG_HOME").or_else(|| env("HOME").map(|h| h.join(".config")));
        dirs.extend(config.map(|d| d.join("Claude")));
    }
    dirs.into_iter().map(|d| d.join(FILE_NAME)).collect()
}

/// Microsoft Store builds keep their AppData inside the package.
fn packaged(packages: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(packages) else { return Vec::new() };
    entries
        .filter_map(Result::ok)
        .filter(|entry| entry.file_name().to_string_lossy().starts_with(PACKAGE_PREFIX))
        .map(|entry| entry.path().join("LocalCache").join("Roaming").join("Claude"))
        .collect()
}

fn read(path: &Path, len: u64) -> Result<HistoryUsage, String> {
    if len > MAX_BYTES {
        return Err(format!("the file is larger than {MAX_BYTES} bytes"));
    }
    let json = fs::read_to_string(path).map_err(|err| err.to_string())?;
    parse(&json).ok_or_else(|| format!("unexpected contents ({} bytes)", json.len()))
}

struct Sample<'a> {
    at_ms: i64,
    org: Option<&'a str>,
    five_hour: Option<f64>,
    weekly: Option<f64>,
}

impl<'a> Sample<'a> {
    fn from_json(sample: &'a Value) -> Option<Self> {
        let at_ms = sample.get("t").and_then(number).filter(|ms| *ms > 0.0)? as i64;
        let usage = sample.get("u");
        let percent = |key: &str| usage.and_then(|u| u.get(key)).and_then(number).filter(|v| *v >= 0.0);
        Some(Self {
            at_ms,
            org: sample.get("org").and_then(Value::as_str),
            five_hour: percent("fh"),
            weekly: percent("sd"),
        })
    }
}

fn parse(json: &str) -> Option<HistoryUsage> {
    let root: Value = serde_json::from_str(json).ok()?;
    if root.get("version").and_then(Value::as_u64) != Some(SCHEMA_VERSION) {
        return None;
    }
    let samples: Vec<Sample> = root.get("samples")?.as_array()?.iter().filter_map(Sample::from_json).collect();
    let newest = samples.iter().max_by_key(|s| s.at_ms)?;
    // Only the newest sample's organization counts, in case you switched accounts.
    let mut mine: Vec<&Sample> = samples.iter().filter(|s| s.org == newest.org).collect();
    mine.sort_by_key(|s| s.at_ms);

    let window = |value: Option<f64>, pick: fn(&Sample) -> Option<f64>, length: Duration| {
        let series: Vec<(i64, f64)> = mine.iter().filter_map(|s| pick(s).map(|v| (s.at_ms, v))).collect();
        value.map(|v| UsageWindow {
            used: (v / 100.0).clamp(0.0, 1.0) as f32,
            resets_at: infer_reset(&series, length),
            estimated: true,
            minutes: Some(length.as_secs() / 60),
        })
    };
    Some(HistoryUsage {
        five_hour: window(newest.five_hour, |s| s.five_hour, FIVE_HOURS),
        weekly: window(newest.weekly, |s| s.weekly, WEEK),
        at: UNIX_EPOCH.checked_add(Duration::from_millis(newest.at_ms as u64))?,
    })
}

/// Finds where the current window began, a drop after a reset or the first use after idling, and adds its length.
fn infer_reset(series: &[(i64, f64)], length: Duration) -> Option<SystemTime> {
    let &(last_t, last_v) = series.last()?;
    if last_v <= IDLE_PERCENT {
        return None;
    }
    let length_ms = length.as_millis() as i64;
    let start = series
        .windows(2)
        .rev()
        .take_while(|pair| last_t - pair[1].0 < length_ms)
        .find(|pair| window_started_between(pair[0], pair[1], length_ms))?;
    let ((t0, _), (t1, _)) = (start[0], start[1]);
    let gap = t1 - t0;
    // A start somewhere in a long gap can't be placed well enough to guess.
    if gap > MAX_RESET_GAP_MS {
        return None;
    }
    // The window began somewhere between the two samples, so take the middle.
    let ends = t0 + gap / 2 + length_ms;
    UNIX_EPOCH.checked_add(Duration::from_millis(u64::try_from(ends).ok()?))
}

fn window_started_between((t0, v0): (i64, f64), (t1, v1): (i64, f64), length_ms: i64) -> bool {
    let halved = v1 <= v0 * 0.5 && v0 - v1 >= 1.0;
    v0 <= IDLE_PERCENT || v0 - v1 >= RESET_DROP_PERCENT || halved || t1 - t0 >= length_ms
}

#[cfg(test)]
mod tests {
    use super::*;

    const MIN: i64 = 60_000;
    const T0: i64 = 1_790_000_000_000;

    fn at(ms: i64) -> SystemTime {
        UNIX_EPOCH + Duration::from_millis(ms as u64)
    }

    fn every_5m(values: &[f64]) -> Vec<(i64, f64)> {
        values.iter().enumerate().map(|(i, &v)| (T0 + 5 * MIN * i as i64, v)).collect()
    }

    #[test]
    fn steady_climb_from_idle() {
        let series = every_5m(&[0.0, 0.0, 0.0, 4.0, 9.0, 15.0, 22.0]);
        let start = T0 + 10 * MIN + 150_000;
        assert_eq!(infer_reset(&series, FIVE_HOURS), Some(at(start + 5 * 60 * MIN)));
    }

    #[test]
    fn reset_mid_series() {
        let series = every_5m(&[10.0, 30.0, 55.0, 70.0, 8.0, 12.0, 20.0]);
        let start = T0 + 15 * MIN + 150_000;
        assert_eq!(infer_reset(&series, FIVE_HOURS), Some(at(start + 5 * 60 * MIN)));
        let series = every_5m(&[40.0, 70.0, 0.0, 0.0, 3.0, 6.0]);
        let start = T0 + 15 * MIN + 150_000;
        assert_eq!(infer_reset(&series, FIVE_HOURS), Some(at(start + 5 * 60 * MIN)));
        let series = every_5m(&[1.0, 2.0, 2.0, 1.0, 1.0]);
        let start = T0 + 10 * MIN + 150_000;
        assert_eq!(infer_reset(&series, WEEK), Some(at(start + 7 * 24 * 60 * MIN)));
    }

    #[test]
    fn rounding_noise_is_not_a_reset() {
        let series = every_5m(&[0.0, 20.0, 31.0, 30.0, 32.0]);
        let start = T0 + 150_000;
        assert_eq!(infer_reset(&series, FIVE_HOURS), Some(at(start + 5 * 60 * MIN)));
    }

    #[test]
    fn no_start_in_samples() {
        assert_eq!(infer_reset(&every_5m(&[40.0, 45.0, 50.0, 70.0]), FIVE_HOURS), None);
        assert_eq!(infer_reset(&every_5m(&[70.0]), FIVE_HOURS), None);
        assert_eq!(infer_reset(&[], FIVE_HOURS), None);
        assert_eq!(infer_reset(&every_5m(&[0.0, 10.0, 0.0]), FIVE_HOURS), None);
        let flat: Vec<f64> = std::iter::once(0.0).chain(std::iter::repeat_n(50.0, 72)).collect();
        assert_eq!(infer_reset(&every_5m(&flat), FIVE_HOURS), None);
    }

    #[test]
    fn gaps() {
        let series = vec![(T0, 0.0), (T0 + 120 * MIN, 30.0), (T0 + 125 * MIN, 35.0)];
        assert_eq!(infer_reset(&series, FIVE_HOURS), None);
        let series = vec![(T0, 0.0), (T0 + 5 * MIN, 70.0), (T0 + 6 * 60 * MIN, 72.0)];
        assert_eq!(infer_reset(&series, FIVE_HOURS), None);
        let series = vec![(T0, 0.0), (T0 + 10 * MIN, 5.0), (T0 + 15 * MIN, 9.0)];
        assert_eq!(infer_reset(&series, FIVE_HOURS), Some(at(T0 + 5 * MIN + 5 * 60 * MIN)));
        let series = vec![(T0, 0.0), (T0 + 5 * MIN, 5.0), (T0 + 90 * MIN, 40.0), (T0 + 95 * MIN, 41.0)];
        assert_eq!(infer_reset(&series, FIVE_HOURS), Some(at(T0 + 150_000 + 5 * 60 * MIN)));
    }

    #[test]
    fn parses_newest_org_only() {
        let json = format!(
            r#"{{"version": 2, "samples": [
                {{"t": {a}, "org": "org-a", "u": {{"fh": 0, "sd": 20}}}},
                {{"t": {b}, "org": "org-a", "u": {{"fh": 40, "sd": 20}}}},
                {{"t": {d}, "org": "org-a", "u": {{"fh": 70, "sd": 21}}}},
                {{"t": {c}, "org": "org-b", "u": {{"fh": 99, "sd": 99}}}},
                {{"t": "bad"}},
                {{"org": "org-a"}}
            ]}}"#,
            a = T0,
            b = T0 + 5 * MIN,
            c = T0 + 7 * MIN,
            d = T0 + 10 * MIN,
        );
        let usage = parse(&json).unwrap();
        assert_eq!(usage.at, at(T0 + 10 * MIN));
        let five = usage.five_hour.unwrap();
        assert!((five.used - 0.70).abs() < 1e-6);
        assert!(five.estimated);
        assert_eq!(five.resets_at, Some(at(T0 + 150_000 + 5 * 60 * MIN)));
        let week = usage.weekly.unwrap();
        assert!((week.used - 0.21).abs() < 1e-6);
        assert_eq!(week.resets_at, None);
    }

    #[test]
    fn packaged_app_files_are_found() {
        let root = std::env::temp_dir().join(format!("notchbase-packages-{}", std::process::id()));
        for name in ["Claude_pzs8sxrjxfjjc", "Claude.Other", "Other_123", "claude_lower"] {
            fs::create_dir_all(root.join(name)).unwrap();
        }
        let found = packaged(&root);
        let _ = fs::remove_dir_all(&root);
        assert_eq!(found, [root.join("Claude_pzs8sxrjxfjjc").join("LocalCache").join("Roaming").join("Claude")]);
        assert!(packaged(&root).is_empty());
    }

    #[test]
    fn parse_is_defensive() {
        assert_eq!(parse(""), None);
        assert_eq!(parse("[]"), None);
        assert_eq!(parse(r#"{"version": 3, "samples": [{"t": 1, "u": {"fh": 1}}]}"#), None);
        assert_eq!(parse(r#"{"samples": [{"t": 1, "u": {"fh": 1}}]}"#), None);
        assert_eq!(parse(r#"{"version": 2, "samples": []}"#), None);
        assert_eq!(parse(r#"{"version": 2}"#), None);
        let usage = parse(r#"{"version": 2, "samples": [{"t": 1790000000000, "u": {"sd": 150}}]}"#).unwrap();
        assert_eq!(usage.five_hour, None);
        assert_eq!(usage.weekly.unwrap().used, 1.0);
        let usage = parse(r#"{"version": 2, "samples": [{"t": 1790000000000}], "extra": true}"#).unwrap();
        assert_eq!((usage.five_hour, usage.weekly), (None, None));
    }
}
