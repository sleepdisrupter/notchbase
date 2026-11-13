//! An app's windows: listing them, taking pictures of them off the main thread (PrintWindow, which asks the window to
//! draw itself, so it works for covered windows too), and raising, minimizing, restoring or closing one. Frames are
//! physical pixels on the virtual screen.

use std::collections::HashMap;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::Instant;

use egui::Rect;
use windows_sys::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{
    BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BitBlt, CreateCompatibleDC, CreateDIBSection, DIB_RGB_COLORS, DeleteDC,
    DeleteObject, GetDC, ReleaseDC, SRCCOPY, SelectObject,
};
use windows_sys::Win32::Storage::Xps::{PRINT_WINDOW_FLAGS, PrintWindow};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GetForegroundWindow, IsWindow, PostMessageW, SW_MINIMIZE, SW_RESTORE, ShowWindow, WM_CLOSE,
};

use super::os::RectPx;
use super::sys;

/// PW_RENDERFULLCONTENT: windows drawn by the GPU (browsers, Electron apps) draw into the picture too.
const RENDER_FULL_CONTENT: PRINT_WINDOW_FLAGS = 2;

#[derive(Clone, Debug, PartialEq)]
pub struct Win {
    pub id: u32,
    pub title: String,
    /// Physical pixels on the virtual screen. A minimized window's is where it comes back to.
    pub frame: Rect,
    pub on_screen: bool,
}

/// Window pictures need no permission on Windows.
pub fn can_capture(_ask: bool) -> bool {
    true
}

fn win_of(hwnd: HWND) -> Win {
    let on_screen = !sys::iconic(hwnd);
    let frame =
        if on_screen { sys::frame(hwnd).map(sys::rect_of) } else { sys::placement(hwnd).map(|(r, _)| sys::rect_of(r)) };
    Win { id: sys::id_of(hwnd), title: sys::title_of(hwnd), frame: frame.unwrap_or(Rect::NOTHING), on_screen }
}

/// An app's windows, front to back, minimized ones included.
pub fn of_app(pid: i32) -> Vec<Win> {
    super::os::windows_of(pid).into_iter().map(win_of).collect()
}

/// Every app window: its app, its id, and whether it's on screen (not minimized).
pub fn every_window() -> Vec<(i32, u32, bool)> {
    sys::app_windows().into_iter().map(|h| (sys::pid_of(h), sys::id_of(h), !sys::iconic(h))).collect()
}

pub fn minimized_of(pid: i32) -> Vec<Win> {
    of_app(pid).into_iter().filter(|w| !w.on_screen).collect()
}

/// Captures windows on a thread of its own and keeps the latest picture of each.
pub struct Thumbs {
    ask: Sender<(u32, u32)>,
    done: Receiver<(u32, egui::ColorImage)>,
    pub cache: HashMap<u32, (egui::TextureHandle, Instant)>,
    asked: HashMap<u32, Instant>,
}

fn texture_options() -> egui::TextureOptions {
    egui::TextureOptions {
        magnification: egui::TextureFilter::Linear,
        minification: egui::TextureFilter::Linear,
        wrap_mode: egui::TextureWrapMode::ClampToEdge,
        mipmap_mode: Some(egui::TextureFilter::Linear),
    }
}

impl Thumbs {
    pub fn new() -> Self {
        let (ask, requests) = channel::<(u32, u32)>();
        let (send, done) = channel();
        std::thread::Builder::new()
            .name("window pictures".into())
            .spawn(move || {
                while let Ok(first) = requests.recv() {
                    // Only the newest request for each window matters.
                    let mut wanted = HashMap::from([(first.0, first.1)]);
                    while let Ok((id, px)) = requests.try_recv() {
                        wanted.insert(id, px);
                    }
                    for (id, px) in wanted {
                        if let Some(image) = picture(id, px)
                            && send.send((id, image)).is_err()
                        {
                            return;
                        }
                    }
                }
            })
            .expect("a thread for window pictures");
        Self { ask, done, cache: HashMap::new(), asked: HashMap::new() }
    }

    /// Asks for a fresh picture of `id`, `px` pixels tall, at most every `every` seconds.
    pub fn want(&mut self, id: u32, px: u32, every: f32) {
        let now = Instant::now();
        if self.asked.get(&id).is_some_and(|at| now.duration_since(*at).as_secs_f32() < every) {
            return;
        }
        self.asked.insert(id, now);
        let _ = self.ask.send((id, px));
    }

