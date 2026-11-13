//! Pixel fixes for shell bitmaps: unused alpha made opaque, straight alpha premultiplied, and area downscaling.

/// Shell bitmaps come with no alpha, straight alpha or premultiplied, so tell which from the pixels.
pub(super) fn premultiply_shell_alpha(rgba: &mut [u8]) {
    if alpha_channel_is_unused(rgba) {
        make_opaque(rgba);
    } else if has_straight_alpha(rgba) {
        premultiply(rgba);
    }
}

fn alpha_channel_is_unused(rgba: &[u8]) -> bool {
    rgba.chunks_exact(4).all(|pixel| pixel[3] == 0)
}

fn has_straight_alpha(rgba: &[u8]) -> bool {
    rgba.chunks_exact(4).any(|pixel| pixel[0].max(pixel[1]).max(pixel[2]) > pixel[3])
}

fn make_opaque(rgba: &mut [u8]) {
    for pixel in rgba.chunks_exact_mut(4) {
        pixel[3] = 255;
    }
}

fn premultiply(rgba: &mut [u8]) {
    for pixel in rgba.chunks_exact_mut(4) {
        let alpha = pixel[3] as u32;
        for channel in &mut pixel[..3] {
            *channel = ((*channel as u32 * alpha + 127) / 255) as u8;
        }
    }
}

pub(super) fn downscale_premultiplied(
    rgba: &[u8],
    width: usize,
    height: usize,
    new_width: usize,
    new_height: usize,
) -> Vec<u8> {
    let columns = area_weights(width, new_width);
    let rows = area_weights(height, new_height);
    let mut out = Vec::with_capacity(new_width * new_height * 4);
    for row_weights in &rows {
        for column_weights in &columns {
            let mut sum = [0f32; 4];
            for &(y, row_weight) in row_weights {
                for &(x, column_weight) in column_weights {
                    let source = &rgba[(y * width + x) * 4..][..4];
                    for (total, &value) in sum.iter_mut().zip(source) {
                        *total += value as f32 * row_weight * column_weight;
                    }
                }
            }
            out.extend(sum.map(|total| total.round().clamp(0.0, 255.0) as u8));
        }
    }
    out
}

/// For each target pixel, the source pixels it covers and how much of each.
fn area_weights(from: usize, to: usize) -> Vec<Vec<(usize, f32)>> {
    let span = from as f32 / to as f32;
    (0..to)
        .map(|target| {
            let (start, end) = (target as f32 * span, (target + 1) as f32 * span);
            (start.floor() as usize..(end.ceil() as usize).min(from))
                .filter_map(|source| {
                    let left = start.max(source as f32);
                    let right = end.min(source as f32 + 1.0);
                    (right > left).then(|| (source, (right - left) / span))
                })
                .collect()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unused_alpha_means_opaque() {
        let mut pixels = vec![10, 20, 30, 0, 40, 50, 60, 0];
        premultiply_shell_alpha(&mut pixels);
        assert_eq!(pixels, [10, 20, 30, 255, 40, 50, 60, 255]);
    }

    #[test]
    fn premultiplied_pixels_are_kept() {
        let mut pixels = vec![64, 32, 0, 128, 0, 0, 0, 0];
        premultiply_shell_alpha(&mut pixels);
        assert_eq!(pixels, [64, 32, 0, 128, 0, 0, 0, 0]);
    }

    #[test]
    fn straight_pixels_get_premultiplied() {
        let mut pixels = vec![255, 128, 0, 128, 10, 10, 10, 255];
        premultiply_shell_alpha(&mut pixels);
        assert_eq!(pixels, [128, 64, 0, 128, 10, 10, 10, 255]);
    }

    #[test]
    fn halving_averages_two_by_two_blocks() {
        #[rustfmt::skip]
        let pixels = [
            0, 0, 0, 0,   255, 255, 255, 255,   100, 0, 0, 255,   100, 0, 0, 255,
            0, 0, 0, 0,   255, 255, 255, 255,   100, 0, 0, 255,   100, 0, 0, 255,
        ];
        assert_eq!(downscale_premultiplied(&pixels, 4, 2, 2, 1), [128, 128, 128, 128, 100, 0, 0, 255]);
    }

    #[test]
    fn uneven_ratios_keep_flat_colors_flat() {
        let flat = [9u8, 99, 199, 255].repeat(7 * 5);
        let scaled = downscale_premultiplied(&flat, 7, 5, 3, 2);
        assert_eq!(scaled.len(), 3 * 2 * 4);
        assert!(scaled.chunks_exact(4).all(|pixel| pixel == [9, 99, 199, 255]));
    }
}
