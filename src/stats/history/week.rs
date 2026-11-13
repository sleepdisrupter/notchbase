//! A week per assistant, six days ago to today by the local clock: each day's peak and whether it hit a limit.

use std::time::SystemTime;

use super::sample::Sample;
use crate::assistant::Provider;
use crate::platform;

pub const DAYS: usize = 7;
const DAY_SECS: i64 = 86_400;
const AT_LIMIT: f32 = 0.995;
/// Weekday initials from 1970-01-01, a Thursday.
const EPOCH_INITIALS: [char; DAYS] = ['T', 'F', 'S', 'S', 'M', 'T', 'W'];

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Day {
    pub peak: Option<f32>,
    pub hit_limit: bool,
    pub weekday: char,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Week {
    pub days: [Day; DAYS],
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct UsageHistory {
    pub claude: Week,
    pub chatgpt: Week,
}

impl Week {
    pub fn is_empty(&self) -> bool {
        self.days.iter().all(|day| day.peak.is_none())
    }

    pub fn days_at_limit(&self) -> usize {
        self.days.iter().filter(|day| day.hit_limit).count()
    }

    /// A week ending today from fixed peaks, for previews and tests.
    pub fn ending_today(peaks: [Option<f32>; DAYS]) -> Week {
        let now = SystemTime::now();
        let today = local_day(unix_ms(now), platform::utc_offset_secs(platform::local_time(), now));
        Week {
            days: std::array::from_fn(|i| {
                let peak = peaks[i].map(|peak| peak.clamp(0.0, 1.0));
                Day { peak, hit_limit: peak.is_some_and(at_limit), weekday: initial(first_day(today) + i as i64) }
            }),
        }
    }

    pub(super) fn of(samples: &[Sample], provider: Provider, now_ms: u64, offset: i64) -> Week {
        let first = first_day(local_day(now_ms, offset));
        let mut days: [Day; DAYS] =
            std::array::from_fn(|i| Day { weekday: initial(first + i as i64), ..Day::default() });
        for sample in samples.iter().filter(|sample| sample.provider == provider) {
            let Ok(slot) = usize::try_from(local_day(sample.at_ms, offset) - first) else { continue };
            let Some(day) = days.get_mut(slot) else { continue };
            if let Some(peak) = sample.peak() {
                day.peak = Some(day.peak.map_or(peak, |shown| shown.max(peak)));
            }
            day.hit_limit |= [sample.five_hour, sample.weekly].into_iter().flatten().any(at_limit);
        }
        Week { days }
    }
}

impl UsageHistory {
    pub fn both(week: Week) -> Self {
        Self { claude: week, chatgpt: week }
    }

    pub fn week(&self, provider: Provider) -> &Week {
        match provider {
            Provider::Claude => &self.claude,
            Provider::Codex => &self.chatgpt,
        }
    }

    pub(super) fn of(samples: &[Sample], now_ms: u64, offset: i64) -> Self {
        Self {
            claude: Week::of(samples, Provider::Claude, now_ms, offset),
            chatgpt: Week::of(samples, Provider::Codex, now_ms, offset),
        }
    }
}

pub(super) fn next_midnight_ms(now_ms: u64, offset: i64) -> u64 {
    let midnight = (local_day(now_ms, offset) + 1) * DAY_SECS - offset;
    u64::try_from(midnight).unwrap_or(0).saturating_mul(1000)
}

pub(super) fn unix_ms(at: SystemTime) -> u64 {
    at.duration_since(SystemTime::UNIX_EPOCH).map_or(0, |since| u64::try_from(since.as_millis()).unwrap_or(u64::MAX))
}

fn local_day(ms: u64, offset: i64) -> i64 {
    (i64::try_from(ms / 1000).unwrap_or(i64::MAX) + offset).div_euclid(DAY_SECS)
}

fn first_day(today: i64) -> i64 {
    today - (DAYS as i64 - 1)
}

fn initial(day: i64) -> char {
    EPOCH_INITIALS[day.rem_euclid(DAYS as i64) as usize]
}

fn at_limit(used: f32) -> bool {
    used >= AT_LIMIT
}

#[cfg(test)]
mod tests {
    use super::*;

    const HOUR_MS: u64 = 3_600_000;
    const MONDAY_NOON_UTC: u64 = 1_789_992_000_000;

    fn sample(provider: Provider, at_ms: u64, five_hour: Option<f32>, weekly: Option<f32>) -> Sample {
        Sample { at_ms, provider, five_hour, weekly }
    }

    fn peaks(week: &Week) -> Vec<Option<f32>> {
        week.days.iter().map(|day| day.peak).collect()
    }

    #[test]
    fn days_run_six_days_ago_to_today() {
        let week = Week::of(&[], Provider::Claude, MONDAY_NOON_UTC, 0);
        let initials: String = week.days.iter().map(|day| day.weekday).collect();
        assert_eq!(initials, "TWTFSSM");
        assert!(week.is_empty());
        assert_eq!(week.days_at_limit(), 0);
    }

    #[test]
    fn each_day_shows_its_peak() {
        let samples = [
            sample(Provider::Claude, MONDAY_NOON_UTC - HOUR_MS, Some(0.2), Some(0.1)),
            sample(Provider::Claude, MONDAY_NOON_UTC, Some(0.6), Some(0.1)),
            sample(Provider::Claude, MONDAY_NOON_UTC + HOUR_MS, Some(0.1), Some(0.2)),
            sample(Provider::Claude, MONDAY_NOON_UTC - 2 * 24 * HOUR_MS, None, Some(0.4)),
            sample(Provider::Claude, MONDAY_NOON_UTC - 7 * 24 * HOUR_MS, Some(0.9), None),
            sample(Provider::Claude, MONDAY_NOON_UTC + 24 * HOUR_MS, Some(0.9), None),
            sample(Provider::Codex, MONDAY_NOON_UTC, Some(0.8), None),
        ];
        let week = Week::of(&samples, Provider::Claude, MONDAY_NOON_UTC, 0);
        assert_eq!(peaks(&week), [None, None, None, None, Some(0.4), None, Some(0.6)]);
        let history = UsageHistory::of(&samples, MONDAY_NOON_UTC, 0);
        assert_eq!(history.week(Provider::Claude), &week);
        assert_eq!(peaks(history.week(Provider::Codex))[6], Some(0.8));
    }

    #[test]
    fn hitting_either_limit_marks_the_day() {
        let samples = [
            sample(Provider::Claude, MONDAY_NOON_UTC, Some(1.0), None),
            sample(Provider::Claude, MONDAY_NOON_UTC - 24 * HOUR_MS, Some(0.3), Some(1.0)),
            sample(Provider::Claude, MONDAY_NOON_UTC - 48 * HOUR_MS, Some(0.99), Some(0.5)),
        ];
        let week = Week::of(&samples, Provider::Claude, MONDAY_NOON_UTC, 0);
        let hits: Vec<bool> = week.days.iter().map(|day| day.hit_limit).collect();
        assert_eq!(hits, [false, false, false, false, false, true, true]);
        assert_eq!(week.days_at_limit(), 2);
    }

    #[test]
    fn days_follow_the_local_clock() {
        let late_sunday_in_new_york = MONDAY_NOON_UTC - 10 * HOUR_MS;
        let samples = [sample(Provider::Claude, late_sunday_in_new_york, Some(0.5), None)];
        let utc = Week::of(&samples, Provider::Claude, MONDAY_NOON_UTC, 0);
        let new_york = Week::of(&samples, Provider::Claude, MONDAY_NOON_UTC, -4 * 3600);
        assert_eq!(peaks(&utc)[6], Some(0.5));
        assert_eq!(peaks(&new_york)[5], Some(0.5));
        assert_eq!(new_york.days[5].weekday, 'S');
    }

    #[test]
    fn next_midnight_is_local() {
        let day_ms = 24 * HOUR_MS;
        let utc_midnight = MONDAY_NOON_UTC + 12 * HOUR_MS;
        assert_eq!(next_midnight_ms(MONDAY_NOON_UTC, 0), utc_midnight);
        assert_eq!(next_midnight_ms(MONDAY_NOON_UTC, -4 * 3600), utc_midnight + 4 * HOUR_MS);
        assert_eq!(next_midnight_ms(utc_midnight, 0), utc_midnight + day_ms);
    }

    #[test]
    fn sample_week_ends_today() {
        let week = Week::ending_today([Some(0.2), None, Some(1.0), Some(1.4), None, None, Some(0.5)]);
        assert_eq!(peaks(&week), [Some(0.2), None, Some(1.0), Some(1.0), None, None, Some(0.5)]);
        assert_eq!(week.days_at_limit(), 2);
        assert!(week.days.iter().all(|day| EPOCH_INITIALS.contains(&day.weekday)));
    }
}
