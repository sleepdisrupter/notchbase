//! Marks each name with the terms it holds, scanning the name bytes in strides so a newer question can stop it.

use std::sync::atomic::{AtomicUsize, Ordering};

use memchr::memmem::Finder;

use crate::find::model::{EntryId, Live, tier};

use super::letter::{self, Share};
use super::parse::Term;
use super::parts::{Part, STRIDE, fan};
use super::{Ground, Stop};

pub const START: u8 = 0x80;
pub const SWEPT: u8 = 0x40;
#[cfg(test)]
pub const TERMS: u8 = 0x3F;

pub const DEBUG_HITS: usize = 20_000;
const NEAR: usize = 4_096;
const END_OF_NAME: u8 = 0;

pub struct Hit<'a> {
    pub id: EntryId,
    pub name: &'a [u8],
    pub leads: bool,
}

#[derive(Default)]
struct Spent {
    junk: AtomicUsize,
    seen: AtomicUsize,
}

impl Spent {
    fn past(counted: &AtomicUsize, most: usize) -> bool {
        counted.fetch_add(1, Ordering::Relaxed) >= most
    }
}

pub fn allowance() -> Option<usize> {
    cfg!(debug_assertions).then_some(DEBUG_HITS)
}

pub fn mark(ground: &Ground<'_>, parts: &mut [Part<'_>], most: Option<usize>) -> Option<bool> {
    let terms = &ground.query.terms;
    let spent: Vec<Spent> = terms.iter().map(|_| Spent::default()).collect();
    let by_rows = (most.is_none() && ground.query.only_letters()).then(|| letter::share(parts.len()));
    let capped = |spent| most.map(|most| (most, spent));
    let any = |cut: bool, one: Option<bool>| Some(cut | one?);
    let marked = fan(parts.iter_mut().collect(), |part| {
        let each = terms.iter().zip(&spent).enumerate();
        each.map(|(index, (term, spent))| mark_term(ground, part, (index, term), capped(spent), by_rows))
            .try_fold(false, any)
    })?;
    marked.into_iter().try_fold(false, any)
}

pub fn scan<'a>(
    live: &Live,
    part: &mut Part<'a>,
    finder: &Finder<'_>,
    stop: &Stop<'_>,
    mut seen: impl FnMut(Hit<'a>, &mut u8) -> bool,
) -> Option<()> {
    let bytes = part.bytes;
    let (mut from, mut known) = (part.from, (part.from, part.first));
    while from < part.to {
        if stop.stopped() {
            return None;
        }
        let aim = (from + STRIDE).min(part.to);
        let until = aim + memchr::memchr(END_OF_NAME, &bytes[aim..=part.to]).unwrap_or(part.to - aim);
        let mut at = from;
        while let Some(found) = finder.find(&bytes[at..=until]) {
            let inside = at + found + usize::from(bytes[at + found] == END_OF_NAME);
            let start = memchr::memrchr(END_OF_NAME, &bytes[..inside]).map_or(0, |nul| nul + 1);
            let end = inside + memchr::memchr(END_OF_NAME, &bytes[inside..]).unwrap_or(bytes.len() - inside);
            let before = start.saturating_sub(1).max(known.0);
            let id = if before - known.0 > NEAR && start < end {
                live.id_at(part.arena, start)
            } else {
                known.1 + memchr::memchr_iter(END_OF_NAME, &bytes[known.0..before]).count() as EntryId
            };
            known = (end.min(part.to), id + 1);
            let hit = Hit { id, name: &bytes[start..end], leads: inside == start };
            let own = part.own.get_mut(id.saturating_sub(part.first) as usize);
            if own.is_some_and(|own| !seen(hit, own)) {
                return Some(());
            }
            if end >= until {
                break;
            }
            at = end;
        }
        from = until;
    }
    Some(())
}

