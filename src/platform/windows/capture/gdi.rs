//! Grabs a monitor with a GDI BitBlt, the fallback when Desktop Duplication fails.

use std::ffi::c_void;
use std::ptr::null_mut;

use windows_sys::Win32::Graphics::Gdi::{
    BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BitBlt, CAPTUREBLT, CreateCompatibleBitmap, CreateCompatibleDC,
    DIB_RGB_COLORS, DeleteDC, DeleteObject, GetDC, GetDIBits, HBITMAP, HDC, ReleaseDC, SRCCOPY, SelectObject,
};

use super::Area;

const BITS_PER_PIXEL: u16 = 32;

pub(super) fn grab_bgra(area: Area) -> Result<Vec<u8>, String> {
    let screen = unsafe { GetDC(null_mut()) };
    if screen.is_null() {
        return Err("the screen can't be read".into());
    }
    let pixels = copy_screen(screen, area);
    unsafe { ReleaseDC(null_mut(), screen) };
    pixels.ok_or_else(|| "screen capture failed".into())
}

fn copy_screen(screen: HDC, area: Area) -> Option<Vec<u8>> {
    let memory = unsafe { CreateCompatibleDC(screen) };
    if memory.is_null() {
        return None;
    }
    let bitmap = unsafe { CreateCompatibleBitmap(screen, area.width, area.height) };
    let pixels = if bitmap.is_null() {
        None
    } else {
        let pixels = blit(screen, memory, bitmap, area);
        unsafe { DeleteObject(bitmap) };
        pixels
    };
    unsafe { DeleteDC(memory) };
    pixels
}

fn blit(screen: HDC, memory: HDC, bitmap: HBITMAP, area: Area) -> Option<Vec<u8>> {
    let Area { x, y, width, height } = area;
    let previous = unsafe { SelectObject(memory, bitmap) };
    let copied = unsafe { BitBlt(memory, 0, 0, width, height, screen, x, y, SRCCOPY | CAPTUREBLT) } != 0;
    unsafe { SelectObject(memory, previous) };
    if !copied {
        return None;
    }
    let mut pixels = vec![0u8; area.bytes()];
    let mut info = bitmap_info(width, height);
    let rows = unsafe {
        GetDIBits(memory, bitmap, 0, height as u32, pixels.as_mut_ptr().cast::<c_void>(), &mut info, DIB_RGB_COLORS)
    };
    (rows == height).then_some(pixels)
}

fn bitmap_info(width: i32, height: i32) -> BITMAPINFO {
    BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width,
            biHeight: -height,
            biPlanes: 1,
            biBitCount: BITS_PER_PIXEL,
            biCompression: BI_RGB,
            ..Default::default()
        },
        ..Default::default()
    }
}
