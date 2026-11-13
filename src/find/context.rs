//! When a question is asked: the time, the part of the day, the app in front and the apps running.

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use super::key::Key;

pub type Stamp = u32;

pub const MINUTE: Stamp = 60;
pub const HOUR: Stamp = 60 * MINUTE;
pub const DAY: Stamp = 24 * HOUR;

const DAYS_IN_A_WEEK: i64 = 7;
const THURSDAY: i64 = 3;
const SATURDAY: u8 = 5;
const BUCKET: i64 = 6 * HOUR as i64;
const BUCKETS: u8 = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Daypart(pub u8);

impl Daypart {
    pub const COUNT: usize = 2 * BUCKETS as usize;
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Context {
    pub now: Stamp,
    pub offset_secs: i32,
    pub weekday: u8,
    pub app: Option<Arc<str>>,
    pub running: Arc<[Key]>,
}

impl Context {
    pub fn at(now: Stamp, offset_secs: i32, app: Option<Arc<str>>) -> Self {
        let days = (i64::from(now) + i64::from(offset_secs)).div_euclid(i64::from(DAY));
        let weekday = (days + THURSDAY).rem_euclid(DAYS_IN_A_WEEK) as u8;
        Self { now, offset_secs, weekday, app, running: Arc::default() }
    }

    pub fn among(self, running: Arc<[Key]>) -> Self {
        Self { running, ..self }
    }

    pub fn current(app: Option<Arc<str>>) -> Self {
        let clock = SystemTime::now();
        let offset = crate::platform::utc_offset_secs(crate::platform::local_time(), clock);
        Self::at(stamp(clock), i32::try_from(offset).unwrap_or(0), app)
    }

    pub fn daypart(&self) -> Daypart {
        let bucket = (self.second_of_day() / BUCKET) as u8;
        let weekend = if self.weekday >= SATURDAY { BUCKETS } else { 0 };
        Daypart(bucket.min(BUCKETS - 1) + weekend)
    }

    pub fn day_start(&self) -> Stamp {
        Stamp::try_from(i64::from(self.now) - self.second_of_day()).unwrap_or(0)
    }

    fn second_of_day(&self) -> i64 {
        (i64::from(self.now) + i64::from(self.offset_secs)).rem_euclid(i64::from(DAY))
    }
}

pub fn now() -> Stamp {
    stamp(SystemTime::now())
}

fn stamp(clock: SystemTime) -> Stamp {
    clock.duration_since(UNIX_EPOCH).map_or(0, |since| Stamp::try_from(since.as_secs()).unwrap_or(Stamp::MAX))
}

#[cfg(test)]
mod tests {
    use super::*;

    const MONDAY_NOON_UTC: Stamp = 1_704_110_400;
    const BERLIN: i32 = 3600;
    const NEW_YORK: i32 = -5 * 3600;

    #[test]
    fn week_starts_on_monday() {
        assert_eq!(Context::at(0, 0, None).weekday, 3, "the epoch began on a Thursday");
        assert_eq!(Context::at(MONDAY_NOON_UTC, 0, None).weekday, 0);
        assert_eq!(Context::at(MONDAY_NOON_UTC + 6 * DAY, 0, None).weekday, 6);
        let late_sunday = MONDAY_NOON_UTC - 12 * HOUR - 30 * MINUTE;
        assert_eq!(Context::at(late_sunday, 0, None).weekday, 6);
        assert_eq!(Context::at(late_sunday, BERLIN, None).weekday, 0, "east of Greenwich it is already Monday");
        assert_eq!(Context::at(MONDAY_NOON_UTC - 10 * HOUR, NEW_YORK, None).weekday, 6);
    }

    #[test]
    fn day_has_four_parts() {
        let part = |now: Stamp, offset: i32| Context::at(now, offset, None).daypart();
        assert_eq!(part(MONDAY_NOON_UTC, 0), Daypart(2));
        assert_eq!(part(MONDAY_NOON_UTC - 7 * HOUR, 0), Daypart(0));
        assert_eq!(part(MONDAY_NOON_UTC + 11 * HOUR, 0), Daypart(3));
        assert_eq!(part(MONDAY_NOON_UTC + 5 * DAY, 0), Daypart(6), "Saturday noon");
        assert_eq!(part(MONDAY_NOON_UTC, NEW_YORK), Daypart(1), "seven in the morning in New York");
        let every: Vec<u8> = (0..7 * 24).map(|hour| part(MONDAY_NOON_UTC + hour * HOUR, 0).0).collect();
        assert!(every.iter().all(|part| usize::from(*part) < Daypart::COUNT));
    }

    #[test]
    fn day_starts_at_midnight() {
        let utc = Context::at(MONDAY_NOON_UTC + 90 * MINUTE, 0, None);
        assert_eq!(utc.day_start(), MONDAY_NOON_UTC - 12 * HOUR);
        let berlin = Context::at(MONDAY_NOON_UTC, BERLIN, None);
        assert_eq!(berlin.day_start(), MONDAY_NOON_UTC - 13 * HOUR);
        assert_eq!(Context::at(10, NEW_YORK, None).day_start(), 0, "never before the epoch");
    }

    #[test]
    fn clock_reads_as_stamp() {
        assert_eq!(stamp(UNIX_EPOCH), 0);
        assert_eq!(stamp(UNIX_EPOCH + std::time::Duration::from_secs(90)), 90);
        assert_eq!(stamp(UNIX_EPOCH - std::time::Duration::from_secs(90)), 0);
        assert!(now() > MONDAY_NOON_UTC);
    }
}
