//! Copies the 31 by 31 pixels around the cursor from the screen with GDI, into a bitmap kept between uses.

use std::ptr::null_mut;

use windows_sys::Win32::Graphics::Gdi::{
    BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BitBlt, CAPTUREBLT, CreateCompatibleBitmap, CreateCompatibleDC,
    DIB_RGB_COLORS, DeleteDC, DeleteObject, GetDC, GetDIBits, HBITMAP, HDC, ReleaseDC, SRCCOPY, SelectObject,
};

use crate::capture::color::loupe::GRAB;
use crate::platform::Capture;

const SIDE: i32 = GRAB as i32;
const UNREADABLE: &str = "couldn't read the screen";

pub(super) struct Grabber {
    dc: HDC,
    bitmap: HBITMAP,
}

impl Grabber {
    pub(super) fn new() -> Result<Self, String> {
        let screen = ScreenDc::get().ok_or(UNREADABLE)?;
        let grabber =
            unsafe { Self { dc: CreateCompatibleDC(screen.0), bitmap: CreateCompatibleBitmap(screen.0, SIDE, SIDE) } };
        if grabber.dc.is_null() || grabber.bitmap.is_null() {
            return Err(UNREADABLE.into());
        }
        Ok(grabber)
    }

    pub(super) fn grab(&self, at: [i32; 2]) -> Option<Capture> {
        if !self.copy_screen(at) {
            return None;
        }
        let mut rgba = self.read_bgra()?;
        bgra_to_opaque_rgba(&mut rgba);
        Some(Capture { width: GRAB, height: GRAB, rgba })
    }

    fn copy_screen(&self, at: [i32; 2]) -> bool {
        let Some(screen) = ScreenDc::get() else { return false };
        let (x, y) = (at[0] - SIDE / 2, at[1] - SIDE / 2);
        unsafe {
            let previous = SelectObject(self.dc, self.bitmap);
            // CAPTUREBLT takes layered windows too, so the grab matches what's on screen.
            let copied = BitBlt(self.dc, 0, 0, SIDE, SIDE, screen.0, x, y, SRCCOPY | CAPTUREBLT);
            SelectObject(self.dc, previous);
            copied != 0
        }
    }

    fn read_bgra(&self) -> Option<Vec<u8>> {
        let header = BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: SIDE,
            biHeight: -SIDE,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB,
            ..Default::default()
        };
        let mut info = BITMAPINFO { bmiHeader: header, ..Default::default() };
        let mut bgra = vec![0u8; GRAB * GRAB * 4];
        let rows = unsafe {
            GetDIBits(self.dc, self.bitmap, 0, SIDE as u32, bgra.as_mut_ptr().cast(), &mut info, DIB_RGB_COLORS)
        };
        (rows == SIDE).then_some(bgra)
    }
}

impl Drop for Grabber {
    fn drop(&mut self) {
        if !self.bitmap.is_null() {
            unsafe { DeleteObject(self.bitmap) };
        }
        if !self.dc.is_null() {
            unsafe { DeleteDC(self.dc) };
        }
    }
}

struct ScreenDc(HDC);

impl ScreenDc {
    fn get() -> Option<Self> {
        let dc = unsafe { GetDC(null_mut()) };
        (!dc.is_null()).then(|| Self(dc))
    }
}

impl Drop for ScreenDc {
    fn drop(&mut self) {
        unsafe { ReleaseDC(null_mut(), self.0) };
    }
}

fn bgra_to_opaque_rgba(pixels: &mut [u8]) {
    for px in pixels.chunks_exact_mut(4) {
        px.swap(0, 2);
        px[3] = 255;
    }
}

#[cfg(test)]
mod tests {
    use super::super::screen::cursor_pos;
    use super::*;

    #[test]
    fn dib_pixels_become_opaque_rgba() {
        let mut pixels = [10, 20, 30, 0, 1, 2, 3, 77];
        bgra_to_opaque_rgba(&mut pixels);
        assert_eq!(pixels, [30, 20, 10, 255, 3, 2, 1, 255]);
    }

    #[test]
    fn grabber_reads_square_repeatedly() {
        let Ok(grabber) = Grabber::new() else { return };
        let Some(grab) = grabber.grab(cursor_pos().unwrap_or([0, 0])) else { return };
        assert_eq!((grab.width, grab.height, grab.rgba.len()), (GRAB, GRAB, GRAB * GRAB * 4));
        assert!(grab.rgba.chunks_exact(4).all(|px| px[3] == 255));
        for _ in 0..30 {
            assert!(grabber.grab([0, 0]).is_some());
        }
    }
}
