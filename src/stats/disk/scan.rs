//! Measures one root. Sizes are allocated blocks, and the walk never follows links, leaves its volume or fetches
//! iCloud files. Build folders and machines go to their own part wherever they sit.

use std::fs::{self, DirEntry, Metadata};
use std::path::Path;
use std::sync::Arc;

use super::plan::{Plan, Root, in_library, pretty};
use super::{ITEMS_KEPT, Item, Part, PartUsage, Storage};

/// Parts and items smaller than this are left out.
const ITEM_FROM: u64 = 8_000_000;
/// In a files part, a file this big is an item of its own, wherever it's nested.
const FILE_FROM: u64 = 50_000_000;
const PARTS: usize = Part::ALL.len();
/// Packages are one item each and are never opened up.
pub(crate) const PACKAGES: [&str; 16] = [
    "app",
    "appex",
    "movpkg",
    "photoslibrary",
    "musiclibrary",
    "tvlibrary",
    "imovielibrary",
    "fcpbundle",
    "logicx",
    "band",
    "xcarchive",
    "framework",
    "bundle",
    "sparsebundle",
    "pvm",
    "utm",
];
const MACHINE_FOLDERS: [&str; 3] = ["pvm", "utm", "vmwarevm"];
const MACHINE_FILES: [&str; 5] = ["vdi", "vmdk", "qcow2", "hdd", "vhdx"];
const CACHE: &str = " cache";

#[derive(Default)]
pub(super) struct Tally {
    sizes: [u64; PARTS],
    items: [Vec<Item>; PARTS],
}

impl Tally {
    fn add(&mut self, part: Part, size: u64) {
        self.sizes[part as usize] += size;
    }

    fn item(&mut self, part: Part, name: String, path: &Path, size: u64, folder: bool) {
        if size >= ITEM_FROM {
            self.items[part as usize].push(Item { name, path: path.to_owned(), size, folder, icon: None });
        }
    }

    pub(super) fn merge(&mut self, other: Tally) {
        for (index, items) in other.items.into_iter().enumerate() {
            self.sizes[index] += other.sizes[index];
            self.items[index].extend(items);
            trim(&mut self.items[index]);
        }
    }

    pub(super) fn storage(&self, measuring: bool) -> Storage {
        let mut parts: Vec<Arc<PartUsage>> = Part::ALL
            .into_iter()
            .filter(|part| self.sizes[*part as usize] >= ITEM_FROM)
            .map(|part| {
                let (size, mut items) = (self.sizes[part as usize], self.items[part as usize].clone());
                trim(&mut items);
                Arc::new(PartUsage { part, size, items })
            })
            .collect();
        parts.sort_by_key(|usage| std::cmp::Reverse(usage.size));
        Storage { parts, measuring }
    }
}

fn trim(items: &mut Vec<Item>) {
    items.sort_by(|a, b| b.size.cmp(&a.size).then_with(|| a.name.cmp(&b.name)));
    items.truncate(ITEMS_KEPT);
}

struct Walk {
    tally: Tally,
    volume: u64,
}

pub(super) fn measure(root: &Root) -> Tally {
    let mut walk = Walk { tally: Tally::default(), volume: 0 };
    let Ok(meta) = fs::symlink_metadata(&root.path) else { return walk.tally };
    if !meta.is_dir() {
        let part = if let Plan::Whole(part) = root.plan { part } else { Part::Other };
        walk.tally.add(part, on_disk(&meta));
        walk.tally.item(part, root.name.clone(), &root.path, on_disk(&meta), false);
        return walk.tally;
    }
    if skipped(&meta) {
        return walk.tally;
    }
    walk.volume = device(&meta);
    walk.root(&root.name, &root.path, root.plan);
    walk.tally
}

impl Walk {
    fn root(&mut self, name: &str, path: &Path, plan: Plan) {
        match plan {
            Plan::Whole(part) => {
                let (size, _) = self.folder(path, part, false);
                self.tally.item(part, name.to_owned(), path, size, true);
            }
            Plan::Each(part) => self.each(path, part, false, ""),
            Plan::Caches => self.each(path, Part::AppData, false, CACHE),
            Plan::Files(part) => self.each(path, part, true, ""),
            Plan::Library => {
                for item in fs::read_dir(path).into_iter().flatten().flatten() {
                    let name = item.file_name().to_string_lossy().into_owned();
                    if item.metadata().is_ok_and(|meta| meta.is_dir() && self.inside(&meta)) {
                        self.root(&name, &item.path(), in_library(&name));
                    } else {
                        self.entry(&item, path, Part::AppData, false);
                    }
                }
            }
        }
    }

