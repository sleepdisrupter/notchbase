//! Apps and settings panes in the index: what counts as one, where it is at home, and the name it shows.

use std::ops::Range;

use crate::find::model::{EntryId, Live, flag, tier};

pub const ENDINGS: [&[u8]; 4] = [b".lnk", b".url", b".appex", b".app"];

const BUNDLE: &[u8] = b".app";
const PANE: &[u8] = b".appex";
const EXTENSIONS: &[u8] = b"extensions";
const EXTENSION_KIT: &[u8] = b"extensionkit";
const START_MENU: &[u8] = b"start menu";
const PROGRAMS: &[u8] = b"programs";
const APPLICATIONS: &[u8] = b"applications";
const UTILITIES: &[u8] = b"utilities";
const CONTENTS: &[u8] = b"contents";
const CORE_SERVICES: &[u8] = b"coreservices";
const FINDER: &[u8] = b"finder.app";
const WEB_LINK: &[u8] = b".url";
const README: &[u8] = b"readme";
const REMOVER: &[u8] = b"uninstall";
const BESIDE_AN_APP: [&[u8]; 8] = [
    b" uninstaller",
    b" help",
    b" documentation",
    b" website",
    b" readme",
    b" license",
    b" release notes",
    b" user guide",
];
const DEEPEST_SHORTCUT: usize = 6;

pub fn launcher(live: &Live, id: EntryId, folded_name: &[u8], flags: u8) -> bool {
    let Some(ending) = ending(folded_name) else { return false };
    if ending == PANE {
        return flags & flag::IDENTITY == flag::DIR && among_the_panes(live, id);
    }
    match (ending == BUNDLE, flags & flag::LINK != 0, flags & flag::DIR != 0) {
        (true, true, _) => in_applications(live, id),
        (true, false, true) => !within_an_app(live, id) || carried(live, id),
        (false, false, false) => in_the_start_menu(live, id),
        _ => false,
    }
}

pub fn pane(folded_name: &[u8]) -> bool {
    ending(folded_name) == Some(PANE)
}

pub fn homed(live: &Live, id: EntryId, flags: u8) -> bool {
    if flags & flag::DIR == 0 || flags & flag::LINK != 0 || pane(live.folded(id)) {
        return true;
    }
    let named = |at: EntryId, name: &[u8]| live.folded(at) == name;
    let parent = live.parent(id);
    let among_apps = || std::iter::successors(parent, |at| live.parent(*at)).any(|at| named(at, APPLICATIONS));
    let the_finder = live.folded(id) == FINDER && parent.is_some_and(|parent| named(parent, CORE_SERVICES));
    the_finder || among_apps()
}

pub fn extra(folded_name: &[u8]) -> bool {
    let name = shown(folded_name);
    let beside = || BESIDE_AN_APP.iter().any(|word| name.ends_with(word));
    folded_name.ends_with(WEB_LINK) || name.starts_with(REMOVER) || name == README || beside()
}

pub fn shown_as(name: &str) -> &str {
    &name[..shown(name.as_bytes()).len()]
}

pub fn shown(name: &[u8]) -> &[u8] {
    &name[..name.len() - ending(name).map_or(0, <[u8]>::len)]
}

pub fn titled(name: &str, marks: Vec<Range<u16>>) -> (&str, Vec<Range<u16>>) {
    let shown = shown_as(name);
    let letters = u16::try_from(shown.chars().count()).unwrap_or(u16::MAX);
    let inside = marks.into_iter().map(|mark| mark.start.min(letters)..mark.end.min(letters));
    (shown, inside.filter(|mark| mark.start < mark.end).collect())
}

pub fn unseen(live: &Live, apps: &mut Vec<EntryId>, id: EntryId) -> bool {
    let name = live.folded(id);
    if !launcher(live, id, name, live.flags(id)) {
        return true;
    }
    let twin = apps.iter().any(|app| shown(live.folded(*app)) == shown(name));
    if !twin {
        apps.push(id);
    }
    !twin
}

fn ending(name: &[u8]) -> Option<&'static [u8]> {
    let ends =
        |ending: &&[u8]| name.len() > ending.len() && name[name.len() - ending.len()..].eq_ignore_ascii_case(ending);
    ENDINGS.into_iter().find(ends)
}

