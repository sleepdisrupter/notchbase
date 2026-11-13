//! Gathers the rows whose names hold the terms and how each fits, with a shared bar so hopeless rows drop out early.

use std::sync::atomic::{AtomicU32, Ordering};

use crate::find::model::{EntryId, tier};
use crate::find::rank::{Judge, Scored, Shortlist, standing_away};
use crate::find::weights::SHORTLIST;

use super::Ground;
use super::fit::{Away, Fit, NOBODY};
use super::mark::START;
use super::parse::MOST_TERMS;
use super::parts::fan;

pub const JUNK_BUDGET: [usize; 3] = [12_000, 6_000, 2_000];

const CHECKS_STOP_EVERY: usize = 4_096;
const HOPELESS: EntryId = 1;
const SHARE: usize = 1_024;

pub struct Found {
    pub best: Shortlist,
    pub tucked: Shortlist,
    pub count: usize,
    pub more: bool,
}

impl Found {
    pub fn offer(&mut self, found: Scored, reach: Away) {
        match found.fit.away {
            away if away <= reach => self.best.offer(found),
            Away::Hidden => self.tucked.offer(found),
            _ => {}
        }
    }
}

#[derive(Default)]
struct Lane {
    leading: Vec<EntryId>,
    rest: Vec<EntryId>,
}

struct Gathered {
    best: Shortlist,
    tucked: Shortlist,
    junk: [Lane; JUNK_BUDGET.len()],
    count: usize,
}

#[derive(Default)]
struct Bar(AtomicU32);

impl Bar {
    fn raised(&self, mine: Option<f32>) -> Option<f32> {
        let shared = f32::from_bits(self.0.load(Ordering::Relaxed));
        let best = mine.map_or(shared, |mine| mine.max(shared));
        if best > shared {
            self.0.fetch_max(best.to_bits(), Ordering::Relaxed);
        }
        (best > 0.0).then_some(best).or(mine)
    }
}

#[derive(Default)]
struct Bars {
    best: Bar,
    tucked: Bar,
}

fn lower(one: Option<f32>, other: Option<f32>) -> Option<f32> {
    Some(one?.min(other?))
}

pub fn fit(ground: &Ground<'_>, id: EntryId, flags: u8, owns: &impl Fn(EntryId) -> u8) -> Option<Fit> {
    let (all, junk) = (ground.all(), tier(flags) != 0);
    let held = |at: EntryId, wanted: u8| {
        let marked = owns(at);
        if junk { marked | ground.late(at, wanted & !marked) } else { marked }
    };
    let mine = held(id, all);
    let (own, read) = (mine & all, ground.read(id, flags));
    let mut have = own | read;
    if have == 0 {
        return None;
    }
    let (mut over, mut at) = ([NOBODY; MOST_TERMS], id);
    while have != all {
        at = ground.live.parent(at)?;
        let missing = all & !have;
        let gained = held(at, missing) & missing;
        for slot in over.iter_mut().enumerate().filter(|(term, _)| gained & (1 << term) != 0) {
            *slot.1 = at;
        }
        have |= gained;
    }
    Some(Fit { own, read, over, leads: mine & START != 0, ..Fit::default() })
}

pub fn collect(ground: &Ground<'_>, own: &[u8], spans: Vec<(EntryId, usize)>) -> Option<Found> {
    let bars = Bars::default();
    let gathered: Option<Vec<Gathered>> = fan(spans, |span| gather(ground, own, span, &bars))?.into_iter().collect();
    let lists = || Shortlist::new(SHORTLIST);
    let mut found = Found { best: lists(), tucked: lists(), count: 0, more: false };
    let mut lanes: [Lane; JUNK_BUDGET.len()] = Default::default();
    for part in gathered? {
        found.best.absorb(part.best);
        found.tucked.absorb(part.tucked);
        found.count += part.count;
        for (lane, junk) in lanes.iter_mut().zip(part.junk) {
            lane.leading.extend(junk.leading);
            lane.rest.extend(junk.rest);
        }
    }
    let mut chosen: Vec<EntryId> = Vec::new();
    for (lane, budget) in lanes.iter().zip(JUNK_BUDGET) {
        found.more |= lane.leading.len() + lane.rest.len() > budget;
        let hopeful = |packed: &&EntryId| **packed & HOPELESS == 0;
        chosen.extend(lane.leading.iter().chain(&lane.rest).take(budget).filter(hopeful).map(|packed| packed >> 1));
    }
    let floor = found.best.floor();
    let shares = chosen.chunks(SHARE).collect();
    let weighed = fan(shares, |share| {
        let marked = |id: EntryId| own.get(id as usize).copied().unwrap_or(0);
        let mut judge = Judge::new(ground);
        let weigh = |id: &EntryId| {
            let flags = ground.live.flags(*id);
            judge.weigh(*id, flags, fit(ground, *id, flags, &marked)?, floor)
        };
        share.iter().filter_map(weigh).collect::<Vec<_>>()
    })?;
    weighed.into_iter().flatten().for_each(|weighed| found.offer(weighed, ground.reach));
    Some(found)
}

