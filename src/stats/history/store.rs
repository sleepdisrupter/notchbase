//! usage-history.jsonl: read once (at most 4 MiB), appended to, and rewritten through a temporary file.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use super::sample::Sample;
use super::series::Series;
use crate::platform;

const FILE: &str = "usage-history.jsonl";
const SWAP: &str = "usage-history.jsonl.tmp";
const MAX_BYTES: u64 = 4 * 1024 * 1024;
pub(super) const MAX_LINES: usize = 2000;
pub(super) const COMPACT_TO: usize = 1500;

pub(super) fn path() -> Option<PathBuf> {
    platform::data_dir().map(|dir| dir.join(FILE))
}

pub(super) fn load(path: &Path, now_ms: u64, compact: bool) -> Series {
    let mut bytes = Vec::new();
    if File::open(path).and_then(|file| file.take(MAX_BYTES).read_to_end(&mut bytes)).is_err() {
        return Series::default();
    }
    let text = String::from_utf8_lossy(&bytes);
    let lines: Vec<&str> = text.lines().filter(|line| !line.trim().is_empty()).collect();
    let mut series = Series::new(lines.iter().filter_map(|line| Sample::parse(line)).collect());
    series.prune(now_ms);
    if series.len() > MAX_LINES {
        series.thin(COMPACT_TO);
    }
    if compact && series.len() != lines.len() {
        rewrite(path, &series).ok();
    }
    series
}

pub(super) fn append(path: &Path, sample: &Sample) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    file.write_all(format!("{}\n", sample.line()).as_bytes())
}

pub(super) fn rewrite(path: &Path, series: &Series) -> io::Result<()> {
    let swap = path.with_file_name(SWAP);
    let text: String = series.samples().iter().map(|sample| format!("{}\n", sample.line())).collect();
    fs::write(&swap, text)?;
    fs::rename(&swap, path)
}

#[cfg(test)]
pub(super) mod scratch {
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicUsize, Ordering};

    static NEXT: AtomicUsize = AtomicUsize::new(0);

    pub(in crate::stats::history) struct Scratch(PathBuf);

    impl Scratch {
        pub(in crate::stats::history) fn new() -> Self {
            let id = NEXT.fetch_add(1, Ordering::Relaxed);
            let dir = std::env::temp_dir().join(format!("notchbase-history-test-{}-{id}", std::process::id()));
            std::fs::remove_dir_all(&dir).ok();
            Self(dir)
        }

        pub(in crate::stats::history) fn file(&self) -> PathBuf {
            self.0.join("nested").join(super::FILE)
        }

        pub(in crate::stats::history) fn dir(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            std::fs::remove_dir_all(&self.0).ok();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::scratch::Scratch;
    use super::*;
    use crate::assistant::Provider;
    use crate::stats::history::series::DAY_MS;

    const NOW: u64 = 1_790_000_000_000;

    fn sample(at_ms: u64, five_hour: f32) -> Sample {
        Sample { at_ms, provider: Provider::Claude, five_hour: Some(five_hour), weekly: None }
    }

    fn lines(path: &Path) -> Vec<String> {
        fs::read_to_string(path).unwrap().lines().map(str::to_owned).collect()
    }

    #[test]
    fn missing_file_is_empty_until_appended() {
        let scratch = Scratch::new();
        let path = scratch.file();
        assert_eq!(load(&path, NOW, true), Series::default());
        assert!(!path.exists(), "loading never creates the file");
        append(&path, &sample(NOW - 1, 0.25)).unwrap();
        append(&path, &sample(NOW, 0.5)).unwrap();
        assert_eq!(lines(&path).len(), 2);
        assert_eq!(load(&path, NOW, true).samples(), [sample(NOW - 1, 0.25), sample(NOW, 0.5)]);
        assert!(scratch.dir().exists());
    }

    #[test]
    fn bad_and_old_lines_are_compacted_away() {
        let scratch = Scratch::new();
        let path = scratch.file();
        let old = sample(NOW - 15 * DAY_MS, 0.9);
        let kept = sample(NOW - DAY_MS, 0.4);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let text = format!("{}\nnot json\n\n{{\"at_ms\":1\n{}\n\u{FFFD}garbage", old.line(), kept.line());
        fs::write(&path, text).unwrap();
        let before = fs::read_to_string(&path).unwrap();
        assert_eq!(load(&path, NOW, false).samples(), [kept]);
        assert_eq!(fs::read_to_string(&path).unwrap(), before, "a read-only load leaves the file alone");
        assert_eq!(load(&path, NOW, true).samples(), [kept]);
        assert_eq!(lines(&path), [kept.line()]);
        assert!(!path.with_file_name(SWAP).exists());
    }

    #[test]
    fn oversized_file_keeps_the_newest() {
        let scratch = Scratch::new();
        let path = scratch.file();
        let samples: Vec<Sample> = (0..MAX_LINES as u64 + 10).map(|i| sample(NOW - 100_000 + i, 0.1)).collect();
        rewrite_all(&path, &samples);
        let series = load(&path, NOW, true);
        assert_eq!(series.len(), COMPACT_TO);
        assert_eq!(series.samples().last(), samples.last());
        assert_eq!(lines(&path).len(), COMPACT_TO);
    }

    fn rewrite_all(path: &Path, samples: &[Sample]) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        rewrite(path, &Series::new(samples.to_vec())).unwrap();
    }
}
