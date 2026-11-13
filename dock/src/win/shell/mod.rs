//! The shell's side of files: pictures and icons from its image factory, dragging files out with SHDoDragDrop,
//! showing a file in File Explorer, and reading shortcuts. Most of this is the notch's own Windows code
//! (`src/filedrag/windows`), shared in spirit with the shelf.

pub(crate) mod bitmap;
pub(crate) mod com;
mod drag_image;
mod pixels;
pub(crate) mod shell_path;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use ::windows::Win32::Foundation::{HWND, SIZE};
use ::windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance, IDataObject, IPersistFile, STGM_READ};
use ::windows::Win32::System::Ole::DROPEFFECT_COPY;
use ::windows::Win32::UI::Shell::Common::ITEMIDLIST;
use ::windows::Win32::UI::Shell::{
    BHID_DataObject, IShellItemImageFactory, IShellLinkW, SHCreateItemFromParsingName,
    SHCreateShellItemArrayFromIDLists, SHDoDragDrop, SHOpenFolderAndSelectItems, SIIGBF, SIIGBF_BIGGERSIZEOK,
    SIIGBF_ICONONLY, SIIGBF_RESIZETOFIT, SIIGBF_THUMBNAILONLY, SLR_NO_UI, ShellLink,
};
use ::windows::core::{HSTRING, Interface as _};

use bitmap::OwnedBitmap;
use com::{ItemIdList, ensure_com_on_this_thread, with_ole};
use pixels::{downscale_premultiplied, premultiply_shell_alpha};
use shell_path::shell_parsable;

pub(crate) fn shell_image(path: &Path, max_px: u32) -> Option<OwnedBitmap> {
    image_of(&parsing_name(path), max_px, SIIGBF_RESIZETOFIT | SIIGBF_BIGGERSIZEOK)
}

fn parsing_name(path: &Path) -> HSTRING {
    HSTRING::from(shell_parsable(path).as_path())
}

/// A shell item's picture: its thumbnail, or its icon, as the flags say.
fn image_of(parsing_name: &HSTRING, max_px: u32, flags: SIIGBF) -> Option<OwnedBitmap> {
    ensure_com_on_this_thread();
    let factory: IShellItemImageFactory = unsafe { SHCreateItemFromParsingName(parsing_name, None) }.ok()?;
    let side = i32::try_from(max_px).ok().filter(|s| *s > 0)?;
    let handle = unsafe { factory.GetImage(SIZE { cx: side, cy: side }, flags) }.ok()?;
    OwnedBitmap::new(handle)
}

/// A bitmap as an egui image, premultiplied and scaled down to fit `max_px`.
fn fitted(bitmap: &OwnedBitmap, max_px: u32) -> Option<egui::ColorImage> {
    let mut pixels = bitmap.read_rgba()?;
    premultiply_shell_alpha(&mut pixels.rgba);
    let (width, height) = fit_within(pixels.width, pixels.height, max_px);
    let rgba = if (width, height) == (pixels.width, pixels.height) {
        pixels.rgba
    } else {
        downscale_premultiplied(&pixels.rgba, pixels.width, pixels.height, width, height)
    };
    Some(egui::ColorImage::from_rgba_premultiplied([width, height], &rgba))
}

fn fit_within(width: usize, height: usize, max_side: u32) -> (usize, usize) {
    let max_side = max_side as usize;
    if width <= max_side && height <= max_side {
        return (width, height);
    }
    let scale = max_side as f64 / width.max(height) as f64;
    let scaled = |side: usize| ((side as f64 * scale).round() as usize).clamp(1, max_side);
    (scaled(width), scaled(height))
}

/// A file's icon, `px` pixels on its longer side.
pub(crate) fn icon(path: &Path, px: u32) -> Option<egui::ColorImage> {
    icon_named(&parsing_name(path), px)
}

/// The icon of anything the shell can parse: a path, or a shell folder such as the Recycle Bin.
pub(crate) fn icon_named(parsing_name: &HSTRING, px: u32) -> Option<egui::ColorImage> {
    fitted(&image_of(parsing_name, px, SIIGBF_ICONONLY | SIIGBF_BIGGERSIZEOK)?, px)
}

