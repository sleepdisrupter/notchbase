//! Click-through and keyboard focus through extended window styles, held keys and buttons, and the cursor.

use windows_sys::Win32::Foundation::POINT;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, VIRTUAL_KEY, VK_CONTROL, VK_LBUTTON, VK_LWIN, VK_MBUTTON, VK_MENU, VK_RBUTTON, VK_RWIN, VK_SHIFT,
    VK_XBUTTON1, VK_XBUTTON2,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{GetCursorPos, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TRANSPARENT};
use winit::window::Window;

use super::window_style::update_ex_style;

pub fn set_click_through(window: &Window, click_through: bool) {
    if click_through {
        update_ex_style(window, WS_EX_TRANSPARENT | WS_EX_LAYERED, 0);
    } else {
        update_ex_style(window, WS_EX_LAYERED, WS_EX_TRANSPARENT);
    }
}

/// A click never activates the notch, so typing into it first lifts WS_EX_NOACTIVATE.
pub fn begin_keyboard_focus(window: &Window) {
    update_ex_style(window, 0, WS_EX_NOACTIVATE);
    window.focus_window();
}

pub fn end_keyboard_focus(window: &Window) {
    update_ex_style(window, WS_EX_NOACTIVATE, 0);
}

pub fn held_modifiers() -> egui::Modifiers {
    let down = |key: VIRTUAL_KEY| unsafe { GetAsyncKeyState(i32::from(key)) } < 0;
    egui::Modifiers {
        alt: down(VK_MENU),
        ctrl: down(VK_CONTROL),
        shift: down(VK_SHIFT),
        mac_cmd: down(VK_LWIN) || down(VK_RWIN),
        command: false,
    }
}

const FIRST_KEY: u16 = 0x08;
const LAST_KEY: u16 = 0xFE;

pub fn any_key_held() -> bool {
    (FIRST_KEY..=LAST_KEY).any(|key| unsafe { GetAsyncKeyState(i32::from(key)) } < 0)
}

pub fn mouse_held() -> bool {
    [VK_LBUTTON, VK_RBUTTON, VK_MBUTTON, VK_XBUTTON1, VK_XBUTTON2]
        .into_iter()
        .any(|button| unsafe { GetAsyncKeyState(i32::from(button)) } < 0)
}

pub fn pressed_within(_seconds: f64) -> bool {
    mouse_held()
}

pub struct PressesElsewhere;

impl PressesElsewhere {
    pub fn take_press(&self) -> bool {
        false
    }
}

pub fn watch_presses_elsewhere() -> Option<PressesElsewhere> {
    None
}

pub fn cursor_in_window(window: &Window) -> Option<egui::Pos2> {
    let mut cursor = POINT { x: 0, y: 0 };
    if unsafe { GetCursorPos(&mut cursor) } == 0 {
        return None;
    }
    let origin = window.inner_position().ok()?;
    let scale = window.scale_factor() as f32;
    Some(egui::pos2((cursor.x - origin.x) as f32 / scale, (cursor.y - origin.y) as f32 / scale))
}
