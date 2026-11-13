//! The freeze recorder. The event loop says which step it is in, and a watchdog thread notices when a step runs long
//! or when the loop sleeps far past the time it asked to wake. Either goes to freezes.log in the data folder, naming
//! the step, so a freeze on someone's computer can be traced afterwards. A loop that overslept is also woken.

use std::io::Write as _;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicPtr, AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock, PoisonError};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// A step of the event loop, named in the log.
pub struct Stage(pub &'static str);

pub static IDLE: Stage = Stage("waiting for the next event");
pub static OUTSIDE: Stage = Stage("inside winit, between events");

/// A step that takes this long is written down.
const SLOW: Duration = Duration::from_millis(400);
/// A loop that sleeps this much past its wake time is woken, and that is written down too.
const OVERSLEPT: Duration = Duration::from_millis(300);
const CHECK_EVERY: Duration = Duration::from_millis(100);
const LARGEST_LOG: u64 = 512 * 1024;
const LOG_NAME: &str = "freezes.log";

struct State {
    epoch: Instant,
    stage: AtomicPtr<Stage>,
    /// When the current stage began, in microseconds since `epoch`.
    since: AtomicU64,
    /// When the loop asked to be woken, while it waits.
    due: AtomicU64,
    waiting: AtomicBool,
    detail: Mutex<String>,
}

fn state() -> &'static State {
    static STATE: OnceLock<State> = OnceLock::new();
    STATE.get_or_init(|| State {
        epoch: Instant::now(),
        stage: AtomicPtr::new((&raw const OUTSIDE).cast_mut()),
        since: AtomicU64::new(0),
        due: AtomicU64::new(u64::MAX),
        waiting: AtomicBool::new(false),
        detail: Mutex::new(String::new()),
    })
}

fn micros(at: Instant) -> u64 {
    at.saturating_duration_since(state().epoch).as_micros() as u64
}

fn now() -> u64 {
    micros(Instant::now())
}

/// The loop starts `stage`.
pub fn enter(stage: &'static Stage) {
    let state = state();
    state.stage.store((&raw const *stage).cast_mut(), Ordering::Release);
    state.since.store(now(), Ordering::Release);
}

/// Runs `work` as `stage`, then goes back to `after`.
pub fn during<T>(stage: &'static Stage, after: &'static Stage, work: impl FnOnce() -> T) -> T {
    enter(stage);
    let result = work();
    enter(after);
    result
}

/// Which part of a stage runs, such as the tool being pumped.
pub fn detail(text: &str) {
    let mut detail = state().detail.lock().unwrap_or_else(PoisonError::into_inner);
    detail.clear();
    detail.push_str(text);
}

/// The loop is about to sleep until `due`.
pub fn sleep_until(due: Instant) {
    let state = state();
    state.due.store(micros(due), Ordering::Release);
    state.waiting.store(true, Ordering::Release);
    enter(&IDLE);
}

/// The loop woke up.
pub fn woke() {
    let state = state();
    state.waiting.store(false, Ordering::Release);
    let due = state.due.swap(u64::MAX, Ordering::AcqRel);
    let late = now().saturating_sub(due);
    if late >= OVERSLEPT.as_micros() as u64 {
        log(&format!("the event loop woke {} ms late", late / 1000));
    }
    enter(&OUTSIDE);
}

/// Starts the watchdog. `nudge` wakes the event loop.
pub fn watch(nudge: impl Fn() + Send + 'static) {
    let started = std::thread::Builder::new().name("notchbase freeze recorder".into()).spawn(move || {
        let state = state();
        let mut reported_since = None;
        let mut nudged_for = None;
        loop {
            std::thread::sleep(CHECK_EVERY);
            let now = now();
            let since = state.since.load(Ordering::Acquire);
            if state.waiting.load(Ordering::Acquire) {
                let due = state.due.load(Ordering::Acquire);
                if due != u64::MAX && now > due + OVERSLEPT.as_micros() as u64 && nudged_for != Some(due) {
                    nudged_for = Some(due);
                    nudge();
                }
                continue;
            }
            let running = now.saturating_sub(since);
            if running >= SLOW.as_micros() as u64 && reported_since != Some(since) {
                reported_since = Some(since);
                // SAFETY: the pointer is always one of the `static` stages.
                let stage = unsafe { &*state.stage.load(Ordering::Acquire) };
                let detail = state.detail.lock().unwrap_or_else(PoisonError::into_inner).clone();
                let detail = if detail.is_empty() { String::new() } else { format!(" ({detail})") };
                log(&format!("the event loop has been stuck {} ms in: {}{detail}", running / 1000, stage.0));
            }
        }
    });
    if let Err(err) = started {
        eprintln!("notchbase: no freeze recorder: {err}");
    }
}

fn log_file() -> Option<PathBuf> {
    crate::platform::data_dir().map(|dir| dir.join(LOG_NAME))
}

fn log(what: &str) {
    eprintln!("notchbase: {what}");
    let Some(file) = log_file() else { return };
    let unix = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
    let line = format!("{unix} notchbase {}: {what}\n", env!("CARGO_PKG_VERSION"));
    if std::fs::metadata(&file).is_ok_and(|meta| meta.len() > LARGEST_LOG) {
        let _ = std::fs::remove_file(&file);
    }
    if let Ok(mut out) = std::fs::OpenOptions::new().create(true).append(true).open(&file) {
        let _ = out.write_all(line.as_bytes());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    static TESTING: Stage = Stage("testing");

    #[test]
    fn during_goes_back_after() {
        let seen = during(&TESTING, &OUTSIDE, || {
            // SAFETY: as in `watch`.
            unsafe { &*state().stage.load(Ordering::Acquire) }.0
        });
        assert_eq!(seen, "testing");
        assert_eq!(unsafe { &*state().stage.load(Ordering::Acquire) }.0, OUTSIDE.0);
    }
}
