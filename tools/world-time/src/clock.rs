//! Where the tool reads the time and time zone, so tests can stop the clock.

use jiff::tz::TimeZone;
use notchbase_sdk::now_ms;

pub trait Clock: 'static {
    fn now() -> u64;
    fn zone() -> TimeZone;
}

pub struct SystemClock;

impl Clock for SystemClock {
    fn now() -> u64 {
        now_ms()
    }

    fn zone() -> TimeZone {
        TimeZone::system()
    }
}
