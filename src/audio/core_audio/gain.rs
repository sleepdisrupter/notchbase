//! Gain is the volume squared, which feels even across the slider. A tap is only needed below full volume or muted.

const UNITY_TOLERANCE: f32 = 1e-3;

pub fn gain(volume: f32, muted: bool) -> f32 {
    if muted { 0.0 } else { volume.clamp(0.0, 1.0).powi(2) }
}

pub fn needs_tap(volume: f32, muted: bool) -> bool {
    muted || (volume - 1.0).abs() > UNITY_TOLERANCE
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gain_curve() {
        assert_eq!(gain(1.0, false), 1.0);
        assert_eq!(gain(0.5, false), 0.25);
        assert_eq!(gain(0.8, true), 0.0);
        assert!(!needs_tap(1.0, false));
        assert!(needs_tap(0.99, false));
        assert!(!needs_tap(0.9995, false), "rounding noise near 100% is still unity");
        assert!(needs_tap(1.0, true));
    }
}
