//! Ranking. A candidate's score adds up how its name matches (whole, stem, start, a word, inside, loose), where it
//! lives, how fresh it is and what Find has learned from you. The golden tests pin real-looking orderings.

#[cfg(test)]
mod golden;
#[cfg(test)]
mod golden_apps;
#[cfg(test)]
mod golden_big;
#[cfg(test)]
mod golden_book;
mod hit;
mod launch;
mod named;
mod place;
mod short;
mod text;
mod words;

use std::cmp::Ordering;
use std::path::{Path, PathBuf};

use super::guess::personal::{Personal, personal};
use super::guess::why::why;
use super::key::Key;
use super::kind::Kind;
use super::model::{EntryId, Live, flag};
use super::query::{Away, FUZZY_BELOW, Fit, Ground, NOBODY};
use super::usage::Usage;
use super::view::{FindRow, SHOWN, Why};
use super::weights::{HABIT_CAP, HOME_FOLDER, TAUGHT};
use words::{ceiling, marked, worded};

pub use hit::*;
pub use launch::{ENDINGS, launcher, shown_as};
pub use named::{named, veiled};
pub use place::{far, standing_away};
pub use short::{Scored, Shortlist};
pub use text::{boundary, cover, initials};

const ALIKE_TOGETHER: usize = 3;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Candidate {
    pub id: EntryId,
    pub keys: [Key; 3],
    pub kind: Kind,
    pub flags: u8,
    pub modified: u32,
    pub created: u32,
}

pub struct Judge<'a> {
    ground: &'a Ground<'a>,
    original: String,
}

impl<'a> Judge<'a> {
    pub fn new(ground: &'a Ground<'a>) -> Self {
        Self { ground, original: String::new() }
    }

    pub fn hopeless(&self, id: EntryId, flags: u8, fit: &Fit, floor: Option<f32>) -> bool {
        floor.is_some_and(|floor| self.fresh_if_hopeful(id, flags, fit, floor).is_none())
    }

    pub fn weigh(&mut self, id: EntryId, flags: u8, fit: Fit, floor: Option<f32>) -> Option<Scored> {
        let live = self.ground.live;
        let floor = floor.unwrap_or(f32::NEG_INFINITY);
        let (modified, fresh) = self.fresh_if_hopeful(id, flags, &fit, floor)?;
        let name = live.folded(id);
        let (quick, lift) = worded(self.ground, name, &fit, None);
        let placed = place::standing(name, flags) + fresh;
        if quick + lift + placed < floor {
            return None;
        }
        let text = if lift > 0.0 {
            live.name_into(id, &mut self.original);
            worded(self.ground, name, &fit, Some(&self.original)).0
        } else {
            quick
        };
        let lay = place::lay(live, id, self.ground.home);
        let away = place::away(live, id, &lay);
        let landmark = lay.at_home && flags & flag::DIR != 0 && away == Away::InSight;
        let home = if landmark { HOME_FOLDER } else { 0.0 };
        let ahead = landmark && fit.leads && !fit.fuzzy && !fit.squeezed;
        Some(Scored { score: text + placed + lay.depth + home, modified, id, fit: Fit { away, ahead, ..fit } })
    }

    fn fresh_if_hopeful(&self, id: EntryId, flags: u8, fit: &Fit, floor: f32) -> Option<(u32, f32)> {
        let live = self.ground.live;
        let at_best = if floor.is_finite() { ceiling(self.ground, fit) + place::standing_at_best(flags) } else { 0.0 };
        if at_best + place::FRESH_BEST < floor {
            return None;
        }
        let modified = live.modified(id);
        let fresh = place::freshness(self.ground.context.now, modified, live.created(id));
        (at_best + fresh >= floor).then_some((modified, fresh))
    }
}

struct Ranked {
    ahead: bool,
    taught: bool,
    unnamed: bool,
    total: f32,
    found: Scored,
    habits: Option<(Candidate, Personal)>,
}

impl Ranked {
    fn in_sight(&self, reach: Away) -> bool {
        self.found.fit.away <= reach || self.taught
    }
}

