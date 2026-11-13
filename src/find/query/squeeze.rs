//! Squeezed phrases: words typed with spaces that a name runs together, like `my file` for `MyFile.txt`.

use std::ops::Range;

use crate::find::rank::boundary;

use super::fuzzy::FUZZY_BELOW;
use super::parse::{Query, Term};

pub const PHRASE_FROM: usize = 3;

pub(super) const SEPARATORS: &[u8] = b" _-";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Squeezed {
    pub at: usize,
    pub end: usize,
    pub skipped: usize,
}

impl Squeezed {
    pub fn runs(&self, name: &[u8]) -> Vec<Range<usize>> {
        let mut runs: Vec<Range<usize>> = Vec::new();
        let lettered = |at: &usize| name.get(*at).is_some_and(|byte| !SEPARATORS.contains(byte));
        for at in (self.at..self.end).filter(lettered) {
            match runs.last_mut().filter(|run| run.end == at) {
                Some(run) => run.end = at + 1,
                None => runs.push(at..at + 1),
            }
        }
        runs
    }
}

pub fn phrase(terms: &[Term]) -> Option<Vec<u8>> {
    let lettered = |byte: &u8| !SEPARATORS.contains(byte);
    let joined: Vec<u8> = terms.iter().flat_map(|term| term.needle.iter().copied()).filter(lettered).collect();
    (terms.iter().all(Term::plain) && joined.len() >= PHRASE_FROM).then_some(joined)
}

pub fn wanted(query: &Query, candidates: usize) -> Option<&[u8]> {
    query.phrase.as_deref().filter(|_| candidates < FUZZY_BELOW)
}

pub fn squeezed(name: &[u8], phrase: &[u8]) -> Option<Squeezed> {
    let mut inside = None;
    for at in memchr::memchr_iter(*phrase.first()?, name) {
        let Some(found) = from(name, at, phrase) else { continue };
        if boundary(name, at) {
            return Some(found);
        }
        inside = inside.or(Some(found));
    }
    inside
}

