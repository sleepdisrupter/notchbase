//! Timestamps in Codex logs: RFC 3339 strings and Unix seconds.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::days_from_civil;

const STAMP_LEN: usize = 19;

pub(super) fn parse(text: &str) -> Option<SystemTime> {
    let (stamp, rest) = text.trim().as_bytes().split_at_checked(STAMP_LEN)?;
    let local = civil_seconds(stamp)?;
    let (nanos, zone) = fraction(rest)?;
    let secs = u64::try_from(local - offset_seconds(zone)?).ok()?;
    UNIX_EPOCH.checked_add(Duration::new(secs, nanos))
}

pub(super) fn from_unix(secs: f64) -> Option<SystemTime> {
    UNIX_EPOCH.checked_add(Duration::try_from_secs_f64(secs).ok()?)
}

fn civil_seconds(b: &[u8]) -> Option<i64> {
    if b[4] != b'-' || b[7] != b'-' || !matches!(b[10], b'T' | b't' | b' ') || b[13] != b':' || b[16] != b':' {
        return None;
    }
    let field = |from: usize, to: usize| digits(&b[from..to]);
    let (year, month, day) = (field(0, 4)?, field(5, 7)?, field(8, 10)?);
    let (hour, minute, second) = (field(11, 13)?, field(14, 16)?, field(17, 19)?);
    if !(1..=12).contains(&month)
        || !(1..=days_in_month(year, month)).contains(&day)
        || hour > 23
        || minute > 59
        || second > 60
    {
        return None;
    }
    Some(days_from_civil(year, month, day) * 86_400 + hour * 3600 + minute * 60 + second)
}

fn fraction(rest: &[u8]) -> Option<(u32, &[u8])> {
    let Some(fraction) = rest.strip_prefix(b".") else {
        return Some((0, rest));
    };
    let len = fraction.iter().take_while(|c| c.is_ascii_digit()).count();
    if len == 0 {
        return None;
    }
    let nanos =
        fraction[..len.min(9)].iter().zip((0..9).rev()).map(|(c, power)| u32::from(c - b'0') * 10u32.pow(power));
    Some((nanos.sum(), &fraction[len..]))
}

fn offset_seconds(zone: &[u8]) -> Option<i64> {
    let (sign, hours, minutes) = match zone {
        b"Z" | b"z" => return Some(0),
        [b'+', h1, h2, b':', m1, m2] => (1, [*h1, *h2], [*m1, *m2]),
        [b'-', h1, h2, b':', m1, m2] => (-1, [*h1, *h2], [*m1, *m2]),
        _ => return None,
    };
    let (hours, minutes) = (digits(&hours)?, digits(&minutes)?);
    (hours <= 23 && minutes <= 59).then_some(sign * (hours * 3600 + minutes * 60))
}

fn digits(part: &[u8]) -> Option<i64> {
    part.iter().try_fold(0, |n, &c| c.is_ascii_digit().then(|| n * 10 + i64::from(c - b'0')))
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

    #[test]
    fn codex_rollout_stamps() {
        let written = parse("2026-09-08T01:57:04.071Z").unwrap();
        assert_eq!(written, UNIX_EPOCH + Duration::from_millis(1_788_832_624_071));
        assert_eq!(parse("2026-09-21T14:13:20Z"), Some(at(1_790_000_000)));
        assert_eq!(parse(" 2026-09-21t14:13:20z "), Some(at(1_790_000_000)));
        assert_eq!(parse("2026-09-21T19:43:20+05:30"), Some(at(1_790_000_000)));
        assert_eq!(parse("2026-09-21T06:13:20-08:00"), Some(at(1_790_000_000)));
        assert_eq!(parse("2024-02-29T00:00:00Z"), Some(at(1_709_164_800)));
        assert_eq!(parse("1970-01-01T00:00:00Z"), Some(UNIX_EPOCH));
        let precise = parse("2026-09-21T14:13:20.1234567891Z").unwrap().duration_since(at(1_790_000_000)).unwrap();
        assert!(precise.abs_diff(Duration::from_nanos(123_456_789)) < Duration::from_nanos(100), "{precise:?}");
    }

    #[test]
    fn rejects_garbage() {
        for bad in [
            "",
            "2026-09-21",
            "2026-09-21T14:13:20",
            "2026-13-01T00:00:00Z",
            "2023-02-29T00:00:00Z",
            "2026-09-31T00:00:00Z",
            "2026-09-21T24:00:00Z",
            "2026-09-21T14:13:20.Z",
            "2026-09-21T14:13:20+5:00",
            "2026-09-21T14:13:20+0530",
            "2026-09-21X14:13:20Z",
            "2026-09-21T14:13:20Zjunk",
            "20a6-09-21T14:13:20Z",
            "1969-12-31T23:59:59Z",
            "2026-09-21T14:13:2\u{e9}Z",
        ] {
            assert_eq!(parse(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn unix_seconds() {
        assert_eq!(from_unix(1_790_000_000.0), Some(at(1_790_000_000)));
        assert_eq!(from_unix(0.5), Some(UNIX_EPOCH + Duration::from_millis(500)));
        assert_eq!(from_unix(-1.0), None);
        assert_eq!(from_unix(f64::NAN), None);
        assert_eq!(from_unix(f64::INFINITY), None);
    }
}
