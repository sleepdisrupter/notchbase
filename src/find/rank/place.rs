//! Where a row lives and what that costs: its tier, its depth under home, how fresh it is, and whether it is hidden,
//! someone else's or machine made.

use crate::find::context::Stamp;
use crate::find::kind::{Kind, extension};
use crate::find::model::{EntryId, Live, flag, tier};
use crate::find::query::Away;
use crate::find::weights::{
    APP, DEPTH, DEPTH_FLOOR, DEPTH_FREE, FOLDER, FRESH_EDIT, FRESH_EDIT_HALF_LIFE, FRESH_NEW, FRESH_NEW_HALF_LIFE,
    HIDDEN, TIER_PENALTY,
};

use super::launch::launcher;

pub const FRESH_BEST: f32 = FRESH_EDIT + FRESH_NEW;

const UNKNOWN: Stamp = 0;
const APPLICATIONS: &[u8] = b"applications";
const USERS: &[u8] = b"users";
const SHARED: &[u8] = b"shared";
const SHARED_FREE: usize = 2;
const SEALED: [&[u8]; 4] = [b".movpkg", b".appex", b".tmp", b".download"];
const SYSTEM: &[u8] = b"system";
const MACHINE_RUN: usize = 16;
const MACHINE_TURNS: usize = 4;
const FADED_AFTER_HALF_LIVES: Stamp = 24;
const DEEPEST_COUNTED: usize = DEPTH_FREE + (DEPTH_FLOOR / DEPTH) as usize;

pub fn standing(name: &[u8], flags: u8) -> f32 {
    let ending = extension(name);
    let launches = Kind::App.extensions().iter().any(|app| ending == Some(app.as_bytes()));
    let kind = match (launches && Kind::of(name, flags) == Kind::App, flags & flag::DIR != 0) {
        (true, _) => APP,
        (false, true) => FOLDER,
        (false, false) => 0.0,
    };
    let hidden = if flags & flag::HIDDEN != 0 { HIDDEN } else { 0.0 };
    kind + hidden + TIER_PENALTY[usize::from(tier(flags)) % TIER_PENALTY.len()]
}

pub fn standing_at_best(flags: u8) -> f32 {
    let hidden = if flags & flag::HIDDEN != 0 { HIDDEN } else { 0.0 };
    APP.max(FOLDER) + hidden + TIER_PENALTY[usize::from(tier(flags)) % TIER_PENALTY.len()]
}

pub fn freshness(now: Stamp, modified: Stamp, created: Stamp) -> f32 {
    faded(FRESH_EDIT, now, modified, FRESH_EDIT_HALF_LIFE) + faded(FRESH_NEW, now, created, FRESH_NEW_HALF_LIFE)
}

pub fn newness(now: Stamp, created: Stamp) -> f32 {
    faded(FRESH_NEW, now, created, FRESH_NEW_HALF_LIFE)
}

