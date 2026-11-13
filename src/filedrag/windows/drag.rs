//! Drags shelf files out through a shell data object and SHDoDragDrop, which runs its own loop until the drop.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

use ::windows::Win32::Foundation::HWND;
use ::windows::Win32::System::Com::IDataObject;
use ::windows::Win32::System::Ole::{DROPEFFECT, DROPEFFECT_COPY};
use ::windows::Win32::UI::Shell::Common::ITEMIDLIST;
use ::windows::Win32::UI::Shell::{BHID_DataObject, SHCreateShellItemArrayFromIDLists, SHDoDragDrop};
use raw_window_handle::{HasWindowHandle as _, RawWindowHandle};
use winit::window::Window;

use super::com::{ItemIdList, with_ole};
use super::drag_image::ensure_drag_image;

const ALLOWED_DROP_EFFECTS: DROPEFFECT = DROPEFFECT_COPY;

static DRAG_IN_PROGRESS: AtomicBool = AtomicBool::new(false);

pub(in crate::filedrag) fn dragging_out() -> bool {
    DRAG_IN_PROGRESS.load(Ordering::Relaxed)
}

pub(in crate::filedrag) fn start_drag(window: &Window, paths: &[PathBuf]) -> Result<(), String> {
    let hwnd = native_window(window).ok_or("The window has no native handle")?;
    let result = with_ole(|| run_modal_drag(hwnd, paths));
    window.request_redraw();
    result
}

pub(super) fn native_window(window: &Window) -> Option<HWND> {
    match window.window_handle().ok()?.as_raw() {
        RawWindowHandle::Win32(handle) => Some(HWND(handle.hwnd.get() as *mut _)),
        _ => None,
    }
}

fn run_modal_drag(hwnd: HWND, paths: &[PathBuf]) -> Result<(), String> {
    let data = shell_data_object(paths)?;
    ensure_drag_image(&data, &paths[0]);
    DRAG_IN_PROGRESS.store(true, Ordering::Relaxed);
    let outcome = unsafe { SHDoDragDrop(Some(hwnd), &data, None, ALLOWED_DROP_EFFECTS) };
    DRAG_IN_PROGRESS.store(false, Ordering::Relaxed);
    outcome.map(|_dropped_or_cancelled| ()).map_err(|e| format!("Drag failed: {}", e.message()))
}

fn shell_data_object(paths: &[PathBuf]) -> Result<IDataObject, String> {
    let lists = paths.iter().map(|path| ItemIdList::for_path(path)).collect::<Result<Vec<_>, _>>()?;
    let pointers: Vec<*const ITEMIDLIST> = lists.iter().map(ItemIdList::as_ptr).collect();
    unsafe {
        SHCreateShellItemArrayFromIDLists(&pointers).and_then(|items| items.BindToHandler(None, &BHID_DataObject))
    }
    .map_err(|e| format!("Couldn't prepare the drag: {}", e.message()))
}
