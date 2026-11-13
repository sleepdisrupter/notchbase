//! Drops on Windows, through our own IDropTarget. It passes the drag image on to the shell's helper, so browser
//! previews stay visible over the notch.

use std::sync::atomic::{AtomicBool, Ordering};

use ::windows::Win32::Foundation::{HWND, POINT, POINTL};
use ::windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance, IDataObject};
use ::windows::Win32::System::Ole::{
    DROPEFFECT, DROPEFFECT_COPY, DROPEFFECT_LINK, DROPEFFECT_NONE, IDropTarget, IDropTarget_Impl, OleInitialize,
    RegisterDragDrop, RevokeDragDrop,
};
use ::windows::Win32::System::SystemServices::MODIFIERKEYS_FLAGS;
use ::windows::Win32::UI::Shell::{CLSID_DragDropHelper, IDropTargetHelper};
use ::windows::core::{Ref, Result, implement};
use winit::window::{Window, WindowId};

use super::super::incoming::{Arrival, Inbox};
use super::super::save::rescue;
use super::data_object;
use super::drag::native_window;

pub(in crate::filedrag) struct Site {
    hwnd: HWND,
}

impl Drop for Site {
    fn drop(&mut self) {
        let _ = unsafe { RevokeDragDrop(self.hwnd) };
    }
}

pub(in crate::filedrag) fn accept_drops(window: &Window, inbox: Inbox) -> Option<Site> {
    let hwnd = native_window(window)?;
    let _ = unsafe { OleInitialize(None) };
    let helper = unsafe { CoCreateInstance(&CLSID_DragDropHelper, None, CLSCTX_INPROC_SERVER) }.ok();
    let target: IDropTarget =
        DropTarget { window: window.id(), hwnd, inbox, helper, accepting: AtomicBool::new(false) }.into();
    match unsafe { RegisterDragDrop(hwnd, &target) } {
        Ok(()) => Some(Site { hwnd }),
        Err(err) => {
            eprintln!("notchbase: the notch can't accept drops: {}", err.message());
            None
        }
    }
}

#[implement(IDropTarget)]
struct DropTarget {
    window: WindowId,
    hwnd: HWND,
    inbox: Inbox,
    helper: Option<IDropTargetHelper>,
    accepting: AtomicBool,
}

impl DropTarget {
    /// Copy, or link when that's all the source allows. Never move, so a source can't delete what it dragged.
    fn choose(&self, effect: *mut DROPEFFECT) -> DROPEFFECT {
        let Some(effect) = (unsafe { effect.as_mut() }) else { return DROPEFFECT_NONE };
        let allowed = |wanted: DROPEFFECT| effect.0 & wanted.0 != 0;
        let chosen = match self.accepting.load(Ordering::Relaxed) || super::super::catches_everything() {
            true if allowed(DROPEFFECT_COPY) => DROPEFFECT_COPY,
            true if allowed(DROPEFFECT_LINK) => DROPEFFECT_LINK,
            _ => DROPEFFECT_NONE,
        };
        *effect = chosen;
        chosen
    }
}

impl IDropTarget_Impl for DropTarget_Impl {
    fn DragEnter(
        &self,
        data: Ref<IDataObject>,
        _keys: MODIFIERKEYS_FLAGS,
        point: &POINTL,
        effect: *mut DROPEFFECT,
    ) -> Result<()> {
        let count = data.as_ref().map_or(0, data_object::count);
        self.accepting.store(count > 0, Ordering::Relaxed);
        let chosen = self.choose(effect);
        if let (Some(helper), Some(data)) = (&self.helper, data.as_ref()) {
            let _ = unsafe { helper.DragEnter(self.hwnd, data, &screen(point), chosen) };
        }
        if count > 0 {
            self.inbox.post(self.window, Arrival::Hover(count));
        }
        if let Some(page) = data.as_ref().and_then(data_object::offered_page) {
            self.inbox.post(self.window, Arrival::Offer(page));
        }
        Ok(())
    }

    fn DragOver(&self, _keys: MODIFIERKEYS_FLAGS, point: &POINTL, effect: *mut DROPEFFECT) -> Result<()> {
        let chosen = self.choose(effect);
        if let Some(helper) = &self.helper {
            let _ = unsafe { helper.DragOver(&screen(point), chosen) };
        }
        Ok(())
    }

    fn DragLeave(&self) -> Result<()> {
        if let Some(helper) = &self.helper {
            let _ = unsafe { helper.DragLeave() };
        }
        if self.accepting.swap(false, Ordering::Relaxed) {
            self.inbox.post(self.window, Arrival::Leave);
        }
        Ok(())
    }

    fn Drop(
        &self,
        data: Ref<IDataObject>,
        _keys: MODIFIERKEYS_FLAGS,
        point: &POINTL,
        effect: *mut DROPEFFECT,
    ) -> Result<()> {
        let chosen = self.choose(effect);
        if let (Some(helper), Some(data)) = (&self.helper, data.as_ref()) {
            let _ = unsafe { helper.Drop(data, &screen(point), chosen) };
        }
        let items = if chosen == DROPEFFECT_NONE {
            Vec::new()
        } else {
            rescue(data.as_ref().map(data_object::read).unwrap_or_default(), self.inbox.folder())
        };
        self.accepting.store(false, Ordering::Relaxed);
        let arrival = if items.is_empty() { Arrival::Leave } else { Arrival::Drop(items) };
        self.inbox.post(self.window, arrival);
        Ok(())
    }
}

fn screen(point: &POINTL) -> POINT {
    POINT { x: point.x, y: point.y }
}

#[cfg(test)]
mod tests;
