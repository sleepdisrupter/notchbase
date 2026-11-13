//! Runs macOS's sips tool for formats the image library can't read, like HEIC, through a temporary PNG.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};

const SIPS: &str = "/usr/bin/sips";

struct TempFile(PathBuf);

impl TempFile {
    fn new(ext: &str) -> Self {
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let name = format!("notchbase-shelf-{}-{n}.{ext}", std::process::id());
        Self(std::env::temp_dir().join(name))
    }

    fn read(&self) -> Option<Vec<u8>> {
        std::fs::read(&self.0).ok()
    }
}

impl Drop for TempFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

fn succeeds(command: &mut Command) -> bool {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

pub(super) fn to_png(path: &Path, max_side: Option<u32>) -> Option<Vec<u8>> {
    let out = TempFile::new("png");
    let mut command = Command::new(SIPS);
    command.args(["-s", "format", "png"]);
    if let Some(max_side) = max_side {
        command.arg("-Z").arg(max_side.to_string());
    }
    command.arg(path).arg("--out").arg(&out.0);
    succeeds(&mut command).then(|| out.read()).flatten()
}

pub(super) fn dimensions(path: &Path) -> Option<[u32; 2]> {
    let output = Command::new(SIPS)
        .args(["-g", "pixelWidth", "-g", "pixelHeight"])
        .arg(path)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let property = |key: &str| text.lines().find_map(|line| line.trim().strip_prefix(key)?.trim().parse::<u32>().ok());
    Some([property("pixelWidth:")?, property("pixelHeight:")?])
}
