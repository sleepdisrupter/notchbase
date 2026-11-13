//! Places the two windows in the 5-hour and weekly slots by length, and shows one that has already reset as 0%.

use std::time::{Duration, SystemTime};

use super::snapshot::{Snapshot, Window};
use super::{PlanUsage, UsageWindow};

const SLOT_MINUTES: [u64; 2] = [5 * 60, 7 * 24 * 60];

pub(super) fn usage(snapshot: &Snapshot, now: SystemTime) -> PlanUsage {
    let [five_hour, weekly] = place([snapshot.primary.as_ref(), snapshot.secondary.as_ref()])
        .map(|window| window.map(|window| shown(window, snapshot.at, now)));
    PlanUsage::Ready { five_hour, weekly, fetched_at: snapshot.at }
}

fn place(windows: [Option<&Window>; 2]) -> [Option<&Window>; 2] {
    // Both want the same slot, so the shorter one takes the 5-hour slot.
    if let [Some(a), Some(b)] = windows
        && slot(a, 0) == slot(b, 1)
    {
        return if length(a, 0) <= length(b, 1) { [Some(a), Some(b)] } else { [Some(b), Some(a)] };
    }
    let mut slots = [None, None];
    for (position, window) in windows.into_iter().enumerate() {
        if let Some(window) = window {
            slots[slot(window, position)] = Some(window);
        }
    }
    slots
}

/// Compares on a log scale: shorter than the geometric mean of 5 hours and a week goes in the 5-hour slot.
fn slot(window: &Window, position: usize) -> usize {
    let [short, long] = SLOT_MINUTES;
    window.minutes.map_or(position, |minutes| usize::from(minutes.saturating_mul(minutes) > short * long))
}

fn length(window: &Window, position: usize) -> u64 {
    window.minutes.unwrap_or(SLOT_MINUTES[position])
}

fn shown(window: &Window, taken: SystemTime, now: SystemTime) -> UsageWindow {
    if has_reset(window, taken, now) {
        return UsageWindow { used: 0.0, resets_at: None, estimated: true, minutes: window.minutes };
    }
    let used = (window.percent / 100.0).clamp(0.0, 1.0) as f32;
    UsageWindow { used, resets_at: window.resets_at, estimated: false, minutes: window.minutes }
}

