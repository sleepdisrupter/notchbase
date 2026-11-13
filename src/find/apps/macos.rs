//! What macOS says about its apps: the names Finder shows, which run only in the background, settings panes and
//! how much each is used.

use std::collections::HashMap;
use std::ffi::c_void;
use std::path::{Path, PathBuf};
use std::ptr::null;

use objc2::Message;
use objc2::rc::{Retained, autoreleasepool};
use objc2::runtime::AnyObject;
use objc2_foundation::{NSArray, NSBundle, NSDate, NSDictionary, NSFileManager, NSNumber, NSString};

use crate::find::context::Stamp;
use crate::find::key::{Key, key_of};

use super::{Read, Used};

type Ref = *const c_void;

const ENDING: &str = ".app";
const PANE_ENDING: &str = "appex";
const EXTENSION: &str = "EXAppExtensionAttributes";
const EXTENSION_POINT: &str = "EXExtensionPointIdentifier";
const SETTINGS_POINT: &str = "com.apple.Settings.extension.ui";
const SETTINGS: &str = "SettingsExtensionAttributes";
const OPENS_BY_ADDRESS: &str = "allowsXAppleSystemPreferencesURLScheme";
const SHOWN_NAME: &str = "CFBundleDisplayName";
const ADDRESS: &str = "x-apple.systempreferences:";
const NAMED_BY_HAND: [(&str, &str); 2] = [("PowerPreferences", "Battery"), ("AppleIDSettings", "Apple Account")];
const NOT_A_NAME: [&str; 3] = ["Extension", "Settings", "Ext"];
const NOT_FOR_PEOPLE: [&str; 1] = ["FollowUps"];
const INFO: &str = "Contents/Info.plist";
const BACKGROUND_ONLY: &str = "LSBackgroundOnly";
const AGENT: &str = "LSUIElement";
const APPLICATIONS: &str = "Applications";
const WRAPPED: &str = "Wrapper";
const CACHES: &str = "Caches";
const LIBRARY: &str = "Library";
const SYSTEM_PLACES: [&str; 5] = ["System", "usr", "opt", "private", "bin"];
const NOT_INDEXED: &str = ".noindex";
const SAID_YES: [&str; 3] = ["1", "yes", "true"];
const EVERY_APP: &str = "kMDItemContentType == \"com.apple.application-bundle\"";
const PATH: &str = "kMDItemPath";
const LAST_USED: &str = "kMDItemLastUsedDate";
const USE_COUNT: &str = "kMDItemUseCount";
const THIS_COMPUTER: &str = "kMDQueryScopeComputer";
const BLOCK_UNTIL_GATHERED: usize = 1;
const NO_SCOPE_OPTIONS: u32 = 0;

#[link(name = "CoreServices", kind = "framework")]
unsafe extern "C" {
    fn MDQueryCreate(allocator: Ref, asking: Ref, values: Ref, sorting: Ref) -> *mut c_void;
    fn MDQuerySetSearchScope(query: Ref, scopes: Ref, options: u32);
    fn MDQueryExecute(query: Ref, options: usize) -> u8;
    fn MDQueryGetResultCount(query: Ref) -> isize;
    fn MDQueryGetAttributeValueOfResultAtIndex(query: Ref, name: Ref, index: isize) -> Ref;
    fn MDQueryGetResultAtIndex(query: Ref, index: isize) -> Ref;
    fn MDItemCopyAttribute(item: Ref, name: Ref) -> *mut c_void;
}

pub fn read(path: &Path) -> Read {
    autoreleasepool(|_| {
        if is_extension(path) {
            let title = pane(path).and_then(|pane| pane_title(path, &pane));
            return Read { unlisted: title.is_none(), title, backstage: false };
        }
        let at_home = among_the_apps(path) && !kept_for_later(path);
        let the_systems_own = !at_home && kept_by_the_system(path);
        let behind_the_scenes = says(path, BACKGROUND_ONLY) || (the_systems_own && says(path, AGENT));
        Read { title: shown(path), backstage: behind_the_scenes, unlisted: !(at_home || is_a_bundle(path)) }
    })
}

