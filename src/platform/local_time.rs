//! The local date and time, the offset from UTC, and whether the clock shows 24 hours.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LocalTime {
    pub year: i32,
    pub month: u32,
    pub day: u32,
    pub hour: u32,
    pub minute: u32,
    pub second: u32,
}

const DAY_SECS: i64 = 86_400;

pub fn utc_offset_secs(local: LocalTime, now: std::time::SystemTime) -> i64 {
    let local_secs = days_from_civil(local.year.into(), local.month.into(), local.day.into()) * DAY_SECS
        + i64::from(local.hour * 3600 + local.minute * 60 + local.second);
    let now_secs = match now.duration_since(std::time::UNIX_EPOCH) {
        Ok(after) => after.as_secs() as i64,
        Err(before) => -(before.duration().as_secs() as i64),
    };
    ((local_secs - now_secs) as f64 / 60.0).round() as i64 * 60
}

/// Days from 1970-01-01, after Howard Hinnant's days_from_civil.
pub fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (month + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

pub fn clock_24h() -> bool {
    static CHOSEN: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *CHOSEN.get_or_init(prefers_24h)
}

#[cfg(windows)]
fn prefers_24h() -> bool {
    use windows_sys::Win32::Globalization::{GetLocaleInfoEx, LOCALE_ITIME};

    let mut out = [0u16; 4];
    let written = unsafe { GetLocaleInfoEx(std::ptr::null(), LOCALE_ITIME, out.as_mut_ptr(), out.len() as i32) };
    written > 0 && out[0] == u16::from(b'1')
}

#[cfg(target_os = "macos")]
fn prefers_24h() -> bool {
    std::process::Command::new("/usr/bin/defaults")
        .args(["read", "-g", "AppleICUForce24HourTime"])
        .output()
        .is_ok_and(|out| String::from_utf8_lossy(&out.stdout).trim() == "1")
}

#[cfg(not(any(windows, target_os = "macos")))]
fn prefers_24h() -> bool {
    true
}

#[cfg(not(windows))]
pub fn local_time() -> LocalTime {
    use std::ffi::{c_char, c_int, c_long};

    #[repr(C)]
    struct Tm {
        second: c_int,
        minute: c_int,
        hour: c_int,
        day: c_int,
        month: c_int,
        year: c_int,
        weekday: c_int,
        yearday: c_int,
        dst: c_int,
        offset: c_long,
        zone: *const c_char,
    }

    unsafe extern "C" {
        fn localtime_r(time: *const i64, out: *mut Tm) -> *mut Tm;
    }

    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64);
    let mut tm = Tm {
        second: 0,
        minute: 0,
        hour: 0,
        day: 1,
        month: 0,
        year: 70,
        weekday: 0,
        yearday: 0,
        dst: 0,
        offset: 0,
        zone: std::ptr::null(),
    };
    unsafe { localtime_r(&now, &mut tm) };
    LocalTime {
        year: 1900 + tm.year,
        month: (tm.month + 1) as u32,
        day: tm.day as u32,
        hour: tm.hour as u32,
        minute: tm.minute as u32,
        second: tm.second as u32,
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, UNIX_EPOCH};

    use super::*;

    #[test]
    fn offset_is_whole_minutes() {
        let now = UNIX_EPOCH + Duration::from_secs(1_790_000_000);
        let local = LocalTime { year: 2026, month: 9, day: 21, hour: 10, minute: 13, second: 20 };
        assert_eq!(utc_offset_secs(local, now), -4 * 3600);
        assert_eq!(utc_offset_secs(local, now + Duration::from_millis(900)), -4 * 3600);
        let east = LocalTime { day: 22, hour: 0, minute: 13, ..local };
        assert_eq!(utc_offset_secs(east, now), 10 * 3600);
        assert_eq!(days_from_civil(1970, 1, 1), 0);
        assert_eq!(days_from_civil(2000, 3, 1), 11_017);
    }
}