pub fn finish(ground: &Ground<'_>, usage: &Usage, shortlist: Vec<Scored>, home: Option<&Path>) -> Vec<FindRow> {
    let live = ground.live;
    let chosen = usage.read(|model| {
        let learned = !model.is_empty();
        let rank = |found: Scored| {
            let who = learned.then(|| candidate(live, found.id));
            let habits = who.map(|who| (who, personal(model, &who, &ground.query.typed, ground.context)));
            let learned = habits.map_or(0.0, |(_, personal)| personal.habit.min(HABIT_CAP) + personal.prefix);
            let taught = habits.is_some_and(|(_, personal)| personal.prefix >= TAUGHT);
            let unnamed = unnamed(&found.fit);
            Ranked { ahead: found.fit.ahead || taught, taught, unnamed, total: found.score + learned, found, habits }
        };
        let mut ranked: Vec<Ranked> = shortlist.into_iter().map(rank).collect();
        let reach = ground.reach;
        let really_found = ranked.iter().filter(|ranked| ranked.in_sight(reach) && !ranked.found.fit.fuzzy).count();
        let named = ranked.iter().any(|ranked| ranked.found.fit.app && !ranked.found.fit.fuzzy);
        if named || really_found >= FUZZY_BELOW {
            ranked.retain(|ranked| !(ranked.found.fit.app && ranked.found.fit.fuzzy));
        }
        let within = if really_found == 0 { reach.max(Away::Hidden) } else { reach };
        ranked.retain(|ranked| ranked.in_sight(within));
        let carried = |ranked: &Ranked| ranked.found.fit.app && live.flags(ranked.found.id) & flag::INSIDE_PACKAGE != 0;
        let hosts: Vec<EntryId> =
            ranked.iter().filter(|ranked| carried(ranked)).filter_map(|ranked| live.parent(ranked.found.id)).collect();
        ranked.retain(|ranked| !hosts.contains(&ranked.found.id));
        let by_score =
            |one: &Ranked, other: &Ranked| best_first(live, (one.total, &one.found), (other.total, &other.found));
        let by_standing =
            |one: &Ranked, other: &Ranked| other.ahead.cmp(&one.ahead).then_with(|| one.unnamed.cmp(&other.unnamed));
        ranked.sort_by(|one, other| by_standing(one, other).then_with(|| by_score(one, other)));
        let mut shown: Vec<(Key, Scored, Option<Why>)> = Vec::with_capacity(SHOWN);
        let (mut apps, mut met, mut waiting) = (Vec::new(), Vec::new(), Vec::new());
        let reason = |ranked: &Ranked| {
            ranked.habits.and_then(|(who, personal)| why(live, model, &who, personal.why?, ground.context))
        };
        for ranked in ranked {
            if shown.len() == SHOWN {
                break;
            }
            let key = live.key(ranked.found.id);
            if shown.iter().any(|(taken, ..)| *taken == key) || !launch::unseen(live, &mut apps, ranked.found.id) {
                continue;
            }
            if crowded(&mut met, live.folded(ranked.found.id)) && !ranked.taught {
                waiting.push((key, ranked));
            } else {
                shown.push((key, ranked.found, reason(&ranked)));
            }
        }
        for (key, ranked) in waiting {
            if shown.len() < SHOWN && shown.iter().all(|(taken, ..)| *taken != key) {
                shown.push((key, ranked.found, reason(&ranked)));
            }
        }
        shown
    });
    let mut rows: Vec<FindRow> = chosen
        .into_iter()
        .map(|(_, found, reason)| row_in(ground.book, live, found.id, marked(ground, &found), reason, false, home))
        .collect();
    told_apart(&mut rows);
    rows
}

fn unnamed(fit: &Fit) -> bool {
    let by_what_it_is = fit.own == 0 && fit.read != 0 && fit.over.iter().all(|above| *above == NOBODY);
    by_what_it_is && !fit.fuzzy && !fit.squeezed
}

