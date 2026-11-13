//! How far around now to read: a day and a half back to two and a half days ahead, or wider when a tool asks.

const HOUR_MS: u64 = 60 * 60 * 1000;
const BEHIND: u64 = 36 * HOUR_MS;
const AHEAD: u64 = 60 * HOUR_MS;
const LONGEST: u64 = 400 * 24 * HOUR_MS;

pub(super) fn around(now_ms: u64, wanted: Option<[u64; 2]>) -> (u64, u64) {
    let (from, to) = (now_ms.saturating_sub(BEHIND), now_ms + AHEAD);
    match wanted {
        Some([start, end]) if end.saturating_sub(start) <= LONGEST => (from.min(start), to.max(end)),
        _ => (from, to),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn window_spans_today_and_tomorrow() {
        let now = 1_800_000_000_000;
        let (from, to) = around(now, None);
        assert!(now - from >= 24 * HOUR_MS);
        assert!(to - now >= 48 * HOUR_MS);
    }

    #[test]
    fn widens_to_a_month_not_years() {
        let now = 1_800_000_000_000;
        let month = [now - 40 * 24 * HOUR_MS, now + 10 * 24 * HOUR_MS];
        assert_eq!(around(now, Some(month)), (month[0], month[1]));
        assert_eq!(around(now, Some([0, now * 2])), around(now, None));
    }
}