pub fn every() -> Vec<PathBuf> {
    let known = autoreleasepool(|_| gathered()).unwrap_or_default();
    known.into_iter().map(|(path, _)| path).collect()
}

fn among_the_apps(path: &Path) -> bool {
    path.components().any(|part| part.as_os_str().eq_ignore_ascii_case(APPLICATIONS))
}

fn kept_by_the_system(path: &Path) -> bool {
    let top = path.components().nth(1).map(|part| part.as_os_str().to_string_lossy().into_owned());
    let library = path.components().any(|part| part.as_os_str().eq_ignore_ascii_case(LIBRARY));
    library || top.is_some_and(|top| SYSTEM_PLACES.iter().any(|place| top.eq_ignore_ascii_case(place)))
}

fn is_a_bundle(path: &Path) -> bool {
    (path.join(INFO).is_file() || path.join(WRAPPED).is_dir()) && !kept_for_later(path)
}

fn kept_for_later(path: &Path) -> bool {
    let set_aside = |part: &std::ffi::OsStr| {
        let part = part.to_string_lossy();
        part.eq_ignore_ascii_case(CACHES) || part.to_ascii_lowercase().ends_with(NOT_INDEXED)
    };
    path.parent().is_some_and(|above| above.components().any(|part| set_aside(part.as_os_str())))
}

pub fn opens(path: &Path) -> Option<String> {
    if !is_extension(path) {
        return None;
    }
    autoreleasepool(|_| {
        let id = pane(path)?.bundleIdentifier()?.to_string();
        Some(format!("{ADDRESS}{id}"))
    })
}

fn is_extension(path: &Path) -> bool {
    path.extension().is_some_and(|ending| ending.eq_ignore_ascii_case(PANE_ENDING))
}

fn pane(path: &Path) -> Option<Retained<NSBundle>> {
    let bundle = NSBundle::bundleWithPath(&NSString::from_str(path.to_str()?))?;
    let within = |held: &NSDictionary<NSString, AnyObject>, key: &str| held.objectForKey(&NSString::from_str(key));
    let attributes = within(&*bundle.infoDictionary()?, EXTENSION)?.downcast::<NSDictionary>().ok()?;
    let attributes: &NSDictionary<NSString, AnyObject> = unsafe { attributes.cast_unchecked() };
    let point = within(attributes, EXTENSION_POINT)?.downcast::<NSString>().ok()?;
    let settings = within(attributes, SETTINGS)?.downcast::<NSDictionary>().ok()?;
    let settings: &NSDictionary<NSString, AnyObject> = unsafe { settings.cast_unchecked() };
    let by_address = within(settings, OPENS_BY_ADDRESS).is_some_and(|allowed| says_yes(&allowed));
    (point.to_string() == SETTINGS_POINT && by_address).then_some(bundle)
}

fn pane_title(path: &Path, pane: &NSBundle) -> Option<String> {
    let localized =
        pane.localizedInfoDictionary().and_then(|named| named.objectForKey(&NSString::from_str(SHOWN_NAME)));
    let localized = localized.and_then(|name| name.downcast::<NSString>().ok()).map(|name| name.to_string());
    let file = path.file_stem()?.to_str()?;
    let named = localized.map(|name| plain(&name)).filter(|name| !name.is_empty()).or_else(|| by_its_file(file));
    named.filter(|name| !NOT_FOR_PEOPLE.contains(&name.as_str()))
}

fn by_its_file(file: &str) -> Option<String> {
    if let Some((_, named)) = NAMED_BY_HAND.iter().find(|(internal, _)| *internal == file) {
        return Some((*named).to_owned());
    }
    let reads_as_a_name = !NOT_A_NAME.iter().any(|ending| file.ends_with(ending));
    reads_as_a_name.then(|| plain(file))
}

fn plain(name: &str) -> String {
    name.trim().replace(['\u{2011}', '\u{2010}'], "-")
}

pub fn settle(_menu: Option<&Path>) {}

