use std::path::Path;

use super::{DragSensor, Sensed, thumbnail};

#[test]
fn fresh_sensor_reports_no_files() {
    assert!(matches!(DragSensor::default().poll(), Sensed::Idle | Sensed::Pressed));
}

#[test]
fn zero_size_thumbnail_is_none() {
    assert!(thumbnail(Path::new("Cargo.toml"), 0).is_none());
}

#[test]
fn missing_file_has_no_thumbnail() {
    assert!(thumbnail(Path::new("definitely/not/here.xyz"), 64).is_none());
}

#[cfg(windows)]
static SHELL: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[cfg(windows)]
pub(super) fn shell_turn() -> std::sync::MutexGuard<'static, ()> {
    SHELL.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[cfg(windows)]
fn windows_dir() -> std::path::PathBuf {
    std::env::var_os("WINDIR").map_or_else(|| r"C:\Windows".into(), Into::into)
}

#[cfg(windows)]
#[test]
fn system_thumbnails_fit_the_size() {
    let _turn = shell_turn();
    for (file, max_px) in [("explorer.exe", 64), ("win.ini", 48), ("explorer.exe", 256)] {
        let image = thumbnail(&windows_dir().join(file), max_px).unwrap_or_else(|| panic!("no thumbnail for {file}"));
        let [width, height] = image.size;
        assert!(width > 0 && height > 0, "{file}: empty image");
        assert!(width.max(height) <= max_px as usize, "{file}: {width}x{height} exceeds {max_px}");
        assert!(image.pixels.iter().any(|pixel| pixel.a() > 0), "{file}: fully transparent");
    }
}

#[cfg(windows)]
#[test]
fn verbatim_paths_get_thumbnails_too() {
    let _turn = shell_turn();
    let verbatim = std::fs::canonicalize(windows_dir().join("explorer.exe")).unwrap();
    assert!(verbatim.to_string_lossy().starts_with(r"\\?\"));
    assert!(thumbnail(&verbatim, 32).is_some());
}