#[cfg(test)]
pub fn depth(live: &Live, id: EntryId, home: Option<EntryId>) -> f32 {
    lay(live, id, home).depth
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Lay {
    pub depth: f32,
    pub below_hidden: bool,
    pub among_apps: bool,
    pub at_home: bool,
}

pub fn lay(live: &Live, id: EntryId, home: Option<EntryId>) -> Lay {
    let (mut levels, mut below_hidden, mut among_apps, mut at_home) = (0, false, false, false);
    let mut at = id;
    while levels < DEEPEST_COUNTED {
        let Some(parent) = live.parent(at) else { break };
        levels += 1;
        if Some(parent) == home {
            at_home = levels == 1;
            break;
        }
        below_hidden |= live.flags(parent) & flag::HIDDEN != 0;
        among_apps |= not_the_users(live, parent, home) || sealed(live.folded(parent));
        among_apps |= levels > SHARED_FREE && shared(live, parent);
        at = parent;
    }
    let depth = (DEPTH * levels.saturating_sub(DEPTH_FREE) as f32).max(DEPTH_FLOOR);
    Lay { depth, below_hidden, among_apps, at_home }
}

pub fn standing_away(flags: u8) -> Away {
    if tier(flags) != 0 || flags & flag::INSIDE_PACKAGE != 0 {
        Away::Foreign
    } else if flags & flag::HIDDEN != 0 {
        Away::Hidden
    } else {
        Away::InSight
    }
}

pub fn away(live: &Live, id: EntryId, lay: &Lay) -> Away {
    let (flags, name) = (live.flags(id), live.folded(id));
    let beside_an_app = || lay.among_apps && !launcher(live, id, name, flags);
    let foreign = lay.below_hidden || beside_an_app() || machine_made(name);
    standing_away(flags).max(if foreign { Away::Foreign } else { Away::InSight })
}

pub fn far(live: &Live, id: EntryId, home: Option<EntryId>) -> Away {
    away(live, id, &lay(live, id, home))
}

pub fn machine_made(name: &[u8]) -> bool {
    let (mut run, mut turns, mut was_digit) = (0usize, 0usize, None);
    for byte in name {
        if !byte.is_ascii_alphanumeric() {
            (run, turns, was_digit) = (0, 0, None);
            continue;
        }
        let digit = byte.is_ascii_digit();
        turns += usize::from(was_digit.is_some_and(|was| was != digit));
        (run, was_digit) = (run + 1, Some(digit));
        if run >= MACHINE_RUN && turns >= MACHINE_TURNS {
            return true;
        }
    }
    false
}

fn sealed(folder: &[u8]) -> bool {
    SEALED.iter().any(|ending| folder.ends_with(ending))
}

fn shared(live: &Live, folder: EntryId) -> bool {
    let top = |at: EntryId| live.parent(at).is_none();
    let users =
        live.parent(folder).filter(|above| live.folded(*above) == USERS && live.parent(*above).is_some_and(top));
    live.folded(folder) == SHARED && users.is_some()
}

fn not_the_users(live: &Live, folder: EntryId, home: Option<EntryId>) -> bool {
    let Some(above) = live.parent(folder) else { return false };
    let top = |at: EntryId| live.parent(at).is_none();
    match live.folded(folder) {
        name if name == SYSTEM => top(above),
        name if name == APPLICATIONS => Some(above) == home || top(above),
        _ => false,
    }
}

fn faded(fresh: f32, now: Stamp, at: Stamp, half_life: Stamp) -> f32 {
    let age = now.saturating_sub(at);
    if at == UNKNOWN || age / half_life >= FADED_AFTER_HALF_LIVES {
        return 0.0;
    }
    fresh * (-(age as f32) / half_life as f32).exp2()
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;
    use crate::find::context::{DAY, HOUR};
    use crate::find::model::with_tier;
    use crate::find::tests::corpus::from_paths;

    const NOW: Stamp = 1_704_110_400;

    #[test]
    fn apps_up_junk_down() {
        assert_eq!(standing(b"calendar.app", flag::DIR), APP);
        assert_eq!(standing(b"setup.exe", 0), APP);
        assert_eq!(standing(b"documents", flag::DIR), FOLDER);
        assert_eq!(standing(b"notes.app", flag::LINK), 0.0, "a link to an app is a link");
        assert_eq!(standing(b"cargo.toml", 0), 0.0);
        assert_eq!(standing(b".cargo", flag::DIR | flag::HIDDEN), FOLDER + HIDDEN);
        let by_tier: Vec<f32> = (0..4).map(|level| standing(b"index.js", with_tier(0, level))).collect();
        assert_eq!(by_tier, TIER_PENALTY);
        for (name, flags) in
            [(&b"calendar.app"[..], flag::DIR), (b".cargo", flag::DIR | flag::HIDDEN), (b"a.js", with_tier(0, 2))]
        {
            assert!(standing_at_best(flags) >= standing(name, flags), "the flags alone never promise too little");
        }
    }

    #[test]
    fn freshness_halves_per_half_life() {
        assert_eq!(freshness(NOW, NOW, NOW), FRESH_EDIT + FRESH_NEW);
        assert_eq!(freshness(NOW, NOW - FRESH_EDIT_HALF_LIFE, UNKNOWN), FRESH_EDIT / 2.0);
        assert_eq!(freshness(NOW, UNKNOWN, NOW - FRESH_NEW_HALF_LIFE), FRESH_NEW / 2.0);
        let this_morning = freshness(NOW, NOW - 3 * HOUR, NOW - 400 * DAY);
        assert!((88.0..90.0).contains(&this_morning), "{this_morning}");
        assert_eq!(freshness(NOW, NOW - 400 * DAY, NOW - 400 * DAY), 0.0);
        assert_eq!(freshness(NOW, UNKNOWN, UNKNOWN), 0.0, "a file with no dates is not new");
        assert_eq!(freshness(NOW, NOW + DAY, UNKNOWN), FRESH_EDIT, "a clock that ran ahead counts as now");
        assert_eq!((newness(NOW, NOW), newness(NOW, UNKNOWN)), (FRESH_NEW, 0.0), "an app is new, never edited");
        assert_eq!(newness(NOW, NOW - FRESH_NEW_HALF_LIFE), FRESH_NEW / 2.0);
    }

    fn disk() -> Live {
        let mut live = from_paths(&[
            "/Users/john/Documents/report.pdf",
            "/Users/john/Projects/site/",
            "/Users/john/Applications/Mine.app/",
            "/Applications/Maps.app/",
            "/Applications/Adobe Photoshop 2026/Adobe Photoshop 2026.app/",
            "/Applications/Adobe Photoshop 2026/Presets/Blue Filter.blw",
            "/System/Applications/Utilities/Terminal.app/",
            "/Users/john/Documents/Applications/cover-letter.pdf",
            "/Users/john/Movies/TV/2-3297277-X3PWNEP5BZENIGAC3TVKIPZBTA74X2QB/",
        ]);
        let put = |live: &mut Live, path: &str, flags: u8| {
            crate::find::tests::corpus::plant(
                live,
                &crate::find::tests::corpus::Planted { path, modified: 1, created: 1, flags },
            )
        };
        put(&mut live, "/Users/john/.zshrc", flag::HIDDEN);
        put(&mut live, "/Users/john/.config/", flag::DIR | flag::HIDDEN);
        put(&mut live, "/Users/john/.config/gh/hosts.yml", 0);
        put(&mut live, "/Users/john/Library/Preferences/app.plist", with_tier(0, 1));
        put(&mut live, "/Users/john/Projects/site/node_modules/left-pad/index.js", with_tier(0, 2));
        put(&mut live, "/Applications/Maps.app/Contents/Info.plist", with_tier(flag::INSIDE_PACKAGE, 1));
        put(&mut live, "/Users/.hidden-home/", flag::DIR | flag::HIDDEN);
        put(&mut live, "/Users/.hidden-home/notes.txt", 0);
        live
    }

    #[test]
    fn hidden_is_a_step_away() {
        let live = disk();
        let id = |path: &str| live.resolve(Path::new(path)).unwrap();
        let home = Some(id("/Users/john"));
        let how_far = |path: &str| far(&live, id(path), home);
        for own in [
            "/Users/john/Documents/report.pdf",
            "/Users/john/Projects/site",
            "/Applications/Maps.app",
            "/Applications",
            "/Users/john/Documents/Applications/cover-letter.pdf",
        ] {
            assert_eq!(how_far(own), Away::InSight, "{own}");
        }
        for hidden in ["/Users/john/.zshrc", "/Users/john/.config"] {
            assert_eq!(how_far(hidden), Away::Hidden, "{hidden}");
        }
        for foreign in [
            "/Users/john/.config/gh/hosts.yml",
            "/Users/john/Library/Preferences/app.plist",
            "/Users/john/Projects/site/node_modules/left-pad/index.js",
            "/Applications/Maps.app/Contents/Info.plist",
        ] {
            assert_eq!(how_far(foreign), Away::Foreign, "{foreign}");
        }
        assert_eq!(how_far("/Users/.hidden-home/notes.txt"), Away::Foreign, "inside a hidden folder");
        assert_eq!(far(&live, id("/Users/.hidden-home/notes.txt"), Some(id("/Users/.hidden-home"))), Away::InSight);
        let lay = lay(&live, id("/Users/john/.config/gh/hosts.yml"), home);
        assert_eq!((lay.depth, lay.below_hidden, lay.among_apps, lay.at_home), (0.0, true, false, false));
    }

    #[test]
    fn beside_apps_not_users() {
        let live = disk();
        let id = |path: &str| live.resolve(Path::new(path)).unwrap();
        let home = Some(id("/Users/john"));
        let how_far = |path: &str| far(&live, id(path), home);
        for app in [
            "/Applications/Maps.app",
            "/Applications/Adobe Photoshop 2026/Adobe Photoshop 2026.app",
            "/System/Applications/Utilities/Terminal.app",
            "/Users/john/Applications/Mine.app",
        ] {
            assert_eq!(how_far(app), Away::InSight, "{app}");
        }
        for beside in [
            "/Applications/Adobe Photoshop 2026",
            "/Applications/Adobe Photoshop 2026/Presets",
            "/Applications/Adobe Photoshop 2026/Presets/Blue Filter.blw",
            "/System/Applications/Utilities",
            "/System/Applications",
        ] {
            assert_eq!(how_far(beside), Away::Foreign, "{beside}");
        }
        assert_eq!(how_far("/Users/john/Documents/Applications"), Away::InSight, "a folder of that name is a folder");
        assert_eq!(how_far("/Users/john/Applications"), Away::InSight, "the folder itself is a place to go");
        assert_eq!(how_far("/System"), Away::InSight, "and so is the top of the system");
    }

    #[test]
    fn vendor_files_not_users() {
        let live = from_paths(&[
            "/Users/Shared/Adobe/Premiere Pro 2026/Learn Panel/payloads/messages.properties",
            "/Users/Shared/Parallels/Windows 11.pvm/",
            "/Users/Shared/holiday.jpg",
            "/Users/john/Shared/Adobe/Deep/Deeper/notes.txt",
            "/Users/john/Movies/TV/WeCrashed/Pilot.movpkg/2-3297277-GYXENMIIOIVEPBXC/",
        ]);
        let home = live.resolve(Path::new("/Users/john"));
        let how_far = |path: &str| far(&live, live.resolve(Path::new(path)).expect("planted"), home);
        let payload = "/Users/Shared/Adobe/Premiere Pro 2026/Learn Panel/payloads/messages.properties";
        assert_eq!(how_far(payload), Away::Foreign);
        assert_eq!(how_far("/Users/Shared/Adobe/Premiere Pro 2026/Learn Panel"), Away::Foreign);
        for kept in ["/Users/Shared/holiday.jpg", "/Users/Shared/Parallels/Windows 11.pvm", "/Users/Shared/Adobe"] {
            assert_eq!(how_far(kept), Away::InSight, "{kept}");
        }
        assert_eq!(how_far("/Users/john/Shared/Adobe/Deep/Deeper/notes.txt"), Away::InSight, "a folder of the user's");
        assert_eq!(how_far("/Users/john/Movies/TV/WeCrashed/Pilot.movpkg"), Away::InSight, "the episode is a file");
        let inside = "/Users/john/Movies/TV/WeCrashed/Pilot.movpkg/2-3297277-GYXENMIIOIVEPBXC";
        assert_eq!(how_far(inside), Away::Foreign, "whatever an older index says of it");
        let fetching = from_paths(&["/Users/john/Movies/TV/Fortitude.tmp/download.movpkg/", "/Users/john/tmp/a.txt"]);
        let home = fetching.resolve(Path::new("/Users/john"));
        let how_far = |path: &str| far(&fetching, fetching.resolve(Path::new(path)).expect("planted"), home);
        assert_eq!(how_far("/Users/john/Movies/TV/Fortitude.tmp/download.movpkg"), Away::Foreign, "still arriving");
        assert_eq!(how_far("/Users/john/Movies/TV/Fortitude.tmp"), Away::InSight);
        assert_eq!(how_far("/Users/john/tmp/a.txt"), Away::InSight, "a folder merely called tmp is the user's");
    }

    #[test]
    fn untyped_name_is_machine_made() {
        for made in [
            "2-3297277-x3pwnep5bzenigac3tvkipzbta74x2qb",
            "2-4ayaqwpl6bgsbunrgr6uh4zene7m6fl2-30286439.m3u8",
            "battery-7e4e453c3909e873.8rmag9rvoz2smbyjrhaiw7qf8.rcgu.o",
            "0097da900b02a0cd942cd60dd7b4850a.app",
        ] {
            assert!(machine_made(made.as_bytes()), "{made}");
        }
        for typed in [
            "report-2026-final.pdf",
            "img_20240101123456.jpg",
            "chapter12finaldraft2.docx",
            "screenshot 2026-10-12 at 16.05.23.png",
            "notchbase-skill.zip",
            "f8f29a9f-158d-4a35-90e8-393939057de5",
            "internationalization-guidelines.md",
            "",
        ] {
            assert!(!machine_made(typed.as_bytes()), "{typed}");
        }
        let live = disk();
        let id = |path: &str| live.resolve(Path::new(path)).unwrap();
        let episode = "/Users/john/Movies/TV/2-3297277-X3PWNEP5BZENIGAC3TVKIPZBTA74X2QB";
        assert_eq!(far(&live, id(episode), Some(id("/Users/john"))), Away::Foreign);
    }

    #[test]
    fn only_home_children_at_home() {
        let live = disk();
        let id = |path: &str| live.resolve(Path::new(path)).unwrap();
        let home = Some(id("/Users/john"));
        assert!(lay(&live, id("/Users/john/Documents"), home).at_home);
        assert!(lay(&live, id("/Users/john/.zshrc"), home).at_home, "whether it leads is for the judge to say");
        assert!(!lay(&live, id("/Users/john/Documents/report.pdf"), home).at_home);
        assert!(!lay(&live, id("/Applications"), home).at_home && !lay(&live, id("/Users/john"), home).at_home);
        assert!(!lay(&live, id("/Users/john/Documents"), None).at_home, "with no home there is no such place");
    }

    #[test]
    fn depth_cost_after_three() {
        let deep = "/Users/john/a/b/c/d/e/f/g/h/i/j/k/l/m/n/o/p.txt";
        let live = from_paths(&[
            "/Users/john/Documents/GitHub/notchbase/Cargo.toml",
            "/Applications/Calendar.app/",
            "/opt/tools/bin/deep/run.sh",
            deep,
        ]);
        let id = |path: &str| live.resolve(Path::new(path)).unwrap();
        let home = Some(id("/Users/john"));
        assert_eq!(depth(&live, id("/Users/john/Documents/GitHub/notchbase/Cargo.toml"), home), DEPTH);
        assert_eq!(depth(&live, id("/Users/john/Documents/GitHub/notchbase"), home), 0.0);
        assert_eq!(depth(&live, id("/Applications/Calendar.app"), home), 0.0);
        assert_eq!(depth(&live, id("/opt/tools/bin/deep/run.sh"), home), 2.0 * DEPTH, "counted from the root");
        assert_eq!(depth(&live, id(deep), home), DEPTH_FLOOR);
        assert_eq!(
            depth(&live, id("/Users/john/a/b/c/d"), None),
            3.0 * DEPTH,
            "with no home everything hangs off the root"
        );
        assert_eq!(depth(&live, id("/"), home), 0.0);
    }
}
