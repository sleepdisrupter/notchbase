//! The notchbase-history thread loads the file once, then appends each sample and compacts it past 2000 lines.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::SystemTime;

use super::sample::Sample;
use super::series::Series;
use super::store::{self, COMPACT_TO, MAX_LINES};
use super::week::unix_ms;
use crate::stats::{Wake, lock};

pub(super) type Loaded = Arc<Mutex<Option<Series>>>;

pub(super) fn spawn(path: Option<PathBuf>, writes: bool, loaded: Loaded, wake: Wake) -> Option<Sender<Sample>> {
    let (samples, received) = mpsc::channel();
    // A read-only history still loads, but its thread never writes.
    let received = writes.then_some(received);
    let spawned = thread::Builder::new()
        .name("notchbase-history".into())
        .spawn(move || run(path.as_deref(), received.as_ref(), &loaded, &wake))
        .is_ok();
    (spawned && writes).then_some(samples)
}

fn run(path: Option<&Path>, received: Option<&Receiver<Sample>>, loaded: &Mutex<Option<Series>>, wake: &Wake) {
    let now = unix_ms(SystemTime::now());
    let mut series = path.map(|path| store::load(path, now, received.is_some())).unwrap_or_default();
    *lock(loaded) = Some(series.clone());
    wake();
    let (Some(path), Some(received)) = (path, received) else { return };
    let mut lines = series.len();
    while let Ok(sample) = received.recv() {
        if record(path, &mut series, sample) {
            lines += 1;
        }
        if lines > MAX_LINES {
            lines = compact(path, &mut series).unwrap_or(lines);
        }
    }
}

fn record(path: &Path, series: &mut Series, sample: Sample) -> bool {
    series.push(sample) && store::append(path, &sample).is_ok()
}

fn compact(path: &Path, series: &mut Series) -> Option<usize> {
    series.prune(unix_ms(SystemTime::now()));
    series.thin(COMPACT_TO);
    store::rewrite(path, series).ok().map(|()| series.len())
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::time::Duration;

    use super::*;
    use crate::assistant::Provider;
    use crate::stats::history::store::scratch::Scratch;

    fn sample(at_ms: u64) -> Sample {
        Sample { at_ms, provider: Provider::Claude, five_hour: Some((at_ms % 997) as f32 / 1000.0), weekly: None }
    }

    fn wait_for(loaded: &Loaded) -> Series {
        for _ in 0..500 {
            if let Some(series) = lock(loaded).take() {
                return series;
            }
            thread::sleep(Duration::from_millis(10));
        }
        panic!("the history never loaded");
    }

    #[test]
    fn loads_once_then_appends_only_news() {
        let scratch = Scratch::new();
        let path = scratch.file();
        let now = unix_ms(SystemTime::now());
        store::append(&path, &sample(now - 1000)).unwrap();
        let loaded = Loaded::default();
        let samples = spawn(Some(path.clone()), true, Arc::clone(&loaded), Arc::new(|| {})).unwrap();
        for at in [now - 2000, now - 1000, now, now + 1] {
            samples.send(sample(at)).unwrap();
        }
        assert_eq!(wait_for(&loaded).samples(), [sample(now - 1000)]);
        drop(samples);
        for _ in 0..500 {
            if fs::read_to_string(&path).is_ok_and(|text| text.lines().count() == 3) {
                break;
            }
            thread::sleep(Duration::from_millis(10));
        }
        let text = fs::read_to_string(&path).unwrap();
        let written: Vec<u64> = text.lines().filter_map(Sample::parse).map(|s| s.at_ms).collect();
        assert_eq!(written, [now - 1000, now, now + 1]);
    }

    #[test]
    fn read_only_history_never_writes() {
        let scratch = Scratch::new();
        let loaded = Loaded::default();
        assert!(spawn(Some(scratch.file()), false, Arc::clone(&loaded), Arc::new(|| {})).is_none());
        assert_eq!(wait_for(&loaded), Series::default());
        assert!(!scratch.dir().exists());
    }

    #[test]
    fn compaction_keeps_the_newest_samples() {
        let scratch = Scratch::new();
        let path = scratch.file();
        let now = unix_ms(SystemTime::now());
        let mut series = Series::default();
        for at in 0..MAX_LINES as u64 + 1 {
            assert!(record(&path, &mut series, sample(now - 10_000 + at)));
        }
        assert_eq!(compact(&path, &mut series), Some(COMPACT_TO));
        assert_eq!(fs::read_to_string(&path).unwrap().lines().count(), COMPACT_TO);
        assert_eq!(series.samples().last(), Some(&sample(now - 10_000 + MAX_LINES as u64)));
    }
}