fn among_the_panes(live: &Live, id: EntryId) -> bool {
    let named = |at: EntryId, name: &[u8]| live.folded(at) == name;
    let Some(extensions) = live.parent(id).filter(|folder| named(*folder, EXTENSIONS)) else { return false };
    live.parent(extensions).is_some_and(|kit| named(kit, EXTENSION_KIT))
}

pub fn stands_alone(flags: u8) -> bool {
    flags & flag::INSIDE_PACKAGE == 0 && tier(flags) == 0
}

fn within_an_app(live: &Live, id: EntryId) -> bool {
    let bundle = |at: &EntryId| ending(live.folded(*at)) == Some(BUNDLE);
    std::iter::successors(live.parent(id), |at| live.parent(*at)).any(|at| bundle(&at))
}

fn carried(live: &Live, id: EntryId) -> bool {
    let named = |at: EntryId, name: &[u8]| live.folded(at) == name;
    let Some(apps) = live.parent(id).filter(|apps| named(*apps, APPLICATIONS)) else { return false };
    let Some(contents) = live.parent(apps).filter(|contents| named(*contents, CONTENTS)) else { return false };
    let host = live.parent(contents).filter(|host| !within_an_app(live, *host));
    host.is_some_and(|host| ending(live.folded(host)) == Some(BUNDLE) && live.flags(host) & flag::DIR != 0)
}

fn in_applications(live: &Live, id: EntryId) -> bool {
    let named = |at: Option<EntryId>, name: &[u8]| at.is_some_and(|at| live.folded(at) == name);
    let parent = live.parent(id);
    let above = parent.and_then(|parent| live.parent(parent));
    named(parent, APPLICATIONS) || (named(parent, UTILITIES) && named(above, APPLICATIONS))
}

