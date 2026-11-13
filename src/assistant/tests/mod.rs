mod board;
mod call;
mod child;
mod flow;
mod runs;
mod waiting;
mod wire;

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use super::Assistant;
use super::server::HoldFor;
use crate::poll::Wake;

const PATIENCE: Duration = Duration::from_secs(10);
const STEP: Duration = Duration::from_millis(10);

pub(in crate::assistant) struct TempDir(PathBuf);

impl TempDir {
    pub(in crate::assistant) fn new() -> Self {
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("notchbase-assistant-test-{}-{nanos}-{n}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }

    pub(in crate::assistant) fn root(&self) -> &Path {
        &self.0
    }

    pub(in crate::assistant) fn path(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

struct Running {
    assistant: Assistant,
    wakes: Arc<AtomicUsize>,
    dir: TempDir,
}

fn running(hold_for: HoldFor) -> Running {
    let dir = TempDir::new();
    let wakes = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&wakes);
    let wake: Wake = Arc::new(move || {
        counter.fetch_add(1, Ordering::SeqCst);
    });
    let assistant = Assistant::start_in(dir.root(), wake, hold_for);
    assert!(assistant.running());
    Running { assistant, wakes, dir }
}

fn eventually<T>(what: &str, mut probe: impl FnMut() -> Option<T>) -> T {
    let deadline = Instant::now() + PATIENCE;
    loop {
        if let Some(found) = probe() {
            return found;
        }
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        thread::sleep(STEP);
    }
}
