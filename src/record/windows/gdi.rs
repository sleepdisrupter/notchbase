//! The fallback when duplication can't start. Each frame copies the area from the screen with BitBlt into a DIB
//! section and draws the cursor at its hotspot.

use std::ffi::c_void;
use std::ptr::null_mut;
use std::time::Instant;

use windows_sys::Win32::Graphics::Gdi::{
    BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BitBlt, CAPTUREBLT, CreateCompatibleDC, CreateDIBSection, DIB_RGB_COLORS,
    DeleteDC, DeleteObject, GdiFlush, GetDC, HBITMAP, HDC, HGDIOBJ, ReleaseDC, SRCCOPY, SelectObject,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CURSOR_SHOWING, CURSORINFO, DI_NORMAL, DrawIconEx, GetCursorInfo, GetIconInfo, HCURSOR, ICONINFO,
};

use crate::record::Region;

const BYTES_PER_PIXEL: usize = 4;

pub(super) struct Grabber {
    screen: HDC,
    memory: HDC,
    bitmap: HBITMAP,
    previous: HGDIOBJ,
    bits: *mut u8,
    origin: [i32; 2],
    size: [usize; 2],
    composed: Vec<u8>,
    cursor: Option<(HCURSOR, [i32; 2])>,
}

impl Grabber {
    pub(super) fn open(region: &Region) -> Result<Self, String> {
        let size = region.size.map(|side| side as usize);
        let screen = unsafe { GetDC(null_mut()) };
        if screen.is_null() {
            return Err("the screen can't be read".into());
        }
        let memory = unsafe { CreateCompatibleDC(screen) };
        let mut bits = null_mut::<c_void>();
        let info = bitmap_info(region.size);
        let bitmap = if memory.is_null() {
            null_mut()
        } else {
            unsafe { CreateDIBSection(memory, &info, DIB_RGB_COLORS, &mut bits, null_mut(), 0) }
        };
        if bitmap.is_null() || bits.is_null() {
            unsafe {
                if !memory.is_null() {
                    DeleteDC(memory);
                }
                ReleaseDC(null_mut(), screen);
            }
            return Err("the screen can't be copied".into());
        }
        let previous = unsafe { SelectObject(memory, bitmap) };
        Ok(Self {
            screen,
            memory,
            bitmap,
            previous,
            bits: bits.cast(),
            origin: region.desktop_origin(),
            size,
            composed: vec![0; size[0] * size[1] * BYTES_PER_PIXEL],
            cursor: None,
        })
    }

    pub(super) fn wait(&mut self, deadline: Instant) {
        std::thread::sleep(deadline.saturating_duration_since(Instant::now()));
    }

    pub(super) fn frame(&mut self) -> &[u8] {
        let [width, height] = self.size.map(|side| side as i32);
        let [x, y] = self.origin;
        let copied = unsafe { BitBlt(self.memory, 0, 0, width, height, self.screen, x, y, SRCCOPY | CAPTUREBLT) } != 0;
        if copied {
            self.draw_cursor();
            // GDI may batch its drawing, so flush before reading the bits directly.
            unsafe { GdiFlush() };
            let bytes = unsafe { std::slice::from_raw_parts(self.bits, self.composed.len()) };
            for (dst, src) in self.composed.chunks_exact_mut(BYTES_PER_PIXEL).zip(bytes.chunks_exact(BYTES_PER_PIXEL)) {
                dst[..3].copy_from_slice(&src[..3]);
                dst[3] = u8::MAX;
            }
        }
        &self.composed
    }

    fn draw_cursor(&mut self) {
        let mut info = CURSORINFO { cbSize: size_of::<CURSORINFO>() as u32, ..Default::default() };
        if unsafe { GetCursorInfo(&mut info) } == 0 || info.flags & CURSOR_SHOWING == 0 || info.hCursor.is_null() {
            return;
        }
        let hotspot = self.hotspot(info.hCursor);
        let [x, y] =
            [info.ptScreenPos.x - hotspot[0] - self.origin[0], info.ptScreenPos.y - hotspot[1] - self.origin[1]];
        unsafe { DrawIconEx(self.memory, x, y, info.hCursor, 0, 0, 0, null_mut(), DI_NORMAL) };
    }

    /// A cursor's hotspot, cached per cursor since asking makes bitmaps we then have to delete.
    fn hotspot(&mut self, cursor: HCURSOR) -> [i32; 2] {
        if let Some((cached, hotspot)) = self.cursor
            && cached == cursor
        {
            return hotspot;
        }
        let mut icon = ICONINFO::default();
        let hotspot = if unsafe { GetIconInfo(cursor, &mut icon) } != 0 {
            unsafe {
                if !icon.hbmMask.is_null() {
                    DeleteObject(icon.hbmMask);
                }
                if !icon.hbmColor.is_null() {
                    DeleteObject(icon.hbmColor);
                }
            }
            [icon.xHotspot as i32, icon.yHotspot as i32]
        } else {
            [0, 0]
        };
        self.cursor = Some((cursor, hotspot));
        hotspot
    }
}

impl Drop for Grabber {
    fn drop(&mut self) {
        unsafe {
            SelectObject(self.memory, self.previous);
            DeleteObject(self.bitmap);
            DeleteDC(self.memory);
            ReleaseDC(null_mut(), self.screen);
        }
    }
}

fn bitmap_info([width, height]: [u32; 2]) -> BITMAPINFO {
    BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width as i32,
            biHeight: -(height as i32),
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB,
            ..Default::default()
        },
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bitmap_is_top_down_32_bit_rgb() {
        let info = bitmap_info([1280, 720]);
        let header = info.bmiHeader;
        let (size, width, height) = (header.biSize, header.biWidth, header.biHeight);
        assert_eq!((size, width, height), (size_of::<BITMAPINFOHEADER>() as u32, 1280, -720));
        let (planes, bits, compression) = (header.biPlanes, header.biBitCount, header.biCompression);
        assert_eq!((planes, bits, compression), (1, 32, BI_RGB));
        assert_eq!(usize::from(bits) / 8, BYTES_PER_PIXEL);
    }
}
