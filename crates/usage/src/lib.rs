//! Claude and ChatGPT plan usage: how much of the 5-hour and weekly limits is used, and when each resets. Each reader
//! works on a thread of its own and calls `Wake` when something new is in. Shared by the Stats page and the dock.

pub mod claude;
pub mod codex;

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

pub use claude::{Claude, PlanUsage, USAGE_STALE_AFTER, UsageWindow};
pub use codex::Codex;

/// Called from a reader's thread when it has something new.
pub type Wake = Arc<dyn Fn() + Send + Sync>;

/// Locks a mutex, and still hands it over if a thread panicked holding it.
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Days from 1970-01-01, after Howard Hinnant's days_from_civil.
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (month + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}