    fn each(&mut self, dir: &Path, part: Part, flat: bool, suffix: &str) {
        for item in fs::read_dir(dir).into_iter().flatten().flatten() {
            let (size, listed) = self.entry(&item, dir, part, flat);
            let name = item.file_name().to_string_lossy().into_owned();
            let name = if flat { name } else { format!("{}{suffix}", pretty(&name)) };
            let folder = item.metadata().is_ok_and(|meta| meta.is_dir());
            if folder || !flat || size < FILE_FROM {
                // A folder counts only what isn't listed on its own, so the items add up to the part.
                self.tally.item(part, name, &item.path(), size.saturating_sub(listed), folder);
            }
        }
    }

    fn folder(&mut self, dir: &Path, part: Part, flat: bool) -> (u64, u64) {
        let (mut size, mut listed) = (0, 0);
        for item in fs::read_dir(dir).into_iter().flatten().flatten() {
            let (inside, items) = self.entry(&item, dir, part, flat);
            size += inside;
            listed += items;
        }
        (size, listed)
    }

    /// Measures one entry. Returns its size and how much of that is already listed as items of its own.
    fn entry(&mut self, item: &DirEntry, parent: &Path, part: Part, flat: bool) -> (u64, u64) {
        let Ok(meta) = item.metadata() else { return (0, 0) };
        let path = item.path();
        let name = item.file_name().to_string_lossy().into_owned();
        let kind = path.extension().map(|ext| ext.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
        if !meta.is_dir() {
            let size = on_disk(&meta);
            let machine = MACHINE_FILES.contains(&kind.as_str());
            let part_here = if machine { Part::Machines } else { part };
            self.tally.add(part_here, size);
            let lists = machine || (flat && size >= FILE_FROM);
            if lists {
                self.tally.item(part_here, name, &path, size, false);
            }
            return if machine { (0, 0) } else { (size, if lists { size } else { 0 }) };
        }
        if !self.inside(&meta) {
            return (0, 0);
        }
        if buildable(&name, parent) {
            let size = weigh(&path, self.volume) + on_disk(&meta);
            self.tally.add(Part::Developer, size);
            let project = parent.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
            self.tally.item(Part::Developer, format!("{project} {name}"), &path, size, true);
            return (0, 0);
        }
        if PACKAGES.contains(&kind.as_str()) || MACHINE_FOLDERS.contains(&kind.as_str()) {
            let size = weigh(&path, self.volume) + on_disk(&meta);
            let machine = MACHINE_FOLDERS.contains(&kind.as_str());
            let part_here = if machine { Part::Machines } else { part };
            self.tally.add(part_here, size);
            let lists = machine || flat;
            if lists {
                let stem = path.file_stem().map_or(name.clone(), |stem| stem.to_string_lossy().into_owned());
                self.tally.item(part_here, pretty(&stem), &path, size, true);
            }
            return if machine { (0, 0) } else { (size, if lists && size >= ITEM_FROM { size } else { 0 }) };
        }
        let (size, listed) = self.folder(&path, part, flat);
        self.tally.add(part, on_disk(&meta));
        (size + on_disk(&meta), listed)
    }

    fn inside(&self, meta: &Metadata) -> bool {
        device(meta) == self.volume && !skipped(meta)
    }
}

/// Build output that goes to Developer wherever it sits: node_modules, DerivedData, and target beside Cargo.toml.
pub(crate) fn buildable(name: &str, parent: &Path) -> bool {
    match name {
        "node_modules" | "DerivedData" => true,
        "target" => fs::symlink_metadata(parent.join("Cargo.toml")).is_ok(),
        _ => false,
    }
}

fn weigh(dir: &Path, volume: u64) -> u64 {
    let mut total = 0;
    for item in fs::read_dir(dir).into_iter().flatten().flatten() {
        let Ok(meta) = item.metadata() else { continue };
        total += on_disk(&meta);
        if meta.is_dir() && device(&meta) == volume && !skipped(&meta) {
            total += weigh(&item.path(), volume);
        }
    }
    total
}

#[cfg(target_os = "macos")]
pub(super) fn load_icons(storage: &mut Storage) {
    for usage in &mut storage.parts {
        if usage.part != Part::Apps {
            continue;
        }
        let mut apps = PartUsage::clone(usage);
        for app in &mut apps.items {
            app.icon = crate::audio::file_icon(&app.path);
        }
        *usage = Arc::new(apps);
    }
}

#[cfg(not(target_os = "macos"))]
pub(super) fn load_icons(_storage: &mut Storage) {}

#[cfg(unix)]
fn on_disk(meta: &Metadata) -> u64 {
    use std::os::unix::fs::MetadataExt as _;
    meta.blocks() * 512
}

#[cfg(not(unix))]
fn on_disk(meta: &Metadata) -> u64 {
    meta.len()
}

#[cfg(unix)]
fn device(meta: &Metadata) -> u64 {
    use std::os::unix::fs::MetadataExt as _;
    meta.dev()
}

#[cfg(not(unix))]
fn device(_meta: &Metadata) -> u64 {
    0
}

#[cfg(target_os = "macos")]
fn skipped(meta: &Metadata) -> bool {
    use std::os::macos::fs::MetadataExt as _;
    // SF_DATALESS: an iCloud file that isn't downloaded, which reading would fetch.
    const DATALESS: u32 = 0x4000_0000;
    meta.st_flags() & DATALESS != 0
}

#[cfg(not(target_os = "macos"))]
fn skipped(_meta: &Metadata) -> bool {
    false
}

/// Utility priority, so a scan never competes with what you're doing.
#[cfg(target_os = "macos")]
pub(super) fn be_gentle() {
    unsafe { libc::pthread_set_qos_class_self_np(libc::qos_class_t::QOS_CLASS_UTILITY, 0) };
}

#[cfg(not(target_os = "macos"))]
pub(super) fn be_gentle() {}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    const BIG: usize = FILE_FROM as usize + 4096;
    const MIDDLING: usize = ITEM_FROM as usize + 4096;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("notchbase-disk-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write(path: PathBuf, bytes: usize) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, vec![1u8; bytes]).unwrap();
    }

