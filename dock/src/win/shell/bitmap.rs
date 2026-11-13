//! An owned HBITMAP, and reading its pixels out as top-down RGBA up to 4096 px a side.

use ::windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAP, BITMAPINFO, BITMAPINFOHEADER, DIB_RGB_COLORS, DeleteObject, GetDC, GetDIBits, GetObjectW, HBITMAP,
    ReleaseDC,
};

const LARGEST_READABLE_SIDE: i32 = 4096;

pub(crate) struct OwnedBitmap(HBITMAP);

pub(crate) struct RgbaPixels {
    pub(crate) rgba: Vec<u8>,
    pub(crate) width: usize,
    pub(crate) height: usize,
}

impl OwnedBitmap {
    pub(crate) fn new(handle: HBITMAP) -> Option<Self> {
        (!handle.is_invalid()).then_some(Self(handle))
    }

    pub(crate) fn handle(&self) -> HBITMAP {
        self.0
    }

    /// For a bitmap the system took over, so we mustn't delete it.
    pub(crate) fn hand_over_to_system(self) {
        std::mem::forget(self);
    }

    pub(crate) fn size(&self) -> Option<(i32, i32)> {
        let mut info = BITMAP::default();
        let written =
            unsafe { GetObjectW(self.0.into(), size_of::<BITMAP>() as i32, Some((&mut info as *mut BITMAP).cast())) };
        (written != 0).then_some((info.bmWidth, info.bmHeight.abs()))
    }

    pub(crate) fn read_rgba(&self) -> Option<RgbaPixels> {
        let (width, height) = self.size()?;
        let readable = 1..=LARGEST_READABLE_SIDE;
        if !readable.contains(&width) || !readable.contains(&height) {
            return None;
        }
        let mut bgra = vec![0u8; width as usize * height as usize * 4];
        let mut format = top_down_32bpp(width, height);
        let rows_read = unsafe {
            let screen = GetDC(None);
            if screen.is_invalid() {
                return None;
            }
            let rows = GetDIBits(
                screen,
                self.0,
                0,
                height as u32,
                Some(bgra.as_mut_ptr().cast()),
                &mut format,
                DIB_RGB_COLORS,
            );
            ReleaseDC(None, screen);
            rows
        };
        if rows_read != height {
            return None;
        }
        Some(RgbaPixels { rgba: swap_red_and_blue(bgra), width: width as usize, height: height as usize })
    }
}

impl Drop for OwnedBitmap {
    fn drop(&mut self) {
        unsafe { _ = DeleteObject(self.0.into()) };
    }
}

fn top_down_32bpp(width: i32, height: i32) -> BITMAPINFO {
    BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width,
            biHeight: -height,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    }
}

fn swap_red_and_blue(mut pixels: Vec<u8>) -> Vec<u8> {
    for pixel in pixels.chunks_exact_mut(4) {
        pixel.swap(0, 2);
    }
    pixels
}
