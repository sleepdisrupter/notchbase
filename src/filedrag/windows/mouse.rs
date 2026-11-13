//! The primary mouse button, honouring swapped buttons. On Windows the sensor can only tell that it's held.

use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VIRTUAL_KEY, VK_LBUTTON, VK_RBUTTON};
use windows_sys::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_SWAPBUTTON};

use super::super::Sensed;

const KEY_DOWN_BIT: u16 = 0x8000;

pub(in crate::filedrag) fn primary_button_down() -> bool {
    let primary = if buttons_swapped() { VK_RBUTTON } else { VK_LBUTTON };
    physically_held(primary)
}

fn buttons_swapped() -> bool {
    unsafe { GetSystemMetrics(SM_SWAPBUTTON) != 0 }
}

fn physically_held(key: VIRTUAL_KEY) -> bool {
    unsafe { GetAsyncKeyState(key.into()) as u16 & KEY_DOWN_BIT != 0 }
}

#[derive(Default)]
pub(in crate::filedrag) struct Sensor;

impl Sensor {
    pub(in crate::filedrag) fn poll(&mut self) -> Sensed {
        if primary_button_down() { Sensed::Pressed } else { Sensed::Idle }
    }
}
