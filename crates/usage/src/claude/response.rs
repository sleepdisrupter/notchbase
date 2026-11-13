//! Parses the usage endpoint's reply: the five_hour and seven_day windows, filled in from the newer limits list.

use std::time::SystemTime;

use serde_json::{Map, Value};

use super::json::number;
use super::rfc3339;
use super::{PlanUsage, UsageWindow};

const FIVE_HOURS: u64 = 5 * 60;
const WEEK: u64 = 7 * 24 * 60;

pub(super) fn parse_usage(body: &str, fetched_at: SystemTime) -> Option<PlanUsage> {
    let root: Value = serde_json::from_str(body).ok()?;
    let root = root.as_object()?;
    if root.contains_key("error") || root.get("type").and_then(Value::as_str) == Some("error") {
        return None;
    }
    let mut five_hour = window(root.get("five_hour"), "utilization", FIVE_HOURS);
    let mut weekly = window(root.get("seven_day"), "utilization", WEEK);
    fill_from_limits(root, &mut five_hour, &mut weekly);
    Some(PlanUsage::Ready { five_hour, weekly, fetched_at })
}

fn fill_from_limits(root: &Map<String, Value>, five_hour: &mut Option<UsageWindow>, weekly: &mut Option<UsageWindow>) {
    for limit in root.get("limits").and_then(Value::as_array).into_iter().flatten() {
        let (slot, minutes) = match limit.get("kind").and_then(Value::as_str) {
            Some("session") => (&mut *five_hour, FIVE_HOURS),
            Some("weekly_all") => (&mut *weekly, WEEK),
            _ => continue,
        };
        if let Some(from_limit) = window(Some(limit), "percent", minutes) {
            merge(slot, from_limit);
        }
    }
}

/// The window from the older fields wins, and the limits list only fills in what's missing.
fn merge(slot: &mut Option<UsageWindow>, from_limit: UsageWindow) {
    match slot {
        Some(window) => {
            if window.resets_at.is_none() {
                window.resets_at = from_limit.resets_at;
            }
        }
        None => *slot = Some(from_limit),
    }
}

fn window(value: Option<&Value>, percent_key: &str, minutes: u64) -> Option<UsageWindow> {
    let obj = value?.as_object()?;
    let percent = obj.get(percent_key).and_then(number)?;
    Some(UsageWindow {
        used: (percent / 100.0).clamp(0.0, 1.0) as f32,
        resets_at: obj.get("resets_at").and_then(Value::as_str).and_then(rfc3339::parse),
        estimated: false,
        minutes: Some(minutes),
    })
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, UNIX_EPOCH};

    use super::*;

    fn at(secs: u64) -> SystemTime {
        UNIX_EPOCH + Duration::from_secs(secs)
    }

    fn unix(t: SystemTime) -> f64 {
        t.duration_since(UNIX_EPOCH).unwrap().as_secs_f64()
    }

    #[test]
    fn usage_percent_becomes_fraction() {
        let body = r#"{
            "five_hour": {"utilization": 23.0, "resets_at": "2026-04-11T07:00:00.528743+00:00"},
            "seven_day": {"utilization": 41, "resets_at": "2026-04-17T00:59:59.951713+00:00"},
            "seven_day_opus": null,
            "seven_day_sonnet": {"utilization": 1.0, "resets_at": "2026-04-16T03:00:00Z"},
            "extra_usage": {"is_enabled": false}
        }"#;
        let Some(PlanUsage::Ready { five_hour: Some(five), weekly: Some(week), fetched_at }) =
            parse_usage(body, at(42))
        else {
            panic!("expected both windows");
        };
        assert_eq!(fetched_at, at(42));
        assert!((five.used - 0.23).abs() < 1e-6);
        assert!((week.used - 0.41).abs() < 1e-6);
        assert!((unix(five.resets_at.unwrap()) - 1_775_890_800.528_743).abs() < 1e-6);
        assert!(week.resets_at.is_some());
    }

    #[test]
    fn usage_low_values_stay_percent() {
        let body = r#"{"five_hour": {"utilization": 1.0, "resets_at": null}, "seven_day": {"utilization": 0.5}}"#;
        let Some(PlanUsage::Ready { five_hour: Some(five), weekly: Some(week), .. }) = parse_usage(body, at(0)) else {
            panic!();
        };
        assert!((five.used - 0.01).abs() < 1e-6);
        assert!((week.used - 0.005).abs() < 1e-6);
        assert_eq!(five.resets_at, None);
    }

    #[test]
    fn usage_clamps_and_skips_null_windows() {
        let body = r#"{"five_hour": {"utilization": 130}, "seven_day": null, "unknown": [1, 2]}"#;
        assert_eq!(
            parse_usage(body, at(7)),
            Some(PlanUsage::Ready {
                five_hour: Some(UsageWindow {
                    used: 1.0,
                    resets_at: None,
                    estimated: false,
                    minutes: Some(FIVE_HOURS)
                }),
                weekly: None,
                fetched_at: at(7),
            })
        );
        assert_eq!(
            parse_usage("{}", at(7)),
            Some(PlanUsage::Ready { five_hour: None, weekly: None, fetched_at: at(7) })
        );
        let no_number = r#"{"five_hour": {"utilization": null, "resets_at": "2026-04-11T07:00:00Z"}}"#;
        assert!(matches!(parse_usage(no_number, at(7)), Some(PlanUsage::Ready { five_hour: None, .. })));
    }

    #[test]
    fn usage_fills_from_limits_array() {
        let body = r#"{
            "five_hour": {"utilization": 9.0},
            "seven_day": {"utilization": 7.0},
            "limits": [
                {"kind": "session", "group": "session", "percent": 9, "resets_at": "2026-09-02T04:09:59Z", "scope": null},
                {"kind": "weekly_all", "group": "weekly", "percent": 7, "resets_at": "2026-09-05T17:59:59Z"},
                {"kind": "weekly_scoped", "group": "weekly", "percent": 12, "resets_at": "2026-09-05T17:59:59Z"}
            ]
        }"#;
        let Some(PlanUsage::Ready { five_hour: Some(five), weekly: Some(week), .. }) = parse_usage(body, at(0)) else {
            panic!();
        };
        assert!((five.used - 0.09).abs() < 1e-6);
        assert_eq!(five.resets_at, Some(at(1_788_322_199)));
        assert!((week.used - 0.07).abs() < 1e-6, "weekly_scoped must not replace weekly_all");
        assert_eq!(week.resets_at, rfc3339::parse("2026-09-05T17:59:59Z"));

        let only_limits = r#"{"limits": [{"kind": "session", "percent": "55", "resets_at": "2026-09-02T04:09:59Z"}]}"#;
        let Some(PlanUsage::Ready { five_hour: Some(five), weekly: None, .. }) = parse_usage(only_limits, at(0)) else {
            panic!();
        };
        assert!((five.used - 0.55).abs() < 1e-6);
    }

    #[test]
    fn usage_rejects_errors_and_junk() {
        assert_eq!(parse_usage("", at(0)), None);
        assert_eq!(parse_usage("[]", at(0)), None);
        assert_eq!(parse_usage("<html>", at(0)), None);
        let err = r#"{"type": "error", "error": {"type": "rate_limit_error", "message": "Rate limited."}}"#;
        assert_eq!(parse_usage(err, at(0)), None);
    }
}
