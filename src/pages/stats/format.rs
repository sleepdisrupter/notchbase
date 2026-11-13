//! How the numbers read: bytes, ages, reset times, plan windows and GPU names.

use std::time::{Duration, SystemTime};

use crate::platform::{self, LocalTime};
use crate::stats::UsageWindow;

const VENDORS: [&str; 3] = ["nvidia", "amd", "intel"];
const HOUR_MINUTES: u64 = 60;
const DAY_MINUTES: u64 = 24 * HOUR_MINUTES;
const DAY_SECS: i64 = 86_400;
const WEEK: Duration = Duration::from_secs(7 * DAY_SECS as u64);
const EPOCH_WEEKDAYS: [&str; 7] = ["Thu", "Fri", "Sat", "Sun", "Mon", "Tue", "Wed"];
const BYTE_UNITS: [&str; 6] = ["bytes", "KB", "MB", "GB", "TB", "PB"];

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Wall {
    offset: i64,
    full_day: bool,
}

impl Wall {
    pub(super) fn here(now: SystemTime) -> Wall {
        Wall::of(platform::local_time(), now, platform::clock_24h())
    }

    fn of(local: LocalTime, now: SystemTime, full_day: bool) -> Wall {
        Wall { offset: platform::utc_offset_secs(local, now), full_day }
    }

    fn weekday_time(self, at: SystemTime) -> String {
        let local = unix_secs(at) + self.offset;
        let weekday = EPOCH_WEEKDAYS[local.div_euclid(DAY_SECS).rem_euclid(7) as usize];
        let of_day = local.rem_euclid(DAY_SECS);
        let (hour, minute) = (of_day / 3600, of_day % 3600 / 60);
        if self.full_day {
            return format!("{weekday} {hour}:{minute:02}");
        }
        let half = if hour < 12 { "AM" } else { "PM" };
        format!("{weekday} {}:{minute:02} {half}", (hour + 11) % 12 + 1)
    }
}

pub(super) fn usage_detail(window: &UsageWindow, now: SystemTime, wall: Wall) -> String {
    let percent = (window.used.clamp(0.0, 1.0) * 100.0).round() as i32;
    match window.resets_at {
        Some(at) => match at.duration_since(now) {
            Ok(left) if left < WEEK => format!("{percent}% used · resets {}", wall.weekday_time(at)),
            Ok(left) => format!("{percent}% used · resets in {}", format_resets_in(left)),
            Err(_) => format!("{percent}% used · resets now"),
        },
        None => format!("{percent}% used"),
    }
}

fn unix_secs(at: SystemTime) -> i64 {
    match at.duration_since(SystemTime::UNIX_EPOCH) {
        Ok(after) => after.as_secs() as i64,
        Err(before) => -(before.duration().as_secs() as i64),
    }
}

pub(super) fn window_label(minutes: Option<u64>, fallback: &str) -> String {
    match minutes {
        Some(DAY_MINUTES) => "Daily".to_owned(),
        Some(m) if m == 7 * DAY_MINUTES => "Weekly".to_owned(),
        Some(m) if (28 * DAY_MINUTES..=31 * DAY_MINUTES).contains(&m) => "Monthly".to_owned(),
        Some(m) if m > 0 && m % DAY_MINUTES == 0 => format!("{}-day", m / DAY_MINUTES),
        Some(m) if m > 0 && m % HOUR_MINUTES == 0 => format!("{}-hour", m / HOUR_MINUTES),
        Some(m) if m > 0 => format!("{m}-minute"),
        _ => fallback.to_owned(),
    }
}

fn format_resets_in(left: Duration) -> String {
    let secs = left.as_secs();
    let (days, hours, minutes) = (secs / 86_400, secs % 86_400 / 3600, secs % 3600 / 60);
    match (days, hours, minutes) {
        (0, 0, 0) => "<1m".to_owned(),
        (0, 0, m) => format!("{m}m"),
        (0, h, 0) => format!("{h}h"),
        (0, h, m) => format!("{h}h {m}m"),
        (d, 0, _) => format!("{d}d"),
        (d, h, _) => format!("{d}d {h}h"),
    }
}

