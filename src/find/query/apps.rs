//! The index's apps, gathered once and kept between questions, so a question that names an app finds it first.

use std::path::PathBuf;

use memchr::memmem::Finder;

use crate::find::apps::Listed;
use crate::find::key::{Key, START};
use crate::find::model::{EntryId, Live};
use crate::find::rank::{ENDINGS, Scored, launcher, named, veiled};
use crate::find::weights::SHORTLIST;

use super::mark::{SWEPT, scan};
use super::parts::{MOST_PARTS, cut, fan};
use super::{Ground, Stop};

const END_OF_NAME: &[u8] = b"\0";

#[derive(Default)]
pub struct Launchers {
    ids: Vec<EntryId>,
    keys: Vec<Key>,
    index: Option<(u64, usize)>,
    rows: usize,
    edition: u64,
}

impl Launchers {
    pub fn refresh(&mut self, live: &Live, stop: &Stop<'_>, own: &mut [u8]) -> Option<()> {
        let index = Some((live.generation(), live.base().len()));
        let known = self.ids.len();
        if self.index != index || live.len() < self.rows {
            self.ids = swept(live, stop, own)?;
            self.keys.clear();
            (self.index, self.edition) = (index, self.edition + 1);
        } else {
            let added = self.rows as EntryId..live.len() as EntryId;
            self.ids.extend(added.filter(|id| launcher(live, *id, live.folded(*id), live.flags(*id))));
            self.edition += u64::from(self.ids.len() != known);
        }
        let keyed = self.keys.len();
        let key = |id: &EntryId| if (*id as usize) < live.len() { live.key(*id) } else { START };
        self.keys.extend(self.ids[keyed..].iter().map(key));
        self.rows = live.len();
        Some(())
    }

    pub fn forget(&mut self) {
        *self = Self { edition: self.edition, ..Self::default() };
    }

    pub fn edition(&self) -> u64 {
        self.edition
    }

    pub fn gathered(&self) -> bool {
        self.index.is_some()
    }

    pub fn roster(&self, live: &Live) -> Vec<Listed> {
        let mut path = PathBuf::new();
        let alive = |(id, _): &(&EntryId, &Key)| (**id as usize) < live.len() && !live.is_dead(**id);
        let listed = |(id, key): (&EntryId, &Key)| {
            live.path_into(*id, &mut path);
            Listed { key: *key, path: path.clone(), modified: live.modified(*id) }
        };
        self.ids.iter().zip(&self.keys).filter(alive).map(listed).collect()
    }

    #[cfg(test)]
    pub fn known(&self) -> &[EntryId] {
        &self.ids
    }
}

pub struct Named {
    pub best: Vec<Scored>,
    pub veiled: Vec<EntryId>,
}

pub fn launchers(ground: &Ground<'_>, known: &mut Launchers, own: &mut [u8]) -> Option<Named> {
    let live = ground.live;
    known.refresh(live, ground.stop, own)?;
    let mut original = String::new();
    let alive = |id: &EntryId| (*id as usize) < live.len() && !live.is_dead(*id);
    let real = |found: &Scored| launcher(live, found.id, live.folded(found.id), live.flags(found.id));
    let (mut best, mut hidden) = (Vec::new(), Vec::new());
    for (id, key) in known.ids.iter().copied().zip(&known.keys).filter(|(id, _)| alive(id)) {
        let card = ground.book.card(*key);
        if veiled(live, id, card) {
            hidden.push(id);
        }
        let running = ground.context.running.contains(key);
        best.extend(named(ground, (id, running), card, &mut original).filter(real));
    }
    if best.len() > SHORTLIST {
        best.sort_unstable_by(|one, other| other.cmp(one));
        best.truncate(SHORTLIST);
    }
    Some(Named { best, veiled: hidden })
}

