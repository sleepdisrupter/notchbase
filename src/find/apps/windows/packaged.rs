//! Writes a Start menu shortcut for each Store app, so the index finds them like any other app.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize,
    IPersistFile,
};
use windows::Win32::UI::Shell::{
    BHID_EnumItems, FOLDERID_AppsFolder, IEnumShellItems, ILFree, IShellItem, IShellLinkW, KF_FLAG_DEFAULT,
    SHGetIDListFromObject, SHGetKnownFolderItem, SIGDN, SIGDN_NORMALDISPLAY, SIGDN_PARENTRELATIVEPARSING, ShellLink,
};
use windows::core::{HSTRING, Interface, Result};

const THREAD: &str = "notchbase-find-store-apps";
const PACKAGED_MARK: char = '!';
const SHORTCUT: &str = "lnk";
const NOT_IN_A_FILE_NAME: [char; 9] = ['\\', '/', ':', '*', '?', '"', '<', '>', '|'];

struct Packaged {
    id: String,
    name: String,
    item: IShellItem,
}

pub(super) fn sync(menu: &Path) -> Vec<(String, PathBuf)> {
    let menu = menu.to_path_buf();
    let work = std::thread::Builder::new().name(THREAD.into()).spawn(move || {
        let joined = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) }.is_ok();
        let written = unsafe { listed() }.map(|apps| written(&menu, apps)).unwrap_or_default();
        if joined {
            unsafe { CoUninitialize() };
        }
        written
    });
    work.ok().and_then(|work| work.join().ok()).unwrap_or_default()
}

fn written(menu: &Path, apps: Vec<Packaged>) -> Vec<(String, PathBuf)> {
    if apps.is_empty() || std::fs::create_dir_all(menu).is_err() {
        return Vec::new();
    }
    let mut kept = Vec::new();
    let mut taken = HashSet::new();
    for app in apps {
        let Some(file) = file_name(&app.name).filter(|file| taken.insert(file.to_lowercase())) else { continue };
        let path = menu.join(file);
        if path.exists() || unsafe { write(&app.item, &path) }.is_ok() {
            kept.push((app.id, path));
        }
    }
    sweep(menu, &kept);
    kept
}

fn sweep(menu: &Path, kept: &[(String, PathBuf)]) {
    let Ok(listed) = std::fs::read_dir(menu) else { return };
    for stale in listed.flatten().map(|entry| entry.path()) {
        let shortcut = stale.extension().is_some_and(|ending| ending.eq_ignore_ascii_case(SHORTCUT));
        if shortcut && !kept.iter().any(|(_, path)| *path == stale) {
            let _ = std::fs::remove_file(stale);
        }
    }
}

fn file_name(shown: &str) -> Option<String> {
    let safe: String =
        shown.chars().map(|letter| if NOT_IN_A_FILE_NAME.contains(&letter) { ' ' } else { letter }).collect();
    let safe = safe.trim().trim_end_matches('.');
    (!safe.is_empty() && !safe.chars().any(char::is_control)).then(|| format!("{safe}.{SHORTCUT}"))
}

unsafe fn listed() -> Result<Vec<Packaged>> {
    let folder: IShellItem = unsafe { SHGetKnownFolderItem(&FOLDERID_AppsFolder, KF_FLAG_DEFAULT, None) }?;
    let items: IEnumShellItems = unsafe { folder.BindToHandler(None, &BHID_EnumItems) }?;
    let mut apps = Vec::new();
    loop {
        let (mut next, mut fetched) = ([None], 0u32);
        if unsafe { items.Next(&mut next, Some(&mut fetched)) }.is_err() || fetched == 0 {
            break;
        }
        let Some(item) = next[0].take() else { break };
        let (name, id) = unsafe { (said(&item, SIGDN_NORMALDISPLAY), said(&item, SIGDN_PARENTRELATIVEPARSING)) };
        if let (Some(name), Some(id)) = (name, id.filter(|id| id.contains(PACKAGED_MARK))) {
            apps.push(Packaged { id, name, item });
        }
    }
    Ok(apps)
}

unsafe fn said(item: &IShellItem, how: SIGDN) -> Option<String> {
    let text = unsafe { item.GetDisplayName(how) }.ok()?;
    let said = unsafe { text.to_string() }.ok();
    unsafe { CoTaskMemFree(Some(text.0.cast_const().cast())) };
    said
}

unsafe fn write(item: &IShellItem, file: &Path) -> Result<()> {
    let link: IShellLinkW = unsafe { CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER) }?;
    let target = unsafe { SHGetIDListFromObject(item) }?;
    let aimed = unsafe { link.SetIDList(target) };
    unsafe { ILFree(Some(target.cast_const())) };
    aimed?;
    unsafe { link.cast::<IPersistFile>()?.Save(&HSTRING::from(file.as_os_str()), true) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shown_name_becomes_file_name() {
        assert_eq!(file_name("Calculator").as_deref(), Some("Calculator.lnk"));
        assert_eq!(file_name("Movies & TV").as_deref(), Some("Movies & TV.lnk"));
        assert_eq!(file_name("3D: Viewer?").as_deref(), Some("3D  Viewer.lnk"));
        assert_eq!(file_name(" Notes. ").as_deref(), Some("Notes.lnk"), "no name ends in a dot or a space");
        assert_eq!((file_name(""), file_name(" ?? "), file_name("a\u{7}b")), (None, None, None));
    }

    #[test]
    #[ignore = "lists the Store apps of this machine and writes a shortcut for each into a scratch folder"]
    fn this_pc_gets_store_shortcuts() {
        let menu = std::env::temp_dir().join(format!("notchbase-store-apps-{}", std::process::id()));
        let kept = sync(&menu);
        println!("{} Store apps, for example {:?}", kept.len(), kept.first());
        assert!(kept.iter().all(|(id, path)| id.contains(PACKAGED_MARK) && path.exists()));
        let _ = std::fs::remove_dir_all(menu);
    }
}