pub fn used() -> Option<HashMap<Key, Used>> {
    let known = autoreleasepool(|_| gathered())?;
    let in_use = known.into_iter().filter(|(_, used)| *used != Used::default());
    let used: HashMap<Key, Used> = in_use.map(|(path, used)| (key_of(&path), used)).collect();
    (!used.is_empty()).then_some(used)
}

fn shown(path: &Path) -> Option<String> {
    let path = NSString::from_str(path.to_str()?);
    let shown = NSFileManager::defaultManager().displayNameAtPath(&path).to_string();
    Some(without_ending(&shown).to_owned()).filter(|shown| !shown.is_empty())
}

fn without_ending(shown: &str) -> &str {
    let shown = shown.trim();
    let cut = shown.len().checked_sub(ENDING.len()).filter(|cut| shown.is_char_boundary(*cut));
    match cut {
        Some(cut) if shown[cut..].eq_ignore_ascii_case(ENDING) => shown[..cut].trim_end(),
        _ => shown,
    }
}

fn says(path: &Path, key: &str) -> bool {
    let Some(info) = path.join(INFO).to_str().map(NSString::from_str) else { return false };
    let said = info_of(&info).and_then(|info| info.objectForKey(&NSString::from_str(key)));
    said.is_some_and(|value| says_yes(&value))
}

#[allow(deprecated)]
fn info_of(file: &NSString) -> Option<Retained<NSDictionary<NSString, AnyObject>>> {
    unsafe { NSDictionary::dictionaryWithContentsOfFile(file) }
}

fn says_yes(value: &AnyObject) -> bool {
    if let Some(number) = value.downcast_ref::<NSNumber>() {
        return number.boolValue();
    }
    let text = value.downcast_ref::<NSString>().map(|text| text.to_string().trim().to_ascii_lowercase());
    text.is_some_and(|text| SAID_YES.contains(&text.as_str()))
}

fn gathered() -> Option<Vec<(PathBuf, Used)>> {
    let [path, last, times] = [PATH, LAST_USED, USE_COUNT].map(NSString::from_str);
    let asking = NSString::from_str(EVERY_APP);
    let values = NSArray::from_retained_slice(&[last.clone(), times.clone()]);
    let scopes = NSArray::from_retained_slice(&[NSString::from_str(THIS_COMPUTER)]);
    let query = unsafe { MDQueryCreate(null(), raw(&asking), raw(&values), null()) };
    let query = unsafe { Retained::from_raw(query.cast::<AnyObject>()) }?;
    let ran = unsafe {
        MDQuerySetSearchScope(raw(&query), raw(&scopes), NO_SCOPE_OPTIONS);
        MDQueryExecute(raw(&query), BLOCK_UNTIL_GATHERED) != 0
    };
    if !ran {
        return None;
    }
    let results = unsafe { MDQueryGetResultCount(raw(&query)) }.max(0);
    let value = |name: &Retained<NSString>, index: isize| unsafe {
        MDQueryGetAttributeValueOfResultAtIndex(raw(&query), raw(name), index).cast::<AnyObject>().as_ref()
    };
    let place = |index: isize| unsafe {
        let item = MDQueryGetResultAtIndex(raw(&query), index);
        Retained::from_raw(MDItemCopyAttribute(item, raw(&path)).cast::<AnyObject>()).filter(|_| !item.is_null())
    };
    let seen = |index: isize| {
        let (last, times) = (value(&last, index), value(&times, index));
        place(index).and_then(|path| result(Some(&path), last, times))
    };
    Some((0..results).filter_map(seen).collect())
}

fn result(path: Option<&AnyObject>, last: Option<&AnyObject>, times: Option<&AnyObject>) -> Option<(PathBuf, Used)> {
    let path = PathBuf::from(path?.downcast_ref::<NSString>()?.to_string());
    let last = last.and_then(|last| last.downcast_ref::<NSDate>()).and_then(|last| stamp(last.timeIntervalSince1970()));
    let times = times.and_then(|times| times.downcast_ref::<NSNumber>()).map(|times| times.unsignedLongLongValue());
    let used =
        Used { last: last.unwrap_or(0), times: times.map_or(0, |times| u32::try_from(times).unwrap_or(u32::MAX)) };
    path.is_absolute().then_some((path, used))
}

