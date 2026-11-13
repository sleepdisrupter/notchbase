//! Parses what was typed into terms: words, `*.ext` endings, paths like `~/docs/`, and words that read as a kind or
//! a date (`pdf`, `last week`).

use memchr::memmem::Finder;

use crate::find::context::Context;
use crate::find::kind::Kind;
use crate::find::model::{EntryId, Live, flag, fold_query};
use crate::find::rank::initials;

use super::fit::Away;
use super::squeeze;
use super::words::{Reading, dated};

pub const MOST_TERMS: usize = 6;

const EXTENSION_WORD_FROM: usize = 2;
const ONE_LETTER: usize = 1;
const SEPARATOR: char = '/';
const HOME: &str = "~";
const ANY_NAME: char = '*';
const DOT: u8 = b'.';
const HIDDEN_MARK: char = '.';
const END_OF_NAME: u8 = 0;

#[derive(Clone, Debug, PartialEq)]
pub enum Above {
    Home,
    Named(Vec<u8>),
}

#[derive(Clone, Debug)]
pub struct Term {
    pub needle: Vec<u8>,
    pub finder: Finder<'static>,
    pub anchored: Finder<'static>,
    pub raw: Box<str>,
    pub cased: bool,
    pub strict: bool,
    pub folders: bool,
    pub above: Vec<Above>,
    pub reading: Option<Reading>,
}

pub struct Query {
    pub terms: Vec<Term>,
    pub typed: Vec<u8>,
    pub phrase: Option<Vec<u8>>,
}

impl Query {
    pub fn only_letters(&self) -> bool {
        self.terms.iter().all(Term::one_letter)
    }
}

impl Term {
    pub fn within(&self, name: &[u8]) -> bool {
        if self.strict { name.ends_with(&self.needle) } else { self.finder.find(name).is_some() }
    }

    pub fn admits(&self, live: &Live, id: EntryId, flags: u8, home: Option<EntryId>) -> bool {
        (!self.folders || flags & flag::DIR != 0) && (self.above.is_empty() || self.lies_below(live, id, home))
    }

    pub fn one_letter(&self) -> bool {
        self.needle.len() == ONE_LETTER && !self.strict
    }

    pub fn plain(&self) -> bool {
        !self.strict && !self.folders && self.above.is_empty() && self.reading.is_none()
    }

    pub fn reach(&self) -> Away {
        let names_a_hidden_file = self.raw.starts_with(HIDDEN_MARK) && self.reading.is_none() && !self.strict;
        match (self.above.is_empty(), names_a_hidden_file) {
            (false, _) => Away::Foreign,
            (true, true) => Away::Hidden,
            (true, false) => Away::InSight,
        }
    }

    fn lies_below(&self, live: &Live, id: EntryId, home: Option<EntryId>) -> bool {
        let mut at = id;
        let mut original = String::new();
        self.above.iter().all(|above| {
            let Some(parent) = live.parent(at) else { return false };
            at = parent;
            match above {
                Above::Home => home == Some(parent),
                Above::Named(start) if live.folded(parent).starts_with(start) => true,
                Above::Named(start) => {
                    live.name_into(parent, &mut original);
                    initials(&original).starts_with(start)
                }
            }
        })
    }

    fn literal(raw: &str, needle: Vec<u8>, strict: bool) -> Option<Self> {
        let sought: Vec<u8> = needle.iter().copied().chain(strict.then_some(END_OF_NAME)).collect();
        let anchored: Vec<u8> = [END_OF_NAME].into_iter().chain(needle.iter().copied()).collect();
        (!needle.is_empty()).then(|| Self {
            finder: Finder::new(&sought).into_owned(),
            anchored: Finder::new(&anchored).into_owned(),
            needle,
            raw: raw.into(),
            cased: raw.chars().any(char::is_uppercase),
            strict,
            folders: false,
            above: Vec::new(),
            reading: None,
        })
    }
}

