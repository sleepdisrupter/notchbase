//! RFC 3339 timestamps, as the usage endpoint sends them, to SystemTime.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::days_from_civil;

const STAMP_LEN: usize = 19;

pub(super) fn parse(text: &str) -> Option<SystemTime> {
    let text = text.trim();
    if text.len() <= STAMP_LEN {
        return None;
    }
    let (stamp, rest) = text.split_at_checked(STAMP_LEN)?;
    let local = civil_seconds(stamp)?;
    let (nanos, zone) = fraction(rest)?;
    let offset = offset_seconds(zone)?;
    since_epoch(local - offset, nanos)
}

fn civil_seconds(stamp: &str) -> Option<i64> {
    let b = stamp.as_bytes();
    if b[4] != b'-' || b[7] != b'-' || !matches!(b[10], b'T' | b't' | b' ') || b[13] != b':' || b[16] != b':' {
        return None;
    }
    let field = |from: usize, to: usize| stamp.get(from..to).and_then(digits);
    let (year, month, day) = (field(0, 4)?, field(5, 7)?, field(8, 10)?);
    let (hour, minute, second) = (field(11, 13)?, field(14, 16)?, field(17, 19)?);
    if !(1..=12).contains(&month)
        || day < 1
        || day > days_in_month(year, month)
        || hour > 23
        || minute > 59
        || second > 60
    {
        return None;
    }
    Some(days_from_civil(year, month, day) * 86_400 + hour * 3600 + minute * 60 + second)
}

fn fraction(rest: &str) -> Option<(u32, &str)> {
    let Some(fraction) = rest.strip_prefix('.') else {
        return Some((0, rest));
    };
    let len = fraction.bytes().take_while(u8::is_ascii_digit).count();
    if len == 0 {
        return None;
    }
    let nanos = fraction.bytes().take(len.min(9)).enumerate().map(|(i, c)| (c - b'0') as u32 * 10u32.pow(8 - i as u32));
    Some((nanos.sum(), &fraction[len..]))
}

fn offset_seconds(zone: &str) -> Option<i64> {
    if matches!(zone, "Z" | "z") {
        return Some(0);
    }
    let sign = match zone.as_bytes().first()? {
        b'+' => 1,
        b'-' => -1,
        _ => return None,
    };
    let tz = &zone[1..];
    let (h, m) = match tz.len() {
        5 if tz.as_bytes()[2] == b':' => (tz.get(0..2)?, tz.get(3..5)?),
        4 => (tz.get(0..2)?, tz.get(2..4)?),
        _ => return None,
    };
    let (h, m) = (digits(h)?, digits(m)?);
    if h > 23 || m > 59 {
        return None;
    }
    Some(sign * (h * 3600 + m * 60))
}

fn digits(part: &str) -> Option<i64> {
    part.bytes().all(|c| c.is_ascii_digit()).then(|| part.parse().ok())?
}

fn since_epoch(secs: i64, nanos: u32) -> Option<SystemTime> {
    if secs >= 0 {
        UNIX_EPOCH.checked_add(Duration::new(secs as u64, nanos))
    } else {
        UNIX_EPOCH
            .checked_sub(Duration::from_secs(secs.unsigned_abs()))?
            .checked_add(Duration::from_nanos(nanos as u64))
    }
}

fn days_in_month(year: i64, month: i64) -> i64 {
    match month {
        2 if (year % 4 == 0 && year % 100 != 0) || year % 400 == 0 => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(secs: u64) -> SystemTime {
        UNIX_EPOCH + Duration::from_secs(secs)
    }

    fn unix(t: SystemTime) -> f64 {
        t.duration_since(UNIX_EPOCH).unwrap().as_secs_f64()
    }

    #[test]
    fn formats() {
        assert_eq!(parse("1970-01-01T00:00:00Z"), Some(UNIX_EPOCH));
        assert_eq!(parse("2000-01-01T00:00:00Z"), Some(at(946_684_800)));
        assert_eq!(parse("2026-09-02T04:09:59Z"), Some(at(1_788_322_199)));
        assert_eq!(parse("2024-02-29T12:00:00Z"), Some(at(1_709_208_000)));
        assert_eq!(parse("2026-01-15T09:30:00+05:30"), Some(at(1_768_449_600)));
        assert_eq!(parse("2026-01-15T09:30:00+0530"), Some(at(1_768_449_600)));
        assert_eq!(parse("2026-01-14T20:00:00-08:00"), parse("2026-01-15T04:00:00Z"));
        assert_eq!(parse("1969-12-31T23:59:59Z"), UNIX_EPOCH.checked_sub(Duration::from_secs(1)));
        assert_eq!(parse("  2000-01-01t00:00:00z "), Some(at(946_684_800)));
    }

    #[test]
    fn fractions() {
        let t = parse("2026-04-11T07:00:00.528743+00:00").unwrap();
        assert!((unix(t) - 1_775_890_800.528_743).abs() < 1e-6);
        let t = parse("2026-04-11T07:00:00.5Z").unwrap();
        assert_eq!(t.duration_since(at(1_775_890_800)).unwrap(), Duration::from_millis(500));
        let t = parse("2026-04-11T07:00:00.1234567891Z").unwrap();
        let frac = t.duration_since(at(1_775_890_800)).unwrap();
        assert!(frac.abs_diff(Duration::from_nanos(123_456_789)) < Duration::from_nanos(100), "{frac:?}");
    }

    #[test]
    fn rejects_garbage() {
        for bad in [
            "",
            "2026-04-11",
            "2026-04-11T07:00:00",
            "2026-13-01T00:00:00Z",
            "2023-02-29T00:00:00Z",
            "2026-04-31T00:00:00Z",
            "2026-04-11T24:00:00Z",
            "2026-04-11T07:00:00.Z",
            "2026-04-11T07:00:00+5:00",
            "2026-04-11T07:00:00+05:0x",
            "2026-04-11X07:00:00Z",
            "2026-04-11T07:00:00Zjunk",
            "20a6-04-11T07:00:00Z",
            "+026-04-11T07:00:00Z",
            "2026-04-11T07:00:0\u{e9}Z",
        ] {
            assert_eq!(parse(bad), None, "{bad:?}");
        }
    }
}