pub(super) fn format_ago(age: Duration) -> String {
    let secs = age.as_secs();
    match secs {
        0..60 => "just now".to_owned(),
        60..3600 => format!("{}m ago", secs / 60),
        3600..86_400 => format!("{}h ago", secs / 3600),
        _ => format!("{}d ago", secs / 86_400),
    }
}

pub(super) fn memory_label(used: u64, total: u64) -> String {
    const GIB: f64 = 1024.0 * 1024.0 * 1024.0;
    let (used, total) = (used as f64 / GIB, total as f64 / GIB);
    let total = if total >= 10.0 { format!("{}", total.round()) } else { format!("{total:.1}") };
    format!("{used:.1} / {total} GB")
}

pub(super) fn format_bytes(bytes: u64) -> String {
    format_bytes_in(bytes, if cfg!(windows) { 1024.0 } else { 1000.0 })
}

fn format_bytes_in(bytes: u64, unit: f64) -> String {
    let mut value = bytes as f64;
    let mut i = 0;
    while value >= unit && i + 1 < BYTE_UNITS.len() {
        value /= unit;
        i += 1;
    }
    if i == 0 {
        return format!("{bytes} bytes");
    }
    if value < 9.95 {
        let text = format!("{value:.1}");
        let text = text.strip_suffix(".0").unwrap_or(&text);
        format!("{text} {}", BYTE_UNITS[i])
    } else {
        format!("{} {}", value.round() as u64, BYTE_UNITS[i])
    }
}

pub(super) fn share_label(share: f64) -> String {
    let percent = share.clamp(0.0, 1.0) * 100.0;
    if percent < 0.05 {
        "0%".to_owned()
    } else if percent < 9.95 {
        let text = format!("{percent:.1}");
        format!("{}%", text.strip_suffix(".0").unwrap_or(&text))
    } else {
        format!("{}%", percent.round() as u32)
    }
}

