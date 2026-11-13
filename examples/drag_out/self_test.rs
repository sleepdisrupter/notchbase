//! The Windows self-test. It styles the window like the notch, scripts a drag onto it, and checks that
//! drops skip a click-through window.

use std::thread::sleep;
use std::time::Duration;

use raw_window_handle::{HasWindowHandle as _, RawWindowHandle};
use windows_sys::Win32::Foundation::{HWND, POINT};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    INPUT, INPUT_0, INPUT_MOUSE, MOUSE_EVENT_FLAGS, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP, MOUSEINPUT, SendInput,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GA_ROOT, GWL_EXSTYLE, GetAncestor, GetWindowLongPtrW, LWA_ALPHA, SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE,
    SWP_NOSIZE, SWP_NOZORDER, SetCursorPos, SetLayeredWindowAttributes, SetWindowLongPtrW, SetWindowPos, WS_EX_LAYERED,
    WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TRANSPARENT, WindowFromPoint,
};
use winit::event_loop::EventLoopProxy;
use winit::window::Window;

use crate::app::Note;
use crate::outcome::print_check;

const NOTCH_LIKE_STYLE: u32 = WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_LAYERED;
const CLICK_THROUGH_STYLE: u32 = WS_EX_TRANSPARENT | WS_EX_LAYERED;

pub fn start(window: &Window, proxy: EventLoopProxy<Note>) {
    let hwnd = native_window(window);
    set_extended_style(hwnd, extended_style(hwnd) | NOTCH_LIKE_STYLE);
    let center = client_center(window);
    let hwnd_value = hwnd as isize;
    std::thread::spawn(move || {
        if drag_onto_itself(center, hwnd_value) {
            let _ = proxy.send_event(Note::SelfTestScriptDone);
        } else {
            eprintln!("something covers the test window; no input was injected");
            std::process::exit(2);
        }
    });
}

pub fn click_through_hides_the_drop_target(window: &Window) -> bool {
    let hwnd = native_window(window);
    let center = client_center(window);
    let style = extended_style(hwnd);
    let targeted_normally = top_level_window_at(center) == hwnd;
    set_extended_style(hwnd, style | CLICK_THROUGH_STYLE);
    let skipped_when_click_through = top_level_window_at(center) != hwnd;
    set_extended_style(hwnd, style);
    print_check("the window is a drop target", targeted_normally)
        & print_check("a click-through window is skipped by drops", skipped_when_click_through)
}

fn drag_onto_itself(center: POINT, hwnd_value: isize) -> bool {
    pause(600);
    move_cursor(center.x, center.y);
    pause(100);
    // Never inject input that another window would get.
    if top_level_window_at(center) as isize != hwnd_value {
        return false;
    }
    press_left(MOUSEEVENTF_LEFTDOWN);
    pause(100);
    for step in 1..=8 {
        move_cursor(center.x + step * 3, center.y + step);
        pause(40);
    }
    pause(400);
    move_cursor(center.x + 30, center.y + 12);
    pause(300);
    press_left(MOUSEEVENTF_LEFTUP);
    pause(700);
    true
}

fn native_window(window: &Window) -> HWND {
    match window.window_handle().expect("window handle").as_raw() {
        RawWindowHandle::Win32(handle) => handle.hwnd.get() as HWND,
        _ => unreachable!("a Win32 window"),
    }
}

fn client_center(window: &Window) -> POINT {
    let origin = window.inner_position().expect("window position");
    let size = window.inner_size();
    POINT { x: origin.x + size.width as i32 / 2, y: origin.y + size.height as i32 / 2 }
}

fn extended_style(hwnd: HWND) -> u32 {
    unsafe { GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32 }
}

fn set_extended_style(hwnd: HWND, style: u32) {
    let keep_everything_else = SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED;
    unsafe {
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, style as isize);
        SetWindowPos(hwnd, std::ptr::null_mut(), 0, 0, 0, 0, keep_everything_else);
        SetLayeredWindowAttributes(hwnd, 0, 255, LWA_ALPHA); // a layered window stays invisible until this is set
    }
}

fn top_level_window_at(point: POINT) -> HWND {
    unsafe { GetAncestor(WindowFromPoint(point), GA_ROOT) }
}

fn press_left(flags: MOUSE_EVENT_FLAGS) {
    let input =
        INPUT { r#type: INPUT_MOUSE, Anonymous: INPUT_0 { mi: MOUSEINPUT { dwFlags: flags, ..Default::default() } } };
    unsafe { SendInput(1, &input, size_of::<INPUT>() as i32) };
}

fn move_cursor(x: i32, y: i32) {
    unsafe { SetCursorPos(x, y) };
}

fn pause(milliseconds: u64) {
    sleep(Duration::from_millis(milliseconds));
}
