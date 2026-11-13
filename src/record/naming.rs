//! File names like `Screen Recording 2026-09-28 at 14.03.27.mp4`, numbered when taken. Screenshots use it too.

use std::path::{Path, PathBuf};

use crate::platform::LocalTime;

pub fn target(folder: &Path, prefix: &str, time: LocalTime, extension: &str) -> PathBuf {
    let stem = stem(prefix, time);
    (1..)
        .map(|n| folder.join(numbered(&stem, n, extension)))
        .find(|path| !path.exists())
        .unwrap_or_else(|| folder.join(format!("{stem}.{extension}")))
}

fn stem(prefix: &str, t: LocalTime) -> String {
    format!("{prefix} {:04}-{:02}-{:02} at {:02}.{:02}.{:02}", t.year, t.month, t.day, t.hour, t.minute, t.second)
}

fn numbered(stem: &str, n: u32, extension: &str) -> String {
    if n == 1 { format!("{stem}.{extension}") } else { format!("{stem} {n}.{extension}") }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TIME: LocalTime = LocalTime { year: 2026, month: 9, day: 8, hour: 7, minute: 3, second: 5 };

    #[test]
    fn names_look_native_and_never_clash() {
        let folder = std::env::temp_dir().join(format!("notchbase-naming-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        let first = target(&folder, "Screen Recording", TIME, "mp4");
        assert_eq!(first.file_name().unwrap(), "Screen Recording 2026-09-08 at 07.03.05.mp4");
        std::fs::write(&first, b"").unwrap();
        let second = target(&folder, "Screen Recording", TIME, "mp4");
        assert_eq!(second.file_name().unwrap(), "Screen Recording 2026-09-08 at 07.03.05 2.mp4");
        std::fs::remove_dir_all(&folder).unwrap();
    }
}
