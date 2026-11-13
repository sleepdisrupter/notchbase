//! Scores a name across every term of a question, and picks the letters to light up for its row.

use std::ops::Range;

use crate::find::apps::Card;
use crate::find::query::{Fit, Ground, Term, squeezed, trace};
use crate::find::weights::{ANCESTOR, BOUNDARY, COVER, RUN, SMART_CASE, TIER_EXACT, TIER_KIND, TIER_WORD};

use super::hit::marks;
use super::launch::{shown, shown_as};
use super::short::Scored;
use super::{named, text};

pub const SEVERAL: usize = 2;

pub fn ceiling(ground: &Ground<'_>, fit: &Fit) -> f32 {
    if fit.fuzzy || fit.squeezed {
        return f32::INFINITY;
    }
    let terms = &ground.query.terms;
    let start = if fit.leads { TIER_EXACT } else { TIER_WORD };
    let top = |index: usize, term: &Term| {
        let quality = BOUNDARY + RUN * term.needle.len() as f32 + COVER + SMART_CASE;
        let read = if fit.reads(index) { TIER_KIND } else { 0.0 };
        if fit.owns(index) { (start + quality).max(read) } else { read.max(ANCESTOR * (TIER_EXACT + quality)) }
    };
    let mean = terms.iter().enumerate().map(|(index, term)| top(index, term)).sum::<f32>() / terms.len().max(1) as f32;
    let whole = |phrase: &[u8]| TIER_EXACT + BOUNDARY + RUN * phrase.len() as f32 + COVER;
    phrase(ground, fit).map_or(mean, |phrase| mean.max(whole(phrase)))
}

pub fn worded(ground: &Ground<'_>, name: &[u8], fit: &Fit, original: Option<&str>) -> (f32, f32) {
    let phrased = phrase(ground, fit).and_then(|phrase| text::phrased(name, phrase));
    if fit.squeezed {
        return (phrased.unwrap_or(0.0), 0.0);
    }
    let terms = &ground.query.terms;
    let (mut total, mut lift) = (0.0, 0.0);
    for (index, term) in terms.iter().enumerate() {
        let (own, read) = (fit.owns(index), fit.reads(index));
        let kind = if read { TIER_KIND } else { 0.0 };
        let graded = (own && !fit.fuzzy).then(|| text::grade(name, term)).flatten();
        let refined =
            graded.map(|graded| original.map_or(graded, |original| text::refine(graded, name, original, term)));
        total += match refined {
            Some(graded) => graded.score.max(kind),
            None if fit.fuzzy => trace(name, &term.needle).map_or(0.0, |traced| traced.score),
            None if read => kind,
            None => {
                ANCESTOR
                    * fit
                        .above(index)
                        .and_then(|above| text::grade(ground.live.folded(above), term))
                        .map_or(0.0, |graded| graded.score)
            }
        };
        let loose = graded.is_some_and(|graded| graded.loose);
        let cased = graded.is_some() && term.cased;
        lift += if loose { text::HUMP_LIFT } else { 0.0 } + if cased { SMART_CASE } else { 0.0 };
    }
    let terms = terms.len().max(1) as f32;
    (phrased.map_or(total / terms, |phrased| phrased.max(total / terms)), lift / terms)
}

pub fn marked(ground: &Ground<'_>, found: &Scored) -> Vec<Range<u16>> {
    let live = ground.live;
    let (name, mut original) = (live.folded(found.id), String::new());
    live.name_into(found.id, &mut original);
    if !found.fit.app {
        return marks(&original, name, lit(ground, &found.fit, name, &original));
    }
    let (file, cased) = (shown(name), shown_as(&original));
    let lit = |name: &[u8], cased: &str| marks(cased, name, named::lit(ground, &found.fit, name, cased));
    match ground.book.card(live.key(found.id)).and_then(Card::titled).filter(|(title, _)| *title != file) {
        Some(_) if found.fit.alias => Vec::new(),
        Some((title, shown)) => lit(title, shown),
        None => lit(file, cased),
    }
}