/// A thumbnail of the file's contents when the shell has one (pictures, videos, PDFs), and whether it is one.
/// Otherwise its icon.
pub(crate) fn thumbnail(path: &Path, px: u32) -> Option<(egui::ColorImage, bool)> {
    let name = parsing_name(path);
    if let Some(bitmap) = image_of(&name, px, SIIGBF_THUMBNAILONLY | SIIGBF_RESIZETOFIT | SIIGBF_BIGGERSIZEOK)
        && let Some(image) = fitted(&bitmap, px)
    {
        return Some((image, true));
    }
    icon_named(&name, px).map(|image| (image, false))
}

/// Shows a file selected in File Explorer, falling back to `explorer.exe /select,` if the shell call fails.
pub(crate) fn reveal(path: &Path) {
    use std::os::windows::process::CommandExt as _;
    ensure_com_on_this_thread();
    if let Ok(item) = ItemIdList::for_path(path)
        && unsafe { SHOpenFolderAndSelectItems(item.as_ptr(), None, 0) }.is_ok()
    {
        return;
    }
    let _ = std::process::Command::new("explorer.exe")
        .raw_arg(format!("/select,\"{}\"", shell_parsable(path).display()))
        .spawn();
}

static DRAGGING: AtomicBool = AtomicBool::new(false);
static ENDED: AtomicBool = AtomicBool::new(false);

pub(crate) fn dragging() -> bool {
    DRAGGING.load(Ordering::Relaxed)
}

pub(crate) fn take_ended() -> bool {
    ENDED.swap(false, Ordering::Relaxed)
}

/// Drags files out through a shell data object. SHDoDragDrop runs its own loop until the drop or the cancel.
pub(crate) fn drag(hwnd: HWND, paths: &[PathBuf]) -> Result<(), String> {
    if paths.is_empty() {
        return Ok(());
    }
    let result = with_ole(|| {
        let data = shell_data_object(paths)?;
        drag_image::ensure_drag_image(&data, &paths[0]);
        DRAGGING.store(true, Ordering::Relaxed);
        let outcome = unsafe { SHDoDragDrop(Some(hwnd), &data, None, DROPEFFECT_COPY) };
        DRAGGING.store(false, Ordering::Relaxed);
        outcome.map(|_| ()).map_err(|e| format!("Drag failed: {}", e.message()))
    });
    ENDED.store(true, Ordering::Relaxed);
    result
}

fn shell_data_object(paths: &[PathBuf]) -> Result<IDataObject, String> {
    let lists = paths.iter().map(|path| ItemIdList::for_path(path)).collect::<Result<Vec<_>, _>>()?;
    let pointers: Vec<*const ITEMIDLIST> = lists.iter().map(ItemIdList::as_ptr).collect();
    unsafe {
        SHCreateShellItemArrayFromIDLists(&pointers).and_then(|items| items.BindToHandler(None, &BHID_DataObject))
    }
    .map_err(|e| format!("Couldn't prepare the drag: {}", e.message()))
}

/// Where a shortcut (`.lnk`) points, if it points at a file.
pub(crate) fn shortcut_target(link: &Path) -> Option<PathBuf> {
    ensure_com_on_this_thread();
    unsafe {
        let shell_link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER).ok()?;
        let file: IPersistFile = shell_link.cast().ok()?;
        file.Load(&HSTRING::from(link), STGM_READ).ok()?;
        let _ = shell_link.Resolve(::windows::Win32::Foundation::HWND::default(), SLR_NO_UI.0 as u32);
        let mut buffer = [0u16; 1024];
        shell_link.GetPath(&mut buffer, std::ptr::null_mut(), 0).ok()?;
        let len = buffer.iter().position(|c| *c == 0).unwrap_or(buffer.len());
        let target = PathBuf::from(String::from_utf16_lossy(&buffer[..len]));
        (len > 0).then_some(target)
    }
}