/// Without a reset time, a window ends one length after the snapshot.
fn has_reset(window: &Window, taken: SystemTime, now: SystemTime) -> bool {
    match (window.resets_at, window.minutes) {
        (Some(resets_at), _) => resets_at <= now,
        (None, Some(minutes)) => {
            taken.checked_add(Duration::from_secs(minutes.saturating_mul(60))).is_some_and(|ends| ends <= now)
        }
        (None, None) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codex::fixture::{T0, at};

    const HOUR: u64 = 3600;

    fn limit(percent: f64, minutes: Option<u64>, resets_at: Option<u64>) -> Option<Window> {
        Some(Window { percent, minutes, resets_at: resets_at.map(at) })
    }

    fn live(used: f32, resets_at: u64) -> Option<UsageWindow> {
        Some(UsageWindow { used, resets_at: Some(at(resets_at)), estimated: false, minutes: None })
    }

    fn reset() -> Option<UsageWindow> {
        Some(UsageWindow { used: 0.0, resets_at: None, estimated: true, minutes: None })
    }

    fn slots_at(primary: Option<Window>, secondary: Option<Window>, now: u64) -> [Option<UsageWindow>; 2] {
        match usage(&Snapshot { at: at(T0), primary, secondary }, at(now)) {
            PlanUsage::Ready { five_hour, weekly, fetched_at } => {
                assert_eq!(fetched_at, at(T0));
                [five_hour, weekly].map(|window| window.map(|window| UsageWindow { minutes: None, ..window }))
            }
            other => panic!("expected numbers, got {other:?}"),
        }
    }

    #[test]
    fn five_hour_and_weekly_windows() {
        let (primary, secondary) =
            (limit(74.0, Some(300), Some(T0 + HOUR)), limit(12.5, Some(10080), Some(T0 + 99 * HOUR)));
        assert_eq!(slots_at(primary, secondary, T0 + 60), [live(0.74, T0 + HOUR), live(0.125, T0 + 99 * HOUR)]);
    }

    #[test]
    fn weekly_only_leaves_five_hour_empty() {
        let weekly = limit(43.0, Some(10080), Some(T0 + 50 * HOUR));
        assert_eq!(slots_at(weekly.clone(), None, T0), [None, live(0.43, T0 + 50 * HOUR)]);
        assert_eq!(slots_at(None, weekly, T0), [None, live(0.43, T0 + 50 * HOUR)]);
        let five = limit(10.0, Some(300), Some(T0 + HOUR));
        assert_eq!(slots_at(None, five, T0), [live(0.1, T0 + HOUR), None]);
    }

    #[test]
    fn other_lengths_go_to_the_closest_slot() {
        let monthly = limit(5.0, Some(43_200), Some(T0 + 400 * HOUR));
        assert_eq!(slots_at(monthly, None, T0), [None, live(0.05, T0 + 400 * HOUR)]);
        let daily = limit(5.0, Some(1440), Some(T0 + 20 * HOUR));
        assert_eq!(slots_at(None, daily, T0), [live(0.05, T0 + 20 * HOUR), None]);
        let two_days = limit(5.0, Some(2880), Some(T0 + 40 * HOUR));
        assert_eq!(slots_at(two_days, None, T0), [None, live(0.05, T0 + 40 * HOUR)]);
        let rounded = [limit(1.0, Some(299), Some(T0 + HOUR)), limit(2.0, Some(10079), Some(T0 + HOUR))];
        assert_eq!(
            slots_at(rounded[0].clone(), rounded[1].clone(), T0),
            [live(0.01, T0 + HOUR), live(0.02, T0 + HOUR)]
        );
    }

    #[test]
    fn shared_slot_orders_by_length() {
        let (hourly, daily) = (limit(1.0, Some(60), Some(T0 + HOUR)), limit(2.0, Some(1440), Some(T0 + 2 * HOUR)));
        let expected = [live(0.01, T0 + HOUR), live(0.02, T0 + 2 * HOUR)];
        assert_eq!(slots_at(daily.clone(), hourly.clone(), T0), expected);
        assert_eq!(slots_at(hourly, daily, T0), expected);
    }

    #[test]
    fn unknown_lengths_keep_their_position() {
        let (a, b) = (limit(1.0, None, Some(T0 + HOUR)), limit(2.0, None, Some(T0 + 2 * HOUR)));
        assert_eq!(slots_at(a.clone(), b.clone(), T0), [live(0.01, T0 + HOUR), live(0.02, T0 + 2 * HOUR)]);
        assert_eq!(slots_at(None, b, T0), [None, live(0.02, T0 + 2 * HOUR)]);
        assert_eq!(slots_at(a, None, T0), [live(0.01, T0 + HOUR), None]);
    }

    #[test]
    fn windows_that_already_reset_show_zero() {
        let (five, week) = (limit(90.0, Some(300), Some(T0 + HOUR)), limit(60.0, Some(10080), Some(T0 + 99 * HOUR)));
        assert_eq!(slots_at(five.clone(), week.clone(), T0 + HOUR), [reset(), live(0.6, T0 + 99 * HOUR)]);
        assert_eq!(slots_at(five, week, T0 + 100 * HOUR), [reset(), reset()]);
    }

    #[test]
    fn no_reset_time_ends_one_length_later() {
        let no_reset = limit(30.0, Some(300), None);
        let waiting = Some(UsageWindow { used: 0.3, resets_at: None, estimated: false, minutes: None });
        assert_eq!(slots_at(no_reset.clone(), None, T0 + 5 * HOUR - 1), [waiting, None]);
        assert_eq!(slots_at(no_reset, None, T0 + 5 * HOUR), [reset(), None]);
        let unknown = limit(30.0, None, None);
        assert_eq!(slots_at(unknown, None, T0 + 1000 * HOUR)[0].as_ref().map(|w| w.used), Some(0.3));
    }

    #[test]
    fn percentages_are_clamped() {
        let (over, under) = (limit(130.0, Some(300), Some(T0 + HOUR)), limit(-4.0, Some(10080), Some(T0 + HOUR)));
        assert_eq!(slots_at(over, under, T0), [live(1.0, T0 + HOUR), live(0.0, T0 + HOUR)]);
    }
}
