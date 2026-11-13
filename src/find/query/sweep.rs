//! When a question is only a kind, an ending or a date, sweeps the index by that instead of by name.

use memchr::memmem::Finder;

use crate::find::model::EntryId;

use super::Ground;
use super::mark::{SWEPT, scan};
use super::parts::{Part, fan};
use super::words::Reading;

const CHECKS_STOP_EVERY: usize = 65_536;
const END_OF_NAME: char = '\0';

pub fn sweep(ground: &Ground<'_>, parts: &mut [Part<'_>]) -> Option<()> {
    let Some(driver) = driver(ground) else { return Some(()) };
    let endings: Vec<Finder<'static>> = match driver {
        Reading::Kind(kind) => kind.extensions().iter().map(|ending| ends_with(ending)).collect(),
        Reading::Ends(_) | Reading::Within(_) => Vec::new(),
    };
    let swept = fan(parts.iter_mut().collect(), |part| {
        if endings.is_empty() { by_column(ground, part, driver) } else { by_ending(ground, part, driver, &endings) }
    })?;
    swept.into_iter().collect()
}

fn driver<'a>(ground: &Ground<'a>) -> Option<&'a Reading> {
    let read = || ground.query.terms.iter().filter_map(|term| term.reading.as_ref());
    let kind = read().find(|reading| matches!(reading, Reading::Kind(_)));
    kind.or_else(|| read().find(|reading| matches!(reading, Reading::Within(_))))
}

fn ends_with(ending: &str) -> Finder<'static> {
    Finder::new(format!(".{ending}{END_OF_NAME}").as_bytes()).into_owned()
}

fn by_ending(ground: &Ground<'_>, part: &mut Part<'_>, driver: &Reading, endings: &[Finder<'_>]) -> Option<()> {
    let live = ground.live;
    let sweep = |ending| {
        scan(live, part, ending, ground.stop, |hit, own| {
            let held = || driver.holds(hit.name, live.flags(hit.id), live.modified(hit.id), live.created(hit.id));
            if *own == 0 && !live.is_dead(hit.id) && held() {
                *own = SWEPT;
            }
            true
        })
    };
    endings.iter().map(sweep).collect()
}

fn by_column(ground: &Ground<'_>, part: &mut Part<'_>, driver: &Reading) -> Option<()> {
    let live = ground.live;
    for (nth, own) in part.own.iter_mut().enumerate() {
        if nth.is_multiple_of(CHECKS_STOP_EVERY) && ground.stop.stopped() {
            return None;
        }
        let id = part.first + nth as EntryId;
        let held = || match driver {
            Reading::Within(span) => span.holds(live.modified(id), live.created(id)),
            named => {
                let flags = live.flags(id);
                named.may_hold(flags) && named.holds(live.folded(id), flags, live.modified(id), live.created(id))
            }
        };
        if *own == 0 && held() && !live.is_dead(id) {
            *own = SWEPT;
        }
    }
    Some(())
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicU64;

    use super::*;
    use crate::find::context::{Context, DAY};
    use crate::find::model::Live;
    use crate::find::query::parts::cut;
    use crate::find::query::{Stop, parse};
    use crate::find::tests::corpus::{EPOCH, Planted, from_paths, plant};

    fn swept(live: &Live, text: &str, latest: u64) -> Option<Vec<String>> {
        let context = Context::at(EPOCH, 0, None);
        let (query, latest) = (parse(text, &context), AtomicU64::new(latest));
        let stop = Stop { latest: &latest, mine: 1 };
        let ground = Ground::new(live, &query, &context, None, &stop);
        let mut own = vec![0; live.len()];
        sweep(&ground, &mut cut(live, &mut own, 6))?;
        assert!(own.iter().all(|own| *own == 0 || *own == SWEPT));
        let names = |id: usize| String::from_utf8_lossy(live.folded(id as EntryId)).into_owned();
        Some((0..own.len()).filter(|id| own[*id] != 0).map(names).collect())
    }

    fn desk() -> Live {
        let mut live =
            from_paths(&["/d/photo.png", "/d/scan.jpeg", "/d/png/", "/d/notes.txt", "/d/Maps.app/", "/d/a.zip"]);
        let old = EPOCH - 3 * DAY;
        plant(&mut live, &Planted { path: "/d/old.png", modified: old, created: old, flags: 0 });
        live
    }

    #[test]
    fn kind_sweeps_by_ending() {
        assert_eq!(swept(&desk(), "image", 1).unwrap(), ["photo.png", "scan.jpeg", "old.png"]);
        assert_eq!(swept(&desk(), "zip", 1).unwrap(), ["a.zip"]);
        assert_eq!(swept(&desk(), "apps", 1).unwrap(), ["maps.app"]);
    }

    #[test]
    fn folders_and_dates_from_columns() {
        assert_eq!(swept(&desk(), "folders", 1).unwrap(), ["/", "d", "png"], "an app is not a folder");
        let today = swept(&desk(), "today", 1).unwrap();
        assert!(today.contains(&"photo.png".to_owned()) && !today.contains(&"old.png".to_owned()));
        assert_eq!(today.len(), desk().len() - 1);
    }

    #[test]
    fn kind_drives_over_date() {
        assert_eq!(swept(&desk(), "today image", 1).unwrap(), ["photo.png", "scan.jpeg", "old.png"]);
    }

    #[test]
    fn plain_words_sweep_nothing() {
        assert_eq!(swept(&desk(), "photo png .zip", 1).unwrap(), Vec::<String>::new());
        assert_eq!(swept(&desk(), "folders", 2), None);
        assert_eq!(swept(&desk(), "image", 2), None);
    }
}
