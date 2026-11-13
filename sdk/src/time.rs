//! Time helpers that also work in portable builds, where the host keeps the clock.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub fn unix_ms(time: SystemTime) -> u64 {
    time.duration_since(UNIX_EPOCH).unwrap_or_default().as_millis().min(u128::from(u64::MAX)) as u64
}

#[cfg(not(target_arch = "wasm32"))]
pub fn now_ms() -> u64 {
    unix_ms(SystemTime::now())
}

#[cfg(target_arch = "wasm32")]
pub fn now_ms() -> u64 {
    #[link(wasm_import_module = "notchbase")]
    unsafe extern "C" {
        fn now_ms() -> u64;
    }
    unsafe { now_ms() }
}

/// Formats a duration like a stopwatch: `1:02`, or `3:00:05` past an hour.
pub fn clock(duration: Duration) -> String {
    let total = duration.as_secs();
    let (hours, minutes, seconds) = (total / 3600, total / 60 % 60, total % 60);
    if hours > 0 { format!("{hours}:{minutes:02}:{seconds:02}") } else { format!("{minutes}:{seconds:02}") }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clocks_read_like_a_stopwatch() {
        assert_eq!(clock(Duration::from_secs(0)), "0:00");
        assert_eq!(clock(Duration::from_secs(62)), "1:02");
        assert_eq!(clock(Duration::from_secs(3 * 3600 + 5)), "3:00:05");
    }
}