pub fn parse(text: &str, context: &Context) -> Query {
    let words: Vec<&str> = text.split_whitespace().collect();
    let mut terms = Vec::new();
    let mut at = 0;
    while at < words.len() && terms.len() < MOST_TERMS {
        let pair = words.get(at + 1).and_then(|next| phrase(&format!("{} {next}", words[at]), context));
        at += if pair.is_some() { 2 } else { 1 };
        terms.extend(pair.or_else(|| word(words[at - 1], context)));
    }
    Query { phrase: squeeze::phrase(&terms), terms, typed: fold_query(text) }
}

fn phrase(pair: &str, context: &Context) -> Option<Term> {
    let needle = fold_query(pair);
    let span = dated(std::str::from_utf8(&needle).ok()?, context)?;
    Term::literal(pair, needle, false).map(|term| Term { reading: Some(Reading::Within(span)), ..term })
}

fn word(typed: &str, context: &Context) -> Option<Term> {
    if typed.contains(SEPARATOR) {
        return path(typed, context);
    }
    if let Some(ending) = typed.strip_prefix(ANY_NAME).filter(|ending| ending.len() > 1 && ending.starts_with('.')) {
        return Term::literal(ending, fold_query(ending), true);
    }
    let needle = fold_query(typed);
    let reading = std::str::from_utf8(&needle).ok().and_then(|folded| read(folded, context));
    Term::literal(typed, needle, false).map(|term| Term { reading, ..term })
}

fn read(folded: &str, context: &Context) -> Option<Reading> {
    if let Some(span) = dated(folded, context) {
        return Some(Reading::Within(span));
    }
    let ending = |bare: &str| Reading::Ends([&[DOT], bare.as_bytes()].concat());
    match folded.strip_prefix('.') {
        Some(bare) => Kind::ending(bare.as_bytes()).map(|_| ending(bare)),
        None if Kind::named(folded).is_some() => Kind::word(folded).map(Reading::Kind),
        None => Kind::word(folded).filter(|_| folded.len() >= EXTENSION_WORD_FROM).map(|_| ending(folded)),
    }
}

