//! Line-up guides on Windows: a topmost, click-through window over each screen, drawn with
//! GDI. One colour stands for "see through", so everything else on the window shows as drawn.
//! The windows live on a thread of their own, which pumps their messages; the engine's thread
//! leaves what to show and nudges it.

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    ANTIALIASED_QUALITY, BeginPaint, BitBlt, CLIP_DEFAULT_PRECIS, CreateCompatibleBitmap, CreateCompatibleDC,
    CreateFontW, CreateSolidBrush, DEFAULT_CHARSET, DEFAULT_PITCH, DT_CENTER, DT_NOCLIP, DT_NOPREFIX, DT_SINGLELINE,
    DT_VCENTER, DeleteDC, DeleteObject, DrawTextW, EndPaint, FW_BOLD, FW_NORMAL, FillRect, GetStockObject, HDC,
    HGDIOBJ, InvalidateRect, NULL_PEN, OUT_DEFAULT_PRECIS, PAINTSTRUCT, RoundRect, SRCCOPY, SelectObject, SetBkMode,
    SetTextColor, SetViewportOrgEx, TRANSPARENT,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetMessageW, HWND_MESSAGE, HWND_TOPMOST,
    LWA_COLORKEY, MSG, PostMessageW, RegisterClassW, SW_SHOWNOACTIVATE, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
    SetLayeredWindowAttributes, SetWindowPos, ShowWindow, TranslateMessage, WM_APP, WM_ERASEBKGND, WM_NCHITTEST,
    WM_PAINT, WNDCLASSW, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_POPUP,
};
use windows::core::w;

use super::{Guide, Marked, Shape, Shown, bounds_of, mark_scene, scene};
use crate::layout::Rect;

/// The colour that is see-through: one nothing is drawn in.
const KEY: COLORREF = COLORREF(0x00FE_00FF);
/// The guides to show changed.
const GUIDES: u32 = WM_APP + 21;
/// The marks to show changed.
const MARKS: u32 = WM_APP + 22;
/// HTTRANSPARENT: the mouse goes to whatever is under the window.
const SEE_THROUGH: isize = -1;

#[derive(Default)]
struct Wanted {
    guides: Option<Vec<Shown>>,
    marks: Option<Vec<Marked>>,
}

static WANTED: Mutex<Wanted> = Mutex::new(Wanted { guides: None, marks: None });
/// The thread's message window, once it's up.
static CONTROL: OnceLock<isize> = OnceLock::new();

/// One screen's window, and what it draws.
struct Overlay {
    rect: Rect,
    guide: Guide,
    shapes: Vec<Shape>,
    marks: Vec<Shape>,
}

thread_local! {
    /// The windows up now, by handle: only touched on the windows' thread.
    static OVERLAYS: RefCell<HashMap<isize, Overlay>> = RefCell::new(HashMap::new());
}

pub fn show(shown: Vec<Shown>) {
    if let Ok(mut wanted) = WANTED.lock() {
        wanted.guides = Some(shown);
    }
    nudge(GUIDES);
}

pub fn mark(marks: Vec<Marked>) {
    if let Ok(mut wanted) = WANTED.lock() {
        wanted.marks = Some(marks);
    }
    nudge(MARKS);
}

/// Tells the windows' thread there's something new, starting it the first time.
fn nudge(what: u32) {
    let control = *CONTROL.get_or_init(|| {
        let (made, wait) = std::sync::mpsc::channel();
        let started = std::thread::Builder::new().name("roam-line-up".into()).spawn(move || run(&made));
        if started.is_err() {
            return 0;
        }
        wait.recv().unwrap_or(0)
    });
    if control != 0 {
        // SAFETY: posting to the thread's own window, by its handle.
        let _ = unsafe { PostMessageW(Some(HWND(control as *mut _)), what, WPARAM(0), LPARAM(0)) };
    }
}

