//! Cursor and monitor helpers for the sampler: the cursor position, nudging it, and the monitor under it.

use windows_sys::Win32::Foundation::POINT;
use windows_sys::Win32::Graphics::Gdi::{GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromPoint};
use windows_sys::Win32::UI::WindowsAndMessaging::{GetCursorPos, SetCursorPos};

const UNBOUNDED: [i32; 4] = [i32::MIN / 2, i32::MIN / 2, i32::MAX / 2, i32::MAX / 2];

pub(super) fn cursor_pos() -> Option<[i32; 2]> {
    let mut p = POINT::default();
    (unsafe { GetCursorPos(&mut p) } != 0).then_some([p.x, p.y])
}

pub(super) fn nudge_cursor(dx: i32, dy: i32) {
    if let Some([x, y]) = cursor_pos() {
        unsafe { SetCursorPos(x + dx, y + dy) };
    }
}

/// Bounds of the monitor nearest a point, or no bounds at all if Windows can't say.
pub(super) fn monitor_rect(at: [i32; 2]) -> [i32; 4] {
    let monitor = unsafe { MonitorFromPoint(POINT { x: at[0], y: at[1] }, MONITOR_DEFAULTTONEAREST) };
    if monitor.is_null() {
        return UNBOUNDED;
    }
    let mut info = MONITORINFO { cbSize: size_of::<MONITORINFO>() as u32, ..Default::default() };
    if unsafe { GetMonitorInfoW(monitor, &mut info) } == 0 {
        return UNBOUNDED;
    }
    let r = info.rcMonitor;
    [r.left, r.top, r.right, r.bottom]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursor_is_on_reported_monitor() {
        let Some(cursor) = cursor_pos() else { return };
        let [left, top, right, bottom] = monitor_rect(cursor);
        assert!((left..right).contains(&cursor[0]) && (top..bottom).contains(&cursor[1]));
    }
}
