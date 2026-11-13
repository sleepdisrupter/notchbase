//! Parses a token_count line into the main codex limit's two windows, from the current schema or an older flat one.

use std::time::{Duration, SystemTime};

use serde_json::{Map, Value};

use super::timestamp;

const MAIN_LIMIT: &str = "codex";

#[derive(Clone, Debug, PartialEq)]
pub(super) struct Window {
    pub(super) percent: f64,
    pub(super) minutes: Option<u64>,
    pub(super) resets_at: Option<SystemTime>,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct Snapshot {
    pub(super) at: SystemTime,
    pub(super) primary: Option<Window>,
    pub(super) secondary: Option<Window>,
}

pub(super) fn parse(line: &str, written: SystemTime) -> Option<Snapshot> {
    let root: Value = serde_json::from_str(line).ok()?;
    let payload = root.get("payload")?;
    if kind(&root) != Some("event_msg") || kind(payload) != Some("token_count") {
        return None;
    }
    let limits = payload.get("rate_limits")?.as_object().filter(|limits| is_main(limits))?;
    let at = root.get("timestamp").and_then(Value::as_str).and_then(timestamp::parse).unwrap_or(written);
    let (primary, secondary) = (window(limits, "primary", at), window(limits, "secondary", at));
    (primary.is_some() || secondary.is_some()).then_some(Snapshot { at, primary, secondary })
}

fn kind(value: &Value) -> Option<&str> {
    value.get("type")?.as_str()
}

/// Older logs have no limit id, and those are the main limit.
fn is_main(limits: &Map<String, Value>) -> bool {
    match limits.get("limit_id") {
        None | Some(Value::Null) => true,
        Some(id) => id.as_str().is_some_and(|id| id.eq_ignore_ascii_case(MAIN_LIMIT)),
    }
}

fn window(limits: &Map<String, Value>, name: &str, at: SystemTime) -> Option<Window> {
    let Some(Value::Object(window)) = limits.get(name) else {
        return flat(limits, name);
    };
    let resets_in = || {
        let seconds = window.get("resets_in_seconds")?.as_f64()?;
        at.checked_add(Duration::try_from_secs_f64(seconds).ok()?)
    };
    Some(Window {
        percent: percent(window.get("used_percent"))?,
        minutes: minutes(window.get("window_minutes")),
        resets_at: window.get("resets_at").and_then(instant).or_else(resets_in),
    })
}

fn flat(limits: &Map<String, Value>, name: &str) -> Option<Window> {
    let field = |suffix: &str| limits.get(format!("{name}_{suffix}").as_str());
    Some(Window {
        percent: percent(field("used_percent"))?,
        minutes: minutes(field("window_minutes")),
        resets_at: None,
    })
}

fn percent(value: Option<&Value>) -> Option<f64> {
    value?.as_f64().filter(|percent| percent.is_finite())
}

fn minutes(value: Option<&Value>) -> Option<u64> {
    value?.as_f64().filter(|minutes| minutes.is_finite() && *minutes >= 1.0).map(|minutes| minutes.round() as u64)
}

fn instant(value: &Value) -> Option<SystemTime> {
    match value {
        Value::Number(secs) => timestamp::from_unix(secs.as_f64()?),
        Value::String(text) => timestamp::parse(text).or_else(|| timestamp::from_unix(text.trim().parse().ok()?)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codex::fixture::{T0, at, stamp, token_count};

    fn only(line: &str) -> Snapshot {
        parse(line, at(1)).unwrap_or_else(|| panic!("no snapshot in {line}"))
    }

    fn limit(percent: f64, minutes: u64, resets_at: Option<u64>) -> Option<Window> {
        Some(Window { percent, minutes: Some(minutes), resets_at: resets_at.map(at) })
    }

    #[test]
    fn current_schema_with_both_windows() {
        let line = token_count(
            &stamp(T0),
            r#"{"limit_id": "codex", "limit_name": null,
                "primary": {"used_percent": 74.0, "window_minutes": 300, "resets_at": 1790003600},
                "secondary": {"used_percent": 12.5, "window_minutes": 10080, "resets_at": 1790604800},
                "credits": {"has_credits": false, "unlimited": false, "balance": "0"},
                "plan_type": "plus", "rate_limit_reached_type": null}"#,
        );
        assert_eq!(
            only(&line),
            Snapshot {
                at: at(T0),
                primary: limit(74.0, 300, Some(1_790_003_600)),
                secondary: limit(12.5, 10080, Some(1_790_604_800)),
            }
        );
    }

    #[test]
    fn single_window_plans() {
        let weekly = r#"{"primary": {"used_percent": 43, "window_minutes": 10080, "resets_at": 1790604800},
            "secondary": null, "plan_type": "plus"}"#;
        let snapshot = only(&token_count(&stamp(T0), weekly));
        assert_eq!((snapshot.primary, snapshot.secondary), (limit(43.0, 10080, Some(1_790_604_800)), None));
        let monthly =
            r#"{"primary": {"used_percent": 5, "window_minutes": 43200, "resets_at": null}, "plan_type": "free"}"#;
        assert_eq!(only(&token_count(&stamp(T0), monthly)).primary, limit(5.0, 43200, None));
    }

    #[test]
    fn older_schemas() {
        let relative = r#"{"primary": {"used_percent": 20, "window_minutes": 299, "resets_in_seconds": 3600},
            "secondary": {"used_percent": 3, "window_minutes": null, "resets_in_seconds": null}}"#;
        let snapshot = only(&token_count(&stamp(T0), relative));
        assert_eq!(snapshot.primary, limit(20.0, 299, Some(T0 + 3600)));
        assert_eq!(snapshot.secondary, Some(Window { percent: 3.0, minutes: None, resets_at: None }));

        let flat = r#"{"primary_used_percent": 9.0, "secondary_used_percent": 1.5,
            "primary_to_secondary_ratio_percent": 3.0, "primary_window_minutes": 300, "secondary_window_minutes": 10080}"#;
        let snapshot = only(&token_count(&stamp(T0), flat));
        assert_eq!((snapshot.primary, snapshot.secondary), (limit(9.0, 300, None), limit(1.5, 10080, None)));
    }

    #[test]
    fn reset_times_may_be_strings() {
        let limits = r#"{"primary": {"used_percent": 1, "window_minutes": 300, "resets_at": "2026-09-21T15:13:20Z"},
            "secondary": {"used_percent": 2, "window_minutes": 10080, "resets_at": "1790604800"}}"#;
        let snapshot = only(&token_count(&stamp(T0), limits));
        assert_eq!(snapshot.primary.unwrap().resets_at, Some(at(T0 + 3600)));
        assert_eq!(snapshot.secondary.unwrap().resets_at, Some(at(1_790_604_800)));
    }

    #[test]
    fn missing_stamp_uses_the_file_time() {
        let line = r#"{"type": "event_msg", "payload": {"type": "token_count", "info": null,
            "rate_limits": {"primary": {"used_percent": 1, "window_minutes": 300}}}}"#;
        assert_eq!(parse(line, at(77)).unwrap().at, at(77));
        let bad_stamp = token_count("yesterday", r#"{"primary": {"used_percent": 1}}"#);
        assert_eq!(parse(&bad_stamp, at(77)).unwrap().at, at(77));
    }

    #[test]
    fn skips_other_buckets_and_empties() {
        let premium = r#"{"limit_id": "premium", "primary": {"used_percent": 50, "window_minutes": 300}}"#;
        assert_eq!(parse(&token_count(&stamp(T0), premium), at(1)), None);
        let upper = r#"{"limit_id": "Codex", "primary": {"used_percent": 50, "window_minutes": 300}}"#;
        assert!(parse(&token_count(&stamp(T0), upper), at(1)).is_some());
        let empty = r#"{"limit_id": "codex", "primary": null, "secondary": null, "plan_type": "plus"}"#;
        assert_eq!(parse(&token_count(&stamp(T0), empty), at(1)), None);
        let no_percent = r#"{"primary": {"window_minutes": 300, "resets_at": 1790003600}}"#;
        assert_eq!(parse(&token_count(&stamp(T0), no_percent), at(1)), None);
        assert_eq!(parse(&token_count(&stamp(T0), "null"), at(1)), None);
    }

    #[test]
    fn stray_and_broken_lines_are_ignored() {
        for line in [
            "",
            "{",
            "[]",
            r#"{"timestamp": "2026-09-21T14:13:20Z", "type": "session_meta", "payload": {"id": "x"}}"#,
            r#"{"type": "response_item", "payload": {"type": "token_count", "rate_limits": {"primary": {"used_percent": 1}}}}"#,
            r#"{"type": "event_msg", "payload": {"type": "agent_message", "rate_limits": {"primary": {"used_percent": 1}}}}"#,
            r#"{"type": "event_msg", "payload": {"type": "token_count", "info": {"total_token_usage": {}}}}"#,
            r#"{"type": "event_msg", "payload": {"type": "token_count", "rate_limits": {"primary": {"used_percent": "NaN"}}}}"#,
        ] {
            assert_eq!(parse(line, at(1)), None, "{line}");
        }
    }
}
