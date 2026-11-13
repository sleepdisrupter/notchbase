//! Turns a ranked candidate into a row for the page: its place, the letters to light up, and the folder that tells
//! two rows with one name apart.

use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::find::apps::{Book, Card};
use crate::find::key::{Key, START, key_of};
use crate::find::kind::Kind;
use crate::find::model::{EntryId, Live, flag, fold_into};
use crate::find::view::{FindRow, Why};

use super::launch::{launcher, pane, shown_as, titled};

const HOME: &str = "~";
const LONGEST_LETTER: usize = 4;

pub fn keys(live: &Live, id: EntryId) -> [Key; 3] {
    let mut path = PathBuf::new();
    live.path_into(id, &mut path);
    let above = |path: Option<&Path>| path.map_or(START, key_of);
    [live.key(id), above(path.parent()), above(path.parent().and_then(Path::parent))]
}

#[cfg(test)]
pub fn row(
    live: &Live,
    id: EntryId,
    marks: Vec<Range<u16>>,
    why: Option<Why>,
    predicted: bool,
    home: Option<&Path>,
) -> FindRow {
    row_in(Book::none(), live, id, marks, why, predicted, home)
}

pub fn row_in(
    book: &Book,
    live: &Live,
    id: EntryId,
    marks: Vec<Range<u16>>,
    why: Option<Why>,
    predicted: bool,
    home: Option<&Path>,
) -> FindRow {
    let (mut path, mut name) = (PathBuf::new(), String::new());
    live.path_into(id, &mut path);
    live.name_into(id, &mut name);
    let (flags, folded, key) = (live.flags(id), live.folded(id), live.key(id));
    let app = launcher(live, id, folded, flags);
    let shown = book.card(key).filter(|_| app).and_then(Card::titled);
    let (title, marks) = match shown {
        Some((_, title)) => (title, marks),
        None if app => titled(&name, marks),
        None => (name.as_str(), marks),
    };
    FindRow {
        key: key.0,
        place: place(path.parent(), home).into(),
        path: Arc::from(path),
        name: title.into(),
        marks,
        kind: if app { Kind::App } else { Kind::of(folded, flags) },
        app,
        folder: flags & flag::DIR != 0,
        cloud: flags & flag::CLOUD != 0,
        modified: live.modified(id),
        why,
        predicted,
        icon: None,
    }
}

pub fn told_apart(rows: &mut [FindRow]) {
    let alike = |one: &FindRow, other: &FindRow| one.app && other.app && one.name == other.name;
    let shared: Vec<bool> = (0..rows.len())
        .map(|at| rows.iter().enumerate().any(|(other, row)| other != at && alike(&rows[at], row)))
        .collect();
    for row in rows.iter_mut().zip(shared).filter_map(|(row, shared)| shared.then_some(row)) {
        let file = row.path.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
        let filed = shown_as(&file);
        let named_for_people = !pane(file.to_lowercase().as_bytes());
        let says_more =
            named_for_people && filed != &*row.name && filed.to_lowercase().contains(&row.name.to_lowercase());
        if says_more {
            (row.name, row.marks) = (filed.into(), Vec::new());
        }
    }
}

pub fn marks(original: &str, folded: &[u8], mut ranges: Vec<Range<usize>>) -> Vec<Range<u16>> {
    let (mut refolded, mut ends) = (Vec::with_capacity(folded.len()), Vec::new());
    for letter in original.chars() {
        fold_into(letter.encode_utf8(&mut [0; LONGEST_LETTER]), &mut refolded);
        ends.push(refolded.len());
    }
    if refolded != folded {
        return Vec::new();
    }
    let letter_at = |byte: usize| ends.partition_point(|end| *end <= byte);
    let silent = |letter: usize| ends.get(letter).is_some_and(|end| letter > 0 && *end == ends[letter - 1]);
    let narrow = |letter: usize| u16::try_from(letter).unwrap_or(u16::MAX);
    ranges.retain(|range| range.start < range.end && range.end <= folded.len());
    ranges.sort_by_key(|range| range.start);
    let mut lit: Vec<Range<u16>> = Vec::with_capacity(ranges.len());
    for range in ranges {
        let mut end = letter_at(range.end - 1) + 1;
        while silent(end) {
            end += 1;
        }
        let (start, end) = (narrow(letter_at(range.start)), narrow(end));
        match lit.last_mut().filter(|last| start <= last.end) {
            Some(last) => last.end = last.end.max(end),
            None => lit.push(start..end),
        }
    }
    lit
}

