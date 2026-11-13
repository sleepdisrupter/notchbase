//! Finds the names that spell the typed words run together, skipping spaces, dashes and underscores, starting
//! from the phrase's rarest letter.

use crate::find::model::EntryId;

use super::Ground;
use super::fuzzy::rarest;
use super::parts::{Part, STRIDE, fan};
use super::squeeze::SEPARATORS;

const END_OF_NAME: u8 = 0;

pub fn holders(ground: &Ground<'_>, parts: &[Part<'_>], phrase: &[u8], most: usize) -> Option<Vec<EntryId>> {
    let rare = rarest(phrase);
    let Some(anchor) = phrase.iter().position(|letter| *letter == rare) else { return Some(Vec::new()) };
    let held = fan(parts.iter().collect(), |part| held(ground, part, (phrase, anchor), most))?;
    let held: Option<Vec<Vec<EntryId>>> = held.into_iter().collect();
    Some(held?.concat())
}

fn held(ground: &Ground<'_>, part: &Part<'_>, (phrase, anchor): (&[u8], usize), most: usize) -> Option<Vec<EntryId>> {
    let (live, all, bytes) = (ground.live, ground.all(), part.bytes);
    let (mut mine, mut named, mut from) = (Vec::new(), None, part.from);
    while from < part.to && mine.len() < most {
        if ground.stop.stopped() {
            return None;
        }
        let until = (from + STRIDE).min(part.to);
        let places = memchr::memchr_iter(phrase[anchor], &bytes[from..=until]).map(|found| from + found);
        for at in places.filter(|at| spelled(bytes, *at, phrase, anchor)) {
            let start = memchr::memrchr(END_OF_NAME, &bytes[..at]).map_or(0, |end| end + 1);
            if named.replace(start) == Some(start) {
                continue;
            }
            let id = live.id_at(part.arena, start);
            let found_already = part.own.get(id.saturating_sub(part.first) as usize).is_none_or(|own| own & all == all);
            if !found_already && !live.is_dead(id) && mine.len() < most {
                mine.push(id);
            }
        }
        from = until;
    }
    Some(mine)
}

fn spelled(bytes: &[u8], at: usize, phrase: &[u8], anchor: usize) -> bool {
    reads(bytes[at + 1..].iter(), phrase[anchor + 1..].iter())
        && reads(bytes[..at].iter().rev(), phrase[..anchor].iter().rev())
}

