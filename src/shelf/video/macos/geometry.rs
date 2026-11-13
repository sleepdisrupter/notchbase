//! Sizes: a track's displayed size after its transform, and fitting a frame within a longest side.

pub(super) fn displayed_dims(width: f64, height: f64, [a, b, c, d]: [f64; 4]) -> Option<[u32; 2]> {
    let shown = [(a * width + c * height).abs().round(), (b * width + d * height).abs().round()];
    let representable = |side: &f64| side.is_finite() && *side >= 1.0 && *side <= f64::from(u32::MAX);
    shown.iter().all(representable).then(|| shown.map(|side| side as u32))
}

pub(super) fn fit_within(width: usize, height: usize, max_side: u32) -> (usize, usize) {
    let (width, height) = (width.max(1), height.max(1));
    let limit = (max_side as usize).max(1);
    let longest = width.max(height);
    if longest <= limit {
        return (width, height);
    }
    let scale = limit as f64 / longest as f64;
    let scaled = |side: usize| ((side as f64 * scale).round() as usize).clamp(1, limit);
    (scaled(width), scaled(height))
}

#[cfg(test)]
mod tests {
    use super::*;

    const IDENTITY: [f64; 4] = [1.0, 0.0, 0.0, 1.0];
    const QUARTER_TURN: [f64; 4] = [0.0, 1.0, -1.0, 0.0];

    #[test]
    fn upright_tracks_keep_size() {
        assert_eq!(displayed_dims(1920.0, 1080.0, IDENTITY), Some([1920, 1080]));
    }

    #[test]
    fn rotated_tracks_swap_sides() {
        assert_eq!(displayed_dims(1920.0, 1080.0, QUARTER_TURN), Some([1080, 1920]));
        assert_eq!(displayed_dims(1920.0, 1080.0, [-1.0, 0.0, 0.0, -1.0]), Some([1920, 1080]));
    }

    #[test]
    fn empty_or_broken_sizes_are_unknown() {
        assert_eq!(displayed_dims(0.0, 1080.0, IDENTITY), None);
        assert_eq!(displayed_dims(f64::NAN, 1080.0, IDENTITY), None);
    }

    #[test]
    fn large_frames_shrink_to_fit() {
        assert_eq!(fit_within(1920, 1080, 480), (480, 270));
        assert_eq!(fit_within(1080, 1920, 480), (270, 480));
    }

    #[test]
    fn small_frames_are_never_enlarged() {
        assert_eq!(fit_within(320, 240, 480), (320, 240));
    }

    #[test]
    fn extreme_shapes_keep_a_pixel() {
        assert_eq!(fit_within(10_000, 1, 480), (480, 1));
        assert_eq!(fit_within(0, 0, 480), (1, 1));
    }
}
