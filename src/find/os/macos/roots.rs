//! The volumes to index, and the parts of the system volume that are sealed, guarded or hold apps.

use std::ffi::{c_char, c_int, c_void};
use std::path::{Path, PathBuf};

use crate::find::os::{Root, root};

const BOOT: &str = "/";
const DATA: &str = "/System/Volumes/Data";
const SYSTEM_MOUNTS: [&str; 3] = ["/System", "/private", "/dev"];
const TIME_MACHINE: &str = "com.apple.TimeMachine";
const TIME_MACHINE_MOUNTS: &str = ".timemachine";
const SPARE_MOUNTS: usize = 8;
const SEALED: [&str; 11] = [
    "/System",
    "/usr",
    "/bin",
    "/sbin",
    "/dev",
    "/private/var/db",
    "/private/var/vm",
    "/Volumes",
    "/.vol",
    "/.nofollow",
    "/.resolve",
];
const SYSTEM_TOOLS: &str = "/System/Library/CoreServices/Applications";
const FINDER: &str = "/System/Library/CoreServices/Finder.app";
const SETTINGS_PANES: &str = "/System/Library/ExtensionKit/Extensions";
const UNSEALED: [&str; 5] = ["/System/Applications", "/usr/local", SYSTEM_TOOLS, FINDER, SETTINGS_PANES];
const APPLICATIONS: [&str; 4] = ["/Applications", "/System/Applications", SYSTEM_TOOLS, SETTINGS_PANES];
const OWN_APPLICATIONS: &str = "Applications";
const GUARDED: [&str; 11] = [
    "Desktop",
    "Documents",
    "Downloads",
    "Library/Mobile Documents",
    "Library/CloudStorage",
    "Library/Containers",
    "Library/Group Containers",
    "Library/Daemon Containers",
    "Library/Application Support/AddressBook",
    "Library/Calendars",
    "Library/Reminders",
];

#[repr(C)]
struct UuidBytes([u8; 16]);

#[link(name = "CoreServices", kind = "framework")]
unsafe extern "C" {
    fn FSEventsCopyUUIDForDevice(device: libc::dev_t) -> *const c_void;
}

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFUUIDGetUUIDBytes(uuid: *const c_void) -> UuidBytes;
    fn CFRelease(object: *const c_void);
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Mount {
    path: PathBuf,
    from: String,
    flags: u32,
    device: libc::dev_t,
}

pub fn roots(volumes: bool) -> Vec<Root> {
    let mut roots = rooted(&mounts(), volumes);
    for root in &mut roots {
        root.volume = volume_id(root.devices[1]);
    }
    roots
}

pub fn sealed() -> Vec<PathBuf> {
    SEALED.map(PathBuf::from).to_vec()
}

pub fn unsealed() -> Vec<PathBuf> {
    UNSEALED.map(PathBuf::from).to_vec()
}

pub fn guarded(home: &Path) -> Vec<PathBuf> {
    GUARDED.map(|folder| home.join(folder)).to_vec()
}

pub fn launchers(home: Option<&Path>) -> Vec<PathBuf> {
    let own = home.map(|home| home.join(OWN_APPLICATIONS));
    APPLICATIONS.map(PathBuf::from).into_iter().chain(own).collect()
}

fn rooted(mounts: &[Mount], volumes: bool) -> Vec<Root> {
    let device_of = |path: &str| mounts.iter().find(|mount| mount.path == Path::new(path)).map(|mount| mount.device);
    let Some(system) = device_of(BOOT) else { return Vec::new() };
    let data = device_of(DATA).unwrap_or(system);
    let boot = Root { path: BOOT.into(), volume: [0; 16], devices: [wide(system), wide(data)], flags: root::BOOT };
    let others = mounts.iter().filter(|mount| volumes && browsable(mount) && ![system, data].contains(&mount.device));
    let others = others.map(|mount| Root {
        path: mount.path.clone(),
        volume: [0; 16],
        devices: [wide(mount.device); 2],
        flags: root::REMOVABLE,
    });
    std::iter::once(boot).chain(others).collect()
}

