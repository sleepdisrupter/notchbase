//! File icons from NSWorkspace, drawn at the size asked for.

use std::path::Path;

use objc2::rc::{Retained, autoreleasepool};
use objc2_app_kit::NSWorkspace;
use objc2_core_foundation::CGFloat;
use objc2_core_graphics::CGImage;
use objc2_foundation::{NSPoint, NSRect, NSSize, NSString};

use super::paths::ns_string_path;
use super::raster::rasterize_to_fit;

pub(in crate::filedrag) fn icon(path: &Path, px: u32) -> Option<egui::ColorImage> {
    autoreleasepool(|_| {
        let path = ns_string_path(path)?;
        let image = finder_icon(&path, px)?;
        rasterize_to_fit(&image, px)
    })
}

pub(super) fn finder_icon(path: &NSString, px: u32) -> Option<Retained<CGImage>> {
    let icon = NSWorkspace::sharedWorkspace().iconForFile(path);
    let side = px as CGFloat;
    let mut wanted = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(side, side));
    unsafe { icon.CGImageForProposedRect_context_hints(&mut wanted, None, None) }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    const SIDE: u32 = 64;

    fn probe(name: &str) -> PathBuf {
        let file = std::env::temp_dir().join(format!("notchbase-icon-{}-{name}", std::process::id()));
        std::fs::write(&file, b"").expect("a file in the system temp directory");
        file
    }

    #[test]
    fn icon_fits_the_size_asked_for() {
        let file = probe("probe.txt");
        let image = crate::filedrag::icon(&file, SIDE);
        let none = crate::filedrag::icon(&file, 0);
        let _ = std::fs::remove_file(&file);
        let [width, height] = image.expect("every file has an icon").size;
        assert!(width > 0 && height > 0 && width.max(height) <= SIDE as usize, "{width}x{height}");
        assert!(none.is_none(), "zero pixels is no icon");
    }
}
