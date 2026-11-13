//! Lists the Downloads folder: settled files, and partial ones (.crdownload, .part) with the names they'll land as.

use std::collections::HashSet;
use std::path::Path;
use std::time::{Duration, SystemTime};

use super::imp;

const PARTIAL_SUFFIXES: [&str; 5] = [".crdownload", ".download", ".part", ".opdownload", ".partial"];
const BUNDLE_MANIFEST: &str = "Info.plist";

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Partial {
    pub(super) name: String,
    pub(super) size: u64,
    pub(super) total: Option<u64>,
    pub(super) lands_as: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct Listing {
    pub(super) partials: Vec<Partial>,
    pub(super) settled: HashSet<String>,
}

pub(super) fn list(folder: &Path) -> Option<Listing> {
    let mut listing = Listing::default();
    for entry in std::fs::read_dir(folder).ok()?.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if finished_name(&name).is_none() {
            listing.settled.insert(name);
            continue;
        }
        let Ok(meta) = entry.metadata() else { continue };
        let (size, total) = if meta.is_dir() { bundle(&entry.path()) } else { (meta.len(), None) };
        listing.partials.push(Partial { name, size, total, lands_as: None });
    }
    Some(listing)
}

pub(super) fn finished_name(name: &str) -> Option<&str> {
    PARTIAL_SUFFIXES.iter().find_map(|suffix| name.strip_suffix(suffix)).filter(|stem| !stem.is_empty())
}

pub(super) fn newest(folder: &Path, within: Duration) -> Option<String> {
    let now = SystemTime::now();
    let recent = |entry: &std::fs::DirEntry| {
        let name = entry.file_name().to_string_lossy().into_owned();
        let meta = entry.metadata().ok().filter(|meta| meta.is_file() && meta.len() > 0)?;
        let modified = meta.modified().ok()?;
        let fresh = now.duration_since(modified).is_ok_and(|age| age <= within);
        (fresh && finished_name(&name).is_none()).then_some((modified, name))
    };
    let files = std::fs::read_dir(folder).ok()?.flatten().filter_map(|entry| recent(&entry));
    files.max_by_key(|(modified, _)| *modified).map(|(_, name)| name)
}

fn bundle(path: &Path) -> (u64, Option<u64>) {
    let inside = std::fs::read_dir(path).into_iter().flatten().flatten();
    let size = inside
        .filter(|entry| entry.file_name() != BUNDLE_MANIFEST)
        .filter_map(|entry| entry.metadata().ok())
        .filter(std::fs::Metadata::is_file)
        .map(|meta| meta.len())
        .sum();
    (size, imp::bundle_total(path))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder(name: &str) -> std::path::PathBuf {
        let folder = std::env::temp_dir().join(format!("notchbase-downloads-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(&folder).unwrap();
        folder
    }

    #[test]
    fn unfinished_downloads_have_suffixes() {
        assert_eq!(finished_name("movie.mp4.crdownload"), Some("movie.mp4"));
        assert_eq!(finished_name("movie.mp4.download"), Some("movie.mp4"));
        assert_eq!(finished_name("archive.zip.part"), Some("archive.zip"));
        assert_eq!(finished_name("movie.mp4"), None);
        assert_eq!(finished_name(".crdownload"), None);
    }

    #[test]
    fn splits_unfinished_from_settled() {
        let folder = folder("split");
        std::fs::write(folder.join("done.pdf"), [0u8; 3]).unwrap();
        std::fs::write(folder.join("movie.mp4.crdownload"), [0u8; 7]).unwrap();
        std::fs::create_dir_all(folder.join("song.mp3.download")).unwrap();
        std::fs::write(folder.join("song.mp3.download/song.mp3"), [0u8; 5]).unwrap();
        std::fs::write(folder.join("song.mp3.download").join(BUNDLE_MANIFEST), "not a manifest").unwrap();
        let mut listing = list(&folder).unwrap();
        listing.partials.sort_by(|a, b| a.name.cmp(&b.name));
        let chrome = Partial { name: "movie.mp4.crdownload".into(), size: 7, total: None, lands_as: None };
        let safari = Partial { name: "song.mp3.download".into(), size: 5, total: None, lands_as: None };
        assert_eq!(listing.partials, [chrome, safari]);
        assert_eq!(listing.settled, HashSet::from(["done.pdf".to_owned()]));
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn newest_file_found_while_fresh() {
        let folder = folder("newest");
        assert_eq!(newest(&folder, Duration::from_secs(60)), None);
        std::fs::write(folder.join("empty.bin"), []).unwrap();
        std::fs::write(folder.join("half.zip.part"), [0u8; 2]).unwrap();
        std::fs::write(folder.join("report.pdf"), [0u8; 4]).unwrap();
        assert_eq!(newest(&folder, Duration::from_secs(60)).as_deref(), Some("report.pdf"));
        assert_eq!(newest(&folder, Duration::ZERO), None);
        assert_eq!(list(&folder.join("missing")), None);
        let _ = std::fs::remove_dir_all(&folder);
    }
}
