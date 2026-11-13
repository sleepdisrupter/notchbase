//! The times to take frames at: evenly spaced from the start, up to the time limit.

use super::super::FrameSpec;

const ROUNDING_SLACK: f64 = 1e-6;

pub(super) fn frame_times(duration: f64, spec: FrameSpec) -> Vec<f64> {
    let span = duration.min(spec.max_seconds);
    let usable = duration > 0.0 && span.is_finite() && span > 0.0 && spec.fps.is_finite() && spec.fps > 0.0;
    if !usable {
        return Vec::new();
    }
    let count = (span * spec.fps - ROUNDING_SLACK).ceil().max(1.0) as usize;
    (0..count).map(|index| index as f64 / spec.fps).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const GIF: FrameSpec = FrameSpec { max_side: 480, fps: 12.0, max_seconds: 30.0 };

    #[test]
    fn frames_evenly_spaced_from_start() {
        let times = frame_times(2.0, GIF);
        assert_eq!(times.len(), 24);
        assert_eq!(times[0], 0.0);
        assert!((times[1] - 1.0 / 12.0).abs() < 1e-12);
        assert!((times[23] - 23.0 / 12.0).abs() < 1e-12);
    }

    #[test]
    fn long_clips_stop_at_the_limit() {
        let times = frame_times(600.0, GIF);
        assert_eq!(times.len(), 360);
        assert!(times.iter().all(|&at| at < GIF.max_seconds));
    }

    #[test]
    fn whole_seconds_add_no_frame() {
        assert_eq!(frame_times(1.0, GIF).len(), 12);
    }

    #[test]
    fn short_clips_get_one_frame() {
        assert_eq!(frame_times(0.01, GIF), vec![0.0]);
    }

    #[test]
    fn unusable_inputs_give_no_frames() {
        assert!(frame_times(0.0, GIF).is_empty());
        assert!(frame_times(f64::NAN, GIF).is_empty());
        assert!(frame_times(2.0, FrameSpec { fps: 0.0, ..GIF }).is_empty());
    }
}