    fn measured(dir: &Path, plan: Plan) -> Storage {
        measure(&Root { name: "Root".into(), path: dir.to_owned(), plan }).storage(false)
    }

    fn names(storage: &Storage, part: Part) -> Vec<String> {
        storage.part(part).map(|usage| usage.items.iter().map(|item| item.name.clone()).collect()).unwrap_or_default()
    }

    #[cfg(unix)]
    #[test]
    fn linked_manifest_is_not_followed() {
        let dir = scratch("manifest-link");
        std::os::unix::fs::symlink(dir.join("elsewhere/Cargo.toml"), dir.join("Cargo.toml")).unwrap();
        assert!(buildable("target", &dir), "the link itself counts, its target is never looked at");
        assert!(!buildable("target", &dir.join("elsewhere")));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn big_files_flat_folders_keep_rest() {
        let dir = scratch("files");
        write(dir.join("work/deep/movie.mov"), BIG);
        write(dir.join("work/notes/a.bin"), MIDDLING);
        write(dir.join("tiny.txt"), 10);
        let storage = measured(&dir, Plan::Files(Part::Documents));
        let documents = storage.part(Part::Documents).unwrap();
        assert_eq!(names(&storage, Part::Documents), ["movie.mov", "work"]);
        assert!(documents.items[0].size >= FILE_FROM && !documents.items[0].folder);
        assert!(documents.items[1].size >= ITEM_FROM && documents.items[1].size < FILE_FROM);
        assert!(documents.size >= documents.items.iter().map(|item| item.size).sum::<u64>());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn build_folders_and_machines_move() {
        let dir = scratch("build");
        write(dir.join("app/Cargo.toml"), 10);
        write(dir.join("app/target/debug/app"), MIDDLING);
        write(dir.join("site/node_modules/pkg/index.js"), MIDDLING);
        write(dir.join("plain/target/readme"), MIDDLING);
        write(dir.join("Windows.pvm/disk.hds"), MIDDLING);
        let storage = measured(&dir, Plan::Files(Part::Documents));
        assert_eq!(names(&storage, Part::Developer), ["app target", "site node_modules"]);
        assert_eq!(names(&storage, Part::Machines), ["Windows"]);
        assert_eq!(names(&storage, Part::Documents), ["plain"], "a target folder without Cargo.toml is just a folder");
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn each_child_is_a_readable_item() {
        let dir = scratch("each");
        write(dir.join("com.google.Chrome/data"), MIDDLING);
        write(dir.join("Slack/data"), MIDDLING * 2);
        write(dir.join("small/data"), 10);
        let caches = measured(&dir, Plan::Caches);
        assert_eq!(names(&caches, Part::AppData), ["Slack cache", "Chrome cache"]);
        let each = measured(&dir, Plan::Each(Part::AppData));
        assert_eq!(names(&each, Part::AppData), ["Slack", "Chrome"]);
        let whole = measured(&dir, Plan::Whole(Part::Developer));
        assert_eq!(names(&whole, Part::Developer), ["Root"]);
        assert!(measured(&dir.join("gone"), Plan::Whole(Part::Developer)).parts.is_empty());
        fs::remove_dir_all(&dir).unwrap();
    }
}