fn crowded<'a>(met: &mut Vec<(&'a [u8], usize)>, name: &'a [u8]) -> bool {
    match met.iter_mut().find(|(known, _)| *known == name) {
        Some((_, times)) => {
            *times += 1;
            *times > ALIKE_TOGETHER
        }
        None => {
            met.push((name, 1));
            false
        }
    }
}

fn candidate(live: &Live, id: EntryId) -> Candidate {
    let flags = live.flags(id);
    let (modified, created) = (live.modified(id), live.created(id));
    Candidate { id, keys: keys(live, id), kind: Kind::of(live.folded(id), flags), flags, modified, created }
}

fn best_first(live: &Live, one: (f32, &Scored), other: (f32, &Scored)) -> Ordering {
    let path = |found: &Scored| {
        let mut path = PathBuf::new();
        live.path_into(found.id, &mut path);
        path.into_os_string().into_encoded_bytes()
    };
    let shorter = || {
        let (one, other) = (path(one.1), path(other.1));
        one.len().cmp(&other.len()).then_with(|| one.cmp(&other))
    };
    other.0.total_cmp(&one.0).then_with(|| other.1.modified.cmp(&one.1.modified)).then_with(shorter)
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicU64;

    use super::*;
    use crate::find::context::Context;
    use crate::find::query::{Stop, parse};
    use crate::find::tests::corpus::{EPOCH, Planted, from_paths, plant};

    #[test]
    fn ties_go_newer_shorter_earlier() {
        let mut live = from_paths(&["/b/notes.txt", "/a/notes.txt", "/a/deeper/notes.txt"]);
        let newer = plant(&mut live, &Planted { path: "/z/notes.txt", modified: EPOCH + 1, created: EPOCH, flags: 0 });
        let id = |path: &str| live.resolve(Path::new(path)).unwrap();
        let found = |id: EntryId| Scored { score: 1.0, modified: live.modified(id), id, fit: Fit::default() };
        let mut all =
            [found(id("/b/notes.txt")), found(id("/a/deeper/notes.txt")), found(newer), found(id("/a/notes.txt"))];
        all.sort_by(|one, other| best_first(&live, (one.score, one), (other.score, other)));
        let path = |found: &Scored| row(&live, found.id, Vec::new(), None, false, None).path.to_path_buf();
        let order: Vec<PathBuf> = all.iter().map(path).collect();
        assert_eq!(order, ["/z/notes.txt", "/a/notes.txt", "/b/notes.txt", "/a/deeper/notes.txt"].map(PathBuf::from));
        let lifted = |found: &Scored| if found.id == id("/a/deeper/notes.txt") { 2.0 } else { 1.0 };
        all.sort_by(|one, other| best_first(&live, (lifted(one), one), (lifted(other), other)));
        assert_eq!(path(&all[0]), Path::new("/a/deeper/notes.txt"), "a higher score always comes first");
    }

    #[test]
    fn below_floor_dropped_early() {
        let live = from_paths(&["/Users/john/MyReport.txt", "/Users/john/report.txt"]);
        let query = parse("report", &Context::at(EPOCH, 0, None));
        let latest = AtomicU64::new(0);
        let context = Context::at(EPOCH, 0, None);
        let stop = Stop { latest: &latest, mine: 0 };
        let ground = Ground::new(&live, &query, &context, None, &stop);
        let mut judge = Judge::new(&ground);
        let id = |path: &str| live.resolve(Path::new(path)).unwrap();
        let (humped, stem) = (id("/Users/john/MyReport.txt"), id("/Users/john/report.txt"));
        let own = Fit { own: 1, ..Fit::default() };
        let open = judge.weigh(humped, 0, own, None).expect("with no floor everything is weighed");
        assert_eq!(judge.weigh(humped, 0, own, Some(open.score)), Some(open), "a tie with the floor is kept");
        assert_eq!(judge.weigh(humped, 0, own, Some(open.score + 0.5)), None);
        let best = judge.weigh(stem, 0, own, None).unwrap();
        assert!(best.score > open.score && open.score > text::grade(b"myreport.txt", &query.terms[0]).unwrap().score);
    }
}
