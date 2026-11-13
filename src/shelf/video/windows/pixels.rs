//! Copies decoded frames out of Media Foundation buffers into opaque RGBA, flipping and turning them as needed.

use ::windows::Win32::Media::MediaFoundation::{IMFMediaType, IMFSample, MF_MT_DEFAULT_STRIDE, MF_MT_FRAME_SIZE};
use ::windows::core::Result;
use image::{RgbaImage, imageops};

use super::geometry::Turn;
use super::media_type::{number, pair};

const BYTES_PER_PIXEL: usize = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Layout {
    pub(super) size: [u32; 2],
    pub(super) stride: i32,
}

impl Layout {
    pub(super) fn of(media_type: &IMFMediaType) -> Option<Self> {
        let size = pair(media_type, &MF_MT_FRAME_SIZE)?;
        let packed = size[0] as i32 * BYTES_PER_PIXEL as i32;
        let stride = number(media_type, &MF_MT_DEFAULT_STRIDE).map_or(packed, |stride| stride as i32);
        Some(Self { size, stride })
    }
}

pub(super) fn sample_rgba(sample: &IMFSample, layout: Layout) -> Result<Option<RgbaImage>> {
    let buffer = unsafe { sample.ConvertToContiguousBuffer() }?;
    let mut data = std::ptr::null_mut();
    let mut length = 0u32;
    unsafe { buffer.Lock(&mut data, None, Some(&raw mut length)) }?;
    let image = if data.is_null() {
        None
    } else {
        bgrx_to_rgba(unsafe { std::slice::from_raw_parts(data, length as usize) }, layout)
    };
    unsafe { buffer.Unlock() }?;
    Ok(image)
}

pub(super) fn turned(image: RgbaImage, turn: Turn) -> RgbaImage {
    match turn {
        Turn::Upright => image,
        Turn::Clockwise => imageops::rotate90(&image),
        Turn::Half => imageops::rotate180(&image),
        Turn::Counterclockwise => imageops::rotate270(&image),
    }
}

fn bgrx_to_rgba(bytes: &[u8], layout: Layout) -> Option<RgbaImage> {
    let [width, height] = layout.size;
    let row_bytes = width as usize * BYTES_PER_PIXEL;
    let pitch = layout.stride.unsigned_abs() as usize;
    if pitch < row_bytes {
        return None;
    }
    let rows = height as usize;
    let mut rgba = Vec::with_capacity(row_bytes * rows);
    for y in 0..rows {
        let stored = if layout.stride >= 0 { y } else { rows - 1 - y };
        let start = stored * pitch;
        let line = bytes.get(start..start + row_bytes)?;
        for pixel in line.chunks_exact(BYTES_PER_PIXEL) {
            rgba.extend_from_slice(&[pixel[2], pixel[1], pixel[0], u8::MAX]);
        }
    }
    RgbaImage::from_raw(width, height, rgba)
}

#[cfg(test)]
mod tests {
    use super::*;

    const BLUE_THEN_RED: [u8; 16] = [255, 0, 0, 7, 255, 0, 0, 7, 0, 0, 255, 7, 0, 0, 255, 7];

    #[test]
    fn top_down_rows_keep_order() {
        let image = bgrx_to_rgba(&BLUE_THEN_RED, Layout { size: [2, 2], stride: 8 }).unwrap();
        assert_eq!(image.get_pixel(0, 0).0, [0, 0, 255, 255]);
        assert_eq!(image.get_pixel(1, 1).0, [255, 0, 0, 255]);
    }

    #[test]
    fn bottom_up_rows_are_flipped() {
        let image = bgrx_to_rgba(&BLUE_THEN_RED, Layout { size: [2, 2], stride: -8 }).unwrap();
        assert_eq!(image.get_pixel(0, 0).0, [255, 0, 0, 255]);
        assert_eq!(image.get_pixel(0, 1).0, [0, 0, 255, 255]);
    }

    #[test]
    fn short_buffers_are_rejected() {
        assert!(bgrx_to_rgba(&BLUE_THEN_RED[..12], Layout { size: [2, 2], stride: 8 }).is_none());
        assert!(bgrx_to_rgba(&BLUE_THEN_RED, Layout { size: [2, 2], stride: 4 }).is_none());
    }

    #[test]
    fn clockwise_puts_left_on_top() {
        let image = bgrx_to_rgba(&BLUE_THEN_RED, Layout { size: [2, 2], stride: 8 }).unwrap();
        let upright = turned(image, Turn::Clockwise);
        assert_eq!(upright.get_pixel(0, 0).0, [255, 0, 0, 255]);
        assert_eq!(upright.get_pixel(1, 0).0, [0, 0, 255, 255]);
    }
}