fn browsable(mount: &Mount) -> bool {
    let has = |flag: c_int| mount.flags & flag as u32 != 0;
    let system = SYSTEM_MOUNTS.iter().any(|prefix| mount.path.starts_with(prefix)) || mount.path == Path::new(BOOT);
    let backup = mount.from.contains(TIME_MACHINE)
        || mount.path.components().any(|part| part.as_os_str().to_string_lossy().contains(TIME_MACHINE))
        || mount.path.components().any(|part| part.as_os_str() == TIME_MACHINE_MOUNTS);
    has(libc::MNT_LOCAL) && !has(libc::MNT_DONTBROWSE | libc::MNT_SNAPSHOT | libc::MNT_RDONLY) && !system && !backup
}

fn wide(device: libc::dev_t) -> u64 {
    device as u64
}

fn mounts() -> Vec<Mount> {
    let known = unsafe { libc::getfsstat(std::ptr::null_mut(), 0, libc::MNT_NOWAIT) };
    let room = usize::try_from(known).unwrap_or(0) + SPARE_MOUNTS;
    let mut table: Vec<libc::statfs> = Vec::with_capacity(room);
    let bytes = c_int::try_from(room * size_of::<libc::statfs>()).unwrap_or(0);
    let filled = unsafe { libc::getfsstat(table.as_mut_ptr(), bytes, libc::MNT_NOWAIT) };
    unsafe { table.set_len(usize::try_from(filled).unwrap_or(0).min(room)) };
    table.iter().map(mount).collect()
}

fn mount(entry: &libc::statfs) -> Mount {
    let device = unsafe { std::ptr::from_ref(&entry.f_fsid).cast::<libc::dev_t>().read() };
    Mount { path: text(&entry.f_mntonname).into(), from: text(&entry.f_mntfromname), flags: entry.f_flags, device }
}

fn text(chars: &[c_char]) -> String {
    let bytes: Vec<u8> = chars.iter().take_while(|byte| **byte != 0).map(|byte| *byte as u8).collect();
    String::from_utf8_lossy(&bytes).into_owned()
}

