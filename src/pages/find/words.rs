//! The words under each name: where it lives, why it was suggested and when it changed.

use jiff::Timestamp;
use jiff::civil::Date;
use jiff::tz::TimeZone;

use crate::bridge::{FindRow, Why};

const MINUTE: i64 = 60;
const HOUR: i64 = 60 * MINUTE;
const DAY: i64 = 24 * HOUR;
const WEEK_DAYS: i64 = 7;
const BETWEEN: &str = " \u{b7} ";
const HOME: &str = "~";
const HOME_NAME: &str = "Home";
const APP: &str = "App";
const PANE: &str = "System Settings";
const GUID: [usize; 5] = [8, 4, 4, 4, 12];
const HINT_FOLDERS: usize = 3;

pub(super) struct Clock {
    pub(super) now: i64,
    pub(super) zone: TimeZone,
}

impl Clock {
    pub(super) fn current() -> Self {
        Self { now: Timestamp::now().as_second(), zone: TimeZone::system() }
    }

    fn date(&self, stamp: i64) -> Option<Date> {
        Some(Timestamp::from_second(stamp).ok()?.to_zoned(self.zone.clone()).date())
    }

    fn days_ago(&self, stamp: i64) -> Option<i64> {
        let span = self.date(stamp)?.until(self.date(self.now)?).ok()?;
        Some(i64::from(span.get_days()))
    }

    fn day(&self, stamp: i64, with_on: bool) -> Option<String> {
        let date = self.date(stamp)?;
        let days = self.days_ago(stamp)?;
        let on = if with_on { "on " } else { "" };
        Some(match days {
            ..=0 => "today".to_owned(),
            1 => "yesterday".to_owned(),
            2..WEEK_DAYS => format!("{on}{}", date.strftime("%a")),
            _ if date.year() == self.date(self.now)?.year() => format!("{on}{}", date.strftime("%-d %b")),
            _ => format!("{}{}", if with_on { "in " } else { "" }, date.year()),
        })
    }
}

pub(super) fn when(modified: u32, clock: &Clock) -> String {
    let stamp = i64::from(modified);
    let elapsed = clock.now - stamp;
    if modified == 0 {
        return String::new();
    }
    match elapsed {
        ..MINUTE => "now".to_owned(),
        MINUTE..HOUR => format!("{}m", elapsed / MINUTE),
        _ if clock.days_ago(stamp) == Some(0) => format!("{}h", elapsed / HOUR),
        _ => clock.day(stamp, false).map(capitalised).unwrap_or_default(),
    }
}

pub(super) fn reason(why: &Why, clock: &Clock) -> String {
    match why {
        Why::Opened(at) => format!("Opened {}", ago(*at, clock)),
        Why::Edited(at) => format!("Edited {}", ago(*at, clock)),
        Why::Added(at) => format!("Added {}", clock.day(i64::from(*at), true).unwrap_or_default()),
        Why::After(name) => format!("Usually after {name}"),
        Why::AroundNow => "Often around now".to_owned(),
        Why::With(app) => format!("With {app}"),
        Why::Busy(1) => "1 change today".to_owned(),
        Why::Busy(changes) => format!("{changes} changes today"),
    }
}

pub(super) fn dated(row: &FindRow, clock: &Clock) -> String {
    if row.app { String::new() } else { when(row.modified, clock) }
}

pub(super) fn told(row: &FindRow, suggesting: bool, clock: &Clock) -> String {
    let (detail, when) = (detail(row, suggesting, clock), dated(row, clock));
    let dated = !suggesting || row.why.is_none();
    if dated && !when.is_empty() { format!("{detail}{BETWEEN}{when}") } else { detail }
}

pub(super) fn beside<'a>(row: &'a FindRow, rows: &[FindRow]) -> &'a str {
    let twins = || rows.iter().filter(|other| other.key != row.key && other.name == row.name);
    let named = nearest_named(row);
    if !twins().any(|twin| nearest_named(twin) == named) {
        return named;
    }
    let apart = |folders: &usize| !twins().any(|twin| last(&twin.place, *folders) == last(&row.place, *folders));
    last(&row.place, (2..HINT_FOLDERS).find(apart).unwrap_or(HINT_FOLDERS))
}

fn nearest_named(row: &FindRow) -> &str {
    let folders = || row.place.rsplit(['/', '\\']).filter(|part| !part.is_empty());
    let named = folders().find(|folder| !coded(folder)).filter(|_| row.place.as_ref() != HOME);
    named.unwrap_or_else(|| folder_name(&row.place))
}

fn last(place: &str, folders: usize) -> &str {
    let whole = place.trim_end_matches(['/', '\\']);
    let from = whole.rmatch_indices(['/', '\\']).nth(folders.saturating_sub(1)).map_or(0, |(at, _)| at + 1);
    &whole[from..]
}