/// Makes the thread's message window and pumps messages for every window on the thread.
fn run(made: &std::sync::mpsc::Sender<isize>) {
    // SAFETY: window classes with valid procedures, windows of them on this thread, and the
    // thread pumping their messages.
    unsafe {
        let instance = GetModuleHandleW(None).unwrap_or_default();
        let control_class = WNDCLASSW {
            lpfnWndProc: Some(control_procedure),
            hInstance: instance.into(),
            lpszClassName: w!("RoamLineUpControl"),
            ..Default::default()
        };
        RegisterClassW(&control_class);
        let overlay_class = WNDCLASSW {
            lpfnWndProc: Some(overlay_procedure),
            hInstance: instance.into(),
            lpszClassName: w!("RoamLineUp"),
            ..Default::default()
        };
        RegisterClassW(&overlay_class);
        let control = CreateWindowExW(
            Default::default(),
            w!("RoamLineUpControl"),
            w!("Roam"),
            Default::default(),
            0,
            0,
            0,
            0,
            Some(HWND_MESSAGE),
            None,
            Some(instance.into()),
            None,
        );
        let Ok(control) = control else {
            let _ = made.send(0);
            return;
        };
        let _ = made.send(control.0 as isize);
        let mut message = MSG::default();
        while GetMessageW(&mut message, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }
}

unsafe extern "system" fn control_procedure(window: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match message {
        GUIDES => {
            let guides = WANTED.lock().ok().and_then(|mut wanted| wanted.guides.take());
            if let Some(guides) = guides {
                apply(guides);
            }
            LRESULT(0)
        }
        MARKS => {
            let marks = WANTED.lock().ok().and_then(|mut wanted| wanted.marks.take());
            if let Some(marks) = marks {
                draw_marks(&marks);
            }
            LRESULT(0)
        }
        // SAFETY: the default handling, with what Windows passed in.
        _ => unsafe { DefWindowProcW(window, message, wparam, lparam) },
    }
}

/// Windows for `shown`, and none on other screens.
fn apply(shown: Vec<Shown>) {
    OVERLAYS.with_borrow_mut(|overlays| {
        let gone: Vec<isize> =
            overlays.iter().filter(|(_, o)| !shown.iter().any(|s| s.rect == o.rect)).map(|(h, _)| *h).collect();
        for handle in gone {
            overlays.remove(&handle);
            // SAFETY: a window this thread made.
            let _ = unsafe { DestroyWindow(HWND(handle as *mut _)) };
        }
        for Shown { rect, guide } in shown {
            let found = overlays.iter_mut().find(|(_, o)| o.rect == rect).map(|(h, o)| (*h, o));
            match found {
                Some((_, overlay)) if overlay.guide == guide => {}
                Some((handle, overlay)) => {
                    overlay.shapes = scene([rect.w, rect.h], &guide);
                    overlay.guide = guide;
                    // SAFETY: a window this thread made.
                    let _ = unsafe { InvalidateRect(Some(HWND(handle as *mut _)), None, false) };
                }
                None => {
                    let shapes = scene([rect.w, rect.h], &guide);
                    if let Some(handle) = open(rect) {
                        overlays.insert(handle, Overlay { rect, guide, shapes, marks: Vec::new() });
                        // SAFETY: the window was just made on this thread; it draws once it shows.
                        unsafe {
                            let window = HWND(handle as *mut _);
                            let _ = ShowWindow(window, SW_SHOWNOACTIVATE);
                            let _ = InvalidateRect(Some(window), None, false);
                        }
                    }
                }
            }
        }
        // Keep them over other always-on-top windows that came up since.
        for handle in overlays.keys() {
            // SAFETY: windows this thread made.
            let _ = unsafe {
                SetWindowPos(
                    HWND(*handle as *mut _),
                    Some(HWND_TOPMOST),
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
                )
            };
        }
    });
}

fn open(rect: Rect) -> Option<isize> {
    // SAFETY: a window of the class this thread registered, made on this thread.
    unsafe {
        let instance = GetModuleHandleW(None).unwrap_or_default();
        let window = CreateWindowExW(
            WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
            w!("RoamLineUp"),
            w!("Roam"),
            WS_POPUP,
            rect.x.round() as i32,
            rect.y.round() as i32,
            rect.w.round() as i32,
            rect.h.round() as i32,
            None,
            None,
            Some(instance.into()),
            None,
        )
        .ok()?;
        let _ = SetLayeredWindowAttributes(window, KEY, 255, LWA_COLORKEY);
        Some(window.0 as isize)
    }
}

/// Each screen's marks, in place of the ones before: only where they were and are is drawn
/// again.
fn draw_marks(marks: &[Marked]) {
    OVERLAYS.with_borrow_mut(|overlays| {
        for (handle, overlay) in overlays.iter_mut() {
            let here: Vec<_> = marks.iter().filter(|m| m.rect == overlay.rect).map(|m| m.mark).collect();
            let shapes = mark_scene([overlay.rect.w, overlay.rect.h], &here);
            if shapes == overlay.marks {
                continue;
            }
            let window = HWND(*handle as *mut _);
            for area in [bounds_of(&overlay.marks), bounds_of(&shapes)].into_iter().flatten() {
                let [x, y, w, h] = area;
                let rect = RECT {
                    left: x.floor() as i32 - 2,
                    top: y.floor() as i32 - 2,
                    right: (x + w).ceil() as i32 + 2,
                    bottom: (y + h).ceil() as i32 + 2,
                };
                // SAFETY: a window this thread made.
                let _ = unsafe { InvalidateRect(Some(window), Some(&rect), false) };
            }
            overlay.marks = shapes;
        }
    });
}

unsafe extern "system" fn overlay_procedure(window: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match message {
        WM_NCHITTEST => LRESULT(SEE_THROUGH),
        WM_ERASEBKGND => LRESULT(1),
        WM_PAINT => {
            // SAFETY: a paint cycle on the window's own thread.
            unsafe { paint(window) };
            LRESULT(0)
        }
        // SAFETY: the default handling, with what Windows passed in.
        _ => unsafe { DefWindowProcW(window, message, wparam, lparam) },
    }
}

fn colour([r, g, b, _]: [u8; 4]) -> COLORREF {
    COLORREF(u32::from(r) | (u32::from(g) << 8) | (u32::from(b) << 16))
}

/// Draws what's in need of it into a picture of its own first, then all at once onto the
/// window, so nothing half-drawn ever shows.
unsafe fn paint(window: HWND) {
    // SAFETY: GDI calls on the window's own thread, with objects made here and let go here.
    unsafe {
        let mut paint = PAINTSTRUCT::default();
        let dc = BeginPaint(window, &mut paint);
        let area = paint.rcPaint;
        let (w, h) = ((area.right - area.left).max(1), (area.bottom - area.top).max(1));
        let memory = CreateCompatibleDC(Some(dc));
        let picture = CreateCompatibleBitmap(dc, w, h);
        let old = SelectObject(memory, HGDIOBJ(picture.0));
        let _ = SetViewportOrgEx(memory, -area.left, -area.top, None);
        let key = CreateSolidBrush(KEY);
        FillRect(memory, &area, key);
        let _ = DeleteObject(HGDIOBJ(key.0));
        OVERLAYS.with_borrow(|overlays| {
            if let Some(overlay) = overlays.get(&(window.0 as isize)) {
                for shape in overlay.shapes.iter().chain(&overlay.marks) {
                    draw(memory, shape);
                }
            }
        });
        let _ = SetViewportOrgEx(memory, 0, 0, None);
        let _ = BitBlt(dc, area.left, area.top, w, h, Some(memory), 0, 0, SRCCOPY);
        SelectObject(memory, old);
        let _ = DeleteObject(HGDIOBJ(picture.0));
        let _ = DeleteDC(memory);
        let _ = EndPaint(window, &paint);
    }
}

unsafe fn draw(dc: HDC, shape: &Shape) {
    let to_rect = |[x, y, w, h]: [f64; 4]| RECT {
        left: x.round() as i32,
        top: y.round() as i32,
        right: (x + w).round() as i32,
        bottom: (y + h).round() as i32,
    };
    // SAFETY: GDI drawing into a memory picture on this thread, with objects let go here.
    unsafe {
        match shape {
            Shape::Rect { at, fill, radius } => {
                let rect = to_rect(*at);
                let brush = CreateSolidBrush(colour(*fill));
                if *radius > 0.5 {
                    let old_brush = SelectObject(dc, HGDIOBJ(brush.0));
                    let old_pen = SelectObject(dc, GetStockObject(NULL_PEN));
                    let round = (radius * 2.0).round() as i32;
                    let _ = RoundRect(dc, rect.left, rect.top, rect.right + 1, rect.bottom + 1, round, round);
                    SelectObject(dc, old_pen);
                    SelectObject(dc, old_brush);
                } else {
                    FillRect(dc, &rect, brush);
                }
                let _ = DeleteObject(HGDIOBJ(brush.0));
            }
            Shape::Text { at, text, size, colour: ink, bold } => {
                let font = CreateFontW(
                    -(size.round() as i32),
                    0,
                    0,
                    0,
                    if *bold { FW_BOLD.0 as i32 } else { FW_NORMAL.0 as i32 },
                    0,
                    0,
                    0,
                    DEFAULT_CHARSET,
                    OUT_DEFAULT_PRECIS,
                    CLIP_DEFAULT_PRECIS,
                    // Plain smoothing, not ClearType: its coloured edges would show against the
                    // see-through colour.
                    ANTIALIASED_QUALITY,
                    DEFAULT_PITCH.0.into(),
                    w!("Segoe UI"),
                );
                let old = SelectObject(dc, HGDIOBJ(font.0));
                SetBkMode(dc, TRANSPARENT);
                SetTextColor(dc, colour(*ink));
                let mut rect = to_rect(*at);
                let mut wide: Vec<u16> = text.encode_utf16().collect();
                DrawTextW(dc, &mut wide, &mut rect, DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX | DT_NOCLIP);
                SelectObject(dc, old);
                let _ = DeleteObject(HGDIOBJ(font.0));
            }
        }
    }
}