fn from(name: &[u8], at: usize, phrase: &[u8]) -> Option<Squeezed> {
    let (mut end, mut skipped) = (at, 0);
    for letter in phrase {
        let gap = name.get(end..)?.iter().take_while(|byte| SEPARATORS.contains(byte)).count();
        if name.get(end + gap) != Some(letter) {
            return None;
        }
        (end, skipped) = (end + gap + 1, skipped + gap);
    }
    Some(Squeezed { at, end, skipped })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::find::context::Context;
    use crate::find::query::parse;
    use crate::find::tests::corpus::Noise;

    const MADE_UP: usize = 4_000;
    const NAME_BYTES: &[u8] = b"ab1 -_.";
    const PHRASE_BYTES: &[u8] = b"ab1";

    fn typed(text: &str) -> Option<String> {
        let query = parse(text, &Context::at(1_704_110_400, 0, None));
        query.phrase.map(|phrase| String::from_utf8_lossy(&phrase).into_owned())
    }

    fn found(name: &str, phrase: &str) -> Option<(usize, usize, usize)> {
        squeezed(name.as_bytes(), phrase.as_bytes()).map(|found| (found.at, found.end, found.skipped))
    }

    #[test]
    fn phrase_joins_plain_words() {
        assert_eq!(typed("TreeSize  Free ").as_deref(), Some("treesizefree"));
        assert_eq!(typed("treesizefree"), typed(" tree size free"));
        assert_eq!(typed("my-file").as_deref(), Some("myfile"));
        assert_eq!(typed("my_file v1.2").as_deref(), Some("myfilev1.2"), "a dot is a letter of the name");
        assert_eq!(
            (typed("ab"), typed("a b"), typed("-_-"), typed("")),
            (None, None, None, None),
            "too short to mean much"
        );
        assert_eq!(typed("a b c").as_deref(), Some("abc"));
    }

    #[test]
    fn read_word_leaves_no_phrase() {
        for text in ["notes pdf", "apps tree", "report today", "notes last week", "*.rs main", "src/main", "notes/"] {
            assert_eq!(typed(text), None, "{text}");
        }
        assert!(typed("gitig main").is_some());
    }

    #[test]
    fn phrase_wanted_when_little_found() {
        let query = parse("my file", &Context::default());
        assert_eq!(wanted(&query, FUZZY_BELOW - 1), Some(&b"myfile"[..]));
        assert_eq!(wanted(&query, FUZZY_BELOW), None, "enough was found");
        assert_eq!(wanted(&parse("pdf file", &Context::default()), 0), None);
    }

    #[test]
    fn found_across_separators() {
        assert_eq!(found("treesize free.lnk", "treesizefree"), Some((0, 13, 1)));
        assert_eq!(found("treesizefree.exe", "treesizefree"), Some((0, 12, 0)));
        assert_eq!(found("shot-0001treesize_free.png", "treesizefree"), Some((9, 22, 1)));
        assert_eq!(found("my - _file.txt", "myfile"), Some((0, 10, 4)));
        assert_eq!(found("google chrome", "googlechrome"), Some((0, 13, 1)));
        assert_eq!(found("google.chrome", "googlechrome"), None, "a dot is not skipped");
        assert_eq!(found("goo", "google"), None);
        assert_eq!(found("my-fil", "myfile"), None);
        assert_eq!(found("anything", ""), None);
    }

    #[test]
    fn prefers_name_start() {
        assert_eq!(found("my file my-file", "myfile"), Some((0, 7, 1)));
        assert_eq!(found("xmyfile my-file", "myfile"), Some((8, 15, 1)), "a word start beats an earlier inside");
        assert_eq!(found("xmyfile ymy_file", "myfile"), Some((1, 7, 0)));
        assert_eq!(found("_my-file", "myfile"), Some((1, 8, 1)));
        assert_eq!(found("mmy file", "myfile"), Some((1, 8, 1)), "a first letter that leads nowhere is passed over");
    }

    #[test]
    fn agrees_with_letter_by_letter() {
        let mut noise = Noise::seeded(5);
        let mut made = |from: &[u8], most: usize| -> Vec<u8> {
            (0..1 + noise.below(most)).map(|_| from[noise.below(from.len())]).collect()
        };
        let mut matched = 0;
        for _ in 0..MADE_UP {
            let (name, phrase) = (made(NAME_BYTES, 12), made(PHRASE_BYTES, 4));
            let letters: Vec<usize> = (0..name.len()).filter(|at| !SEPARATORS.contains(&name[*at])).collect();
            let spelled = |run: &&[usize]| run.iter().map(|at| name[*at]).eq(phrase.iter().copied());
            let places: Vec<(usize, usize)> =
                letters.windows(phrase.len()).filter(spelled).map(|run| (run[0], run[run.len() - 1] + 1)).collect();
            let best = places.iter().find(|(at, _)| boundary(&name, *at)).or(places.first());
            let expected = best.map(|(at, end)| Squeezed { at: *at, end: *end, skipped: end - at - phrase.len() });
            let (shown, typed) = (String::from_utf8_lossy(&name), String::from_utf8_lossy(&phrase));
            assert_eq!(squeezed(&name, &phrase), expected, "{typed:?} in {shown:?}");
            matched += usize::from(expected.is_some());
        }
        assert!(matched > MADE_UP / 8 && matched < MADE_UP, "{matched} of the made-up names held their phrase");
    }

    #[test]
    fn runs_between_separators_light() {
        let ends = |runs: Vec<Range<usize>>| runs.iter().map(|run| (run.start, run.end)).collect::<Vec<_>>();
        let runs = |name: &str, phrase: &str| {
            ends(squeezed(name.as_bytes(), phrase.as_bytes()).expect("the phrase is there").runs(name.as_bytes()))
        };
        assert_eq!(runs("google chrome.lnk", "googlechrome"), [(0, 6), (7, 13)]);
        assert_eq!(runs("shot-0001treesize_free.png", "treesizefree"), [(9, 17), (18, 22)]);
        assert_eq!(runs("a-b_c d", "abcd"), [(0, 1), (2, 3), (4, 5), (6, 7)]);
        assert_eq!(runs("treesizefree.exe", "sizefree"), [(4, 12)]);
        assert_eq!(ends(Squeezed { at: 2, end: 9, skipped: 0 }.runs(b"abc")), [(2, 3)], "never past the name");
    }
}