fn in_the_start_menu(live: &Live, id: EntryId) -> bool {
    let named = |at: EntryId, name: &[u8]| live.folded(at) == name;
    let programs = |at: &EntryId| named(*at, PROGRAMS) && live.parent(*at).is_some_and(|menu| named(menu, START_MENU));
    std::iter::successors(live.parent(id), |at| live.parent(*at)).take(DEEPEST_SHORTCUT).any(|at| programs(&at))
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;
    use crate::find::model::with_tier;
    use crate::find::tests::corpus::{EPOCH, Planted, plant};

    const MENU: &str = "/Users/me/AppData/Roaming/Microsoft/Windows/Start Menu/Programs";
    const SYSTEM: u8 = 1;

    fn disk(planted: &[(&str, u8)]) -> Live {
        let mut live = Live::empty();
        for (path, flags) in planted {
            plant(&mut live, &Planted { path, modified: EPOCH, created: EPOCH, flags: *flags });
        }
        live
    }

    fn launches(live: &Live, path: &str) -> bool {
        let id = live.resolve(Path::new(path)).unwrap_or_else(|| panic!("{path} was planted"));
        launcher(live, id, live.folded(id), live.flags(id))
    }

    #[test]
    fn bundle_launches_unless_nested() {
        let packaged = with_tier(flag::INSIDE_PACKAGE, SYSTEM);
        let live = disk(&[
            ("/Applications/Xcode.app/", 0),
            ("/Users/me/Downloads/Tool.app/", 0),
            ("/Applications/Xcode.app/Contents/Helper.app/", packaged),
            ("/Applications/Xcode.app/Contents/Frameworks/Kit.framework/Resources/Agent.app/", packaged),
            ("/Users/me/Library/Caches/Updater.app/", with_tier(0, SYSTEM)),
            ("/Users/me/Parallels/Windows 11.pvm/VM.app/", packaged),
            ("/System/Library/Frameworks/Tk.framework/Versions/8.5/Resources/Wish.app/", packaged),
            ("/System/Library/CoreServices/Dock.app/", with_tier(0, SYSTEM)),
            ("/Users/me/Documents/notes.app", 0),
            ("/Users/me/Documents/Applications/", 0),
        ]);
        assert!(launches(&live, "/Applications/Xcode.app") && launches(&live, "/Users/me/Downloads/Tool.app"));
        assert!(!launches(&live, "/Applications/Xcode.app/Contents/Helper.app"), "a helper inside an app");
        let agent = "/Applications/Xcode.app/Contents/Frameworks/Kit.framework/Resources/Agent.app";
        assert!(!launches(&live, agent), "however deep inside the app it lies");
        assert!(launches(&live, "/Users/me/Library/Caches/Updater.app"), "a bundle in a system place is an app too");
        assert!(
            launches(&live, "/Users/me/Parallels/Windows 11.pvm/VM.app"),
            "and one inside a package that is no app"
        );
        assert!(launches(&live, "/System/Library/Frameworks/Tk.framework/Versions/8.5/Resources/Wish.app"));
        assert!(launches(&live, "/System/Library/CoreServices/Dock.app"));
        assert!(!launches(&live, "/Users/me/Documents/notes.app"), "a file is not a bundle");
        assert!(!launches(&live, "/Users/me/Documents/Applications") && !launches(&live, "/Applications"));
    }

    #[test]
    fn nested_applications_launch() {
        let packaged = with_tier(flag::INSIDE_PACKAGE, SYSTEM);
        let inside = |rest: &str| format!("/Applications/Xcode.app/Contents/{rest}");
        let live = disk(&[
            ("/Applications/Xcode.app/", 0),
            (&inside(""), packaged),
            (&inside("Applications/"), packaged),
            (&inside("Applications/Instruments.app/"), packaged),
            (&inside("Applications/Instruments.app/Contents/Applications/Deeper.app/"), packaged),
            (&inside("Frameworks/Applications/Agent.app/"), packaged),
            (&inside("Applications/notes.app"), packaged),
            ("/Users/me/Library/Caches/Old.app/", with_tier(0, SYSTEM)),
            ("/Users/me/Library/Caches/Old.app/Contents/Applications/Tool.app/", packaged),
            ("/Users/me/Contents/Applications/Loose.app/", 0),
        ]);
        assert!(launches(&live, "/Applications/Xcode.app/Contents/Applications/Instruments.app"));
        let deeper = inside("Applications/Instruments.app/Contents/Applications/Deeper.app");
        assert!(!launches(&live, &deeper), "its host is itself inside an app");
        assert!(!launches(&live, &inside("Frameworks/Applications/Agent.app")), "not directly in Contents");
        assert!(!launches(&live, &inside("Applications/notes.app")), "a file is not a bundle");
        assert!(launches(&live, "/Users/me/Library/Caches/Old.app/Contents/Applications/Tool.app"), "any host carries");
        assert!(launches(&live, "/Users/me/Contents/Applications/Loose.app"), "it stands alone anyway");
    }

    #[test]
    fn where_launchers_are_at_home() {
        let live = disk(&[
            ("/Applications/Xcode.app/", 0),
            ("/Applications/Adobe Photoshop 2026/Adobe Photoshop 2026.app/", 0),
            ("/Users/me/Applications/Mine.app/", 0),
            ("/System/Library/CoreServices/Finder.app/", 0),
            ("/System/Library/CoreServices/Applications/Keychain Access.app/", 0),
            ("/Applications/Safari.app", flag::LINK),
            (&format!("{MENU}/Notepad.lnk"), with_tier(0, SYSTEM)),
            ("/Users/me/Applications (Parallels)/Windows 11 Applications.localized/0097da.app/", 0),
            ("/Users/me/Downloads/Tool.app/", 0),
            ("/Users/me/Documents/project/dist/Install.app/", 0),
        ]);
        let at_home = |path: &str| {
            let id = live.resolve(Path::new(path)).unwrap_or_else(|| panic!("{path} was planted"));
            homed(&live, id, live.flags(id))
        };
        for app in [
            "/Applications/Xcode.app",
            "/Applications/Adobe Photoshop 2026/Adobe Photoshop 2026.app",
            "/Users/me/Applications/Mine.app",
            "/System/Library/CoreServices/Finder.app",
            "/System/Library/CoreServices/Applications/Keychain Access.app",
            "/Applications/Safari.app",
        ] {
            assert!(at_home(app), "{app}");
        }
        assert!(at_home(&format!("{MENU}/Notepad.lnk")), "a start menu is where shortcuts live");
        for app in [
            "/Users/me/Applications (Parallels)/Windows 11 Applications.localized/0097da.app",
            "/Users/me/Downloads/Tool.app",
            "/Users/me/Documents/project/dist/Install.app",
        ] {
            assert!(!at_home(app), "{app}");
        }
    }

    #[test]
    fn panes_launch_from_extensions() {
        let kit = "/System/Library/ExtensionKit/Extensions";
        let live = disk(&[
            (&format!("{kit}/Bluetooth.appex/"), 0),
            (&format!("{kit}/Wi-Fi.appex/"), 0),
            (&format!("{kit}/Bluetooth.appex/Contents/Nested.appex/"), with_tier(flag::INSIDE_PACKAGE, SYSTEM)),
            (&format!("{kit}/notes.appex"), 0),
            ("/Applications/Maps.app/Contents/PlugIns/Widget.appex/", with_tier(flag::INSIDE_PACKAGE, SYSTEM)),
            ("/Users/me/Extensions/Mine.appex/", 0),
        ]);
        assert!(launches(&live, &format!("{kit}/Bluetooth.appex")) && launches(&live, &format!("{kit}/Wi-Fi.appex")));
        assert!(!launches(&live, &format!("{kit}/Bluetooth.appex/Contents/Nested.appex")));
        assert!(!launches(&live, &format!("{kit}/notes.appex")), "a file is no extension");
        assert!(!launches(&live, "/Applications/Maps.app/Contents/PlugIns/Widget.appex"), "an app's own plug-in");
        assert!(!launches(&live, "/Users/me/Extensions/Mine.appex"), "not the system's kit");
        assert!(pane(b"bluetooth.appex") && !pane(b"maps.app") && !pane(b"appex"));
        assert_eq!((shown_as("Wi-Fi.appex"), shown(b"bluetooth.appex")), ("Wi-Fi", &b"bluetooth"[..]));
        let id = live.resolve(Path::new(&format!("{kit}/Bluetooth.appex"))).unwrap();
        assert!(homed(&live, id, live.flags(id)), "a pane is at home in the system");
    }

    #[test]
    fn extras_told_from_the_app() {
        for name in [
            "uninstall steam.lnk",
            "uninstaller.lnk",
            "treesize free help.lnk",
            "steam support center.url",
            "readme.lnk",
            "vlc documentation.lnk",
            "vlc release notes.lnk",
            "uninstall adobe creative cloud.app",
        ] {
            assert!(extra(name.as_bytes()), "{name}");
        }
        for name in ["steam.lnk", "help desk.lnk", "readme viewer.app", "license manager.lnk", "safari.app", "url.lnk"]
        {
            assert!(!extra(name.as_bytes()), "{name}");
        }
    }

    #[test]
    fn linked_app_launch_places() {
        let live = disk(&[
            ("/Applications/Safari.app", flag::LINK),
            ("/Applications/Utilities/Terminal.app", flag::LINK),
            ("/Users/me/Applications/Mine.app", flag::LINK),
            ("/Users/me/Desktop/Safari.app", flag::LINK),
            ("/Applications/Games/Chess.app", flag::LINK),
            ("/opt/Utilities/Tool.app", flag::LINK),
            ("/Applications/current", flag::LINK),
        ]);
        for app in
            ["/Applications/Safari.app", "/Applications/Utilities/Terminal.app", "/Users/me/Applications/Mine.app"]
        {
            assert!(launches(&live, app), "{app}");
        }
        for link in [
            "/Users/me/Desktop/Safari.app",
            "/Applications/Games/Chess.app",
            "/opt/Utilities/Tool.app",
            "/Applications/current",
        ] {
            assert!(!launches(&live, link), "{link}");
        }
    }

    #[test]
    fn shortcut_launch_places() {
        let live = disk(&[
            (&format!("{MENU}/Notepad.lnk"), with_tier(0, SYSTEM)),
            (&format!("{MENU}/Steam/Portal.url"), with_tier(0, SYSTEM)),
            (&format!("{MENU}/Vendor/Suite/Tools/Deep.lnk"), with_tier(0, SYSTEM)),
            (&format!("{MENU}/a/b/c/d/e/f/Deeper.lnk"), with_tier(0, SYSTEM)),
            (&format!("{MENU}/desktop.ini"), flag::HIDDEN),
            (&format!("{MENU}/Folder.lnk/"), 0),
            ("/Users/me/Desktop/Notepad.lnk", 0),
            ("/Users/me/AppData/Roaming/Microsoft/Windows/Recent/report.docx.lnk", with_tier(0, SYSTEM)),
            ("/Users/me/AppData/Roaming/Microsoft/Windows/Start Menu/Pinned.lnk", with_tier(0, SYSTEM)),
            ("/Users/me/Programs/Tool.lnk", 0),
        ]);
        assert!(
            launches(&live, &format!("{MENU}/Notepad.lnk")) && launches(&live, &format!("{MENU}/Steam/Portal.url"))
        );
        assert!(launches(&live, &format!("{MENU}/Vendor/Suite/Tools/Deep.lnk")), "three folders deeper");
        assert!(!launches(&live, &format!("{MENU}/a/b/c/d/e/f/Deeper.lnk")), "the walk up is bounded");
        assert!(!launches(&live, &format!("{MENU}/desktop.ini")) && !launches(&live, &format!("{MENU}/Folder.lnk")));
        assert!(!launches(&live, "/Users/me/Desktop/Notepad.lnk"), "a shortcut on the Desktop is a file");
        assert!(!launches(&live, "/Users/me/AppData/Roaming/Microsoft/Windows/Recent/report.docx.lnk"));
        assert!(!launches(&live, "/Users/me/AppData/Roaming/Microsoft/Windows/Start Menu/Pinned.lnk"));
        assert!(!launches(&live, "/Users/me/Programs/Tool.lnk"), "programs has to lie in a start menu");
    }

    #[test]
    fn shown_name_loses_ending() {
        assert_eq!(
            (shown_as("TreeSize Free.lnk"), shown_as("Safari.app"), shown_as("Portal.URL")),
            ("TreeSize Free", "Safari", "Portal")
        );
        assert_eq!((shown_as("Caf\u{e9}.app"), shown(b"google chrome.lnk")), ("Caf\u{e9}", &b"google chrome"[..]));
        for kept in ["notes.txt", "setup.exe", ".app", "lnk", "archive.app.zip", ""] {
            assert_eq!(shown_as(kept), kept);
        }
    }

    #[test]
    fn title_keeps_inner_marks() {
        let lit = |name: &str, marks: &[(u16, u16)]| {
            let (shown, marks) = titled(name, marks.iter().map(|(start, end)| *start..*end).collect());
            (shown.to_owned(), marks.iter().map(|mark| (mark.start, mark.end)).collect::<Vec<_>>())
        };
        let tree = "TreeSize Free".to_owned();
        assert_eq!(lit("TreeSize Free.lnk", &[(0, 8), (9, 13)]), (tree.clone(), vec![(0, 8), (9, 13)]));
        assert_eq!(lit("TreeSize Free.lnk", &[(9, 15)]), (tree.clone(), vec![(9, 13)]), "cut where the name ends");
        assert_eq!(lit("TreeSize Free.lnk", &[(0, 4), (13, 17)]), (tree, vec![(0, 4)]), "the ending lights nothing");
        assert_eq!(lit("Caf\u{e9}.app", &[(3, 8)]), ("Caf\u{e9}".to_owned(), vec![(3, 4)]), "letters, not bytes");
        assert_eq!(lit("notes.txt", &[(0, 9)]), ("notes.txt".to_owned(), vec![(0, 9)]), "no launcher ending, no cut");
    }

    #[test]
    fn second_twin_was_seen() {
        let everyone = "/ProgramData/Microsoft/Windows/Start Menu/Programs";
        let live = disk(&[
            (&format!("{MENU}/TreeSize Free.lnk"), 0),
            (&format!("{everyone}/TreeSize Free/TreeSize Free.lnk"), 0),
            (&format!("{everyone}/TreeSize Free/TreeSize Free Help.lnk"), 0),
            ("/Users/me/Desktop/TreeSize Free.lnk", 0),
        ]);
        let id = |path: &str| live.resolve(Path::new(path)).unwrap();
        let mut apps = Vec::new();
        assert!(unseen(&live, &mut apps, id(&format!("{MENU}/TreeSize Free.lnk"))));
        assert!(!unseen(&live, &mut apps, id(&format!("{everyone}/TreeSize Free/TreeSize Free.lnk"))));
        assert!(unseen(&live, &mut apps, id(&format!("{everyone}/TreeSize Free/TreeSize Free Help.lnk"))));
        assert!(unseen(&live, &mut apps, id("/Users/me/Desktop/TreeSize Free.lnk")), "a file is never a twin");
        assert!(unseen(&live, &mut apps, id("/Users/me/Desktop/TreeSize Free.lnk")) && apps.len() == 2);
    }
}
