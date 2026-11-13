use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub(super) const T0: u64 = 1_790_000_000;

static NEXT: AtomicUsize = AtomicUsize::new(0);

pub(super) fn at(secs: u64) -> SystemTime {
    UNIX_EPOCH + Duration::from_secs(secs)
}

pub(super) fn stamp(secs: u64) -> String {
    let (days, rest) = ((secs / 86_400) as i64 + 719_468, secs % 86_400);
    let era = days.div_euclid(146_097);
    let doe = days - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.000Z", rest / 3600, rest / 60 % 60, rest % 60)
}

pub(super) fn token_count(stamp: &str, rate_limits: &str) -> String {
    let payload = format!(r#"{{"type":"token_count","info":null,"rate_limits":{rate_limits}}}"#);
    format!(r#"{{"timestamp":"{stamp}","type":"event_msg","payload":{payload}}}"#).replace('\n', " ")
}

pub(super) struct Scratch(PathBuf);

impl Scratch {
    pub(super) fn new() -> Self {
        let id = NEXT.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("notchbase-codex-test-{}-{id}", std::process::id()));
        fs::remove_dir_all(&dir).ok();
        fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }

    pub(super) fn path(&self) -> &Path {
        &self.0
    }

    pub(super) fn write(&self, relative: &str, lines: &[String], modified: SystemTime) -> PathBuf {
        let path = relative.split('/').fold(self.0.clone(), |path, part| path.join(part));
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, lines.iter().map(|line| format!("{line}\n")).collect::<String>()).unwrap();
        File::options().write(true).open(&path).unwrap().set_modified(modified).unwrap();
        path
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).ok();
    }
}

#[test]
fn stamps_match_the_parser() {
    assert_eq!(stamp(T0), "2026-09-21T14:13:20.000Z");
    assert_eq!(stamp(1_709_164_800), "2024-02-29T00:00:00.000Z");
    assert_eq!(stamp(0), "1970-01-01T00:00:00.000Z");
}
