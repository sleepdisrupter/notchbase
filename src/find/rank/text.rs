//! Grades a name against one term: whole, stem, start, start of a word, initials, or anywhere inside.

use std::ops::Range;

use crate::find::kind::extension;
use crate::find::query::{Term, squeezed};
use crate::find::weights::{
    BOUNDARY, COVER, GAP, GAP_FLOOR, RUN, SMART_CASE, TIER_EXACT, TIER_INSIDE, TIER_KIND, TIER_PREFIX, TIER_STEM,
    TIER_WORD,
};

pub const HUMP_LIFT: f32 = TIER_WORD - TIER_INSIDE + BOUNDARY;

const SEPARATORS: &[u8] = b"/_-. ";
const DOT: usize = 1;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Graded {
    pub score: f32,
    pub at: usize,
    pub loose: bool,
}

impl Graded {
    pub fn range(&self, term: &Term) -> Range<usize> {
        self.at..self.at + term.needle.len()
    }
}

pub fn grade(name: &[u8], term: &Term) -> Option<Graded> {
    let length = term.needle.len();
    if term.strict {
        let at = name.len().checked_sub(length).filter(|_| name.ends_with(&term.needle))?;
        return Some(Graded { score: TIER_KIND, at, loose: false });
    }
    let first = term.finder.find(name)?;
    let quality = RUN * length.saturating_sub(1) as f32 + cover(length, name.len());
    if first == 0 {
        let whole = if length == name.len() { TIER_EXACT } else { TIER_PREFIX };
        let tier = if length == stem(name).len() && length < name.len() { TIER_STEM } else { whole };
        return Some(Graded { score: tier + BOUNDARY + quality, at: 0, loose: false });
    }
    let at_an_edge = occurrences(name, term, first).find(|at| starts_a_word(name[*at - 1], name[*at]));
    Some(match at_an_edge {
        Some(at) => Graded { score: TIER_WORD + BOUNDARY + quality, at, loose: false },
        None => Graded { score: TIER_INSIDE + quality, at: first, loose: true },
    })
}

pub fn refine(graded: Graded, name: &[u8], original: &str, term: &Term) -> Graded {
    let cased = original.as_bytes();
    let hump = |at: &usize| cased[*at].is_ascii_uppercase() && cased[*at - 1].is_ascii_lowercase();
    let humped = (graded.loose && cased.len() == name.len())
        .then(|| occurrences(name, term, graded.at).find(hump))
        .flatten()
        .map(|at| Graded { score: graded.score + HUMP_LIFT, at, loose: false });
    let graded = humped.unwrap_or(graded);
    let exact = term.cased && original.contains(&*term.raw);
    Graded { score: graded.score + if exact { SMART_CASE } else { 0.0 }, ..graded }
}

pub fn phrased(name: &[u8], phrase: &[u8]) -> Option<f32> {
    let found = squeezed(name, phrase)?;
    let edge = boundary(name, found.at);
    let tier = match (found.at == 0, edge) {
        (true, _) if found.end == name.len() => TIER_EXACT,
        (true, _) if found.end == stem(name).len() => TIER_STEM,
        (true, _) => TIER_PREFIX,
        (false, true) => TIER_WORD,
        (false, false) => TIER_INSIDE,
    };
    let start = if edge { BOUNDARY } else { 0.0 };
    let gaps = (GAP * found.skipped as f32).max(GAP_FLOOR);
    Some(tier + start + RUN * phrase.len().saturating_sub(1) as f32 + gaps + cover(phrase.len(), name.len()))
}

pub fn cover(typed: usize, name: usize) -> f32 {
    COVER * typed as f32 / name.max(1) as f32
}

pub fn boundary(name: &[u8], at: usize) -> bool {
    at == 0 || name.get(at).is_some_and(|here| starts_a_word(name[at - 1], *here))
}

pub fn initials(original: &str) -> Vec<u8> {
    heads(original).map(|at| original.as_bytes()[at].to_ascii_lowercase()).collect()
}

pub fn heads(original: &str) -> impl Iterator<Item = usize> + '_ {
    let bytes = original.as_bytes();
    let hump = move |at: usize| bytes[at - 1].is_ascii_lowercase() && bytes[at].is_ascii_uppercase();
    let leads = move |at: &usize| *at == 0 || starts_a_word(bytes[*at - 1], bytes[*at]) || hump(*at);
    (0..bytes.len()).filter(leads).filter(move |at| bytes[*at].is_ascii_alphanumeric())
}

fn starts_a_word(before: u8, here: u8) -> bool {
    SEPARATORS.contains(&before) || before.is_ascii_digit() != here.is_ascii_digit()
}

fn stem(name: &[u8]) -> &[u8] {
    extension(name).map_or(name, |ending| &name[..name.len() - ending.len() - DOT])
}

