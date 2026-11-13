use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::personal::{Personal, personal};
use super::why::{Signal, why};
use crate::find::context::{Context, DAY, HOUR, MINUTE, Stamp};
use crate::find::key::{Key, key_of};
use crate::find::kind::Kind;
use crate::find::model::Live;
use crate::find::rank::{Candidate, keys};
use crate::find::tests::corpus::from_paths;
use crate::find::usage::{Seen, Usage, Used};
use crate::find::view::{How, Pick, Why};
use crate::find::weights::{EDIT_SESSION, HABIT_CAP, HEAT, OPEN_RECENT, OPENED, PREFIX, TIER_EXACT, TIER_INSIDE};

pub(super) const MONDAY_NOON: Stamp = 1_704_110_400;
pub(super) const CARGO: &str = "/Users/john/GitHub/notchbase/Cargo.toml";
pub(super) const MAIN: &str = "/Users/john/GitHub/notchbase/src/main.rs";
pub(super) const TODO: &str = "/Users/john/notes/todo.md";
const CALENDAR: &str = "/Applications/Calendar.app";
const SRC: &str = "/Users/john/GitHub/notchbase/src/";
const CLOSE: f32 = 0.5;

pub(super) fn world() -> Live {
    from_paths(&[CARGO, MAIN, TODO, CALENDAR, SRC, "/Users/john/GitHub/notchbase/README.md"])
}

pub(super) fn key(path: &str) -> Key {
    key_of(Path::new(path))
}

pub(super) fn at(now: Stamp, app: Option<&str>) -> Context {
    Context::at(now, 0, app.map(Arc::from))
}

pub(super) fn open(usage: &Usage, path: &str, typed: &str, context: &Context) {
    let pick = Pick { path: PathBuf::from(path), typed: typed.to_owned(), rank: 0, how: How::Open, predicted: false };
    usage.picked(&pick, Kind::Other, context);
}

pub(super) fn edit(usage: &Usage, path: &str, sessions: u32, from: Stamp) -> Stamp {
    let ends = from + sessions * EDIT_SESSION;
    for session in 0..sessions {
        let at = from + session * EDIT_SESSION;
        usage.observe(&[Seen::Edited { path: PathBuf::from(path), at, plain: true }], at);
    }
    ends
}

fn candidate(live: &Live, path: &str) -> Candidate {
    let id = live.resolve(Path::new(path)).unwrap();
    let (flags, kind) = (live.flags(id), Kind::of(live.folded(id), live.flags(id)));
    Candidate { id, keys: keys(live, id), kind, flags, modified: live.modified(id), created: live.created(id) }
}

fn scored(usage: &Usage, live: &Live, path: &str, typed: &str, context: &Context) -> Personal {
    usage.read(|model| personal(model, &candidate(live, path), typed.as_bytes(), context))
}

fn told(usage: &Usage, live: &Live, path: &str, signal: Signal, context: &Context) -> Option<Why> {
    usage.read(|model| why(live, model, &candidate(live, path), signal, context))
}

#[test]
fn two_picks_two_letters() {
    let (live, usage) = (world(), Usage::load(None, MONDAY_NOON));
    open(&usage, CARGO, "cargo", &at(MONDAY_NOON, None));
    let once = scored(&usage, &live, CARGO, "ca", &at(MONDAY_NOON + HOUR, None));
    assert!((once.prefix - PREFIX / 2.0).abs() < CLOSE, "{once:?}");
    open(&usage, CARGO, "cargo", &at(MONDAY_NOON + DAY, None));
    let now = at(MONDAY_NOON + DAY + HOUR, None);
    let twice = scored(&usage, &live, CARGO, "ca", &now);
    assert!(twice.prefix > 0.98 * PREFIX && twice.prefix <= PREFIX, "{twice:?}");
    assert!(TIER_INSIDE + twice.prefix > TIER_EXACT, "a remembered pick lifts a loose match over an exact one");
    assert_eq!(scored(&usage, &live, CARGO, "c", &now).prefix, twice.prefix, "one letter recalls it as well");
    assert_eq!(scored(&usage, &live, CALENDAR, "ca", &now).prefix, 0.0);
    assert_eq!(scored(&usage, &live, CARGO, "x", &now).prefix, 0.0);
}

#[test]
fn opens_earn_by_count_and_age() {
    let (live, usage) = (world(), Usage::load(None, MONDAY_NOON));
    open(&usage, CARGO, "", &at(MONDAY_NOON, None));
    let fresh = scored(&usage, &live, CARGO, "", &at(MONDAY_NOON, None));
    let often = OPENED * (1.0 - (-1.0f32 / 3.0).exp());
    assert!((fresh.habit - (often + OPEN_RECENT)).abs() < CLOSE, "{fresh:?}");
    assert_eq!(fresh.why, Some(Signal::Opened));
    let stale = scored(&usage, &live, CARGO, "", &at(MONDAY_NOON + 3 * DAY, None));
    assert!(stale.habit < fresh.habit - OPEN_RECENT / 2.0 + CLOSE && stale.habit > OPEN_RECENT / 2.0);
    assert_eq!(told(&usage, &live, CARGO, Signal::Opened, &at(MONDAY_NOON, None)), Some(Why::Opened(MONDAY_NOON)));
}

