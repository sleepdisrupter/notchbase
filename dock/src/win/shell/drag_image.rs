//! Gives a drag a picture when the shell didn't: the file's thumbnail, centred on the cursor.

use std::path::Path;

use ::windows::Win32::Foundation::{COLORREF, POINT, SIZE};
use ::windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, CoCreateInstance, DVASPECT_CONTENT, FORMATETC, IDataObject, TYMED_HGLOBAL,
};
use ::windows::Win32::System::DataExchange::RegisterClipboardFormatW;
use ::windows::Win32::UI::Shell::{CLSID_DragDropHelper, IDragSourceHelper, SHDRAGIMAGE};
use ::windows::core::w;

use super::bitmap::OwnedBitmap;
use super::shell_image;

const FALLBACK_DRAG_IMAGE_PX: u32 = 96;
const NO_COLOR_KEY: COLORREF = COLORREF(0xFFFF_FFFF);

pub(crate) fn ensure_drag_image(data: &IDataObject, representative: &Path) {
    if !carries_drag_image(data) {
        attach_thumbnail_as_drag_image(data, representative);
    }
}

fn carries_drag_image(data: &IDataObject) -> bool {
    let format = unsafe { RegisterClipboardFormatW(w!("DragImageBits")) };
    let query = FORMATETC {
        cfFormat: format as u16,
        ptd: std::ptr::null_mut(),
        dwAspect: DVASPECT_CONTENT.0,
        lindex: -1,
        tymed: TYMED_HGLOBAL.0 as u32,
    };
    format != 0 && unsafe { data.QueryGetData(&query) }.is_ok()
}

fn attach_thumbnail_as_drag_image(data: &IDataObject, path: &Path) {
    let Ok(helper) =
        (unsafe { CoCreateInstance::<_, IDragSourceHelper>(&CLSID_DragDropHelper, None, CLSCTX_INPROC_SERVER) })
    else {
        return;
    };
    let Some(bitmap) = shell_image(path, FALLBACK_DRAG_IMAGE_PX) else {
        return;
    };
    let Some((width, height)) = bitmap.size() else {
        return;
    };
    let image = centered_on_cursor(&bitmap, width, height);
    if unsafe { helper.InitializeFromBitmap(&image, data) }.is_ok() {
        bitmap.hand_over_to_system();
    }
}

fn centered_on_cursor(bitmap: &OwnedBitmap, width: i32, height: i32) -> SHDRAGIMAGE {
    SHDRAGIMAGE {
        sizeDragImage: SIZE { cx: width, cy: height },
        ptOffset: POINT { x: width / 2, y: height / 2 },
        hbmpDragImage: bitmap.handle(),
        crColorKey: NO_COLOR_KEY,
    }
}