fn path(typed: &str, context: &Context) -> Option<Term> {
    let mut steps: Vec<&str> = typed.split(SEPARATOR).collect();
    let folders = steps.last().is_some_and(|last| last.is_empty());
    steps.retain(|step| !step.is_empty());
    let name = steps.pop()?;
    let above = |step: &&str| if *step == HOME { Above::Home } else { Above::Named(fold_query(step)) };
    let term = word(name, context)?;
    let reading = term.reading.filter(|reading| steps.is_empty() || !reading.launches());
    Some(Term { folders: folders || term.folders, above: steps.iter().rev().map(above).collect(), reading, ..term })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::find::tests::corpus::from_paths;

    fn terms(text: &str) -> Vec<Term> {
        parse(text, &Context::at(1_704_110_400, 0, None)).terms
    }

    fn needles(text: &str) -> Vec<String> {
        terms(text).iter().map(|term| String::from_utf8_lossy(&term.needle).into_owned()).collect()
    }

    #[test]
    fn words_folded_six_kept() {
        assert_eq!(needles("  Cargo   TOML "), ["cargo", "toml"]);
        assert_eq!(needles("a b c d e f g h").len(), MOST_TERMS);
        assert!(terms("   ").is_empty() && terms("").is_empty());
        let query = parse(" Car ", &Context::default());
        assert_eq!((query.typed.as_slice(), query.terms[0].cased, &*query.terms[0].raw), (&b" car "[..], true, "Car"));
        assert!(!terms("car")[0].cased && terms("car")[0].plain());
    }

    #[test]
    fn dot_reaches_hidden_files() {
        let reach = |text: &str| terms(text).iter().map(Term::reach).max().unwrap_or_default();
        for asked in [".zshrc", ".git", ".env notes"] {
            assert_eq!(reach(asked), Away::Hidden, "{asked}");
        }
        for asked in ["library/prefs", "~/.config/", "src/app", "etc/hosts .conf"] {
            assert_eq!(reach(asked), Away::Foreign, "{asked}");
        }
        for plain in ["zshrc", "config.json", ".pdf", ".rs main", "*.lnk", "apps", "today", "a.b", ""] {
            assert_eq!(reach(plain), Away::InSight, "{plain}");
        }
    }

    #[test]
    fn star_dot_asks_for_ending() {
        let strict = &terms("*.RS")[0];
        assert!(strict.strict && strict.reading.is_none());
        assert!(strict.within(b"main.rs") && !strict.within(b"main.rst") && !strict.within(b"rs"));
        assert_eq!(strict.finder.needle(), b".rs\0");
        let dotted = &terms(".rs")[0];
        assert_eq!((dotted.strict, &dotted.reading), (false, &Some(Reading::Ends(b".rs".to_vec()))));
        assert!(dotted.within(b"main.rs") && dotted.within(b".rsync-filter"));
        assert_eq!(terms(".gitig")[0].reading, None, "an unknown ending is only text");
        assert_eq!(needles("* *. *x"), ["*", "*.", "*x"]);
    }

    #[test]
    fn readings_stay_text_too() {
        let read = |text: &str| terms(text).into_iter().map(|term| term.reading).collect::<Vec<_>>();
        assert_eq!(read("folders"), [Some(Reading::Kind(Kind::Folder))]);
        assert_eq!(
            read("zip pdf rs"),
            [
                Some(Reading::Kind(Kind::Archive)),
                Some(Reading::Ends(b".pdf".to_vec())),
                Some(Reading::Ends(b".rs".to_vec()))
            ]
        );
        assert_eq!(read("c readme"), [None, None], "one letter is never read as an ending");
        assert_eq!(needles("pdf today"), ["pdf", "today"]);
        assert!(matches!(read("PDF Today")[1], Some(Reading::Within(_))));
        assert_eq!(needles("notes last week 2024"), ["notes", "last week", "2024"]);
        assert!(matches!(read("this month")[..], [Some(Reading::Within(span))] if span.label == "this month"));
        assert_eq!(needles("last weekend"), ["last", "weekend"]);
    }

    #[test]
    fn slash_asks_for_folders() {
        let folder = &terms("src/")[0];
        assert_eq!((&folder.needle[..], folder.folders, folder.above.len()), (&b"src"[..], true, 0));
        let below = &terms("~/Doc/notch")[0];
        assert_eq!(below.above, [Above::Named(b"doc".to_vec()), Above::Home], "the nearest folder comes first");
        assert_eq!((&below.needle[..], below.folders, &*below.raw), (&b"notch"[..], false, "notch"));
        let rust = &terms("src/*.rs")[0];
        assert!(rust.strict && rust.above == [Above::Named(b"src".to_vec())]);
        assert!(terms("/").is_empty() && terms("//").is_empty());
        assert_eq!(terms("src/app")[0].reading, None, "below a folder, app is a name and not every app");
        assert!(terms("app")[0].reading.is_some() && terms("apps/")[0].reading.is_some());
        assert!(terms("docs/pdf")[0].reading.is_some(), "an ending still narrows what lies below");
        assert_eq!(needles("/usr/"), ["usr"]);
    }

    #[test]
    fn fragment_holds_below_named() {
        let live = from_paths(&[
            "/Users/john/Documents/GitHub/notchbase/",
            "/Users/john/Documents/notchbase-old/",
            "/Users/john/doc.txt",
            "/Users/john/Music/track.mp3",
        ]);
        let id = |path: &str| live.resolve(std::path::Path::new(path)).unwrap();
        let home = Some(id("/Users/john"));
        let admits = |text: &str, path: &str| terms(text)[0].admits(&live, id(path), live.flags(id(path)), home);
        assert!(admits("gh/notch", "/Users/john/Documents/GitHub/notchbase"), "the initials of GitHub");
        assert!(admits("git/notch", "/Users/john/Documents/GitHub/notchbase"));
        assert!(admits("doc/gi/notch", "/Users/john/Documents/GitHub/notchbase"));
        assert!(!admits("gh/notch", "/Users/john/Documents/notchbase-old"));
        assert!(admits("~/doc", "/Users/john/doc.txt") && !admits("~/track", "/Users/john/Music/track.mp3"));
        assert!(
            admits("mus/track", "/Users/john/Music/track.mp3") && !admits("mus/track/", "/Users/john/Music/track.mp3")
        );
        assert!(!admits("a/b/c/d/e/users", "/Users"), "there are not that many folders above");
    }
}