fn coded(folder: &str) -> bool {
    let bytes = folder.as_bytes();
    let length = GUID.iter().sum::<usize>() + GUID.len() - 1;
    let reads_as_one = |from: usize| {
        let mut at = from;
        GUID.iter().enumerate().all(|(group, digits)| {
            let dashed = group == 0 || bytes[at - 1] == b'-';
            let hex = bytes[at..at + digits].iter().all(u8::is_ascii_hexdigit);
            at += digits + 1;
            dashed && hex
        })
    };
    (0..=bytes.len().saturating_sub(length)).any(|from| bytes.len() >= from + length && reads_as_one(from))
}

pub(super) fn detail(row: &FindRow, suggesting: bool, clock: &Clock) -> String {
    let kind = if row.pane() { PANE } else { APP };
    let (place, folder) = if row.app { (kind, kind) } else { (&*row.place, folder_name(&row.place)) };
    match row.why.as_ref().filter(|_| suggesting) {
        Some(why) => format!("{}{BETWEEN}{folder}", reason(why, clock)),
        None => place.to_owned(),
    }
}

fn ago(at: u32, clock: &Clock) -> String {
    let stamp = i64::from(at);
    let elapsed = clock.now - stamp;
    match elapsed {
        ..MINUTE => "just now".to_owned(),
        MINUTE..HOUR => format!("{} min ago", elapsed / MINUTE),
        HOUR..DAY if clock.days_ago(stamp) == Some(0) => format!("{}h ago", elapsed / HOUR),
        _ => match clock.days_ago(stamp) {
            Some(days @ 2..WEEK_DAYS) => format!("{days} days ago"),
            _ => clock.day(stamp, true).unwrap_or_default(),
        },
    }
}

fn capitalised(text: String) -> String {
    let mut chars = text.chars();
    chars.next().map(|first| first.to_uppercase().chain(chars).collect()).unwrap_or_default()
}

pub(super) fn items(count: u32) -> String {
    let digits = count.to_string();
    let mut grouped = String::new();
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            grouped.push(',');
        }
        grouped.push(digit);
    }
    match count {
        0 => String::new(),
        1 => "1 item".to_owned(),
        _ => format!("{grouped} items"),
    }
}

pub(super) fn folder_name(place: &str) -> &str {
    if place == HOME {
        return HOME_NAME;
    }
    place.trim_end_matches(['/', '\\']).rsplit(['/', '\\']).find(|part| !part.is_empty()).unwrap_or(place)
}

#[cfg(test)]
mod pane_tests {
    use std::path::Path;
    use std::sync::Arc;

    use super::*;
    use crate::bridge::Kind;

    #[test]
    fn settings_pane_says_where() {
        let row = |path: &str| FindRow {
            key: 1,
            path: Arc::from(Path::new(path)),
            name: "x".into(),
            place: "/System".into(),
            marks: Vec::new(),
            kind: Kind::App,
            app: true,
            folder: true,
            cloud: false,
            modified: 5,
            why: None,
            predicted: false,
            icon: None,
        };
        let clock = Clock { now: 10, zone: TimeZone::UTC };
        let pane = row("/System/Library/ExtensionKit/Extensions/Bluetooth.appex");
        assert_eq!(told(&pane, false, &clock), "System Settings", "and no date, like any app");
        assert_eq!(told(&row("/Applications/Maps.app"), false, &clock), "App");
        assert!(pane.pane() && !row("/Applications/Maps.app").pane());
        assert!(!FindRow { app: false, ..pane }.pane(), "a file that merely ends that way is a file");
    }