pub(super) fn short_gpu_name(name: &str) -> String {
    let cleaned = name.replace("(R)", "").replace("(TM)", "").replace("(tm)", "");
    let mut words: Vec<&str> = cleaned.split_whitespace().collect();
    if words.len() > 1 && VENDORS.contains(&words[0].to_ascii_lowercase().as_str()) {
        words.remove(0);
    }
    words.join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    const GB: u64 = 1_000_000_000;
    const GIB: u64 = 1 << 30;

    #[test]
    fn bytes() {
        assert_eq!(memory_label(10_522_669_875, 17_179_869_184), "9.8 / 16 GB");
        assert_eq!(memory_label(3 * GIB, 8 * GIB), "3.0 / 8.0 GB");
        assert_eq!(format_bytes_in(212 * GB, 1000.0), "212 GB");
        assert_eq!(format_bytes_in(212 * GIB, 1024.0), "212 GB");
        assert_eq!(format_bytes_in(212 * GIB + GIB / 3, 1024.0), "212 GB");
        assert_eq!(format_bytes_in(1_500 * GB, 1000.0), "1.5 TB");
        assert_eq!(format_bytes_in(1_000 * GB, 1000.0), "1 TB");
        assert_eq!(format_bytes_in(9_960_000_000, 1000.0), "10 GB");
        assert_eq!(format_bytes_in(9_940_000_000, 1000.0), "9.9 GB");
        assert_eq!(format_bytes_in(512, 1000.0), "512 bytes");
        assert_eq!(format_bytes_in(0, 1024.0), "0 bytes");
        assert_eq!(format_bytes_in(1536, 1024.0), "1.5 KB");
        assert_eq!(format_bytes_in(u64::MAX, 1000.0), "18447 PB");
    }

    #[test]
    fn resets_in() {
        let m = |m: u64| Duration::from_secs(m * 60);
        assert_eq!(format_resets_in(Duration::from_secs(0)), "<1m");
        assert_eq!(format_resets_in(Duration::from_secs(59)), "<1m");
        assert_eq!(format_resets_in(m(14)), "14m");
        assert_eq!(format_resets_in(m(2 * 60 + 14) + Duration::from_secs(40)), "2h 14m");
        assert_eq!(format_resets_in(m(3 * 60)), "3h");
        assert_eq!(format_resets_in(m((3 * 24 + 4) * 60 + 59)), "3d 4h");
        assert_eq!(format_resets_in(m(2 * 24 * 60 + 30)), "2d");
    }

    #[test]
    fn window_labels_name_length() {
        assert_eq!(window_label(Some(300), "x"), "5-hour");
        assert_eq!(window_label(Some(10_080), "x"), "Weekly");
        assert_eq!(window_label(Some(43_200), "x"), "Monthly");
        assert_eq!(window_label(Some(1_440), "x"), "Daily");
        assert_eq!(window_label(Some(2_880), "x"), "2-day");
        assert_eq!(window_label(Some(90), "x"), "90-minute");
        assert_eq!(window_label(None, "Weekly"), "Weekly");
        assert_eq!(window_label(Some(0), "5-hour"), "5-hour");
    }

    #[test]
    fn usage_details() {
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000);
        let wall = Wall { offset: 0, full_day: true };
        let window = |used: f32, left: Option<i64>| UsageWindow {
            used,
            resets_at: left.map(|s| {
                if s >= 0 { now + Duration::from_secs(s as u64) } else { now - Duration::from_secs(s.unsigned_abs()) }
            }),
            estimated: false,
            minutes: None,
        };
        assert_eq!(usage_detail(&window(0.23, Some(2 * 3600 + 14 * 60)), now, wall), "23% used · resets Mon 16:00");
        assert_eq!(usage_detail(&window(0.414, Some((3 * 24 + 4) * 3600)), now, wall), "41% used · resets Thu 17:46");
        assert_eq!(usage_detail(&window(0.5, Some(8 * 24 * 3600)), now, wall), "50% used · resets in 8d");
        assert_eq!(usage_detail(&window(1.0, None), now, wall), "100% used");
        assert_eq!(usage_detail(&window(0.5, Some(-5)), now, wall), "50% used · resets now");
        let estimated = |w: UsageWindow| UsageWindow { estimated: true, ..w };
        assert_eq!(
            usage_detail(&estimated(window(0.7, Some(2 * 3600 + 10 * 60))), now, wall),
            "70% used · resets Mon 15:56"
        );
        assert_eq!(usage_detail(&estimated(window(0.7, Some(-5))), now, wall), "70% used · resets now");
        assert_eq!(usage_detail(&estimated(window(0.21, None)), now, wall), "21% used");
    }

    #[test]
    fn reset_reads_local_weekday() {
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1_790_000_000);
        let local = LocalTime { year: 2026, month: 9, day: 21, hour: 10, minute: 13, second: 20 };
        let wall = Wall::of(local, now, false);
        assert_eq!(wall.offset, -4 * 3600);
        let thursday = now + Duration::from_secs(3 * 86_400 - 43 * 60 - 20);
        assert_eq!(wall.weekday_time(thursday), "Thu 9:30 AM");
        assert_eq!(Wall { full_day: true, ..wall }.weekday_time(thursday), "Thu 9:30");
        assert_eq!(wall.weekday_time(now + Duration::from_secs(14 * 3600 - 13 * 60 - 20)), "Tue 12:00 AM");
        assert_eq!(Wall::of(local, now + Duration::from_millis(900), true).offset, wall.offset);
    }

    #[test]
    fn ages() {
        assert_eq!(format_ago(Duration::from_secs(5)), "just now");
        assert_eq!(format_ago(Duration::from_secs(150)), "2m ago");
        assert_eq!(format_ago(Duration::from_secs(3 * 3600 + 5)), "3h ago");
        assert_eq!(format_ago(Duration::from_secs(49 * 3600)), "2d ago");
    }

    #[test]
    fn gpu_names() {
        assert_eq!(short_gpu_name("Intel(R) Iris(R) Xe Graphics"), "Iris Xe Graphics");
        assert_eq!(short_gpu_name("NVIDIA GeForce RTX 4070"), "GeForce RTX 4070");
        assert_eq!(short_gpu_name("Apple M4 GPU"), "Apple M4 GPU");
        assert_eq!(short_gpu_name("AMD"), "AMD");
    }
}