fn gather(ground: &Ground<'_>, own: &[u8], (first, names): (EntryId, usize), bars: &Bars) -> Option<Gathered> {
    let marked = |id: EntryId| own.get(id as usize).copied().unwrap_or(0);
    let mine = own.get(first as usize..)?.get(..names)?;
    let mut judge = Judge::new(ground);
    let lists = || Shortlist::new(SHORTLIST);
    let mut gathered = Gathered { best: lists(), tucked: lists(), junk: Default::default(), count: 0 };
    let (mut from, mut seen) = (0, 0usize);
    while let Some(skipped) = mine[from..].iter().position(|own| *own != 0) {
        let id = first + (from + skipped) as EntryId;
        from += skipped + 1;
        seen += 1;
        if seen.is_multiple_of(CHECKS_STOP_EVERY) && ground.stop.stopped() {
            return None;
        }
        let flags = ground.live.flags(id);
        let standing = standing_away(flags);
        if standing == Away::Foreign && ground.reach < Away::Foreign {
            continue;
        }
        let Some(fit) = fit(ground, id, flags, &marked) else { continue };
        gathered.count += usize::from(standing <= ground.reach);
        let in_sight = bars.best.raised(gathered.best.floor());
        let floor = match ground.reach {
            Away::InSight => lower(in_sight, bars.tucked.raised(gathered.tucked.floor())),
            Away::Hidden | Away::Foreign => in_sight,
        };
        match usize::from(tier(flags)).checked_sub(1).and_then(|junk| gathered.junk.get_mut(junk)) {
            Some(lane) => {
                let packed = id << 1 | EntryId::from(judge.hopeless(id, flags, &fit, in_sight));
                if fit.leads { lane.leading.push(packed) } else { lane.rest.push(packed) }
            }
            None => match judge.weigh(id, flags, fit, floor) {
                Some(weighed) if weighed.fit.away <= ground.reach => gathered.best.offer(weighed),
                Some(weighed) if weighed.fit.away == Away::Hidden => gathered.tucked.offer(weighed),
                _ => {}
            },
        }
    }
    Some(gathered)
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicU64;

    use super::*;
    use crate::find::context::Context;
    use crate::find::model::Live;
    use crate::find::query::{Stop, parse};
    use crate::find::tests::corpus::{EPOCH, from_paths};

    fn fits(live: &Live, text: &str, path: &str) -> Option<Fit> {
        let context = Context::at(EPOCH, 0, None);
        let (query, latest) = (parse(text, &context), AtomicU64::new(0));
        let stop = Stop { latest: &latest, mine: 0 };
        let ground = Ground::new(live, &query, &context, None, &stop);
        let id = live.resolve(std::path::Path::new(path)).unwrap();
        fit(&ground, id, live.flags(id), &|id| ground.owned(id))
    }

    fn desk() -> Live {
        from_paths(&["/code/notchbase/README.md", "/code/notchbase/src/main.rs", "/code/other/README.md", "/pdf/a.txt"])
    }

    #[test]
    fn words_in_name_or_folder_above() {
        let live = desk();
        let notchbase = live.resolve(std::path::Path::new("/code/notchbase")).unwrap();
        for text in ["notchbase readme", "readme notchbase"] {
            let fit = fits(&live, text, "/code/notchbase/README.md").expect("the folder supplies the other word");
            let readme = usize::from(text.starts_with("notch"));
            assert_eq!(
                (fit.own, fit.above(1 - readme), fit.above(readme)),
                (1 << readme, Some(notchbase), None),
                "{text}"
            );
            assert_eq!(fits(&live, text, "/code/other/README.md"), None);
            assert_eq!(fits(&live, text, "/code/notchbase/src/main.rs"), None, "no word is in its own name");
        }
        assert!(fits(&live, "code notch main", "/code/notchbase/src/main.rs").is_some(), "two folders, any depth");
        assert_eq!(fits(&live, "notchbase", "/code/notchbase").map(|fit| fit.own), Some(1));
    }

    #[test]
    fn dropped_only_below_both_floors() {
        assert_eq!(lower(Some(700.0), Some(650.0)), Some(650.0));
        assert_eq!(lower(Some(650.0), Some(700.0)), Some(650.0));
        assert_eq!((lower(None, Some(700.0)), lower(Some(700.0), None), lower(None, None)), (None, None, None));
    }

    #[test]
    fn shared_bar_only_rises() {
        let bar = Bar::default();
        assert_eq!((bar.raised(None), bar.raised(Some(-20.0))), (None, Some(-20.0)), "nothing is known yet");
        assert_eq!(bar.raised(Some(700.0)), Some(700.0));
        assert_eq!(
            (bar.raised(None), bar.raised(Some(650.0)), bar.raised(Some(-5.0))),
            (Some(700.0), Some(700.0), Some(700.0))
        );
        assert_eq!((bar.raised(Some(812.5)), bar.raised(None)), (Some(812.5), Some(812.5)));
    }

    #[test]
    fn reading_counts_only_for_name() {
        let live = desk();
        let read = fits(&live, "rs notchbase", "/code/notchbase/src/main.rs").expect("a .rs file below notchbase");
        assert_eq!((read.own, read.read), (1, 1), "the letters are in the name and the ending was read");
        assert!(fits(&live, "today notchbase", "/code/notchbase/README.md").is_some());
        assert_eq!(
            fits(&live, "pdf today", "/pdf/a.txt").map(|fit| (fit.read, fit.above(0).is_some())),
            Some((2, true))
        );
        assert_eq!(
            fits(&live, "folder main", "/code/notchbase/src/main.rs"),
            None,
            "the folder above is not this file"
        );
    }
}
