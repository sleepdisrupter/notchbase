//! A term of one letter. It marks every name that holds the letter, with an allowance for each junk tier so one
//! letter can't fill the answer with cache files.

use crate::find::model::{EntryId, tier};

use super::Ground;
use super::collect::JUNK_BUDGET;
use super::mark::START;
use super::parse::Term;
use super::parts::Part;

const CHECKS_STOP_EVERY: usize = 65_536;

pub type Share = [Lane; JUNK_BUDGET.len()];

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Lane {
    leading: usize,
    rest: usize,
}

impl Lane {
    fn spent(&self) -> bool {
        self.leading == 0 && self.rest == 0
    }

    fn wants(&self, leads: bool) -> bool {
        self.rest > 0 || (leads && self.leading > 0)
    }

    fn take(&mut self, leads: bool) {
        let from = if leads && self.leading > 0 { &mut self.leading } else { &mut self.rest };
        *from = from.saturating_sub(1);
    }
}

pub fn share(parts: usize) -> Share {
    JUNK_BUDGET.map(|budget| {
        let each = budget.div_ceil(parts.max(1));
        Lane { leading: each, rest: each }
    })
}

pub fn mark(ground: &Ground<'_>, part: &mut Part<'_>, (index, term): (usize, &Term), mut share: Share) -> Option<bool> {
    let (live, bit) = (ground.live, 1u8 << index);
    let Some(&letter) = term.needle.first() else { return Some(false) };
    let mut cut = false;
    for (nth, own) in part.own.iter_mut().enumerate() {
        if nth.is_multiple_of(CHECKS_STOP_EVERY) && ground.stop.stopped() {
            return None;
        }
        let id = part.first + nth as EntryId;
        let flags = live.flags(id);
        let mut junk = usize::from(tier(flags)).checked_sub(1).and_then(|junk| share.get_mut(junk));
        if junk.as_ref().is_some_and(|lane| lane.spent()) {
            cut = true;
            continue;
        }
        let name = live.folded(id);
        let leads = name.first() == Some(&letter);
        if junk.as_ref().is_some_and(|lane| !lane.wants(leads)) {
            cut = true;
            continue;
        }
        let holds = leads || memchr::memchr(letter, name).is_some();
        if !holds || live.is_dead(id) || !term.admits(live, id, flags, ground.home) {
            continue;
        }
        if let Some(lane) = junk.as_mut() {
            lane.take(leads);
        }
        *own |= bit | if leads { START } else { 0 };
    }
    Some(cut)
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicU64;

    use super::*;
    use crate::find::context::Context;
    use crate::find::model::{Live, with_tier};
    use crate::find::query::parts::cut;
    use crate::find::query::{Stop, parse};
    use crate::find::tests::corpus::{Planted, from_paths, plant};

    const DEPENDENCY: u8 = 2;

    fn few(leading: usize, rest: usize) -> Share {
        [Lane { leading, rest }; JUNK_BUDGET.len()]
    }

    fn marked(live: &Live, text: &str, share: Share, latest: u64) -> Option<(Vec<String>, bool)> {
        let (query, context, latest) = (parse(text, &Context::default()), Context::default(), AtomicU64::new(latest));
        let stop = Stop { latest: &latest, mine: 0 };
        let ground = Ground::new(live, &query, &context, None, &stop);
        let mut own = vec![0; live.len()];
        let cut = mark(&ground, &mut cut(live, &mut own, 1)[0], (0, &query.terms[0]), share)?;
        let name = |id: usize| {
            let leads = if own[id] & START != 0 { "^" } else { "" };
            format!("{leads}{}", String::from_utf8_lossy(live.folded(id as EntryId)))
        };
        Some(((0..own.len()).filter(|id| own[*id] != 0).map(name).collect(), cut))
    }

    fn desk() -> Live {
        let mut live = from_paths(&["/home/Elm/", "/home/kept.txt", "/home/sky.png", "/home/tree.md"]);
        let junk = with_tier(0, DEPENDENCY);
        for name in ["one.js", "two.js", "three.js", "eel.js", "nine.js", "eight.js", "six.js"] {
            plant(&mut live, &Planted { path: &format!("/lib/{name}"), modified: 1, created: 1, flags: junk });
        }
        live
    }

    #[test]
    fn letter_marks_user_files() {
        let (names, cut) = marked(&desk(), "e", share(1), 0).unwrap();
        assert_eq!(
            names,
            ["home", "^elm", "kept.txt", "tree.md", "one.js", "three.js", "^eel.js", "nine.js", "^eight.js"]
        );
        assert!(!cut, "an allowance that is not reached cuts nothing");
        assert_eq!(marked(&desk(), "E", share(1), 0).unwrap().0, names, "the letter is folded like the names");
    }

    #[test]
    fn junk_gets_an_allowance() {
        let (names, cut) = marked(&desk(), "e", few(0, 2), 0).unwrap();
        assert_eq!(names, ["home", "^elm", "kept.txt", "tree.md", "one.js", "three.js"]);
        assert!(cut, "junk was left unread");
        let (names, cut) = marked(&desk(), "e", few(0, 0), 0).unwrap();
        assert_eq!((names, cut), (vec!["home".to_owned(), "^elm".into(), "kept.txt".into(), "tree.md".into()], true));
        let mut live = desk();
        plant(&mut live, &Planted { path: "/top/seen.txt", modified: 1, created: 1, flags: 0 });
        let (names, _) = marked(&live, "e", few(0, 0), 0).unwrap();
        assert_eq!(names[4..], ["seen.txt"], "a user file that comes after spent junk is still marked");
    }

    #[test]
    fn leading_junk_own_allowance() {
        let (names, _) = marked(&desk(), "e", few(1, 1), 0).unwrap();
        assert_eq!(names[4..], ["one.js", "^eel.js"], "the first that holds it and the first that starts with it");
        let (names, _) = marked(&desk(), "e", few(2, 0), 0).unwrap();
        assert_eq!(names[4..], ["^eel.js", "^eight.js"]);
        let (names, _) = marked(&desk(), "e", few(1, 9), 0).unwrap();
        assert_eq!(names[4..], ["one.js", "three.js", "^eel.js", "nine.js", "^eight.js"], "then it shares the rest");
    }

    #[test]
    fn allowance_split_by_part() {
        let lanes = share(4);
        assert_eq!(lanes.map(|lane| lane.rest), [3_000, 1_500, 500]);
        assert!(lanes.iter().all(|lane| lane.leading == lane.rest));
        assert_eq!(share(0), share(1));
        let mut live = desk();
        let cached = with_tier(0, 3);
        plant(&mut live, &Planted { path: "/tmp/late.bin", modified: 1, created: 1, flags: cached });
        let mut lanes = few(0, 0);
        lanes[2] = Lane { leading: 0, rest: 1 };
        assert_eq!(
            marked(&live, "e", lanes, 0).unwrap().0[4..],
            ["late.bin"],
            "spent dependencies cost a cache nothing"
        );
    }

    #[test]
    fn buried_unmarked_newer_stops() {
        let mut live = desk();
        let gone = live.resolve(std::path::Path::new("/home/kept.txt")).unwrap();
        live.bury(gone);
        assert_eq!(marked(&live, "e", few(0, 0), 0).unwrap().0, ["home", "^elm", "tree.md"]);
        assert_eq!(marked(&live, "e", share(1), 1), None);
    }
}
