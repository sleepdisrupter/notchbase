//! Extracts a 48 pixel icon from a file and turns it into an RGBA image.

use std::ffi::c_void;
use std::sync::Arc;

use ::windows::Win32::Foundation::S_OK;
use ::windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAP, BITMAPINFO, BITMAPINFOHEADER, DIB_RGB_COLORS, DeleteObject, GetDC, GetDIBits, GetObjectW, HBITMAP,
    HDC, ReleaseDC,
};
use ::windows::Win32::UI::Shell::SHDefExtractIconW;
use ::windows::Win32::UI::WindowsAndMessaging::{DestroyIcon, GetIconInfo, HICON, ICONINFO};
use ::windows::core::HSTRING;
use egui::ColorImage;

pub const ICON_PX: u32 = 48;
const SMALL_ICON_PX: u32 = 16;
const MAX_ICON_SIDE: i32 = 256;

pub fn load_icon(file: &str, index: i32) -> Option<Arc<ColorImage>> {
    let wide = HSTRING::from(file);
    let mut icon = HICON::default();
    let sizes = ICON_PX | (SMALL_ICON_PX << 16);
    let hr = unsafe { SHDefExtractIconW(&wide, index, 0, Some(&mut icon), None, sizes) };
    if hr != S_OK || icon.is_invalid() {
        return None;
    }
    let image = unsafe { icon_to_image(icon) };
    let _ = unsafe { DestroyIcon(icon) };
    image.map(Arc::new)
}

unsafe fn icon_to_image(icon: HICON) -> Option<ColorImage> {
    let mut info = ICONINFO::default();
    unsafe { GetIconInfo(icon, &mut info) }.ok()?;
    let image = unsafe { bitmaps_to_image(info.hbmColor, info.hbmMask) };
    for bitmap in [info.hbmColor, info.hbmMask] {
        if !bitmap.is_invalid() {
            let _ = unsafe { DeleteObject(bitmap.into()) };
        }
    }
    image
}

unsafe fn bitmaps_to_image(color: HBITMAP, mask: HBITMAP) -> Option<ColorImage> {
    if color.is_invalid() {
        return None;
    }
    let (w, h) = unsafe { bitmap_size(color) }?;
    let dc = unsafe { GetDC(None) };
    let pixels = unsafe { read_bits(dc, color, w, h) };
    let needs_mask = pixels.as_ref().is_some_and(|p| p.chunks_exact(4).all(|px| px[3] == 0));
    let mask = if needs_mask && !mask.is_invalid() { unsafe { read_bits(dc, mask, w, h) } } else { None };
    unsafe { ReleaseDC(None, dc) };
    let rgba = bgra_to_rgba(&pixels?, mask.as_deref());
    Some(ColorImage::from_rgba_unmultiplied([w as usize, h as usize], &rgba))
}

unsafe fn bitmap_size(bitmap: HBITMAP) -> Option<(i32, i32)> {
    let mut header = BITMAP::default();
    let got = unsafe {
        GetObjectW(bitmap.into(), size_of::<BITMAP>() as i32, Some((&mut header as *mut BITMAP).cast::<c_void>()))
    };
    let (w, h) = (header.bmWidth, header.bmHeight.abs());
    let fits = (1..=MAX_ICON_SIDE).contains(&w) && (1..=MAX_ICON_SIDE).contains(&h);
    (got != 0 && fits).then_some((w, h))
}

unsafe fn read_bits(dc: HDC, bitmap: HBITMAP, w: i32, h: i32) -> Option<Vec<u8>> {
    let mut info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: w,
            biHeight: -h,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut buf = vec![0u8; w as usize * h as usize * 4];
    let lines = unsafe { GetDIBits(dc, bitmap, 0, h as u32, Some(buf.as_mut_ptr().cast()), &mut info, DIB_RGB_COLORS) };
    (lines == h).then_some(buf)
}

fn bgra_to_rgba(bgra: &[u8], mask: Option<&[u8]>) -> Vec<u8> {
    let has_alpha = bgra.chunks_exact(4).any(|px| px[3] != 0);
    let mut out = Vec::with_capacity(bgra.len());
    for (i, px) in bgra.chunks_exact(4).enumerate() {
        let alpha = if has_alpha { px[3] } else { mask_alpha(mask, i) };
        out.extend_from_slice(&[px[2], px[1], px[0], alpha]);
    }
    out
}

fn mask_alpha(mask: Option<&[u8]>, pixel: usize) -> u8 {
    match mask.and_then(|m| m.get(pixel * 4..pixel * 4 + 3)) {
        Some(m) if m.iter().any(|&c| c != 0) => 0,
        _ => 255,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bgra_converts_and_masks() {
        assert_eq!(bgra_to_rgba(&[1, 2, 3, 200], None), vec![3, 2, 1, 200]);
        let pixels = [10, 20, 30, 0, 40, 50, 60, 0];
        let mask = [255, 255, 255, 0, 0, 0, 0, 0];
        assert_eq!(bgra_to_rgba(&pixels, Some(&mask)), vec![30, 20, 10, 0, 60, 50, 40, 255]);
        assert_eq!(bgra_to_rgba(&pixels, None), vec![30, 20, 10, 255, 60, 50, 40, 255]);
    }
}
