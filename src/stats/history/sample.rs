//! One line of the log: when, which assistant, and how much of each window was used.

use serde::{Deserialize, Serialize};

use crate::assistant::Provider;
use crate::stats::PlanUsage;

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub(super) struct Sample {
    pub(super) at_ms: u64,
    pub(super) provider: Provider,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) five_hour: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) weekly: Option<f32>,
}

impl Sample {
    pub(super) fn of(provider: Provider, usage: &PlanUsage) -> Option<Sample> {
        let PlanUsage::Ready { five_hour, weekly, fetched_at } = usage else { return None };
        let at_ms = u64::try_from(fetched_at.duration_since(std::time::UNIX_EPOCH).ok()?.as_millis()).ok()?;
        let (five_hour, weekly) = (five_hour.as_ref().map(|w| w.used), weekly.as_ref().map(|w| w.used));
        (five_hour.is_some() || weekly.is_some()).then_some(Sample { at_ms, provider, five_hour, weekly })
    }

    pub(super) fn line(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }

    pub(super) fn parse(line: &str) -> Option<Sample> {
        let sample: Sample = serde_json::from_str(line.trim()).ok()?;
        let finite = |value: Option<f32>| value.is_none_or(f32::is_finite);
        (finite(sample.five_hour) && finite(sample.weekly)).then_some(sample)
    }

    pub(super) fn same_values(&self, other: &Sample) -> bool {
        self.five_hour == other.five_hour && self.weekly == other.weekly
    }

    /// The day's figure: the 5-hour window, or the weekly one for a plan without it.
    pub(super) fn peak(&self) -> Option<f32> {
        self.five_hour.or(self.weekly).map(|used| used.clamp(0.0, 1.0))
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, UNIX_EPOCH};

    use super::*;
    use crate::stats::UsageWindow;

    fn window(used: f32) -> Option<UsageWindow> {
        Some(UsageWindow { used, resets_at: None, estimated: false, minutes: None })
    }

    #[test]
    fn only_ready_usage_becomes_a_sample() {
        let fetched_at = UNIX_EPOCH + Duration::from_millis(1_790_000_000_123);
        let ready = PlanUsage::Ready { five_hour: window(0.42), weekly: window(0.1), fetched_at };
        let expected =
            Sample { at_ms: 1_790_000_000_123, provider: Provider::Codex, five_hour: Some(0.42), weekly: Some(0.1) };
        assert_eq!(Sample::of(Provider::Codex, &ready), Some(expected));
        let empty = PlanUsage::Ready { five_hour: None, weekly: None, fetched_at };
        assert_eq!(Sample::of(Provider::Claude, &empty), None);
        assert_eq!(Sample::of(Provider::Claude, &PlanUsage::Loading), None);
        assert_eq!(Sample::of(Provider::Claude, &PlanUsage::Expired), None);
    }

    #[test]
    fn lines_round_trip_and_skip_missing() {
        let sample = Sample { at_ms: 5, provider: Provider::Claude, five_hour: Some(0.42), weekly: None };
        assert_eq!(sample.line(), r#"{"at_ms":5,"provider":"claude","five_hour":0.42}"#);
        assert_eq!(Sample::parse(&sample.line()), Some(sample));
        let weekly = Sample { at_ms: 5, provider: Provider::Codex, five_hour: None, weekly: Some(1.0) };
        assert_eq!(Sample::parse("  {\"at_ms\":5,\"provider\":\"codex\",\"weekly\":1.0}\r"), Some(weekly));
    }

    #[test]
    fn corrupt_lines_are_rejected() {
        let lines =
            ["", "{", "null", r#"{"at_ms":5,"provider":"gemini"}"#, r#"{"at_ms":-1,"provider":"claude"}"#, "\u{1}"];
        for line in lines {
            assert_eq!(Sample::parse(line), None, "{line:?}");
        }
    }

    #[test]
    fn peak_prefers_the_five_hour_window() {
        let sample = |five_hour, weekly| Sample { at_ms: 0, provider: Provider::Claude, five_hour, weekly };
        assert_eq!(sample(Some(0.3), Some(0.9)).peak(), Some(0.3));
        assert_eq!(sample(None, Some(0.9)).peak(), Some(0.9));
        assert_eq!(sample(Some(1.4), None).peak(), Some(1.0));
        assert_eq!(sample(None, None).peak(), None);
    }
}