    #[test]
    fn nearest_named_folder_tells_apart() {
        let placed = |place: &str| FindRow {
            key: 1,
            path: Arc::from(Path::new("/x/Zen.app")),
            name: "Zen".into(),
            place: place.into(),
            marks: Vec::new(),
            kind: Kind::App,
            app: true,
            folder: true,
            cloud: false,
            modified: 5,
            why: None,
            predicted: false,
            icon: None,
        };
        let stub = "~/Applications (Parallels)/{cd127fd0-bfe9-4344-a337-6fe3149c42eb} Applications.localized";
        assert_eq!(beside(&placed(stub), &[]), "Applications (Parallels)");
        assert_eq!(beside(&placed("/Applications"), &[]), "Applications");
        assert_eq!(beside(&placed("~/Documents/GitHub/notchbase/src"), &[]), "src");
        assert_eq!(beside(&placed("C:\\Users\\me\\Desktop"), &[]), "Desktop");
        assert_eq!(beside(&placed("~"), &[]), folder_name("~"));
        let only = "/cd127fd0-bfe9-4344-a337-6fe3149c42eb";
        assert_eq!(beside(&placed(only), &[]), "cd127fd0-bfe9-4344-a337-6fe3149c42eb", "better a code than nothing");
        assert!(coded("{CD127FD0-BFE9-4344-A337-6FE3149C42EB}") && !coded("2026-09-27-zero-budget") && !coded(""));
        let keyed = |key: u64, place: &str| FindRow { key, ..placed(place) };
        let rows = [
            keyed(1, "~/Documents/GitHub/notchbase/src"),
            keyed(2, "~/Documents/GitHub/nikitin-crm/src"),
            keyed(3, "~/Desktop"),
        ];
        assert_eq!(beside(&rows[0], &rows), "notchbase/src", "two alike are told apart by the folder above");
        assert_eq!(beside(&rows[1], &rows), "nikitin-crm/src");
        assert_eq!(beside(&rows[2], &rows), "Desktop", "one that already differs keeps the short hint");
        let deep = [keyed(1, "/a/x/lib/src"), keyed(2, "/b/x/lib/src")];
        assert_eq!(beside(&deep[0], &deep), "x/lib/src", "no more than three folders are named");
        assert_eq!((last("~/a/b/", 1), last("~/a/b", 2), last("b", 3)), ("b", "a/b", "b"));
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::sync::Arc;

    use super::*;
    use crate::bridge::Kind;

    const NOON: i64 = 1_757_678_400;

    fn clock() -> Clock {
        Clock { now: NOON, zone: TimeZone::UTC }
    }

    fn back(seconds: i64) -> u32 {
        (NOON - seconds) as u32
    }

    #[test]
    fn dates_read_like_a_finder_column() {
        let clock = clock();
        assert_eq!(when(back(20), &clock), "now");
        assert_eq!(when(back(12 * MINUTE), &clock), "12m");
        assert_eq!(when(back(3 * HOUR), &clock), "3h");
        assert_eq!(when(back(DAY), &clock), "Yesterday");
        assert_eq!(when(back(3 * DAY), &clock), "Tue");
        assert_eq!(when(back(30 * DAY), &clock), "13 Aug");
        assert_eq!(when(back(400 * DAY), &clock), "2024");
        assert_eq!(when(0, &clock), "", "an unknown date says nothing");
    }

    #[test]
    fn reasons_say_why() {
        let clock = clock();
        assert_eq!(reason(&Why::Opened(back(2 * HOUR)), &clock), "Opened 2h ago");
        assert_eq!(reason(&Why::Edited(back(12 * MINUTE)), &clock), "Edited 12 min ago");
        assert_eq!(reason(&Why::Added(back(HOUR)), &clock), "Added today");
        assert_eq!(reason(&Why::Opened(back(3 * DAY)), &clock), "Opened 3 days ago");
        assert_eq!(reason(&Why::Opened(back(30 * DAY)), &clock), "Opened on 13 Aug");
        assert_eq!(reason(&Why::After("Cargo.toml".into()), &clock), "Usually after Cargo.toml");
        assert_eq!(reason(&Why::AroundNow, &clock), "Often around now");
        assert_eq!(reason(&Why::With("Xcode".into()), &clock), "With Xcode");
        assert_eq!(reason(&Why::Busy(14), &clock), "14 changes today");
        assert_eq!(reason(&Why::Busy(1), &clock), "1 change today");
    }

    #[test]
    fn suggestion_names_reason_and_folder() {
        let row = FindRow {
            key: 1,
            path: Arc::from(Path::new("/Users/john/src/main.rs")),
            name: "main.rs".into(),
            place: "~/src".into(),
            marks: Vec::new(),
            kind: Kind::Code,
            app: false,
            folder: false,
            cloud: false,
            modified: 0,
            why: Some(Why::After("Cargo.toml".into())),
            predicted: true,
            icon: None,
        };
        assert_eq!(detail(&row, true, &clock()), "Usually after Cargo.toml \u{b7} src");
        assert_eq!(detail(&row, false, &clock()), "~/src");
        assert_eq!(folder_name("~"), "Home");
        assert_eq!(folder_name("C:\\Users\\john\\Desktop"), "Desktop");
    }

    #[test]
    fn apps_say_app_without_date() {
        let app = FindRow {
            key: 2,
            path: Arc::from(Path::new("/ProgramData/Microsoft/Windows/Start Menu/Programs/TreeSize Free.lnk")),
            name: "TreeSize Free".into(),
            place: "/ProgramData/Microsoft/Windows/Start Menu/Programs".into(),
            marks: Vec::new(),
            kind: Kind::App,
            app: true,
            folder: false,
            cloud: false,
            modified: back(3 * HOUR),
            why: Some(Why::Opened(back(2 * HOUR))),
            predicted: false,
            icon: None,
        };
        assert_eq!(detail(&app, false, &clock()), "App");
        assert_eq!(detail(&app, true, &clock()), "Opened 2h ago \u{b7} App");
        assert_eq!(detail(&FindRow { why: None, ..app.clone() }, true, &clock()), "App");
        assert_eq!(dated(&app, &clock()), "", "when a shortcut was made says nothing");
        let installer = FindRow { app: false, name: "TreeSizeFreeSetup.exe".into(), ..app };
        assert_eq!(dated(&installer, &clock()), "3h");
        assert_eq!(detail(&installer, false, &clock()), "/ProgramData/Microsoft/Windows/Start Menu/Programs");
    }

    #[test]
    fn counts_group_thousands() {
        assert_eq!(items(0), "");
        assert_eq!(items(1), "1 item");
        assert_eq!(items(980), "980 items");
        assert_eq!(items(12_480), "12,480 items");
        assert_eq!(items(1_452_310), "1,452,310 items");
    }
}