fn reads<'a>(mut bytes: impl Iterator<Item = &'a u8>, mut letters: impl Iterator<Item = &'a u8>) -> bool {
    let skipped = |byte: &u8| SEPARATORS.iter().any(|separator| separator == byte);
    letters.all(|letter| bytes.find(|byte| *byte == letter || !skipped(byte)) == Some(letter))
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicU64;

    use super::*;
    use crate::find::context::Context;
    use crate::find::model::{Entry, Live, flag};
    use crate::find::query::fuzzy::{SURVIVORS, survivors, wanted};
    use crate::find::query::parts::cut;
    use crate::find::query::{Stop, parse, squeezed};
    use crate::find::tests::corpus::{Noise, from_paths};

    const MADE_UP: usize = 180_000;
    const NAME_BYTES: &[u8] = b"ab1 -_.";
    const PHRASES: [&str; 6] = ["ab", "b1a", "aab", "1", "ba1b", "abab1"];
    const EVERY: usize = usize::MAX;
    const SOME: usize = 100;
    const OVERLAY: usize = 1;

    fn made_up() -> Live {
        let mut live = Live::empty();
        let root = live.add(None, Entry { name: "/", modified: 0, created: 0, flags: flag::DIR });
        let mut noise = Noise::seeded(11);
        for _ in 0..MADE_UP {
            let name: Vec<u8> = (0..1 + noise.below(12)).map(|_| NAME_BYTES[noise.below(NAME_BYTES.len())]).collect();
            let name = String::from_utf8_lossy(&name);
            live.add(Some(root), Entry { name: &name, modified: 0, created: 0, flags: 0 });
        }
        live
    }

    fn halves<'a>(live: &'a Live, own: &'a mut [u8]) -> Vec<Part<'a>> {
        let added = live.arenas()[OVERLAY];
        let middle = added.bytes.len() / 2;
        let split = middle + memchr::memchr(END_OF_NAME, &added.bytes[middle..]).expect("a name ends after the middle");
        let next = live.id_at(OVERLAY, split + 1);
        let (left, right) = own.split_at_mut(next as usize);
        let whole = |from, to, first, own| Part { arena: OVERLAY, bytes: added.bytes, from, to, first, own };
        vec![whole(0, split, added.first, left), whole(split, added.bytes.len() - 1, next, right)]
    }

    fn asking<T>(live: &Live, text: &str, latest: u64, work: impl FnOnce(&Ground<'_>) -> T) -> T {
        let (context, latest) = (Context::default(), AtomicU64::new(latest));
        let query = parse(text, &context);
        work(&Ground::new(live, &query, &context, None, &Stop { latest: &latest, mine: 0 }))
    }

    #[test]
    fn finds_exactly_the_holders() {
        let live = made_up();
        assert!(live.arenas()[OVERLAY].bytes.len() > STRIDE, "the names span more than one stride");
        let mut own = vec![0; live.len()];
        for phrase in PHRASES {
            let phrase = phrase.as_bytes();
            let holding = |id: &EntryId| squeezed(live.folded(*id), phrase).is_some();
            let expected: Vec<EntryId> = (0..live.len() as EntryId).filter(holding).collect();
            assert!(expected.len() > SOME && expected.len() < MADE_UP, "{} hold it", expected.len());
            let found = |parts: &[Part<'_>]| asking(&live, "ab", 0, |ground| holders(ground, parts, phrase, EVERY));
            assert_eq!(found(&cut(&live, &mut own, 6)), Some(expected.clone()), "in the parts a question is cut into");
            assert_eq!(found(&halves(&live, &mut own)), Some(expected), "cut in two at a name's end");
        }
    }

    #[test]
    fn found_or_buried_rows_passed() {
        let mut live =
            from_paths(&["/d/my-file.txt", "/d/My File.txt", "/d/myfile.txt", "/d/my_file my file", "/d/family.txt"]);
        let id = |live: &Live, path: &str| live.resolve(std::path::Path::new(path)).expect("planted");
        let named = |live: &Live, own: &mut [u8], most: usize| -> Vec<String> {
            let parts = cut(live, own, 6);
            let found = asking(live, "my file", 0, |ground| holders(ground, &parts, b"myfile", most));
            let name = |id: &EntryId| String::from_utf8_lossy(live.folded(*id)).into_owned();
            found.expect("nobody asked again").iter().map(name).collect()
        };
        let mut own = vec![0; live.len()];
        let every = ["my-file.txt", "my file.txt", "myfile.txt", "my_file my file"];
        assert_eq!(named(&live, &mut own, EVERY), every, "a name that holds it twice is one row");
        assert_eq!(named(&live, &mut own, 2), every[..2]);
        own[id(&live, "/d/myfile.txt") as usize] = 0b11;
        own[id(&live, "/d/My File.txt") as usize] = 0b01;
        assert_eq!(named(&live, &mut own, EVERY), ["my-file.txt", "my file.txt", "my_file my file"]);
        live.bury(id(&live, "/d/my-file.txt"));
        assert_eq!(named(&live, &mut own, EVERY), ["my file.txt", "my_file my file"]);
    }

    #[test]
    fn cut_short_scan_still_finds() {
        let loose: Vec<String> = (0..SURVIVORS + SOME).map(|nth| format!("/d/my other file {nth}.txt")).collect();
        let mut paths: Vec<&str> = loose.iter().map(String::as_str).collect();
        paths.extend(["/d/my-file.txt", "/d/My File.txt", "/d/myfile.txt"]);
        let live = from_paths(&paths);
        let mut own = vec![0; live.len()];
        own[live.resolve(std::path::Path::new("/d/myfile.txt")).expect("planted") as usize] = 0b1;
        let alike = asking(&live, "myfile", 0, |ground| {
            let (loose, phrase) = (wanted(ground.query, 0), ground.query.phrase.as_deref());
            survivors(ground, &mut cut(&live, &mut own, 6), loose, phrase).expect("nobody asked again")
        });
        assert_eq!(alike.spread.len(), SURVIVORS, "the spread letters stop at their share, before the last names");
        let held: Vec<String> =
            alike.squeezed.iter().map(|id| String::from_utf8_lossy(live.folded(*id)).into_owned()).collect();
        assert_eq!(held, ["my-file.txt", "my file.txt"], "the name that holds the word itself was found already");
    }

    #[test]
    fn dropped_rare_letter_still_finds() {
        let live = from_paths(&["/d/wait...what.txt", "/d/wait-...-what.txt", "/d/plain.txt"]);
        let mut own = vec![0; live.len()];
        own[live.resolve(std::path::Path::new("/d/wait-...-what.txt")).expect("planted") as usize] = 0b1;
        let alike = asking(&live, "-...", 0, |ground| {
            let (loose, phrase) = (wanted(ground.query, 0), ground.query.phrase.as_deref());
            assert_eq!((loose.map(|term| &term.needle[..]), phrase), (Some(&b"-..."[..]), Some(&b"..."[..])));
            survivors(ground, &mut cut(&live, &mut own, 6), loose, phrase).expect("nobody asked again")
        });
        let held: Vec<&[u8]> = alike.squeezed.iter().map(|id| live.folded(*id)).collect();
        assert_eq!(held, [&b"wait...what.txt"[..]], "the dash that was looked for is not a letter of the phrase");
    }

    #[test]
    fn newer_question_stops_pass() {
        let live = from_paths(&["/d/my-file.txt"]);
        let mut own = vec![0; live.len()];
        let parts = cut(&live, &mut own, 6);
        assert_eq!(asking(&live, "my file", 9, |ground| holders(ground, &parts, b"myfile", EVERY)), None);
        assert_eq!(asking(&live, "my file", 0, |ground| holders(ground, &parts, b"", EVERY)), Some(Vec::new()));
        assert_eq!(asking(&live, "my file", 0, |ground| holders(ground, &[], b"myfile", EVERY)), Some(Vec::new()));
    }

    #[test]
    fn compares_around_skipping_separators() {
        let names = b"my-file\0m\0y file\0my__ -file.txt";
        let at = |from: usize, letter: u8| from + memchr::memchr(letter, &names[from..]).expect("the letter is there");
        assert!(spelled(names, at(0, b'f'), b"myfile", 2), "across the dash");
        assert!(spelled(names, at(0, b'm'), b"myfile", 0) && spelled(names, at(0, b'e'), b"myfile", 5));
        assert!(!spelled(names, at(8, b'm'), b"myfile", 0), "the next name is another name");
        assert!(!spelled(names, at(10, b'f'), b"myfile", 2), "and so is the one before");
        assert!(spelled(names, at(17, b'f'), b"myfile", 2), "any run of separators is skipped");
        assert!(!spelled(names, at(0, b'f'), b"myfiles", 2) && !spelled(names, at(0, b'f'), b"amyfile", 3));
    }
}
