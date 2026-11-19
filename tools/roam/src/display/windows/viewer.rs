//! A lent screen: a topmost window over all of it that shows the host's pictures, decoded with
//! Media Foundation and drawn with GDI, and hands what the mouse and keyboard do over it to
//! the host. The window lives on its own thread, which pumps its messages; pictures are decoded
//! on the stream's thread and only drawn on the window's.

use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BeginPaint, DIB_RGB_COLORS, EndPaint, HALFTONE, InvalidateRect, PAINTSTRUCT,
    SRCCOPY, SetStretchBltMode, StretchDIBits,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::{ReleaseCapture, SetCapture, SetFocus};
use windows::Win32::UI::WindowsAndMessaging::{
    CREATESTRUCTW, CS_HREDRAW, CS_VREDRAW, CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW,
    GWLP_USERDATA, GetClientRect, GetMessageW, GetWindowLongPtrW, MA_ACTIVATE, MSG, PostMessageW, PostQuitMessage,
    RegisterClassW, SW_SHOW, SetCursor, SetForegroundWindow, SetWindowLongPtrW, ShowWindow, TranslateMessage, WM_APP,
    WM_DESTROY, WM_ERASEBKGND, WM_KEYDOWN, WM_KEYUP, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MBUTTONDOWN, WM_MBUTTONUP,
    WM_MOUSEACTIVATE, WM_MOUSEHWHEEL, WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_NCCREATE, WM_NCDESTROY, WM_PAINT, WM_RBUTTONDOWN,
    WM_RBUTTONUP, WM_SETCURSOR, WM_SYSKEYDOWN, WM_SYSKEYUP, WM_XBUTTONDOWN, WM_XBUTTONUP, WNDCLASSW, WS_EX_TOOLWINDOW,
    WS_EX_TOPMOST, WS_POPUP,
};
use windows::core::w;

use super::decoder::Decoder;
use crate::display::{Picture, Remote, RemoteSink, Showing, nv12};
use crate::keys;
use crate::layout::Rect;

/// Asks the window to close itself, from another thread.
const CLOSE: u32 = WM_APP + 1;

/// What the window and the stream share.
struct Shared {
    remote: RemoteSink,
    /// The newest picture in BGRA, `size` pixels.
    frame: Mutex<Vec<u8>>,
    size: [u32; 2],
}

pub struct Viewer {
    shared: Arc<Shared>,
    window: isize,
    thread: Option<JoinHandle<()>>,
    decoder: Decoder,
    bgra: Vec<u8>,
}

// SAFETY: the decoder is a free-threaded Media Foundation object made in the multithreaded
// apartment, and the window is only ever posted to by its handle.
unsafe impl Send for Viewer {}

impl Viewer {
    pub fn open(rect: Rect, size: [u32; 2], remote: RemoteSink) -> Result<Self, String> {
        let decoder = Decoder::new(size)?;
        let pixels = size[0] as usize * size[1] as usize * 4;
        let shared = Arc::new(Shared { remote, frame: Mutex::new(vec![0; pixels]), size });
        let (made, window) = mpsc::channel();
        let thread = {
            let shared = shared.clone();
            std::thread::Builder::new()
                .name("roam-display-window".into())
                .spawn(move || run(rect, shared, &made))
                .map_err(|err| err.to_string())?
        };
        let window = match window.recv() {
            Ok(Ok(window)) => window,
            Ok(Err(why)) => return Err(why),
            Err(_) => return Err("The window for the picture didn't open".into()),
        };
        Ok(Self { shared, window, thread: Some(thread), decoder, bgra: vec![0; pixels] })
    }

    fn hwnd(&self) -> HWND {
        HWND(self.window as *mut _)
    }
}