fn place(parent: Option<&Path>, home: Option<&Path>) -> String {
    let Some(parent) = parent else { return String::new() };
    let below_home = home.and_then(|home| parent.strip_prefix(home).ok());
    match below_home {
        Some(below) if below.as_os_str().is_empty() => HOME.to_owned(),
        Some(below) => Path::new(HOME).join(below).to_string_lossy().into_owned(),
        None => parent.to_string_lossy().into_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::find::apps::{Read, Used};
    use crate::find::key::chain;
    use crate::find::model::fold_query;
    use crate::find::tests::corpus::{EPOCH, Planted, from_paths, plant};

    #[test]
    fn row_place_seen_from_home() {
        let live = from_paths(&["/Users/john/Documents/GitHub/notes.md", "/Users/john/todo.txt", "/opt/tool/"]);
        let home = Path::new("/Users/john");
        let row_of =
            |path: &str| row(&live, live.resolve(Path::new(path)).unwrap(), Vec::new(), None, false, Some(home));
        let notes = row_of("/Users/john/Documents/GitHub/notes.md");
        assert_eq!((&*notes.name, &*notes.place), ("notes.md", "~/Documents/GitHub"));
        assert_eq!(
            (&*notes.path, notes.key),
            (Path::new("/Users/john/Documents/GitHub/notes.md"), key_of(&notes.path).0)
        );
        assert!(!notes.folder && !notes.app && !notes.cloud && notes.icon.is_none() && notes.why.is_none());
        assert_eq!(&*row_of("/Users/john/todo.txt").place, "~");
        let tool = row_of("/opt/tool");
        assert_eq!((&*tool.place, tool.folder, tool.kind), ("/opt", true, Kind::Folder));
        assert_eq!(&*row_of("/").place, "", "a root is nowhere");
    }

    #[test]
    fn launcher_row_keeps_path() {
        let menu = "/ProgramData/Microsoft/Windows/Start Menu/Programs";
        let mut live = from_paths(&[
            &format!("{menu}/Maps.lnk"),
            "/Users/john/Desktop/Maps.lnk",
            "/Applications/Maps.app/",
            "/Users/john/Downloads/setup.exe",
        ]);
        let linked = Planted { path: "/Applications/Safari.app", modified: EPOCH, created: EPOCH, flags: flag::LINK };
        plant(&mut live, &linked);
        let lit = |path: &str, marks: &[(u16, u16)]| {
            let marks = marks.iter().map(|(start, end)| *start..*end).collect();
            row(&live, live.resolve(Path::new(path)).unwrap(), marks, None, false, None)
        };
        let ends = |row: &FindRow| row.marks.iter().map(|lit| (lit.start, lit.end)).collect::<Vec<_>>();
        let row_of = |path: &str| lit(path, &[]);
        let shortcut = lit(&format!("{menu}/Maps.lnk"), &[(0, 2), (4, 8)]);
        assert_eq!((&*shortcut.name, shortcut.app, shortcut.kind), ("Maps", true, Kind::App));
        assert_eq!((&*shortcut.place, shortcut.folder, ends(&shortcut)), (menu, false, vec![(0, 2)]));
        assert_eq!((&*shortcut.path, shortcut.key), (&*Path::new(menu).join("Maps.lnk"), key_of(&shortcut.path).0));
        let (linked, bundle) = (row_of("/Applications/Safari.app"), row_of("/Applications/Maps.app"));
        assert_eq!((&*linked.name, linked.app, linked.kind, linked.folder), ("Safari", true, Kind::App, false));
        assert_eq!((&*bundle.name, bundle.app, bundle.kind, bundle.folder), ("Maps", true, Kind::App, true));
        assert_eq!(&*bundle.place, "/Applications", "where it lies stays true");
        let link = lit("/Users/john/Desktop/Maps.lnk", &[(4, 8)]);
        assert_eq!((&*link.name, link.app, link.kind, ends(&link)), ("Maps.lnk", false, Kind::Link, vec![(4, 8)]));
        let installer = row_of("/Users/john/Downloads/setup.exe");
        assert_eq!((&*installer.name, installer.app, installer.kind), ("setup.exe", false, Kind::App));
    }

    #[test]
    fn twins_told_apart_by_file() {
        let live = from_paths(&[
            "/System/Applications/Siri.app/",
            "/System/Applications/Siri AI.app/",
            "/Applications/Zen.app/",
            "/Users/john/Applications (Parallels)/ba8b643480a4.app/",
            "/Users/john/Documents/Siri",
        ]);
        let titled = |path: &str, title: &str| {
            let id = live.resolve(Path::new(path)).unwrap();
            let lit: Vec<Range<u16>> = (!title.is_empty()).then_some(0..2).into_iter().collect();
            let row = row(&live, id, lit, None, false, None);
            if title.is_empty() { row } else { FindRow { name: title.into(), ..row } }
        };
        let mut rows = vec![
            titled("/System/Applications/Siri.app", "Siri"),
            titled("/System/Applications/Siri AI.app", "Siri"),
            titled("/Applications/Zen.app", "Zen"),
            titled("/Users/john/Applications (Parallels)/ba8b643480a4.app", "Zen"),
            titled("/Users/john/Documents/Siri", ""),
        ];
        told_apart(&mut rows);
        let names: Vec<&str> = rows.iter().map(|row| &*row.name).collect();
        assert_eq!(names, ["Siri", "Siri AI", "Zen", "Zen", "Siri"], "a hash says nothing, a file is no app");
        assert_eq!((rows[0].marks.len(), rows[1].marks.len(), rows[2].marks.len()), (1, 0, 1), "a new title is unlit");
        let mut alone = vec![titled("/System/Applications/Siri AI.app", "Siri")];
        told_apart(&mut alone);
        assert_eq!(&*alone[0].name, "Siri", "one app alone keeps the name it shows");
    }

    #[test]
    fn book_titles_only_apps() {
        let live = from_paths(&["/System/Applications/FindMy.app/", "/Users/john/FindMy.txt"]);
        let (app, file) = (Path::new("/System/Applications/FindMy.app"), Path::new("/Users/john/FindMy.txt"));
        let shown = |title: &str| {
            Card::of(Read { title: Some(title.to_owned()), backstage: false, unlisted: false }, Used::default(), 0)
        };
        let book = Book::holding([(key_of(app), shown("Find My")), (key_of(file), shown("Not An App"))]);
        let lit: Vec<Range<u16>> = std::iter::once(0..4).collect();
        let row_of = |path: &Path| row_in(&book, &live, live.resolve(path).unwrap(), lit.clone(), None, false, None);
        assert_eq!((&*row_of(app).name, row_of(app).app, row_of(app).marks.len()), ("Find My", true, 1));
        assert_eq!(&*row_of(file).name, "FindMy.txt");
        let unread = row(&live, live.resolve(app).unwrap(), Vec::new(), None, false, None);
        assert_eq!(&*unread.name, "FindMy", "until the book knows it, the file names it");
    }

    #[test]
    fn candidate_keys_three_levels() {
        let live = from_paths(&["/Users/john/todo.txt"]);
        let users = chain(START, "Users");
        let todo = live.resolve(Path::new("/Users/john/todo.txt")).unwrap();
        assert_eq!(keys(&live, todo), [chain(chain(users, "john"), "todo.txt"), chain(users, "john"), users]);
        let root = live.resolve(Path::new("/")).unwrap();
        assert_eq!(keys(&live, root), [START; 3]);
    }

    #[test]
    fn marks_light_whole_letters() {
        let lit = |original: &str, folded: &[u8], ranges: &[Range<usize>]| -> Vec<(u16, u16)> {
            marks(original, folded, ranges.to_vec()).iter().map(|lit| (lit.start, lit.end)).collect()
        };
        assert_eq!(lit("Cargo.toml", b"cargo.toml", &[6..10, 0..2]), [(0, 2), (6, 10)]);
        assert_eq!(lit("Cargo.toml", b"cargo.toml", &[0..3, 2..5, 5..6]), [(0, 6)], "touching ranges become one");
        let folded = fold_query("Caf\u{e9} bar");
        let (accent, bar) = (3..folded.len() - 4, folded.len() - 3..folded.len());
        assert_eq!(lit("Caf\u{e9} bar", &folded, &[bar, accent]), [(3, 4), (5, 8)], "letters, not bytes");
        assert_eq!(lit("a.txt", b"a.txt", &[2..2, 3..9]), [], "a range outside the name lights nothing");
        assert_eq!(lit("Resume", b"another name", &[0..3, 4..5]), [], "a fold that cannot be retraced lights nothing");
        assert_eq!(lit("", b"", &[]), []);
    }
}
