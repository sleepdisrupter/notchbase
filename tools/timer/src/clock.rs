//! Where the tool reads the time, so tests can stop the clock.

use notchbase_sdk::now_ms;

pub trait Clock: 'static {
    fn now() -> u64;
}

pub struct SystemClock;

impl Clock for SystemClock {
    fn now() -> u64 {
        now_ms()
    }
}