    /// Turns finished pictures into textures. Returns true when one arrived.
    pub fn collect(&mut self, ctx: &egui::Context) -> bool {
        let mut any = false;
        while let Ok((id, image)) = self.done.try_recv() {
            match self.cache.get_mut(&id) {
                Some((texture, at)) => {
                    texture.set(image, texture_options());
                    *at = Instant::now();
                }
                None => {
                    let texture = ctx.load_texture(format!("window {id}"), image, texture_options());
                    self.cache.insert(id, (texture, Instant::now()));
                }
            }
            any = true;
        }
        any
    }

    pub fn get(&self, id: u32) -> Option<&egui::TextureHandle> {
        self.cache.get(&id).map(|(t, _)| t)
    }

    /// Takes a picture that was captured elsewhere, so it shows straight away.
    pub fn put(&mut self, ctx: &egui::Context, id: u32, image: egui::ColorImage) {
        let texture = ctx.load_texture(format!("window {id}"), image, texture_options());
        self.cache.insert(id, (texture, Instant::now()));
        self.asked.insert(id, Instant::now());
    }

    /// Forgets windows that are gone.
    pub fn keep(&mut self, alive: impl Fn(u32) -> bool) {
        self.cache.retain(|id, _| alive(*id));
        self.asked.retain(|id, _| alive(*id));
    }
}

/// A picture of window `id`, `px` pixels tall, taken now. A minimized window draws nothing, so it gets the last
/// picture taken of it while it was up.
pub fn picture(id: u32, px: u32) -> Option<egui::ColorImage> {
    let hwnd = sys::hwnd_of(id);
    if unsafe { IsWindow(hwnd) } == 0 {
        return None;
    }
    if sys::iconic(hwnd) {
        return super::hooks::last_picture(id).map(|(_, image)| smaller(&image, px as usize));
    }
    capture(hwnd).map(|image| smaller(&image, px as usize))
}

/// The window as it draws itself, cropped to its visible frame, full size.
pub fn capture(hwnd: HWND) -> Option<egui::ColorImage> {
    let outer = sys::outer(hwnd)?;
    let shown = sys::frame(hwnd).unwrap_or(outer);
    let (w, h) = (outer.right - outer.left, outer.bottom - outer.top);
    if w <= 0 || h <= 0 || w > 16_384 || h > 16_384 {
        return None;
    }
    let pixels = unsafe { draw_into_bitmap(w, h, |dc| PrintWindow(hwnd, dc, RENDER_FULL_CONTENT) != 0) }?;
    // The invisible resize borders around the visible frame.
    let (x0, y0) = ((shown.left - outer.left).clamp(0, w) as usize, (shown.top - outer.top).clamp(0, h) as usize);
    let (x1, y1) = ((shown.right - outer.left).clamp(0, w) as usize, (shown.bottom - outer.top).clamp(0, h) as usize);
    let image = crop(&pixels, w as usize, x0, y0, x1, y1)?;
    // A window that drew nothing (some GPU apps when covered) comes out black: no picture rather than a black one.
    let lit = image.pixels.iter().step_by(97).any(|c| c.r() > 8 || c.g() > 8 || c.b() > 8);
    lit.then_some(image)
}

/// A 32-bit top-down bitmap `w` by `h`, drawn by `draw`, as BGRA bytes.
unsafe fn draw_into_bitmap(
    w: i32,
    h: i32,
    draw: impl FnOnce(windows_sys::Win32::Graphics::Gdi::HDC) -> bool,
) -> Option<Vec<u8>> {
    unsafe {
        let screen = GetDC(std::ptr::null_mut());
        if screen.is_null() {
            return None;
        }
        let dc = CreateCompatibleDC(screen);
        let mut info: BITMAPINFO = std::mem::zeroed();
        info.bmiHeader = BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: w,
            biHeight: -h,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB,
            ..std::mem::zeroed()
        };
        let mut bits: *mut std::ffi::c_void = std::ptr::null_mut();
        let bitmap = CreateDIBSection(dc, &info, DIB_RGB_COLORS, &mut bits, std::ptr::null_mut(), 0);
        let mut out = None;
        if !bitmap.is_null() && !bits.is_null() {
            let old = SelectObject(dc, bitmap);
            if draw(dc) {
                let len = w as usize * h as usize * 4;
                out = Some(std::slice::from_raw_parts(bits as *const u8, len).to_vec());
            }
            SelectObject(dc, old);
            DeleteObject(bitmap);
        }
        DeleteDC(dc);
        ReleaseDC(std::ptr::null_mut(), screen);
        out
    }
}