fn mark_term(
    ground: &Ground<'_>,
    part: &mut Part<'_>,
    (index, term): (usize, &Term),
    cap: Option<(usize, &Spent)>,
    by_rows: Option<Share>,
) -> Option<bool> {
    if let Some(share) = by_rows {
        return letter::mark(ground, part, (index, term), share);
    }
    let (live, bit) = (ground.live, 1u8 << index);
    let note = |hit: Hit<'_>, own: &mut u8, anywhere: bool| {
        if anywhere && cap.is_some_and(|(most, spent)| Spent::past(&spent.seen, most)) {
            return false;
        }
        if *own & bit != 0 {
            return true;
        }
        let flags = if cap.is_some() || term.folders { live.flags(hit.id) } else { 0 };
        let spent = tier(flags) != 0 && cap.is_some_and(|(most, spent)| Spent::past(&spent.junk, most));
        if !spent && !live.is_dead(hit.id) && term.admits(live, hit.id, flags, ground.home) {
            *own |= bit | if hit.leads { START } else { 0 };
        }
        true
    };
    if cap.is_some() && !term.strict {
        scan(live, part, &term.anchored, ground.stop, |hit, own| note(hit, own, false))?;
    }
    scan(live, part, &term.finder, ground.stop, |hit, own| note(hit, own, true))?;
    Some(false)
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicU64;

    use super::*;
    use crate::find::context::Context;
    use crate::find::model::with_tier;
    use crate::find::query::collect::JUNK_BUDGET;
    use crate::find::query::parse;
    use crate::find::query::parts::cut;
    use crate::find::tests::corpus::{Planted, from_paths, grown, plant};

    fn marks(live: &Live, text: &str, most: usize) -> Vec<u8> {
        marks_at_most(live, text, most, allowance()).0
    }

    fn marks_at_most(live: &Live, text: &str, parts: usize, most: Option<usize>) -> (Vec<u8>, bool) {
        let (query, context, latest) = (parse(text, &Context::default()), Context::default(), AtomicU64::new(0));
        let stop = Stop { latest: &latest, mine: 0 };
        let ground = Ground::new(live, &query, &context, None, &stop);
        let mut own = vec![0; live.len()];
        let cut = mark(&ground, &mut cut(live, &mut own, parts), most).expect("nobody asked anything newer");
        (own, cut)
    }

    fn marked(live: &Live, own: &[u8]) -> Vec<String> {
        let names = |id: usize| String::from_utf8_lossy(live.folded(id as EntryId)).into_owned();
        (0..own.len()).filter(|id| own[*id] != 0).map(names).collect()
    }

    #[test]
    fn names_get_their_word_bits() {
        let live = from_paths(&["/src/Cargo.toml", "/src/car.png", "/src/scar/", "/src/notes.toml", "/Cargo.lock"]);
        let own = marks(&live, "car toml", 6);
        assert_eq!(marked(&live, &own), ["cargo.toml", "car.png", "scar", "notes.toml", "cargo.lock"]);
        let bits = |name: &str| own[(0..live.len()).find(|id| live.folded(*id as EntryId) == name.as_bytes()).unwrap()];
        assert_eq!(bits("cargo.toml"), 0b11 | START);
        assert_eq!((bits("car.png"), bits("scar"), bits("notes.toml"), bits("src")), (0b01 | START, 0b01, 0b10, 0));
        assert_eq!(START & TERMS | SWEPT & TERMS | START & SWEPT, 0, "the three kinds of bits never overlap");
    }

    #[test]
    fn misfits_get_no_bit() {
        let mut live = from_paths(&["/a/main.rs", "/a/main.rst", "/a/old.rs", "/a/rs/", "/a/src/", "/a/src.txt"]);
        let gone = live.resolve(std::path::Path::new("/a/old.rs")).unwrap();
        live.bury(gone);
        assert_eq!(marked(&live, &marks(&live, "*.rs", 6)), ["main.rs"]);
        assert_eq!(marked(&live, &marks(&live, "src/", 6)), ["src"]);
    }

    #[test]
    fn threads_mark_like_one() {
        let mut live = grown(21, 3_000);
        let long = "x".repeat(700);
        for nth in 0..4_000 {
            let path = format!("/wide/{long}-report-{nth}.txt");
            plant(&mut live, &Planted { path: &path, modified: 1, created: 1, flags: 0 });
        }
        let mut shared = vec![0; live.len()];
        assert!(cut(&live, &mut shared, 6).len() > 1);
        let (alone, shared) = (marks(&live, "report 1 e", 1), marks(&live, "report 1 e", 6));
        assert_eq!(alone, shared);
        assert!(alone.iter().filter(|own| **own & 0b111 == 0b111).count() > 1_000, "many names hold all three words");
        let (by_rows, apart) = (marks_at_most(&live, "1 e", 1, None), marks_at_most(&live, "1 e", 6, None));
        assert_eq!(by_rows, apart, "single letters walk the rows of every part to the same marks");
        assert!(by_rows.0.iter().filter(|own| **own & 0b11 == 0b11).count() > 1_000 && !by_rows.1);
    }

    #[test]
    fn letters_use_an_allowance() {
        let (mut live, junk) = (Live::empty(), with_tier(0, 3));
        let files = JUNK_BUDGET[2] + 50;
        for nth in 0..files {
            plant(
                &mut live,
                &Planted { path: &format!("/store/cache-{nth}.tmp"), modified: 1, created: 1, flags: junk },
            );
        }
        let holding = |own: &[u8], bits: u8| own.iter().filter(|own| **own & bits == bits).count();
        let (alone, cut) = marks_at_most(&live, "e", 1, None);
        assert_eq!((holding(&alone, 0b01), cut), (JUNK_BUDGET[2] + 1, true), "the allowance and the folder above");
        let (beside, cut) = marks_at_most(&live, "e cache", 1, None);
        assert_eq!((holding(&beside, 0b11), cut), (files, false), "a word beside the letter leaves nothing out");
    }

    #[test]
    fn newer_question_stops_marking() {
        let live = grown(4, 500);
        let (query, context, latest) = (parse("e", &Context::default()), Context::default(), AtomicU64::new(9));
        let stop = Stop { latest: &latest, mine: 8 };
        let ground = Ground::new(&live, &query, &context, None, &stop);
        let mut own = vec![0; live.len()];
        assert_eq!(mark(&ground, &mut cut(&live, &mut own, 6), allowance()), None);
    }

    #[test]
    fn capped_build_limits_hits() {
        let mut live = Live::empty();
        let junk = with_tier(0, 2);
        for name in ["old-index-1.js", "old-index-2.js", "index-1.js", "index-2.js", "index-3.js", "index-4.js"] {
            plant(&mut live, &Planted { path: &format!("/node_modules/{name}"), modified: 1, created: 1, flags: junk });
        }
        for name in ["an-index.txt", "index.html", "index-a.txt", "index-b.txt", "index-c.txt"] {
            plant(&mut live, &Planted { path: &format!("/a/b/c/d/e/f/g/{name}"), modified: 1, created: 1, flags: 0 });
        }
        let (query, context, latest) = (parse("index", &Context::default()), Context::default(), AtomicU64::new(0));
        let stop = Stop { latest: &latest, mine: 0 };
        let ground = Ground::new(&live, &query, &context, None, &stop);
        let listed = |most: Option<usize>| {
            let mut own = vec![0; live.len()];
            mark(&ground, &mut cut(&live, &mut own, 6), most).unwrap();
            marked(&live, &own)
        };
        let user = ["index.html", "index-a.txt", "index-b.txt", "index-c.txt"];
        assert_eq!(listed(Some(3)), [&["index-1.js", "index-2.js", "index-3.js"][..], &user[..]].concat());
        assert_eq!(
            listed(Some(1)),
            [&["index-1.js"][..], &user[..]].concat(),
            "no junk can cost a user file its place"
        );
        assert_eq!(listed(Some(20)).len(), 11, "an allowance that is not reached cuts nothing");
        assert_eq!(listed(None).len(), 11, "a release build marks every hit");
        assert_eq!(allowance(), if cfg!(debug_assertions) { Some(20_000) } else { None });
    }
}