pub fn sweep(ground: &Ground<'_>, known: &Launchers, own: &mut [u8]) {
    let live = ground.live;
    if !ground.reads_apps() {
        return;
    }
    for id in known.ids.iter().copied().filter(|id| !live.is_dead(*id)) {
        if let Some(own) = own.get_mut(id as usize).filter(|own| **own == 0) {
            *own = SWEPT;
        }
    }
}

pub fn replace(mut shortlist: Vec<Scored>, apps: Named) -> Vec<Scored> {
    let mut named: Vec<EntryId> = apps.best.iter().map(|app| app.id).chain(apps.veiled).collect();
    named.sort_unstable();
    shortlist.retain(|found| named.binary_search(&found.id).is_err());
    shortlist.extend(apps.best);
    shortlist
}

fn swept(live: &Live, stop: &Stop<'_>, own: &mut [u8]) -> Option<Vec<EntryId>> {
    let ending = |ending: &&[u8]| Finder::new(&[ending, END_OF_NAME].concat()).into_owned();
    let endings: Vec<Finder<'static>> = ENDINGS.iter().map(ending).collect();
    let mut parts = cut(live, own, MOST_PARTS);
    let found = fan(parts.iter_mut().collect(), |part| {
        let mut mine = Vec::new();
        for ending in &endings {
            scan(live, part, ending, stop, |hit, _| {
                if launcher(live, hit.id, hit.name, live.flags(hit.id)) {
                    mine.push(hit.id);
                }
                true
            })?;
        }
        Some(mine)
    })?;
    let found: Option<Vec<Vec<EntryId>>> = found.into_iter().collect();
    Some(found?.concat())
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};
    use std::sync::atomic::AtomicU64;

    use super::*;
    use crate::find::context::Context;
    use crate::find::model::{Room, flag, with_tier};
    use crate::find::query::{Fit, parse};
    use crate::find::tests::corpus::{EPOCH, Planted, from_paths, plant};

    const SHORTCUT: &str = "/ProgramData/Microsoft/Windows/Start Menu/Programs/Maps.lnk";
    const APPS: [&str; 3] = ["/Applications/Maps.app", "/Applications/Safari.app", SHORTCUT];

    fn put(live: &mut Live, path: &str, flags: u8) -> EntryId {
        plant(live, &Planted { path, modified: EPOCH, created: EPOCH, flags })
    }

    fn disk() -> Live {
        let mut live = from_paths(&["/Applications/Maps.app/", "/Users/me/Desktop/Maps.lnk", "/Users/me/maps.txt"]);
        put(&mut live, SHORTCUT, with_tier(0, 1));
        put(&mut live, "/Applications/Safari.app", flag::LINK);
        live
    }

    fn asking<T>(live: &Live, text: &str, work: impl FnOnce(&Ground<'_>) -> T) -> T {
        let context = Context::at(EPOCH, 0, None);
        let (query, latest) = (parse(text, &context), AtomicU64::new(0));
        work(&Ground::new(live, &query, &context, None, &Stop { latest: &latest, mine: 0 }))
    }

    fn paths(live: &Live, ids: impl IntoIterator<Item = EntryId>) -> Vec<String> {
        let mut path = PathBuf::new();
        let mut paths: Vec<String> = Vec::new();
        for id in ids {
            live.path_into(id, &mut path);
            paths.push(path.to_string_lossy().into_owned());
        }
        paths.sort_unstable();
        paths
    }

    fn met(live: &Live) -> Launchers {
        let (mut known, latest) = (Launchers::default(), AtomicU64::new(0));
        known.refresh(live, &Stop { latest: &latest, mine: 0 }, &mut vec![0; live.len()]).expect("nobody asked again");
        known
    }

    #[test]
    fn launchers_found_by_ending_and_place() {
        let live = disk();
        assert_eq!(paths(&live, met(&live).ids), APPS, "the shortcut on the Desktop and the text file are files");
        assert!(met(&Live::empty()).ids.is_empty());
    }

    #[test]
    fn known_kept_until_index_changes() {
        let (mut live, latest) = (disk(), AtomicU64::new(9));
        let mut known = met(&live);
        let overtaken = Stop { latest: &latest, mine: 1 };
        let again = |known: &mut Launchers, live: &Live| known.refresh(live, &overtaken, &mut vec![0; live.len()]);
        assert_eq!(again(&mut known, &live), Some(()), "nothing is swept, so nothing can be stopped");
        put(&mut live, "/Applications/Notes.app/", 0);
        put(&mut live, "/Users/me/notes.txt", 0);
        assert_eq!(again(&mut known, &live), Some(()), "rows added since are read one by one");
        assert_eq!(
            paths(&live, known.ids.clone()),
            ["/Applications/Maps.app", "/Applications/Notes.app", APPS[1], APPS[2]]
        );
        let rebuilt = live.rebuilt(&Room { cap: usize::MAX, level: 0, boot: None });
        live.adopt(rebuilt);
        assert_eq!(again(&mut known, &live), None, "a rebuilt index is swept again, and a newer question stops that");
        assert_eq!(known.ids.len(), 4, "what was known stays until the sweep is whole");
        assert_eq!(paths(&live, met(&live).ids).len(), 4);
    }

    #[test]
    fn used_launcher_rechecked() {
        let mut live = disk();
        let mut known = met(&live);
        let named = |live: &Live, known: &mut Launchers| {
            let found = asking(live, "maps", |ground| launchers(ground, known, &mut vec![0; live.len()]).unwrap().best);
            assert!(found.iter().all(|found| found.fit.app));
            paths(live, found.iter().map(|found| found.id))
        };
        assert_eq!(named(&live, &mut known), ["/Applications/Maps.app", SHORTCUT]);
        live.bury(live.resolve(Path::new("/Applications/Maps.app")).unwrap());
        assert_eq!(named(&live, &mut known), [SHORTCUT], "a buried app is gone at once");
        let file = live.resolve(Path::new("/Users/me/maps.txt")).unwrap();
        known.ids.extend([file, EntryId::MAX]);
        assert_eq!(named(&live, &mut known), [SHORTCUT], "an id that is no launcher, or no row at all, is passed over");
    }

    #[test]
    fn named_apps_replace_held() {
        let found =
            |id: EntryId, score: f32, app: bool| Scored { score, modified: 0, id, fit: Fit { app, ..Fit::default() } };
        let pipeline = vec![found(4, 500.0, false), found(7, 320.0, false), found(9, 610.0, false)];
        let named = Named { best: vec![found(7, 900.0, true), found(2, 880.0, true)], veiled: vec![9] };
        let merged = replace(pipeline, named);
        let kept: Vec<(EntryId, f32, bool)> =
            merged.iter().map(|found| (found.id, found.score, found.fit.app)).collect();
        assert_eq!(kept, [(4, 500.0, false), (7, 900.0, true), (2, 880.0, true)], "9 shows another name");
    }

    #[test]
    fn apps_kind_sweeps_launchers() {
        let live = disk();
        let (known, id) = (met(&live), |path: &str| live.resolve(Path::new(path)).unwrap());
        let swept = |text: &str| {
            let mut own = vec![0; live.len()];
            asking(&live, text, |ground| sweep(ground, &known, &mut own));
            paths(&live, (0..live.len() as EntryId).filter(|id| own[*id as usize] == SWEPT))
        };
        assert_eq!(swept("apps start"), APPS);
        assert_eq!(swept("maps start"), Vec::<String>::new(), "only a question that asks for apps");
        let read = |path: &str| asking(&live, "apps maps", |ground| ground.read(id(path), live.flags(id(path))));
        assert_eq!(APPS.map(read), [0b01; 3]);
        assert_eq!((read("/Users/me/Desktop/Maps.lnk"), read("/Users/me/maps.txt")), (0, 0));
    }
}