fn volume_id(device: u64) -> [u8; 16] {
    let uuid = unsafe { FSEventsCopyUUIDForDevice(device as libc::dev_t) };
    if uuid.is_null() {
        return [0; 16];
    }
    let bytes = unsafe { CFUUIDGetUUIDBytes(uuid) };
    unsafe { CFRelease(uuid) };
    bytes.0
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::MetadataExt as _;

    use super::*;

    const SYSTEM_DEVICE: libc::dev_t = 16_777_233;
    const DATA_DEVICE: libc::dev_t = 16_777_238;
    const LOCAL: u32 = libc::MNT_LOCAL as u32;
    const HIDDEN: u32 = LOCAL | libc::MNT_DONTBROWSE as u32;
    const READ_ONLY: u32 = LOCAL | libc::MNT_RDONLY as u32;

    fn mounted(path: &str, from: &str, flags: u32, device: libc::dev_t) -> Mount {
        Mount { path: path.into(), from: from.into(), flags, device }
    }

    fn laptop() -> Vec<Mount> {
        vec![
            mounted("/", "/dev/disk3s1s1", READ_ONLY | libc::MNT_ROOTFS as u32, SYSTEM_DEVICE),
            mounted("/dev", "devfs", HIDDEN, 1),
            mounted("/System/Volumes/VM", "/dev/disk3s6", HIDDEN, 2),
            mounted("/System/Volumes/Preboot", "/dev/disk3s2", HIDDEN, 3),
            mounted("/System/Volumes/Data", "/dev/disk3s5", HIDDEN, DATA_DEVICE),
            mounted("/Volumes/Work", "/dev/disk5s1", LOCAL, 40),
            mounted("/Volumes/Installer", "/dev/disk6s1", READ_ONLY, 41),
            mounted("/Volumes/Share", "//nas/share", 0, 42),
            mounted("/Volumes/.timemachine/ABC/2026.backup", "com.apple.TimeMachine.2026@/dev/disk7s1", READ_ONLY, 43),
            mounted("/Volumes/com.apple.TimeMachine.localsnapshots/x", "/dev/disk3s5", LOCAL, 44),
            mounted("/Volumes/Snap", "/dev/disk8s1", LOCAL | libc::MNT_SNAPSHOT as u32, 45),
            mounted("/Users/me/mount", "/dev/disk9s1", LOCAL, 46),
        ]
    }

    #[test]
    fn boot_root_carries_both_devices() {
        let roots = rooted(&laptop(), false);
        assert_eq!(roots.len(), 1, "without other volumes there is one root");
        assert_eq!(roots[0].path, Path::new("/"));
        assert_eq!(roots[0].devices, [wide(SYSTEM_DEVICE), wide(DATA_DEVICE)]);
        assert_eq!(roots[0].flags, root::BOOT);
    }

    #[test]
    fn only_local_volumes_join() {
        let roots = rooted(&laptop(), true);
        let paths: Vec<&Path> = roots.iter().map(|root| root.path.as_path()).collect();
        assert_eq!(paths, [Path::new("/"), Path::new("/Volumes/Work"), Path::new("/Users/me/mount")]);
        assert!(roots[1..].iter().all(|root| root.flags == root::REMOVABLE && root.devices[0] == root.devices[1]));
        assert_eq!(roots[1].devices, [40, 40]);
    }

    #[test]
    fn missing_volumes_still_answer() {
        let plain = [mounted("/", "/dev/disk1s1", LOCAL, 9)];
        assert_eq!(rooted(&plain, true)[0].devices, [9, 9]);
        assert!(rooted(&[], true).is_empty());
    }

    #[test]
    fn sealed_doors_and_guarded_home() {
        let (sealed, open) = (sealed(), unsealed());
        assert!(open.iter().all(|door| sealed.iter().any(|wall| door.starts_with(wall))));
        assert_eq!(open.len(), 5, "the system's apps, its tools, the Finder, the settings panes and /usr/local");
        assert!(open.contains(&PathBuf::from(FINDER)) && !open.contains(&PathBuf::from("/System/Library")));
        assert!(sealed.contains(&PathBuf::from("/Volumes")) && sealed.contains(&PathBuf::from("/System")));
        let guarded = guarded(Path::new("/Users/me"));
        assert!(guarded.iter().all(|folder| folder.starts_with("/Users/me")));
        let named = [
            "Desktop",
            "Documents",
            "Downloads",
            "Library/Mobile Documents",
            "Library/CloudStorage",
            "Library/Containers",
            "Library/Group Containers",
        ];
        for folder in named {
            assert!(guarded.contains(&Path::new("/Users/me").join(folder)), "{folder}");
        }
    }

    #[test]
    fn app_folders_are_launchers() {
        let shared = ["/Applications", "/System/Applications", SYSTEM_TOOLS, SETTINGS_PANES].map(PathBuf::from);
        assert_eq!(launchers(None), shared);
        assert_eq!(launchers(Some(Path::new("/Users/me")))[..4], shared);
        assert_eq!(launchers(Some(Path::new("/Users/me")))[4..], [PathBuf::from("/Users/me/Applications")]);
        assert!(shared[1..].iter().all(|apps| unsealed().contains(apps)), "the system's own lie behind doors");
    }

    #[test]
    fn boot_root_allows_data_volume() {
        let roots = roots(false);
        let boot = roots.first().expect("a boot root");
        assert_eq!((boot.path.as_path(), boot.flags, roots.len()), (Path::new("/"), root::BOOT, 1));
        for path in ["/Users", "/System/Applications"] {
            let device = std::fs::metadata(path).expect("a system folder").dev();
            assert!(boot.devices.contains(&device), "{path} is on device {device}, outside {:?}", boot.devices);
        }
        assert_ne!(boot.devices[0], boot.devices[1], "the sealed system and the data volume are two devices");
        assert_ne!(boot.volume, [0; 16], "the data volume has an event database");
    }
}
