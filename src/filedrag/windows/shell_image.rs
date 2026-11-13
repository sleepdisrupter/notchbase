//! Thumbnails and icons from IShellItemImageFactory, as bitmaps.

use std::path::Path;

use ::windows::Win32::Foundation::SIZE;
use ::windows::Win32::UI::Shell::{
    IShellItemImageFactory, SHCreateItemFromParsingName, SIIGBF, SIIGBF_BIGGERSIZEOK, SIIGBF_ICONONLY,
    SIIGBF_RESIZETOFIT,
};
use ::windows::core::HSTRING;

use super::bitmap::OwnedBitmap;
use super::com::ensure_com_on_this_thread;
use super::shell_path::shell_parsable;

pub(super) fn shell_image(path: &Path, max_px: u32) -> Option<OwnedBitmap> {
    image_of(path, max_px, SIIGBF_RESIZETOFIT | SIIGBF_BIGGERSIZEOK)
}

pub(super) fn shell_icon(path: &Path, max_px: u32) -> Option<OwnedBitmap> {
    image_of(path, max_px, SIIGBF_ICONONLY | SIIGBF_BIGGERSIZEOK)
}

fn image_of(path: &Path, max_px: u32, flags: SIIGBF) -> Option<OwnedBitmap> {
    ensure_com_on_this_thread();
    let parsing_name = HSTRING::from(shell_parsable(path).as_path());
    let factory: IShellItemImageFactory = unsafe { SHCreateItemFromParsingName(&parsing_name, None) }.ok()?;
    let side = i32::try_from(max_px).ok()?;
    let requested = SIZE { cx: side, cy: side };
    let handle = unsafe { factory.GetImage(requested, flags) }.ok()?;
    OwnedBitmap::new(handle)
}
