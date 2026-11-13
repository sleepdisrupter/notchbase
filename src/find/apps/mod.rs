//! Apps in the index: the name the system shows for each, whether it only runs in the background, and how much it
//! is used. The librarian reads all of it into a `Book`.

#[cfg(any(test, target_os = "windows"))]
mod assist;
mod librarian;
mod typo;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
use macos as imp;

#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "windows")]
use windows as imp;

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
mod other;
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
use other as imp;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use super::context::Stamp;
use super::key::Key;
use super::model::fold_query;
use super::usage::{faded, filled, since};
use super::weights::{APP_USED, APP_USED_FILLS_AT, APP_USED_LATELY, APP_USED_LATELY_HALF_LIFE};

#[cfg(test)]
pub(in crate::find) use librarian::Sources;
pub use librarian::{Librarian, Roster};
pub use typo::mistyped;

const OWN_MENU: [&str; 3] = ["apps", "Start Menu", "Programs"];

static NONE: LazyLock<Book> = LazyLock::new(Book::default);

pub fn own_menu() -> Option<PathBuf> {
    crate::platform::local_data_dir().map(|data| OWN_MENU.iter().fold(data, |below, step| below.join(step)))
}

pub(in crate::find) fn settle(menu: Option<&Path>) {
    imp::settle(menu);
}