fn phrase<'a>(ground: &Ground<'a>, fit: &Fit) -> Option<&'a [u8]> {
    let whole = fit.squeezed || (ground.query.terms.len() >= SEVERAL && fit.own == ground.all());
    ground.query.phrase.as_deref().filter(|_| whole)
}

fn lit(ground: &Ground<'_>, fit: &Fit, name: &[u8], original: &str) -> Vec<Range<usize>> {
    if fit.squeezed {
        let found = ground.query.phrase.as_deref().and_then(|phrase| squeezed(name, phrase));
        return found.map_or_else(Vec::new, |found| found.runs(name));
    }
    let mut ranges = Vec::new();
    for (index, term) in ground.query.terms.iter().enumerate() {
        if fit.fuzzy {
            ranges.extend(trace(name, &term.needle).into_iter().flat_map(|traced| traced.at).map(|at| at..at + 1));
        } else if let Some(graded) = text::grade(name, term).filter(|_| fit.owns(index)) {
            ranges.push(text::refine(graded, name, original, term).range(term));
        }
    }
    ranges
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::sync::atomic::AtomicU64;

    use super::*;
    use crate::find::context::Context;
    use crate::find::model::Live;
    use crate::find::query::{Stop, parse};
    use crate::find::tests::corpus::{EPOCH, from_paths};
    use crate::find::weights::{GAP, TIER_PREFIX, TIER_STEM};

    fn desk() -> Live {
        from_paths(&[
            "/d/My File.txt",
            "/d/my-file-old.txt",
            "/d/myfile.txt",
            "/d/a-b ab",
            "/d/x report-main.exe",
            "/my/file.txt",
        ])
    }

    fn judged<T>(text: &str, path: &str, fit: Fit, judge: impl FnOnce(&Ground<'_>, &Scored, &[u8]) -> T) -> T {
        let (live, context) = (desk(), Context::at(EPOCH, 0, None));
        let (query, latest) = (parse(text, &context), AtomicU64::new(0));
        let stop = Stop { latest: &latest, mine: 0 };
        let ground = Ground::new(&live, &query, &context, None, &stop);
        let id = live.resolve(Path::new(path)).expect("planted");
        judge(&ground, &Scored { score: 0.0, modified: 0, id, fit }, live.folded(id))
    }

    fn graded(text: &str, path: &str, fit: Fit) -> (f32, f32) {
        judged(text, path, fit, |ground, found, name| worded(ground, name, &found.fit, None))
    }

    fn lit(text: &str, path: &str, fit: Fit) -> Vec<(u16, u16)> {
        judged(text, path, fit, |ground, found, _| {
            marked(ground, found).iter().map(|lit| (lit.start, lit.end)).collect()
        })
    }

    fn owning(own: u8) -> Fit {
        Fit { own, ..Fit::default() }
    }

    #[test]
    fn joined_words_graded_as_name() {
        let stem = TIER_STEM + BOUNDARY + 5.0 * RUN + GAP + text::cover(6, 11);
        assert_eq!(graded("my file", "/d/My File.txt", owning(0b11)), (stem, 0.0));
        let unbroken = TIER_STEM + BOUNDARY + 5.0 * RUN + text::cover(6, 10);
        assert_eq!(graded("my file", "/d/myfile.txt", owning(0b11)).0, unbroken, "nothing was skipped");
        let start = TIER_PREFIX + BOUNDARY + 5.0 * RUN + GAP + text::cover(6, 15);
        assert_eq!(graded("my file", "/d/my-file-old.txt", owning(0b11)).0, start);
        let (apart, _) = graded("file my", "/d/My File.txt", owning(0b11));
        assert!(apart < stem, "the other order is two words, not the name: {apart}");
    }

    #[test]
    fn folder_words_graded_singly() {
        let one = graded("myfile", "/d/myfile.txt", owning(0b1));
        assert_eq!(one, (TIER_STEM + BOUNDARY + 5.0 * RUN + text::cover(6, 10), 0.0));
        let mut below = owning(0b10);
        below.over[0] = desk().resolve(Path::new("/my")).expect("planted");
        let (folder, own) =
            (TIER_EXACT + BOUNDARY + (RUN + COVER), TIER_STEM + BOUNDARY + (3.0 * RUN + text::cover(4, 8)));
        assert_eq!(graded("my file", "/my/file.txt", below).0, (ANCESTOR * folder + own) / 2.0);
    }

    #[test]
    fn squeezed_graded_by_phrase() {
        let stem = TIER_STEM + BOUNDARY + 5.0 * RUN + GAP + text::cover(6, 11);
        assert_eq!(graded("myfile", "/d/My File.txt", Fit::squeezed()), (stem, 0.0));
        assert_eq!(graded("my-fi le", "/d/My File.txt", Fit::squeezed()), (stem, 0.0));
        assert_eq!(graded("yourfile", "/d/My File.txt", Fit::squeezed()), (0.0, 0.0));
        assert_eq!(
            graded("pdf file", "/d/My File.txt", Fit::squeezed()),
            (0.0, 0.0),
            "a word that is read leaves no phrase"
        );
    }

    #[test]
    fn ceiling_never_too_low() {
        let cases = [
            ("my file", "/d/My File.txt", owning(0b11)),
            ("my file", "/d/myfile.txt", owning(0b11)),
            ("ab ab", "/d/a-b ab", Fit { leads: false, ..owning(0b11) }),
            ("report main", "/d/x report-main.exe", Fit { leads: false, ..owning(0b11) }),
            ("MY FILE", "/d/My File.txt", owning(0b11)),
            ("myfile", "/d/myfile.txt", owning(0b1)),
        ];
        for (text, path, fit) in cases {
            let (promised, earned) = judged(text, path, fit, |ground, found, name| {
                let mut original = String::new();
                ground.live.name_into(found.id, &mut original);
                (ceiling(ground, &found.fit), worded(ground, name, &found.fit, Some(&original)).0)
            });
            assert!(
                promised >= earned && earned >= graded(text, path, fit).0,
                "{text} on {path}: {promised}, {earned}"
            );
        }
        let whole = graded("ab ab", "/d/a-b ab", Fit { leads: false, ..owning(0b11) }).0;
        assert_eq!(
            whole,
            TIER_EXACT + BOUNDARY + 3.0 * RUN + 2.0 * GAP + text::cover(4, 6),
            "no word leads, the phrase does"
        );
        let loose = |fit: Fit| judged("myfile", "/d/My File.txt", fit, |ground, found, _| ceiling(ground, &found.fit));
        assert_eq!((loose(Fit::squeezed()), loose(Fit::fuzzy())), (f32::INFINITY, f32::INFINITY));
    }

    #[test]
    fn squeezed_lights_runs() {
        assert_eq!(lit("myfile", "/d/My File.txt", Fit::squeezed()), [(0, 2), (3, 7)]);
        assert_eq!(lit("myfile", "/d/my-file-old.txt", Fit::squeezed()), [(0, 2), (3, 7)]);
        assert_eq!(lit("file my", "/d/My File.txt", owning(0b11)), [(0, 2), (3, 7)]);
        assert_eq!(lit("my file", "/d/myfile.txt", owning(0b11)), [(0, 6)]);
        assert_eq!(lit("my file", "/my/file.txt", owning(0b10)), [(0, 4)], "only the word the name holds");
        assert_eq!(lit("mfl", "/d/myfile.txt", Fit::fuzzy()), [(0, 1), (2, 3), (4, 5)]);
        assert_eq!(lit("yourfile", "/d/myfile.txt", Fit::squeezed()), []);
    }
}