fn occurrences<'a>(name: &'a [u8], term: &'a Term, first: usize) -> impl Iterator<Item = usize> + 'a {
    let later = move |from: usize| term.finder.find(name.get(from..)?).map(|at| from + at);
    std::iter::successors(Some(first), move |at| later(at + 1))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::find::context::Context;
    use crate::find::query::parse;

    fn term(text: &str) -> Term {
        parse(text, &Context::default()).terms.remove(0)
    }

    fn graded(name: &str, typed: &str) -> Graded {
        let term = term(typed);
        let folded = name.to_ascii_lowercase();
        let quick = grade(folded.as_bytes(), &term).unwrap_or_else(|| panic!("{typed} is in {name}"));
        refine(quick, folded.as_bytes(), name, &term)
    }

    fn score(name: &str, typed: &str) -> f32 {
        graded(name, typed).score
    }

    #[test]
    fn checked_example_adds_up() {
        assert_eq!(score("Cargo.toml", "ca"), 800.0 + 16.0 + 8.0 + 12.0);
        assert_eq!(score("Calendar.app", "ca"), 800.0 + 16.0 + 8.0 + 10.0);
        assert_eq!(score(".cargo", "ca"), 650.0 + 16.0 + 8.0 + 20.0);
    }

    #[test]
    fn grades_rank_in_order() {
        let whole = score("readme", "readme");
        let stem = score("readme.md", "readme");
        let start = score("readme-old.md", "readme");
        let word = score("old-readme.md", "readme");
        let inside = score("unreadmeant.md", "readme");
        assert!(whole > stem && stem > start && start > word && word > inside);
        assert_eq!(score("old.tar.gz", "old.tar"), TIER_STEM + BOUNDARY + 6.0 * RUN + cover(7, 10));
        assert_eq!(score(".zshrc", ".zshrc"), TIER_EXACT + BOUNDARY + 5.0 * RUN + COVER, "a dotfile has no stem");
        assert_eq!(grade(b"notes.txt", &term("zebra")), None);
    }

    #[test]
    fn word_boundaries() {
        for (name, typed) in
            [("my_report.txt", "rep"), ("my report.txt", "rep"), ("src/main", "main"), ("v2final", "fin")]
        {
            assert_eq!(
                score(name, typed),
                TIER_WORD + BOUNDARY + RUN * (typed.len() - 1) as f32 + cover(typed.len(), name.len())
            );
        }
        assert_eq!(graded("report2024.txt", "2024").at, 6);
        let humped = graded("MyReport.txt", "rep");
        assert_eq!(
            (humped.score, humped.at, humped.loose),
            (TIER_WORD + BOUNDARY + 2.0 * RUN + cover(3, 12), 2, false)
        );
        let flat = grade(b"myreport.txt", &term("rep")).unwrap();
        assert_eq!((flat.score, flat.loose), (TIER_INSIDE + 2.0 * RUN + cover(3, 12), true));
        assert_eq!(humped.score - flat.score, HUMP_LIFT);
        assert_eq!(graded("barBazbar-baz", "baz").at, 10, "a separator is found without looking at the capitals");
        assert!(boundary(b"a-b", 0) && boundary(b"a-b", 2) && !boundary(b"a-b", 1) && !boundary(b"ab", 9));
    }

    #[test]
    fn capitals_reward_never_filter() {
        assert_eq!(score("README.md", "READ") - score("readme.md", "READ"), SMART_CASE);
        assert_eq!(score("README.md", "read"), score("readme.md", "read"));
        assert_eq!(score("Caf\u{e9} Notes.txt", "Notes"), score("Caf\u{e9} notes.txt", "Notes") + SMART_CASE);
    }

    #[test]
    fn strict_ending_marks_ending() {
        let rust = term("*.rs");
        let main = grade(b"main.rs", &rust).unwrap();
        assert_eq!((main.score, main.range(&rust)), (TIER_KIND, 4..7));
        assert_eq!(grade(b"main.rst", &rust), None);
        assert_eq!(grade(b"rs", &rust), None);
    }

    #[test]
    fn initials_are_word_heads() {
        assert_eq!(initials("GitHub"), b"gh");
        assert_eq!(initials("my-old_project 2"), b"mop2");
        assert_eq!(initials(".config"), b"c");
        assert_eq!(initials("Documents"), b"d");
        assert_eq!(initials(""), b"");
        assert_eq!(heads("Visual Studio Code").collect::<Vec<_>>(), [0, 7, 14]);
        assert_eq!(heads("TreeSize (Free)").collect::<Vec<_>>(), [0, 4], "a bracket starts no word");
    }

    #[test]
    fn phrase_pays_for_separators() {
        let graded = |name: &str, phrase: &str| phrased(name.as_bytes(), phrase.as_bytes());
        let run = 11.0 * RUN;
        assert_eq!(graded("treesize free", "treesizefree"), Some(TIER_EXACT + BOUNDARY + run + GAP + cover(12, 13)));
        assert_eq!(graded("treesize free.lnk", "treesizefree"), Some(TIER_STEM + BOUNDARY + run + GAP + cover(12, 17)));
        assert_eq!(graded("treesizefreesetup.exe", "treesizefree"), Some(TIER_PREFIX + BOUNDARY + run + cover(12, 21)));
        let shot = "shot-0001treesize_free.png";
        assert_eq!(graded(shot, "treesizefree"), Some(TIER_WORD + BOUNDARY + run + GAP + cover(12, 26)));
        assert_eq!(graded("mytreesizefree.png", "treesizefree"), Some(TIER_INSIDE + run + cover(12, 18)));
        let spread = graded("a-b-c-d-e-f-g-h-i-j-k-l", "abcdefghijkl");
        assert_eq!(
            spread,
            Some(TIER_EXACT + BOUNDARY + run + GAP_FLOOR + cover(12, 23)),
            "gaps cost no more than the floor"
        );
        assert_eq!(graded("treesize.lnk", "treesizefree"), None);
        assert!(graded("my file.txt", "myfile") < Some(score("myfile.txt", "myfile")), "a literal match stays ahead");
        assert_eq!(
            graded("myfile.txt", "myfile"),
            Some(score("myfile.txt", "myfile")),
            "with nothing skipped it is the word"
        );
    }
}