/// Part of a BGRA picture `w` wide, as an opaque egui image.
fn crop(bgra: &[u8], w: usize, x0: usize, y0: usize, x1: usize, y1: usize) -> Option<egui::ColorImage> {
    let (cw, ch) = (x1.checked_sub(x0)?, y1.checked_sub(y0)?);
    if cw == 0 || ch == 0 {
        return None;
    }
    let mut pixels = Vec::with_capacity(cw * ch);
    for y in y0..y1 {
        let row = &bgra[(y * w + x0) * 4..(y * w + x1) * 4];
        pixels.extend(row.chunks_exact(4).map(|p| egui::Color32::from_rgb(p[2], p[1], p[0])));
    }
    Some(egui::ColorImage::new([cw, ch], pixels))
}

/// What part of the screen looks like right now, `px` pixels tall: for the flight recorder.
pub fn screen_picture(size: egui::Vec2, px: u32) -> Option<egui::ColorImage> {
    let (w, h) = (size.x as i32, size.y as i32);
    if w <= 0 || h <= 0 {
        return None;
    }
    let pixels = unsafe {
        draw_into_bitmap(w, h, |dc| {
            let screen = GetDC(std::ptr::null_mut());
            let ok = BitBlt(dc, 0, 0, w, h, screen, 0, 0, SRCCOPY) != 0;
            ReleaseDC(std::ptr::null_mut(), screen);
            ok
        })
    }?;
    crop(&pixels, w as usize, 0, 0, w as usize, h as usize).map(|i| smaller(&i, px as usize))
}

/// A smaller copy of `image`, at most `height` tall, averaging blocks of pixels.
pub fn smaller(image: &egui::ColorImage, height: usize) -> egui::ColorImage {
    let [w, h] = image.size;
    if h <= height || w == 0 || h == 0 || height == 0 {
        return image.clone();
    }
    let step = h.div_ceil(height);
    let (nw, nh) = ((w / step).max(1), (h / step).max(1));
    let mut out = Vec::with_capacity(nw * nh);
    for y in 0..nh {
        for x in 0..nw {
            let mut sum = [0u32; 4];
            for dy in 0..step {
                for dx in 0..step {
                    let c = image.pixels[(y * step + dy).min(h - 1) * w + (x * step + dx).min(w - 1)];
                    for (s, v) in sum.iter_mut().zip(c.to_array()) {
                        *s += u32::from(v);
                    }
                }
            }
            let n = (step * step) as u32;
            let [r, g, b, a] = sum.map(|s| (s / n) as u8);
            out.push(egui::Color32::from_rgba_premultiplied(r, g, b, a));
        }
    }
    egui::ColorImage::new([nw, nh], out)
}

/// Brings a window forward, restoring it if it's minimized.
pub fn raise(_pid: i32, win: &Win) {
    super::os::bring_forward(sys::hwnd_of(win.id));
}

/// Asks a window to close, as its close button would.
pub fn close(_pid: i32, win: &Win) {
    unsafe { PostMessageW(sys::hwnd_of(win.id), WM_CLOSE, 0 as WPARAM, 0 as LPARAM) };
}

pub fn frame_of(_pid: i32, id: u32) -> Option<Rect> {
    sys::frame(sys::hwnd_of(id)).map(sys::rect_of)
}

pub fn set_minimized(_pid: i32, id: u32, minimized: bool) -> bool {
    let hwnd = sys::hwnd_of(id);
    unsafe { ShowWindow(hwnd, if minimized { SW_MINIMIZE } else { SW_RESTORE }) };
    sys::iconic(hwnd) == minimized
}

/// Whether the front window fills the whole of `screen`, as a full-screen game or video does.
pub fn fullscreen_on(screen: RectPx) -> bool {
    let hwnd = unsafe { GetForegroundWindow() };
    if hwnd.is_null() || sys::iconic(hwnd) {
        return false;
    }
    if matches!(sys::class_name(hwnd).as_str(), "Progman" | "WorkerW" | "Shell_TrayWnd" | "Shell_SecondaryTrayWnd") {
        return false;
    }
    if sys::pid_of(hwnd) == std::process::id() as i32 {
        return false;
    }
    sys::outer(hwnd).is_some_and(|r| {
        r.left <= screen.left && r.top <= screen.top && r.right >= screen.right && r.bottom >= screen.bottom
    })
}
