//! Scales an image size down to fit a square, keeping its shape and at least one pixel a side.

pub(super) fn fit_within(width: usize, height: usize, max_side: u32) -> (usize, usize) {
    let max_side = max_side as usize;
    if width <= max_side && height <= max_side {
        return (width, height);
    }
    let scale = max_side as f64 / width.max(height) as f64;
    let scaled = |side: usize| ((side as f64 * scale).round() as usize).clamp(1, max_side);
    (scaled(width), scaled(height))
}

#[cfg(test)]
mod tests {
    use super::fit_within;

    #[test]
    fn small_sizes_are_kept() {
        assert_eq!(fit_within(64, 32, 128), (64, 32));
        assert_eq!(fit_within(128, 128, 128), (128, 128));
    }

    #[test]
    fn large_sizes_shrink_to_fit() {
        assert_eq!(fit_within(256, 128, 128), (128, 64));
        assert_eq!(fit_within(100, 400, 128), (32, 128));
    }

    #[test]
    fn thin_sizes_keep_at_least_one_pixel() {
        assert_eq!(fit_within(1000, 1, 128), (128, 1));
    }
}
