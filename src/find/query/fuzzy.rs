//! Loose matching for when little matches outright: the typed letters in order with gaps, or as initials.

use memchr::memmem::Finder;

use crate::find::model::EntryId;
use crate::find::rank::{boundary, cover};
use crate::find::weights::{BOUNDARY, GAP, GAP_FLOOR, RUN, TIER_FUZZY};

use super::Ground;
use super::holders::holders;
use super::mark::{DEBUG_HITS, scan};
use super::parse::{Query, Term};
use super::parts::{Part, fan};
use super::squeeze::squeezed;

pub const FUZZY_BELOW: usize = 8;

const FUZZY_FROM: usize = 3;
const SLIP_REACH: usize = 2;
const LETTERS_A_SLIP: usize = 4;
pub(super) const SURVIVORS: usize = 2_000;
const COMMON_FIRST: &[u8] = b".-_e0a1i2o3r4t5n6s7l8c9dupmhgbfywkvxzjq";

#[derive(Clone, Debug, PartialEq)]
pub struct Traced {
    pub score: f32,
    pub at: Vec<usize>,
}

#[derive(Debug, Default, PartialEq)]
pub struct Survivors {
    pub squeezed: Vec<EntryId>,
    pub spread: Vec<EntryId>,
}

enum Alike {
    Squeezed,
    Spread,
}

pub fn wanted(query: &Query, candidates: usize) -> Option<&Term> {
    let [term] = &query.terms[..] else { return None };
    let loose = candidates < FUZZY_BELOW && term.needle.len() >= FUZZY_FROM && !term.strict && term.above.is_empty();
    loose.then_some(term)
}

