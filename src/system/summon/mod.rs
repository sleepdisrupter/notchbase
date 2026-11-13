//! The summon key that opens Find: Command-Space on macOS, a lone press of the Windows key on Windows. The key tap is
//! installed while the setting is on, and tried again every few seconds when the system refuses it.

#[cfg(any(test, target_os = "macos"))]
mod chord;
#[cfg(any(test, target_os = "windows"))]
mod lone;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
use self::macos as imp;

#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "windows")]
use self::windows as imp;

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
mod other;
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
use self::other as imp;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use self::imp::KeyTap;
use crate::poll::Wake;

const THREAD: &str = "notchbase-summon";
const RESTING: Duration = Duration::from_millis(250);
const RETRY: Duration = Duration::from_secs(3);

pub type Pressed = Arc<dyn Fn() + Send + Sync>;

#[derive(Default)]
struct Shared {
    enabled: AtomicBool,
    blocked: AtomicBool,
    stop: AtomicBool,
}

pub struct Summon {
    shared: Arc<Shared>,
}

impl Summon {
    pub fn start(pressed: Pressed, wake: Wake) -> Self {
        let shared = Arc::new(Shared::default());
        let watching = shared.clone();
        let install = move || KeyTap::install(pressed.clone());
        let _ = std::thread::Builder::new().name(THREAD.into()).spawn(move || watch(&watching, &wake, install));
        Self { shared }
    }

    pub fn set_enabled(&self, on: bool) {
        self.shared.enabled.store(on, Ordering::Relaxed);
    }

    pub fn blocked(&self) -> bool {
        self.shared.enabled.load(Ordering::Relaxed) && self.shared.blocked.load(Ordering::Relaxed)
    }
}

impl Drop for Summon {
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::Relaxed);
    }
}

fn watch<Tap>(shared: &Shared, wake: &Wake, install: impl Fn() -> Result<Tap, String>) {
    let mut tap: Option<Tap> = None;
    let mut retry_at = Instant::now();
    while !shared.stop.load(Ordering::Relaxed) {
        let now = Instant::now();
        let wanted = shared.enabled.load(Ordering::Relaxed);
        if !wanted {
            tap = None;
        } else if tap.is_none() && now >= retry_at {
            retry_at = now + RETRY;
            tap = install().ok();
        }
        let blocked = wanted && tap.is_none();
        if shared.blocked.swap(blocked, Ordering::Relaxed) != blocked {
            wake();
        }
        std::thread::sleep(RESTING);
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicUsize;

    use super::*;

    const SOON: Duration = Duration::from_secs(5);

    fn until(what: impl Fn() -> bool) -> bool {
        let deadline = Instant::now() + SOON;
        while !what() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(5));
        }
        what()
    }

    #[test]
    fn key_taken_only_while_wanted() {
        let shared = Arc::new(Shared::default());
        let (installs, wakes) = (Arc::new(AtomicUsize::new(0)), Arc::new(AtomicUsize::new(0)));
        let allowed = Arc::new(AtomicBool::new(false));
        let (watching, counted, woken, may) = (shared.clone(), installs.clone(), wakes.clone(), allowed.clone());
        let wake: Wake = Arc::new(move || {
            woken.fetch_add(1, Ordering::Relaxed);
        });
        let install = move || {
            counted.fetch_add(1, Ordering::Relaxed);
            if may.load(Ordering::Relaxed) { Ok(()) } else { Err("not allowed".to_owned()) }
        };
        let watcher = std::thread::spawn(move || watch(&watching, &wake, install));
        std::thread::sleep(RESTING / 2);
        assert_eq!(installs.load(Ordering::Relaxed), 0, "nothing is taken while the switch is off");
        let summon = Summon { shared: shared.clone() };
        assert!(!summon.blocked());
        summon.set_enabled(true);
        assert!(until(|| summon.blocked()), "a refused key is reported");
        assert_eq!((installs.load(Ordering::Relaxed), wakes.load(Ordering::Relaxed)), (1, 1));
        summon.set_enabled(false);
        assert!(until(|| wakes.load(Ordering::Relaxed) == 2) && !summon.blocked(), "switched off, nothing is blocked");
        allowed.store(true, Ordering::Relaxed);
        summon.set_enabled(true);
        assert!(until(|| installs.load(Ordering::Relaxed) >= 2));
        std::thread::sleep(RESTING * 2);
        assert!(!summon.blocked(), "the key was taken");
        drop(summon);
        watcher.join().expect("the watcher stops when the service is dropped");
    }
}
