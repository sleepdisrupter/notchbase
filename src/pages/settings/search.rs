//! Searching every setting: what was typed, which settings it finds, and how a found row shows where it lives.
//!
//! A row painted as a search result is "spotted": the rows module reads the spot to put the row's place before its
//! detail and to light the words that matched, so every view's own rows can be shown in the results unchanged.

use egui::Id;

use super::group::Group;
use super::item::Item;
use crate::bridge::PanelData;
use crate::ui::list::Lit;

const SPOT: &str = "notchbase.settings.spot";
const CRUMB: &str = " \u{203a} ";

#[derive(Default)]
pub(super) struct Search {
    pub(super) query: String,
    /// The chip picked under the field: only this group's results.
    pub(super) only: Option<Group>,
}

impl Search {
    pub(super) fn is_on(&self) -> bool {
        !self.query.trim().is_empty()
    }

    pub(super) fn set(&mut self, query: String) {
        if query.trim() != self.query.trim() {
            self.only = None;
        }
        self.query = query;
    }

    pub(super) fn clear(&mut self) {
        *self = Self::default();
    }
}

/// One setting the search found.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Found {
    pub(super) group: Group,
    pub(super) item: Item,
    /// Where it lives, shown before its detail: the group, or the tool a tool's own setting belongs to.
    pub(super) crumb: String,
}

/// What a row says, as the search reads it.
pub(super) struct Said {
    pub(super) title: String,
    pub(super) detail: String,
    /// Other words someone might look for it by.
    pub(super) words: &'static str,
}

/// Every setting that has each typed word somewhere in it, those named by their title first.
pub(super) fn find(query: &str, data: &PanelData) -> Vec<Found> {
    let terms = terms(query);
    if terms.is_empty() {
        return Vec::new();
    }
    let mut found: Vec<(u8, Found)> = Vec::new();
    for group in Group::tiles() {
        for (item, crumb) in group.searched(data) {
            let Some(said) = item.said(data) else { continue };
            if let Some(rank) = rank(&terms, &said, &crumb) {
                found.push((rank, Found { group, item, crumb }));
            }
        }
    }
    found.sort_by_key(|(rank, _)| *rank);
    found.into_iter().map(|(_, found)| found).collect()
}

fn terms(query: &str) -> Vec<String> {
    query.split_whitespace().map(str::to_lowercase).collect()
}

/// 0 when the title starts with the first word, 1 when a word in the title does, 2 when the title holds it, 3 when
/// only the rest does. None when any word is missing.
fn rank(terms: &[String], said: &Said, crumb: &str) -> Option<u8> {
    let title = said.title.to_lowercase();
    let all = format!("{title} {} {} {}", said.detail.to_lowercase(), said.words, crumb.to_lowercase());
    if !terms.iter().all(|term| all.contains(term.as_str())) {
        return None;
    }
    let first = terms.first()?;
    let rank = if title.starts_with(first.as_str()) {
        0
    } else if title.split(|c: char| !c.is_alphanumeric()).any(|word| word.starts_with(first.as_str())) {
        1
    } else if title.contains(first.as_str()) {
        2
    } else {
        3
    };
    Some(rank)
}

/// The detail of a found row, with its place before it.
pub(super) fn crumbed(crumb: &str, detail: &str) -> String {
    match (crumb.is_empty(), detail.is_empty()) {
        (true, _) => detail.to_owned(),
        (false, true) => crumb.to_owned(),
        (false, false) => format!("{crumb}{CRUMB}{detail}"),
    }
}

/// Where the first typed word shows in the title, or else in the detail.
pub(super) fn lit(query: &str, title: &str, detail: &str) -> Option<Lit> {
    let term = terms(query).into_iter().next()?;
    if let Some((start, end)) = run(title, &term) {
        return Some(Lit::Title(start, end));
    }
    run(detail, &term).map(|(start, end)| Lit::Detail(start, end))
}

fn run(text: &str, term: &str) -> Option<(usize, usize)> {
    let lowered = text.to_lowercase();
    // Lowercasing can change a letter's length; then the bytes no longer line up, and nothing is lit.
    if lowered.len() != text.len() {
        return None;
    }
    let start = lowered.find(term)?;
    Some((start, start + term.len()))
}

/// Where a row painted as a search result lives, and what was typed.
#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct Spot {
    pub(super) query: String,
    pub(super) crumb: String,
}

/// Marks the rows painted from now on as search results, until `clear_spot`.
pub(super) fn set_spot(ui: &egui::Ui, spot: Spot) {
    ui.data_mut(|memory| memory.insert_temp(Id::new(SPOT), spot));
}

pub(super) fn clear_spot(ui: &egui::Ui) {
    ui.data_mut(|memory| memory.remove::<Spot>(Id::new(SPOT)));
}

pub(super) fn spot(ui: &egui::Ui) -> Option<Spot> {
    ui.data(|memory| memory.get_temp::<Spot>(Id::new(SPOT)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn said(title: &str, detail: &str, words: &'static str) -> Said {
        Said { title: title.into(), detail: detail.into(), words }
    }

    #[test]
    fn every_word_must_be_somewhere() {
        let lock = said("Show on the lock screen", "Stays visible while the Mac is locked", "");
        assert_eq!(rank(&terms("show"), &lock, "Notch"), Some(0));
        assert_eq!(rank(&terms("lock"), &lock, "Notch"), Some(1));
        assert_eq!(rank(&terms("creen"), &lock, "Notch"), Some(2));
        assert_eq!(rank(&terms("visible"), &lock, "Notch"), Some(3));
        assert_eq!(rank(&terms("notch lock"), &lock, "Notch"), Some(3), "the place counts too");
        assert_eq!(rank(&terms("lock dock"), &lock, "Notch"), None);
        let displays = said("All displays", "Show an island on every screen", "monitors screens external");
        assert_eq!(rank(&terms("MONITORS"), &displays, ""), Some(3), "words only it is known by");
        assert_eq!(rank(&terms("  "), &displays, ""), None);
    }

    #[test]
    fn lights_first_word() {
        assert_eq!(lit("screen", "Show on the lock screen", "x"), Some(Lit::Title(17, 23)));
        assert_eq!(lit("Screen rec", "Permissions", "Accessibility, Screen Recording"), Some(Lit::Detail(15, 21)));
        assert_eq!(lit("dock", "Haptics", "A light tap"), None);
        assert_eq!(lit("", "Haptics", ""), None);
        assert_eq!(lit("i", "\u{130}stanbul", ""), None, "lowercasing grew the text, so nothing is lit");
    }

    #[test]
    fn crumb_goes_before_detail() {
        assert_eq!(crumbed("Dock", "The edge of the screen"), "Dock \u{203a} The edge of the screen");
        assert_eq!(crumbed("Dock", ""), "Dock");
        assert_eq!(crumbed("", "Detail"), "Detail");
    }

    #[test]
    fn new_words_drop_chip() {
        let mut search = Search::default();
        search.set("dock".into());
        search.only = Some(Group::Dock);
        search.set("dock ".into());
        assert_eq!(search.only, Some(Group::Dock), "a trailing space is the same search");
        search.set("docks".into());
        assert_eq!(search.only, None);
        assert!(search.is_on());
        search.clear();
        assert!(!search.is_on());
    }
}
