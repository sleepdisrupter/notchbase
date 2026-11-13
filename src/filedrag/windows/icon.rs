//! File icons from the shell, at the size asked for.

use std::path::Path;

use super::shell_image::shell_icon;
use super::thumbnail::fitted;

pub(in crate::filedrag) fn icon(path: &Path, px: u32) -> Option<egui::ColorImage> {
    fitted(&shell_icon(path, px)?, px)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::filedrag::tests::shell_turn;

    const SIDE: u32 = 48;

    fn probe(name: &str) -> PathBuf {
        let file = std::env::temp_dir().join(format!("notchbase-icon-{}-{name}", std::process::id()));
        std::fs::write(&file, b"").expect("a file in the system temp directory");
        file
    }

    #[test]
    fn icon_fits_the_size_asked_for() {
        let _turn = shell_turn();
        let file = probe("probe.txt");
        let image = crate::filedrag::icon(&file, SIDE);
        let none = crate::filedrag::icon(&file, 0);
        let _ = std::fs::remove_file(&file);
        let image = image.expect("every file has an icon");
        let [width, height] = image.size;
        assert!(width > 0 && height > 0 && width.max(height) <= SIDE as usize, "{width}x{height}");
        assert!(image.pixels.iter().any(|pixel| pixel.a() > 0), "not fully transparent");
        assert!(none.is_none(), "zero pixels is no icon");
    }
}