fn raw<T: Message>(object: &Retained<T>) -> Ref {
    Retained::as_ptr(object).cast()
}

fn stamp(seconds: f64) -> Option<Stamp> {
    (seconds.is_finite() && seconds >= 1.0 && seconds <= f64::from(Stamp::MAX)).then_some(seconds as Stamp)
}

#[cfg(test)]
mod tests {
    use super::*;

    const MONDAY_NOON: Stamp = 1_704_110_400;

    #[test]
    fn shown_name_drops_ending() {
        assert_eq!(without_ending("Safari.app"), "Safari");
        assert_eq!(without_ending("Find My"), "Find My");
        assert_eq!(without_ending(" Notes.APP "), "Notes");
        assert_eq!(without_ending("Apple.application"), "Apple.application", "only the ending itself is cut");
        assert_eq!(without_ending("Caf\u{e9}.App"), "Caf\u{e9}");
        assert_eq!(without_ending(".app"), "");
        assert_eq!(without_ending("\u{e9}pp"), "\u{e9}pp", "never cut inside a letter");
    }

    #[test]
    fn unnamed_pane_uses_file_name() {
        assert_eq!(by_its_file("Bluetooth").as_deref(), Some("Bluetooth"));
        assert_eq!(by_its_file("Touch ID & Password").as_deref(), Some("Touch ID & Password"));
        assert_eq!(by_its_file("PowerPreferences").as_deref(), Some("Battery"), "the one the system misnames");
        assert_eq!(by_its_file("AppleIDSettings").as_deref(), Some("Apple Account"));
        for internal in
            ["HeadphoneSettingsExtension", "SecurityImprovementsExtension", "DisplaysExt", "GeneralSettings"]
        {
            assert_eq!(by_its_file(internal), None, "{internal}");
        }
        assert_eq!(plain(" Wi\u{2011}Fi "), "Wi-Fi", "a hyphen that does not break is typed as a hyphen");
        assert!(is_extension(Path::new("/x/Bluetooth.appex")) && !is_extension(Path::new("/x/Maps.app")));
        assert_eq!(opens(Path::new("/Applications/Maps.app")), None, "an app is opened as itself");
    }

    #[test]
    fn plist_yes_as_number_or_word() {
        let (yes, no) = (NSNumber::new_bool(true), NSNumber::new_bool(false));
        let (yes, no): (&AnyObject, &AnyObject) = (&yes, &no);
        assert!(says_yes(yes) && !says_yes(no));
        for (said, meant) in [("1", true), ("YES", true), (" true ", true), ("0", false), ("no", false), ("", false)] {
            let said_text = NSString::from_str(said);
            let value: &AnyObject = &said_text;
            assert_eq!(says_yes(value), meant, "{said:?}");
        }
        let neither = NSDate::dateWithTimeIntervalSince1970(1.0);
        let value: &AnyObject = &neither;
        assert!(!says_yes(value));
    }

    #[test]
    fn result_lists_every_known_app() {
        let path = NSString::from_str("/Applications/Zen.app");
        let date = NSDate::dateWithTimeIntervalSince1970(f64::from(MONDAY_NOON) + 0.4);
        let count = NSNumber::new_u32(1_736);
        let (path, date, count): (&AnyObject, &AnyObject, &AnyObject) = (&path, &date, &count);
        let zen = PathBuf::from("/Applications/Zen.app");
        let seen = |used: Used| Some((zen.clone(), used));
        assert_eq!(result(Some(path), Some(date), Some(count)), seen(Used { last: MONDAY_NOON, times: 1_736 }));
        assert_eq!(result(Some(path), None, Some(count)), seen(Used { last: 0, times: 1_736 }));
        assert_eq!(result(Some(path), Some(date), None), seen(Used { last: MONDAY_NOON, times: 0 }));
        assert_eq!(result(Some(path), None, None), seen(Used::default()), "known, though never seen in use");
        assert_eq!(result(None, Some(date), Some(count)), None);
        assert_eq!(result(Some(date), Some(date), Some(count)), None, "a path has to be text");
    }