impl Showing for Viewer {
    fn picture(&mut self, picture: &Picture) -> Result<(), String> {
        let Some(decoded) = self.decoder.decode(&picture.data)? else { return Ok(()) };
        let [width, height] = self.shared.size.map(|side| side as usize);
        if decoded.stride < width || decoded.nv12.len() < decoded.stride * decoded.rows * 3 / 2 || decoded.rows < height
        {
            return Err("The picture came out the wrong size".into());
        }
        nv12::to_bgra(&decoded.nv12, [width, height], decoded.stride, decoded.rows, &mut self.bgra);
        if let Ok(mut frame) = self.shared.frame.lock() {
            std::mem::swap(&mut *frame, &mut self.bgra);
        }
        // SAFETY: asks the window, on its own thread, to draw again.
        let _ = unsafe { InvalidateRect(Some(self.hwnd()), None, false) };
        Ok(())
    }
}

impl Drop for Viewer {
    fn drop(&mut self) {
        // SAFETY: the window closes itself on its own thread.
        let _ = unsafe { PostMessageW(Some(self.hwnd()), CLOSE, WPARAM(0), LPARAM(0)) };
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// Makes the window and pumps its messages until it closes.
fn run(rect: Rect, shared: Arc<Shared>, made: &mpsc::Sender<Result<isize, String>>) {
    let class = w!("RoamLentDisplay");
    // SAFETY: a window class with a valid procedure, then a window of it on this thread, which
    // pumps its messages until it's destroyed. The shared state's pointer lives in the window
    // and is taken back when the window goes.
    unsafe {
        let instance = GetModuleHandleW(None).unwrap_or_default();
        let info = WNDCLASSW {
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(procedure),
            hInstance: instance.into(),
            lpszClassName: class,
            ..Default::default()
        };
        RegisterClassW(&info);
        let raw = Arc::into_raw(shared);
        let window = CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
            class,
            w!("Roam"),
            WS_POPUP,
            rect.x.round() as i32,
            rect.y.round() as i32,
            rect.w.round() as i32,
            rect.h.round() as i32,
            None,
            None,
            Some(instance.into()),
            Some(raw.cast()),
        );
        let window = match window {
            Ok(window) => window,
            Err(err) => {
                drop(Arc::from_raw(raw));
                let _ = made.send(Err(format!("Couldn't open the window for the picture: {}", err.message())));
                return;
            }
        };
        let _ = ShowWindow(window, SW_SHOW);
        let _ = SetForegroundWindow(window);
        let _ = made.send(Ok(window.0 as isize));
        let mut message = MSG::default();
        while GetMessageW(&mut message, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }
}

/// The shared state of a window, while it's alive.
unsafe fn shared<'a>(window: HWND) -> Option<&'a Shared> {
    // SAFETY: the pointer was set from `Arc::into_raw` on creation and is cleared before it's
    // dropped.
    unsafe { (GetWindowLongPtrW(window, GWLP_USERDATA) as *const Shared).as_ref() }
}

fn low(value: isize) -> i32 {
    i32::from(value as u16 as i16)
}

fn high(value: isize) -> i32 {
    i32::from((value >> 16) as u16 as i16)
}

unsafe extern "system" fn procedure(window: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    // SAFETY: everything below is on the window's own thread, with what Windows passed in.
    unsafe {
        if message == WM_NCCREATE {
            let create = &*(lparam.0 as *const CREATESTRUCTW);
            SetWindowLongPtrW(window, GWLP_USERDATA, create.lpCreateParams as isize);
            return DefWindowProcW(window, message, wparam, lparam);
        }
        if message == WM_NCDESTROY {
            let raw = SetWindowLongPtrW(window, GWLP_USERDATA, 0) as *const Shared;
            if !raw.is_null() {
                drop(Arc::from_raw(raw));
            }
            return DefWindowProcW(window, message, wparam, lparam);
        }
        let Some(shared) = shared(window) else { return DefWindowProcW(window, message, wparam, lparam) };
        let send = |remote: Remote| (shared.remote)(remote);
        match message {
            CLOSE => {
                let _ = DestroyWindow(window);
                LRESULT(0)
            }
            WM_DESTROY => {
                PostQuitMessage(0);
                LRESULT(0)
            }
            WM_ERASEBKGND => LRESULT(1),
            WM_PAINT => {
                paint(window, shared);
                LRESULT(0)
            }
            WM_SETCURSOR => {
                // The host draws its own pointer into the picture.
                SetCursor(None);
                LRESULT(1)
            }
            WM_MOUSEACTIVATE => {
                let _ = SetFocus(Some(window));
                LRESULT(MA_ACTIVATE as isize)
            }
            WM_MOUSEMOVE => {
                let mut client = RECT::default();
                let _ = GetClientRect(window, &mut client);
                let across = (client.right - client.left).max(1) as f32;
                let down = (client.bottom - client.top).max(1) as f32;
                send(Remote::Move([low(lparam.0) as f32 / across, high(lparam.0) as f32 / down]));
                LRESULT(0)
            }
            WM_LBUTTONDOWN | WM_LBUTTONUP | WM_RBUTTONDOWN | WM_RBUTTONUP | WM_MBUTTONDOWN | WM_MBUTTONUP
            | WM_XBUTTONDOWN | WM_XBUTTONUP => {
                let button = match message {
                    WM_LBUTTONDOWN | WM_LBUTTONUP => 0,
                    WM_RBUTTONDOWN | WM_RBUTTONUP => 1,
                    WM_MBUTTONDOWN | WM_MBUTTONUP => 2,
                    _ if (wparam.0 >> 16) & 0xFFFF == 2 => 4,
                    _ => 3,
                };
                let down = matches!(message, WM_LBUTTONDOWN | WM_RBUTTONDOWN | WM_MBUTTONDOWN | WM_XBUTTONDOWN);
                if down {
                    SetCapture(window);
                } else {
                    let _ = ReleaseCapture();
                }
                send(Remote::Button { button, down });
                LRESULT(if matches!(message, WM_XBUTTONDOWN | WM_XBUTTONUP) { 1 } else { 0 })
            }
            WM_MOUSEWHEEL | WM_MOUSEHWHEEL => {
                let amount = high(wparam.0 as isize) as f32;
                let (dx, dy) = if message == WM_MOUSEWHEEL { (0.0, amount) } else { (amount, 0.0) };
                send(Remote::Wheel { dx, dy });
                LRESULT(0)
            }
            WM_KEYDOWN | WM_KEYUP | WM_SYSKEYDOWN | WM_SYSKEYUP => {
                let down = matches!(message, WM_KEYDOWN | WM_SYSKEYDOWN);
                let repeat = down && lparam.0 & (1 << 30) != 0;
                let scan = ((lparam.0 >> 16) & 0xFF) as u16;
                let extended = lparam.0 & (1 << 24) != 0;
                if !repeat && let Some(hid) = keys::from_windows(scan, extended) {
                    send(Remote::Key { hid, down });
                }
                // Alt and F10 go to the host, not to this window's (missing) menu.
                LRESULT(0)
            }
            _ => DefWindowProcW(window, message, wparam, lparam),
        }
    }
}

/// Draws the newest picture over the whole window.
unsafe fn paint(window: HWND, shared: &Shared) {
    // SAFETY: a paint cycle on the window's own thread, drawing from a buffer that's locked
    // for the call.
    unsafe {
        let mut paint = PAINTSTRUCT::default();
        let dc = BeginPaint(window, &mut paint);
        let mut client = RECT::default();
        let _ = GetClientRect(window, &mut client);
        let [width, height] = shared.size.map(|side| side as i32);
        let info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width,
                biHeight: -height,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let (across, down) = (client.right - client.left, client.bottom - client.top);
        if [across, down] != [width, height] {
            SetStretchBltMode(dc, HALFTONE);
        }
        if let Ok(frame) = shared.frame.lock() {
            StretchDIBits(
                dc,
                0,
                0,
                across,
                down,
                0,
                0,
                width,
                height,
                Some(frame.as_ptr().cast()),
                &info,
                DIB_RGB_COLORS,
                SRCCOPY,
            );
        }
        let _ = EndPaint(window, &paint);
    }
}
