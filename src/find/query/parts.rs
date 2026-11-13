//! Cuts the index's name bytes into parts, one for each query thread, and runs work on them side by side.

use std::num::NonZeroUsize;
use std::sync::Mutex;
use std::thread;

use crate::find::QUERY_THREAD;
use crate::find::model::{EntryId, Live};
use crate::find::shared::lock;

pub const STRIDE: usize = 1 << 20;
pub const MOST_PARTS: usize = 64;

const MOST_THREADS: usize = 6;
const END_OF_NAME: u8 = 0;

pub struct Part<'a> {
    pub arena: usize,
    pub bytes: &'a [u8],
    pub from: usize,
    pub to: usize,
    pub first: EntryId,
    pub own: &'a mut [u8],
}

pub fn threads() -> usize {
    thread::available_parallelism().map_or(1, NonZeroUsize::get).clamp(1, MOST_THREADS)
}

pub fn cut<'a>(live: &'a Live, own: &'a mut [u8], most: usize) -> Vec<Part<'a>> {
    let arenas = live.arenas();
    let bytes: usize = arenas.iter().map(|arena| arena.bytes.len()).sum();
    let pieces = (bytes / STRIDE).clamp(1, most.max(1));
    let mut parts = Vec::with_capacity(pieces + arenas.len());
    let (mut rest, mut given) = (own, 0);
    for (index, arena) in arenas.iter().enumerate() {
        let after = arenas.get(index + 1).map_or(live.len() as EntryId, |next| next.first);
        let last = arena.bytes.len().saturating_sub(1);
        let share = (pieces * arena.bytes.len()).div_ceil(bytes.max(1)).max(1);
        let (mut from, mut first) = (0, arena.first);
        for piece in 1..=share {
            let aim = last * piece / share;
            let to = aim + memchr::memchr(END_OF_NAME, &arena.bytes[aim..]).unwrap_or(last - aim);
            if to <= from {
                continue;
            }
            let next = if to >= last { after } else { live.id_at(index, to + 1) };
            let unowned = std::mem::take(&mut rest);
            let skipped = (first.saturating_sub(given) as usize).min(unowned.len());
            let (_, unowned) = unowned.split_at_mut(skipped);
            let (mine, unowned) = unowned.split_at_mut((next.saturating_sub(first) as usize).min(unowned.len()));
            (rest, given) = (unowned, first + mine.len() as EntryId);
            parts.push(Part { arena: index, bytes: arena.bytes, from, to, first, own: mine });
            (from, first) = (to, next);
        }
    }
    parts
}

pub fn spans(parts: &[Part<'_>]) -> Vec<(EntryId, usize)> {
    parts.iter().map(|part| (part.first, part.own.len())).collect()
}

pub fn fan<T: Send, R: Send>(jobs: Vec<T>, work: impl Fn(T) -> R + Sync) -> Option<Vec<R>> {
    let (total, workers) = (jobs.len(), threads().min(jobs.len()));
    if workers < 2 {
        return Some(jobs.into_iter().map(work).collect());
    }
    let queue = Mutex::new(jobs.into_iter().enumerate());
    let drain = || {
        let mut done = Vec::new();
        loop {
            let next = lock(&queue).next();
            let Some((nth, job)) = next else { break done };
            done.push((nth, work(job)));
        }
    };
    let mut done = thread::scope(|scope| {
        let helper = |_| thread::Builder::new().name(QUERY_THREAD.to_owned()).spawn_scoped(scope, drain);
        let helpers: Vec<_> = (1..workers).filter_map(|nth| helper(nth).ok()).collect();
        let mut done = drain();
        for helper in helpers {
            done.extend(helper.join().unwrap_or_else(|panic| std::panic::resume_unwind(panic)));
        }
        done
    });
    done.sort_unstable_by_key(|(nth, _)| *nth);
    (done.len() == total).then(|| done.into_iter().map(|(_, result)| result).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::find::model::{Entry, flag};
    use crate::find::tests::corpus::grown;

    fn names(part: &Part<'_>) -> Vec<Vec<u8>> {
        let inside = &part.bytes[part.from + 1..part.to];
        inside.split(|byte| *byte == END_OF_NAME).map(<[u8]>::to_vec).collect()
    }

    #[test]
    fn small_index_is_one_part() {
        let live = grown(5, 300);
        let mut own = vec![0; live.len()];
        let parts = cut(&live, &mut own, 6);
        assert_eq!(parts.len(), 1);
        assert_eq!((parts[0].first, parts[0].own.len(), parts[0].from), (0, 300, 0));
        assert_eq!(names(&parts[0]).len(), 300);
    }

    #[test]
    fn big_index_cut_between_names() {
        let mut live = grown(9, 2_000);
        let long = "x".repeat(600);
        for nth in 0..4_000 {
            live.add(Some(0), Entry { name: &format!("{long}-{nth}"), modified: 1, created: 1, flags: flag::DIR });
        }
        let mut own = vec![0; live.len()];
        let parts = cut(&live, &mut own, 6);
        assert!((2..=6).contains(&parts.len()), "{} parts", parts.len());
        let mut next = 0;
        for part in &parts {
            assert_eq!((part.first, part.bytes[part.from], part.bytes[part.to]), (next, END_OF_NAME, END_OF_NAME));
            let held = names(part);
            assert_eq!(held.len(), part.own.len());
            assert!(held.iter().enumerate().all(|(nth, name)| live.folded(part.first + nth as EntryId) == &name[..]));
            next += held.len() as EntryId;
        }
        assert_eq!(next as usize, live.len());
        let spans = spans(&parts);
        assert_eq!((spans.len(), spans[0], spans[1].0 as usize), (parts.len(), (0, parts[0].own.len()), spans[0].1));
        assert_eq!(cut(&live, &mut own, 1).len(), 1, "asked for one part, it is cut into one however big it is");
    }

    #[test]
    fn empty_index_has_no_parts() {
        let live = Live::empty();
        assert!(cut(&live, &mut [], 6).is_empty());
    }

    #[test]
    fn fan_keeps_order() {
        assert_eq!(fan(Vec::<u32>::new(), |job| job), Some(Vec::new()));
        assert_eq!(fan(vec![7], |job| job * 2), Some(vec![14]));
        let squares = fan((0..5u64).collect(), |job| job * job);
        assert_eq!(squares, Some(vec![0, 1, 4, 9, 16]));
        let many: Vec<usize> = (0..500).collect();
        assert_eq!(
            fan(many.clone(), |job| job + 1),
            Some(many.iter().map(|job| job + 1).collect()),
            "more jobs than threads"
        );
    }

    #[test]
    fn helper_panic_fails_question() {
        let fell = std::panic::catch_unwind(|| fan(vec![1, 2, 3], |job| assert_ne!(job, 1, "the first helper fails")));
        assert!(fell.is_err());
    }
}