    #[test]
    #[ignore = "reads the real apps of this machine and asks its Spotlight index how they are used"]
    fn this_mac_shows_real_names() {
        for app in
            ["/System/Applications/FindMy.app", "/System/Library/CoreServices/Finder.app", "/Applications/Safari.app"]
        {
            println!("{app}: {:?}", read(Path::new(app)));
        }
        assert_eq!(read(Path::new("/System/Applications/FindMy.app")).title.as_deref(), Some("Find My"));
        assert_eq!(read(Path::new("/Applications/Safari.app")).title.as_deref(), Some("Safari"));
        assert!(!read(Path::new("/System/Library/CoreServices/Finder.app")).backstage);
        let missing = Read { title: Some("Thing".into()), unlisted: true, ..Read::default() };
        assert_eq!(read(Path::new("/no/such/Thing.app")), missing, "a folder that is no bundle is no app");
        let dock = read(Path::new("/System/Library/CoreServices/Dock.app"));
        assert_eq!((dock.unlisted, dock.backstage), (false, true), "an agent is listed, behind the scenes");
        let installer = read(Path::new("/System/Library/CoreServices/Installer.app"));
        assert_eq!((installer.unlisted, installer.backstage), (false, false), "a system app a person opens");
        let known = every();
        assert!(known.len() > 100 && known.iter().all(|app| app.is_absolute()), "{} apps", known.len());
        assert!(known.iter().any(|app| app.ends_with("CoreServices/Dock.app")), "the system's own are among them");
        assert!(among_the_apps(Path::new("/Applications/Zen.app")) && !among_the_apps(Path::new("/x/Zen.app")));
        assert!(kept_by_the_system(Path::new("/System/Library/CoreServices/Dock.app")));
        assert!(kept_by_the_system(Path::new("/Users/me/Library/Application Support/X/Agent.app")));
        assert!(kept_by_the_system(Path::new("/Library/Application Support/Adobe/Broker.app")));
        assert!(!kept_by_the_system(Path::new("/Users/me/Parallels/Windows 11.pvm/VM.app")), "a person's own place");
        assert!(!kept_by_the_system(Path::new("/Volumes/Work/Tools/Menu.app")));
        let placeholder = "/Users/me/Library/Daemon Containers/1E/Data/Library/Caches/Placeholders-v6.noindex/Grok.app";
        assert!(kept_for_later(Path::new(placeholder)) && kept_for_later(Path::new("/x/Caches/Old.app")));
        assert!(kept_for_later(Path::new("/x/Updates.noindex/New.app")), "what the system itself does not index");
        assert!(!kept_for_later(Path::new("/Applications/Caches.app")) && !kept_for_later(Path::new("/x/Tool.app")));
        let kit = Path::new("/System/Library/ExtensionKit/Extensions");
        for (file, shown) in [("Wi-Fi", "Wi-Fi"), ("DisplaysExt", "Displays"), ("PowerPreferences", "Battery")] {
            let pane = read(&kit.join(format!("{file}.appex")));
            assert_eq!((pane.title.as_deref(), pane.unlisted), (Some(shown), false), "{file}");
        }
        assert!(read(&kit.join("AccessibilitySettingsSearchExtension.appex")).unlisted, "no pane of its own");
        let address = opens(&kit.join("Bluetooth.appex"));
        assert_eq!(address.as_deref(), Some("x-apple.systempreferences:com.apple.BluetoothSettings"));
        let used = used().expect("Spotlight knows some apps");
        let finder = used.get(&key_of(Path::new("/System/Library/CoreServices/Finder.app")));
        println!("{} apps were used, Finder: {finder:?}", used.len());
        assert!(used.values().all(|used| *used != Used::default()));
    }
}