#[test]
fn habit_stays_under_cap() {
    let (live, usage) = (world(), Usage::load(None, MONDAY_NOON));
    let mut now = MONDAY_NOON;
    for _ in 0..12 {
        open(&usage, CARGO, "", &at(now, Some("Xcode")));
        open(&usage, MAIN, "", &at(now + MINUTE, Some("Xcode")));
        now = edit(&usage, MAIN, 2, now + 2 * MINUTE);
    }
    open(&usage, CARGO, "", &at(now, Some("Xcode")));
    let all_at_once = scored(&usage, &live, MAIN, "main", &at(now + MINUTE, Some("Xcode")));
    assert_eq!(all_at_once.habit, HABIT_CAP);
    assert!(all_at_once.why.is_some());
    let every_signal = [Signal::Opened, Signal::Edited, Signal::Heat, Signal::Sequence, Signal::App, Signal::Daypart];
    let context = at(now + MINUTE, Some("Xcode"));
    assert!(every_signal.iter().all(|signal| told(&usage, &live, MAIN, *signal, &context).is_some()));
    assert_eq!(told(&usage, &live, MAIN, Signal::Sequence, &context), Some(Why::After("Cargo.toml".into())));
    assert_eq!(told(&usage, &live, MAIN, Signal::App, &context), Some(Why::With("Xcode".into())));
    assert_eq!(told(&usage, &live, MAIN, Signal::Daypart, &context), Some(Why::AroundNow));
}

#[test]
fn edited_scores_silently() {
    let (live, usage) = (world(), Usage::load(None, MONDAY_NOON));
    let now = at(edit(&usage, TODO, 4, MONDAY_NOON), None);
    let edited = scored(&usage, &live, TODO, "todo", &now);
    assert!(edited.habit > HEAT && edited.prefix == 0.0, "{edited:?}");
    assert_eq!(edited.why, Some(Signal::Heat));
    assert_eq!(told(&usage, &live, TODO, Signal::Edited, &now), None);
    assert_eq!(told(&usage, &live, TODO, Signal::Opened, &now), None);
    assert_eq!(told(&usage, &live, TODO, Signal::Heat, &now), Some(Why::Busy(4)));
    open(&usage, TODO, "", &now);
    assert!(matches!(told(&usage, &live, TODO, Signal::Edited, &now), Some(Why::Edited(_))));
}

#[test]
fn heat_warms_folders_and_files() {
    let (live, usage) = (world(), Usage::load(None, MONDAY_NOON));
    let now = at(edit(&usage, MAIN, 3, MONDAY_NOON), None);
    let (folder, sibling, uncle) = (
        scored(&usage, &live, SRC, "", &now),
        scored(&usage, &live, CARGO, "", &now),
        scored(&usage, &live, TODO, "", &now),
    );
    assert!(folder.habit > sibling.habit && sibling.habit > 0.0 && uncle.habit == 0.0, "{folder:?} {sibling:?}");
    assert_eq!((folder.why, sibling.why, uncle.why), (Some(Signal::Heat), Some(Signal::Heat), None));
    assert_eq!(told(&usage, &live, SRC, Signal::Heat, &now), Some(Why::Busy(3)));
}

#[test]
fn strangers_get_nothing() {
    let (live, usage) = (world(), Usage::load(None, MONDAY_NOON));
    let now = at(MONDAY_NOON, Some("Xcode"));
    assert_eq!(scored(&usage, &live, CALENDAR, "cal", &now), Personal::default());
    usage.seed(&[Used { path: PathBuf::from(CALENDAR), last: MONDAY_NOON - HOUR }], MONDAY_NOON);
    assert!(scored(&usage, &live, CALENDAR, "cal", &now).habit > 0.0);
    usage.observe(&[Seen::Removed { path: PathBuf::from(CALENDAR) }], MONDAY_NOON);
    assert_eq!(scored(&usage, &live, CALENDAR, "cal", &now).habit, 0.0);
    assert_eq!(told(&usage, &live, CALENDAR, Signal::Opened, &now), None);
}

#[test]
fn daypart_needs_four_opens() {
    let (live, usage) = (world(), Usage::load(None, MONDAY_NOON));
    let long_after = at(MONDAY_NOON + 28 * DAY, None);
    (0..3).for_each(|day| open(&usage, CARGO, "", &at(MONDAY_NOON + day * DAY, None)));
    assert_eq!(told(&usage, &live, CARGO, Signal::Daypart, &long_after), None);
    let before = scored(&usage, &live, CARGO, "", &long_after).habit;
    open(&usage, CARGO, "", &at(MONDAY_NOON + 3 * DAY, None));
    assert_eq!(told(&usage, &live, CARGO, Signal::Daypart, &long_after), Some(Why::AroundNow));
    assert!(scored(&usage, &live, CARGO, "", &long_after).habit > before + 40.0);
    let at_night = at(MONDAY_NOON + 28 * DAY + 10 * HOUR, None);
    assert_eq!(told(&usage, &live, CARGO, Signal::Daypart, &at_night), None);
}
