//! Samples in time order, each newer than the last for its assistant, with repeats kept at most every 10 minutes.

use super::sample::Sample;

pub(super) const DAY_MS: u64 = 86_400_000;
pub(super) const KEEP_FOR_MS: u64 = 14 * DAY_MS;
const SAME_VALUES_GAP_MS: u64 = 10 * 60_000;

#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct Series {
    samples: Vec<Sample>,
}

impl Series {
    pub(super) fn new(mut samples: Vec<Sample>) -> Self {
        samples.sort_by_key(|sample| sample.at_ms);
        Self { samples }
    }

    pub(super) fn samples(&self) -> &[Sample] {
        &self.samples
    }

    pub(super) fn len(&self) -> usize {
        self.samples.len()
    }

    pub(super) fn push(&mut self, sample: Sample) -> bool {
        let last = self.samples.iter().rev().find(|kept| kept.provider == sample.provider);
        let news = last.is_none_or(|last| {
            sample.at_ms > last.at_ms && (!sample.same_values(last) || sample.at_ms - last.at_ms >= SAME_VALUES_GAP_MS)
        });
        if news {
            let at = self.samples.partition_point(|kept| kept.at_ms <= sample.at_ms);
            self.samples.insert(at, sample);
        }
        news
    }

    pub(super) fn prune(&mut self, now_ms: u64) {
        let oldest = now_ms.saturating_sub(KEEP_FOR_MS);
        self.samples.retain(|sample| sample.at_ms >= oldest);
    }

    pub(super) fn thin(&mut self, keep: usize) {
        let extra = self.samples.len().saturating_sub(keep);
        self.samples.drain(..extra);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assistant::Provider;

    const MINUTE: u64 = 60_000;

    fn sample(provider: Provider, at_ms: u64, five_hour: f32) -> Sample {
        Sample { at_ms, provider, five_hour: Some(five_hour), weekly: None }
    }

    #[test]
    fn only_newer_samples_are_kept() {
        let mut series = Series::default();
        assert!(series.push(sample(Provider::Claude, 10 * MINUTE, 0.1)));
        assert!(!series.push(sample(Provider::Claude, 10 * MINUTE, 0.2)), "the same fetch twice");
        assert!(!series.push(sample(Provider::Claude, 5 * MINUTE, 0.2)), "an older fetch");
        assert!(series.push(sample(Provider::Codex, 5 * MINUTE, 0.2)), "providers are separate");
        assert!(series.push(sample(Provider::Claude, 11 * MINUTE, 0.3)));
        let order: Vec<u64> = series.samples().iter().map(|s| s.at_ms / MINUTE).collect();
        assert_eq!(order, [5, 10, 11]);
    }

    #[test]
    fn unchanged_numbers_wait_ten_minutes() {
        let mut series = Series::default();
        assert!(series.push(sample(Provider::Claude, 0, 0.4)));
        assert!(!series.push(sample(Provider::Claude, 3 * MINUTE, 0.4)));
        assert!(!series.push(sample(Provider::Claude, 9 * MINUTE, 0.4)));
        assert!(series.push(sample(Provider::Claude, 10 * MINUTE, 0.4)));
        assert!(series.push(sample(Provider::Claude, 11 * MINUTE, 0.41)));
        assert_eq!(series.len(), 3);
    }

    #[test]
    fn prune_drops_old_thin_keeps_newest() {
        let now = 30 * DAY_MS;
        let mut series = Series::new((0..30).rev().map(|day| sample(Provider::Claude, day * DAY_MS, 0.5)).collect());
        series.prune(now);
        assert_eq!(series.len(), 14);
        assert_eq!(series.samples()[0].at_ms, 16 * DAY_MS);
        series.thin(4);
        let days: Vec<u64> = series.samples().iter().map(|s| s.at_ms / DAY_MS).collect();
        assert_eq!(days, [26, 27, 28, 29]);
        series.thin(10);
        assert_eq!(series.len(), 4);
    }
}