pub fn survivors(
    ground: &Ground<'_>,
    parts: &mut [Part<'_>],
    loose: Option<&Term>,
    phrase: Option<&[u8]>,
) -> Option<Survivors> {
    let (live, all) = (ground.live, ground.all());
    let most = (SURVIVORS / parts.len().max(1)).max(1);
    let across =
        |parts: &[Part<'_>]| phrase.map_or_else(|| Some(Vec::new()), |phrase| holders(ground, parts, phrase, most));
    let Some(term) = loose else { return Some(Survivors { squeezed: across(parts)?, spread: Vec::new() }) };
    let rare = rarest(&term.needle);
    let in_the_phrase = phrase.is_none_or(|phrase| phrase.contains(&rare));
    let rarest = Finder::new(&[rare]).into_owned();
    let patience = if cfg!(debug_assertions) { DEBUG_HITS / parts.len().max(1) } else { usize::MAX };
    let survived = fan(parts.iter_mut().collect(), |part| {
        let (mut mine, mut looked, mut whole) = (Survivors::default(), 0usize, true);
        scan(live, part, &rarest, ground.stop, |hit, own| {
            let admitted = || term.admits(live, hit.id, live.flags(hit.id), ground.home);
            let fresh = *own & all != all && !live.is_dead(hit.id) && admitted();
            match fresh.then(|| alike(hit.name, &term.needle, phrase)).flatten() {
                Some(Alike::Squeezed) if mine.squeezed.len() < most => mine.squeezed.push(hit.id),
                Some(Alike::Spread) if *own == 0 && mine.spread.len() < most => mine.spread.push(hit.id),
                _ => {}
            }
            looked += 1;
            whole = mine.spread.len() < most && looked < patience;
            whole
        })?;
        Some((mine, whole))
    })?;
    let (mut every, mut whole) = (Survivors::default(), true);
    for part in survived {
        let (part, read_to_its_end) = part?;
        every.squeezed.extend(part.squeezed);
        every.spread.extend(part.spread);
        whole &= read_to_its_end;
    }
    if !(whole && in_the_phrase) {
        every.squeezed = across(parts)?;
    }
    Some(every)
}

pub fn trace(name: &[u8], needle: &[u8]) -> Option<Traced> {
    let mut end = 0;
    for byte in needle {
        end += memchr::memchr(*byte, name.get(end..)?)? + 1;
    }
    let mut at = vec![0; needle.len()];
    let mut before = end;
    for (slot, byte) in at.iter_mut().zip(needle).rev() {
        before = memchr::memrchr(*byte, &name[..before])?;
        *slot = before;
    }
    let skipped = (end - at.first()?).saturating_sub(needle.len());
    let runs = at.windows(2).filter(|pair| pair[1] == pair[0] + 1).count();
    let edges = at.iter().filter(|at| boundary(name, **at)).count();
    let gaps = (GAP * skipped as f32).max(GAP_FLOOR);
    let score = TIER_FUZZY + BOUNDARY * edges as f32 + RUN * runs as f32 + gaps + cover(needle.len(), name.len());
    Some(Traced { score, at })
}

fn alike(name: &[u8], needle: &[u8], phrase: Option<&[u8]>) -> Option<Alike> {
    if !follows(name, phrase.unwrap_or(needle)) {
        return None;
    }
    if phrase.is_some_and(|phrase| squeezed(name, phrase).is_some()) {
        return Some(Alike::Squeezed);
    }
    let spread = phrase.is_none_or(|phrase| phrase == needle) || follows(name, needle);
    (spread && trace(name, needle).is_some_and(|traced| telling(name, &traced))).then_some(Alike::Spread)
}

fn telling(name: &[u8], traced: &Traced) -> bool {
    let at = &traced.at;
    let slips = || at.windows(2).filter(|pair| pair[1] != pair[0] + 1 && !boundary(name, pair[1]));
    let starts_a_word = at.first().is_some_and(|first| boundary(name, *first));
    let near = slips().all(|pair| pair[1] - pair[0] - 1 <= SLIP_REACH);
    starts_a_word && near && slips().count() <= (at.len() + LETTERS_A_SLIP / 2) / LETTERS_A_SLIP
}

fn follows(name: &[u8], needle: &[u8]) -> bool {
    let mut from = 0;
    needle.iter().all(|byte| {
        let found = name.get(from..).and_then(|rest| memchr::memchr(*byte, rest));
        from += found.map_or(0, |at| at + 1);
        found.is_some()
    })
}

pub(super) fn rarest(needle: &[u8]) -> u8 {
    let rank = |byte: &u8| COMMON_FIRST.iter().position(|common| common == byte).unwrap_or(COMMON_FIRST.len());
    needle.iter().copied().max_by_key(rank).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicU64;

    use super::*;
    use crate::find::context::Context;
    use crate::find::query::parts::cut;
    use crate::find::query::{Stop, parse};
    use crate::find::tests::corpus::from_paths;

    fn only(text: &str) -> Query {
        parse(text, &Context::default())
    }

    #[test]
    fn trace_finds_tightest_run() {
        let traced = trace(b"notch_base_readme.md", b"nbr").unwrap();
        assert_eq!(traced.at, [0, 6, 11]);
        assert_eq!(traced.score, TIER_FUZZY + 3.0 * BOUNDARY + 9.0 * GAP + cover(3, 20));
        let far = trace(b"a-------------------b", b"ab").unwrap();
        assert_eq!(
            far.score,
            TIER_FUZZY + BOUNDARY * 2.0 + GAP_FLOOR + cover(2, 21),
            "a gap costs no more than its floor"
        );
        assert_eq!(trace(b"xxabxxabc", b"abc").unwrap().at, [6, 7, 8], "the later, tighter run");
        assert_eq!(trace(b"a-b-c", b"abc").unwrap().score, TIER_FUZZY + 3.0 * BOUNDARY + 2.0 * GAP + cover(3, 5));
        let tight = trace(b"cargo.toml", b"cago").unwrap();
        assert_eq!(
            (tight.at.as_slice(), tight.score),
            (&[0, 1, 3, 4][..], TIER_FUZZY + BOUNDARY + 2.0 * RUN + GAP + cover(4, 10))
        );
        assert_eq!(trace(b"cargo.toml", b"cz"), None);
        assert_eq!(trace(b"ab", b"ba"), None, "the order matters");
        assert!(follows(b"cargo.toml", b"cgtl") && !follows(b"cargo.toml", b"cgg") && follows(b"x", b""));
    }

    #[test]
    fn guess_by_initials_or_gaps() {
        let tells = |name: &[u8], typed: &[u8]| matches!(alike(name, typed, None), Some(Alike::Spread));
        assert!(tells(b"notch_base_readme.md", b"nbr"), "the initials");
        assert!(tells(b"config.json", b"confg") && tells(b"cargo.toml", b"cago"), "a letter left out");
        assert!(tells(b"notchbase", b"ntchbse"), "two, in a longer word");
        assert!(tells(b"budget-report.pdf", b"budrep"), "the start of each word");
        assert!(!tells(b"2026-09-27-zero-budget-evidence.md", b"zen"), "letters gathered from across the name");
        assert!(!tells(b"iphone mirroring", b"img"), "nor a last letter found far away");
        assert!(!tells(b"reconfig.json", b"confg"), "nor a word begun in the middle of another");
        assert!(!tells(b"cargo.toml", b"cgtl"), "nor every other letter");
    }

    #[test]
    fn rarest_letter_is_sought() {
        assert_eq!(
            (rarest(b"quiz"), rarest(b"eat"), rarest(b"a-1"), rarest(b"e\xc3"), rarest(b"")),
            (b'q', b't', b'1', 0xc3, 0)
        );
    }

    #[test]
    fn wanted_only_when_little_found() {
        assert!(wanted(&only("crgo"), 0).is_some() && wanted(&only("crgo"), FUZZY_BELOW - 1).is_some());
        assert!(wanted(&only("crgo"), FUZZY_BELOW).is_none(), "enough was found");
        assert!(wanted(&only("cr"), 0).is_none() && wanted(&only("crgo toml"), 0).is_none());
        assert!(wanted(&only("*.rsx"), 0).is_none() && wanted(&only("src/crgo"), 0).is_none());
        assert!(wanted(&only("crgo/"), 0).is_some_and(|term| term.folders));
    }

    #[test]
    fn survivors_hold_letters_in_order() {
        let live = from_paths(&["/d/Cargo.toml", "/d/cargo/", "/d/crag.txt", "/d/orange.png", "/d/c-r-g-o.md"]);
        let (query, context, latest) = (only("crgo"), Context::default(), AtomicU64::new(0));
        let stop = Stop { latest: &latest, mine: 0 };
        let ground = Ground::new(&live, &query, &context, None, &stop);
        let mut own = vec![0; live.len()];
        let taken = live.resolve(std::path::Path::new("/d/c-r-g-o.md")).unwrap();
        own[taken as usize] = 1;
        let alike = survivors(&ground, &mut cut(&live, &mut own, 6), query.terms.first(), None).unwrap();
        let named = |ids: &[EntryId]| -> Vec<String> {
            ids.iter().map(|id| String::from_utf8_lossy(live.folded(*id)).into_owned()).collect()
        };
        assert_eq!((named(&alike.spread), alike.squeezed.len()), (vec!["cargo.toml".to_owned(), "cargo".into()], 0));
        let folders = only("crgo/");
        let ground = Ground::new(&live, &folders, &context, None, &stop);
        let alike = survivors(&ground, &mut cut(&live, &mut own, 6), folders.terms.first(), None).unwrap();
        assert_eq!(alike.spread.len(), 1, "only the folder");
    }

    #[test]
    fn squeezed_name_survives_as_phrase() {
        let live =
            from_paths(&["/d/my-file.txt", "/d/My File.txt", "/d/myfile.txt", "/d/my other file.txt", "/d/family.txt"]);
        let (context, latest) = (Context::default(), AtomicU64::new(0));
        let stop = Stop { latest: &latest, mine: 0 };
        let mut own = vec![0; live.len()];
        let found = |text: &str, own: &mut [u8]| {
            let query = only(text);
            let ground = Ground::new(&live, &query, &context, None, &stop);
            let loose = wanted(&query, 0);
            let alike = survivors(&ground, &mut cut(&live, own, 6), loose, query.phrase.as_deref()).unwrap();
            let named = |ids: &[EntryId]| -> Vec<String> {
                ids.iter().map(|id| String::from_utf8_lossy(live.folded(*id)).into_owned()).collect()
            };
            (named(&alike.squeezed), named(&alike.spread))
        };
        let every = vec!["my-file.txt".to_owned(), "my file.txt".into(), "myfile.txt".into()];
        assert_eq!(found("myfile", &mut own), (every.clone(), vec!["my other file.txt".to_owned()]));
        assert_eq!(
            found("my-file", &mut own),
            (every.clone(), Vec::new()),
            "the dash was typed, so spread letters need it"
        );
        assert_eq!(found("my fi le", &mut own), (every, Vec::new()), "several words are never spread");
        let taken = live.resolve(std::path::Path::new("/d/myfile.txt")).unwrap();
        own[taken as usize] = 0b01;
        assert_eq!(
            found("myfile", &mut own).0,
            ["my-file.txt", "my file.txt"],
            "what holds every word was found already"
        );
        assert_eq!(found("my fi le", &mut own).0.len(), 3, "one word of three is not every word");
        let nothing = survivors(&Ground::new(&live, &only("ab"), &context, None, &stop), &mut [], None, None);
        assert_eq!(nothing, Some(Survivors::default()), "with nothing to look for nothing is scanned");
    }
}