pub fn opens(path: &Path) -> Option<String> {
    imp::opens(path)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Listed {
    pub key: Key,
    pub path: PathBuf,
    pub modified: Stamp,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Used {
    pub last: Stamp,
    pub times: u32,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Read {
    pub title: Option<String>,
    pub backstage: bool,
    pub unlisted: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Card {
    pub title: Box<str>,
    pub folded: Box<[u8]>,
    pub backstage: bool,
    pub listed: bool,
    pub used: Used,
    read_at: Stamp,
}

impl Card {
    pub fn of(read: Read, used: Used, read_at: Stamp) -> Self {
        let title: Box<str> = read.title.as_deref().map(str::trim).unwrap_or_default().into();
        let folded = fold_query(&title).into_boxed_slice();
        Self { title, folded, backstage: read.backstage, listed: !read.unlisted, used, read_at }
    }

    pub fn titled(&self) -> Option<(&[u8], &str)> {
        (!self.title.is_empty()).then_some((&*self.folded, &*self.title))
    }

    pub fn familiar(&self, now: Stamp) -> f32 {
        if self.used == Used::default() {
            return 0.0;
        }
        let often = APP_USED * filled(self.used.times as f32, APP_USED_FILLS_AT);
        let lately =
            if self.used.last == 0 { 0.0 } else { faded(since(now, self.used.last), APP_USED_LATELY_HALF_LIFE) };
        often + APP_USED_LATELY * lately
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Book {
    cards: HashMap<Key, Card>,
}

impl Book {
    pub fn none() -> &'static Self {
        &NONE
    }

    pub fn card(&self, key: Key) -> Option<&Card> {
        self.cards.get(&key)
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.cards.len()
    }

    #[cfg(test)]
    pub fn holding(cards: impl IntoIterator<Item = (Key, Card)>) -> Self {
        Self { cards: cards.into_iter().collect() }
    }

    pub(in crate::find) fn revised(
        &self,
        listed: &[Listed],
        used: Option<&HashMap<Key, Used>>,
        mut read: impl FnMut(&Listed) -> Read,
    ) -> Self {
        let mut cards = HashMap::with_capacity(listed.len());
        for app in listed {
            let known = self.cards.get(&app.key);
            let seen = |known: &Card| used.map_or(known.used, |used| used.get(&app.key).copied().unwrap_or_default());
            let card = match known.filter(|known| known.read_at == app.modified) {
                Some(known) => Card { used: seen(known), ..known.clone() },
                None => {
                    let used = used.and_then(|used| used.get(&app.key).copied());
                    Card::of(read(app), used.or(known.map(|known| known.used)).unwrap_or_default(), app.modified)
                }
            };
            cards.insert(app.key, card);
        }
        Self { cards }
    }
}

pub(in crate::find) fn read(app: &Listed) -> Read {
    imp::read(&app.path)
}

pub(in crate::find) fn every() -> Vec<PathBuf> {
    imp::every()
}

pub(in crate::find) fn used() -> Option<HashMap<Key, Used>> {
    imp::used()
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::path::Path;

    use super::*;
    use crate::find::context::DAY;
    use crate::find::key::key_of;

    const NOW: Stamp = 1_704_110_400;

    fn listed(path: &str, modified: Stamp) -> Listed {
        Listed { key: key_of(Path::new(path)), path: path.into(), modified }
    }

    fn titled(title: &str) -> Read {
        Read { title: Some(title.to_owned()), ..Read::default() }
    }

    #[test]
    fn card_holds_folded_name() {
        let card = Card::of(titled(" Find My "), Used::default(), 7);
        assert_eq!(card.titled(), Some((&b"find my"[..], "Find My")));
        let accented = Card::of(titled("Caf\u{e9}"), Used::default(), 7);
        assert_eq!(accented.titled(), Some((&b"cafe"[..], "Caf\u{e9}")));
        assert_eq!(Card::of(Read::default(), Used::default(), 7).titled(), None, "no name was read");
        assert!(Card::of(Read::default(), Used::default(), 7).listed, "an app is listed unless it was read otherwise");
        assert!(!Card::of(Read { unlisted: true, ..Read::default() }, Used::default(), 7).listed);
        assert_eq!(Card::of(titled("  "), Used::default(), 7).titled(), None);
    }

    #[test]
    fn used_app_is_familiar() {
        let card = |last: Stamp, times: u32| Card::of(Read::default(), Used { last, times }, 0);
        assert_eq!(card(0, 0).familiar(NOW), 0.0);
        let daily = card(NOW, 1_700).familiar(NOW);
        assert!((daily - (APP_USED + APP_USED_LATELY)).abs() < 0.5, "{daily}");
        let (once, often) = (card(NOW, 1).familiar(NOW), card(NOW, 60).familiar(NOW));
        assert!(once < often && often < daily, "{once} {often} {daily}");
        let (today, last_month) = (card(NOW, 20).familiar(NOW), card(NOW - 30 * DAY, 20).familiar(NOW));
        assert!(today - last_month > APP_USED_LATELY * 0.9, "{today} {last_month}");
        assert!(card(0, 20).familiar(NOW) > 0.0, "a count without a date still counts");
    }

    #[test]
    fn revision_reads_only_changes() {
        let (maps, notes) = (listed("/Applications/Maps.app", 5), listed("/Applications/Notes.app", 5));
        let reads = Cell::new(0);
        let read = |app: &Listed| {
            reads.set(reads.get() + 1);
            titled(&format!("{} {}", app.path.file_stem().unwrap().to_string_lossy(), app.modified))
        };
        let first = Book::default().revised(&[maps.clone(), notes.clone()], None, read);
        assert_eq!((first.len(), reads.get()), (2, 2));
        let updated = Listed { modified: 9, ..maps.clone() };
        let second = first.revised(std::slice::from_ref(&updated), None, read);
        assert_eq!(reads.get(), 3, "only the app that changed was read again");
        assert_eq!(second.card(maps.key).map(|card| &*card.title), Some("Maps 9"));
        assert_eq!(second.card(notes.key), None, "an app no longer listed is forgotten");
        let third = second.revised(&[updated], None, read);
        assert_eq!((reads.get(), &third), (3, &second));
    }

    #[test]
    fn asked_use_replaces_count() {
        let maps = listed("/Applications/Maps.app", 5);
        let seen = HashMap::from([(maps.key, Used { last: NOW, times: 12 })]);
        let read = |_: &Listed| titled("Maps");
        let first = Book::default().revised(std::slice::from_ref(&maps), Some(&seen), read);
        assert_eq!(first.card(maps.key).map(|card| card.used), Some(Used { last: NOW, times: 12 }));
        let kept = first.revised(std::slice::from_ref(&maps), None, read);
        assert_eq!(kept.card(maps.key).map(|card| card.used.times), Some(12), "nothing was asked, nothing changes");
        let changed = Listed { modified: 6, ..maps.clone() };
        let reread = first.revised(std::slice::from_ref(&changed), None, read);
        assert_eq!(reread.card(maps.key).map(|card| card.used.times), Some(12), "an update keeps what was known");
        let unused = first.revised(std::slice::from_ref(&maps), Some(&HashMap::new()), read);
        assert_eq!(unused.card(maps.key).map(|card| card.used), Some(Used::default()), "the system forgot it");
    }

    #[test]
    fn empty_book_knows_nothing() {
        assert_eq!(Book::none().len(), 0);
        assert_eq!(Book::none().card(key_of(Path::new("/Applications/Maps.app"))), None);
    }
}
