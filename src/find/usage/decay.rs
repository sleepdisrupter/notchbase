//! The small sums behind fading memories: half-life decay, habits that fill towards one, and shares of a whole.

use crate::find::context::Stamp;

pub fn since(now: Stamp, then: Stamp) -> Stamp {
    now.saturating_sub(then)
}

pub fn faded(elapsed: Stamp, half_life: Stamp) -> f32 {
    (-(elapsed as f32) / half_life as f32).exp2()
}

pub fn decayed(value: f32, elapsed: Stamp, half_life: Stamp) -> f32 {
    value * faded(elapsed, half_life)
}

pub fn filled(amount: f32, fills_at: f32) -> f32 {
    1.0 - (-amount.max(0.0) / fills_at).exp()
}

pub fn share(part: f32, whole: f32) -> f32 {
    if whole > 0.0 { (part / whole).clamp(0.0, 1.0) } else { 0.0 }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::find::context::DAY;
    use crate::find::weights::{EDIT_HALF_LIFE, HEAT_HALF_LIFE, OPEN_HALF_LIFE, PAIR_HALF_LIFE, PREFIX_HALF_LIFE};

    const CLOSE: f32 = 1e-4;

    #[test]
    fn halves_at_each_half_life() {
        for half_life in [OPEN_HALF_LIFE, EDIT_HALF_LIFE, HEAT_HALF_LIFE, PAIR_HALF_LIFE, PREFIX_HALF_LIFE] {
            assert!((decayed(8.0, half_life, half_life) - 4.0).abs() < CLOSE, "{half_life}");
            assert!((decayed(8.0, 2 * half_life, half_life) - 2.0).abs() < CLOSE, "{half_life}");
        }
    }

    #[test]
    fn everything_fades_in_the_end() {
        assert_eq!(decayed(3.0, 0, DAY), 3.0);
        assert!(decayed(3.0, 400 * DAY, DAY) < CLOSE);
        assert_eq!(since(10, 25), 0, "a stamp from the future is no time ago");
        assert_eq!(since(25, 10), 15);
    }

    #[test]
    fn habit_fills_towards_one() {
        assert_eq!(filled(0.0, 3.0), 0.0);
        assert!((filled(3.0, 3.0) - 0.632_120_6).abs() < CLOSE);
        assert!(filled(1_000.0, 3.0) <= 1.0);
        assert_eq!(filled(-5.0, 3.0), 0.0, "a negative count is no habit");
    }

    #[test]
    fn share_of_nothing_is_nothing() {
        assert_eq!(share(1.0, 0.0), 0.0);
        assert_eq!(share(1.0, 4.0), 0.25);
        assert_eq!(share(9.0, 4.0), 1.0);
    }
}
