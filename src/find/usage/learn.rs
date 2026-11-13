//! Learning from picks. Opening, dragging, shelving or revealing a result counts as an open at its own weight, and
//! teaches the typed memory, the time of day, the app and what was opened just before. The system's own last-used
//! dates seed opens too.

use crate::find::context::{Context, MINUTE, Stamp};
use crate::find::key::key_of;
use crate::find::kind::Kind;
use crate::find::model::fold_query;
use crate::find::view::{How, Pick};
use crate::find::weights::{PICK_DRAG, PICK_OPEN, PICK_REVEAL, PICK_SHELVE, SEED_OPEN, SEQUENCE_WINDOW};

use super::decay::since;
use super::model::Model;
use super::prefix::Typed;
use super::record::{Used, day_of};

const SAME_OPEN_WITHIN: Stamp = 2 * MINUTE;

impl Model {
    pub(super) fn picked(&mut self, pick: &Pick, kind: Kind, context: &Context) {
        let (now, key) = (context.now, key_of(&pick.path));
        let weight = match pick.how {
            How::Open | How::AsAdmin => PICK_OPEN,
            How::Drag => PICK_DRAG,
            How::Shelve => PICK_SHELVE,
            How::Reveal => PICK_REVEAL,
        };
        let app = context.app.as_deref().and_then(|name| self.app_slot(name));
        let Some(record) = self.admitted(&pick.path, true, now) else { return };
        record.open(now, weight);
        record.forgive(day_of(now));
        record.count_daypart(context.daypart());
        app.into_iter().for_each(|app| record.count_app(app));
        self.credit_open(&pick.path, kind == Kind::Folder, now, weight, now);
        let typed = Typed::of(&fold_query(&pick.typed));
        self.passed_over(typed, key, pick.rank, now);
        self.prefixes.learn(typed, key, weight, now);
        if let Some((before, _)) = self.previous.filter(|(_, at)| since(now, *at) <= SEQUENCE_WINDOW) {
            self.pairs.learn(before, key, weight, now);
        }
        self.previous = Some((key, now));
        self.book.dirty = true;
        self.book.picked_at = Some(now);
    }

    pub(super) fn seeded(&mut self, used: &Used, now: Stamp) {
        let last = used.last.min(now);
        let counted_until = self.files.get(key_of(&used.path)).map(|record| record.last_open);
        let already_counted =
            counted_until.is_some_and(|until| until != 0 && last <= until.saturating_add(SAME_OPEN_WITHIN));
        if last == 0 || already_counted {
            return;
        }
        let Some(record) = self.admitted(&used.path, true, now) else { return };
        record.open(last, SEED_OPEN);
        record.forgive(day_of(now));
        self.credit_open(&used.path, false, last, SEED_OPEN, now);
        self.book.dirty = true;
    }

    fn app_slot(&mut self, name: &str) -> Option<u8> {
        let files = &self.files;
        self.apps.intern(name, |slot| files.rows().iter().any(|record| record.uses_app(slot)))
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};
    use std::sync::Arc;

    use super::*;
    use crate::find::context::{DAY, HOUR};
    use crate::find::key::Key;

    const MONDAY_NOON: Stamp = 1_704_110_400;
    const CARGO: &str = "/Users/john/GitHub/notchbase/Cargo.toml";
    const MAIN: &str = "/Users/john/GitHub/notchbase/src/main.rs";

    fn pick(path: &str, typed: &str, how: How, rank: u8) -> Pick {
        Pick { path: PathBuf::from(path), typed: typed.to_owned(), rank, how, predicted: false }
    }

    fn at(now: Stamp, app: Option<&str>) -> Context {
        Context::at(now, 0, app.map(Arc::from))
    }

    fn key(path: &str) -> Key {
        key_of(Path::new(path))
    }

    #[test]
    fn pick_weight_follows_action() {
        let mut model = Model::default();
        for (how, weight) in [(How::Open, 1.0), (How::Drag, 1.0), (How::Shelve, 0.8), (How::Reveal, 0.7)] {
            let path = format!("/Users/john/{how:?}.txt");
            model.picked(&pick(&path, "", how, 0), Kind::Text, &at(MONDAY_NOON, None));
            let record = model.file(key(&path)).unwrap();
            assert_eq!((record.opens, record.last_open), (weight, MONDAY_NOON), "{how:?}");
        }
        assert_eq!(model.book.picked_at, Some(MONDAY_NOON));
        assert!(model.book.dirty && model.prefixes.rows().is_empty(), "a pick from the empty field teaches no text");
    }

    #[test]
    fn pick_remembers_text_time_app() {
        let mut model = Model::default();
        model.picked(&pick(CARGO, "  Car", How::Open, 0), Kind::Text, &at(MONDAY_NOON, Some("Xcode")));
        let record = model.file(key(CARGO)).unwrap();
        assert_eq!(record.dayparts[2], 1);
        assert_eq!(model.with_app(record, "Xcode").map(|habit| habit.count), Some(1.0));
        assert!(model.with_app(record, "Preview").is_none());
        assert_eq!(model.remembered(b"car", key(CARGO), MONDAY_NOON), 0.5);
        assert_eq!(model.remembered(b"c", key(CARGO), MONDAY_NOON), 0.5);
        let folder = model.folder(key("/Users/john/GitHub/notchbase")).unwrap();
        assert_eq!((folder.opens, folder.heat), (1.0, 0.0));
    }

    #[test]
    fn picked_folder_credits_itself() {
        let mut model = Model::default();
        model.picked(&pick("/Users/john/GitHub", "git", How::Reveal, 0), Kind::Folder, &at(MONDAY_NOON, None));
        assert_eq!(model.folder(key("/Users/john/GitHub")).map(|folder| folder.opens), Some(0.7));
        assert!(model.folder(key("/Users/john")).is_none() && model.file(key("/Users/john/GitHub")).is_some());
    }

    #[test]
    fn quick_picks_form_a_sequence() {
        let mut model = Model::default();
        model.picked(&pick(CARGO, "car", How::Open, 0), Kind::Text, &at(MONDAY_NOON, None));
        model.picked(&pick(MAIN, "main", How::Open, 0), Kind::Code, &at(MONDAY_NOON + 10 * MINUTE, None));
        assert!(model.pairs.follows(key(CARGO), key(MAIN), MONDAY_NOON).is_some());
        let much_later = MONDAY_NOON + 3 * HOUR;
        model.picked(&pick(CARGO, "car", How::Open, 0), Kind::Text, &at(much_later, None));
        assert!(model.pairs.follows(key(MAIN), key(CARGO), much_later).is_none(), "the window had closed");
        assert_eq!(model.after_previous(key(MAIN), much_later).map(|follows| follows.share), Some(1.0));
        assert!(model.after_previous(key(MAIN), much_later + SEQUENCE_WINDOW + 1).is_none());
    }

    #[test]
    fn lower_pick_halves_first_row() {
        let mut model = Model::default();
        let calendar = "/Applications/Calendar.app";
        (0..2).for_each(|_| model.picked(&pick(CARGO, "car", How::Open, 0), Kind::Text, &at(MONDAY_NOON, None)));
        let before = model.remembered(b"ca", key(CARGO), MONDAY_NOON);
        model.picked(&pick(calendar, "ca", How::Open, 1), Kind::App, &at(MONDAY_NOON, None));
        let (cargo, app) =
            (model.remembered(b"ca", key(CARGO), MONDAY_NOON), model.remembered(b"ca", key(calendar), MONDAY_NOON));
        assert!(cargo < before / 2.0 && app > 0.0, "{cargo} {app}");
        model.picked(&pick(calendar, "ca", How::Open, 1), Kind::App, &at(MONDAY_NOON, None));
        assert!(model.remembered(b"ca", key(calendar), MONDAY_NOON) > model.remembered(b"ca", key(CARGO), MONDAY_NOON));
    }

    #[test]
    fn same_seed_counts_once() {
        let mut model = Model::default();
        let used = Used { path: PathBuf::from(CARGO), last: MONDAY_NOON - DAY };
        model.seeded(&used, MONDAY_NOON);
        model.seeded(&used, MONDAY_NOON);
        model.seeded(&used, MONDAY_NOON + HOUR);
        let record = model.file(key(CARGO)).unwrap();
        assert_eq!((record.opens, record.last_open), (1.0, MONDAY_NOON - DAY));
        assert_eq!(model.folder(key("/Users/john/GitHub/notchbase")).map(|folder| folder.opens), Some(1.0));
        assert!(model.book.picked_at.is_none() && model.previous.is_none());
    }

    #[test]
    fn newer_seed_counts_again() {
        let mut model = Model::default();
        model.seeded(&Used { path: PathBuf::from(CARGO), last: MONDAY_NOON - DAY }, MONDAY_NOON);
        model.seeded(&Used { path: PathBuf::from(CARGO), last: MONDAY_NOON - 2 * DAY }, MONDAY_NOON);
        model.seeded(&Used { path: PathBuf::from(CARGO), last: MONDAY_NOON - HOUR }, MONDAY_NOON);
        let record = model.file(key(CARGO)).unwrap();
        assert!(record.opens > 1.9 && record.opens < 2.0 && record.last_open == MONDAY_NOON - HOUR);
        model.seeded(&Used { path: PathBuf::from(MAIN), last: 0 }, MONDAY_NOON);
        model.seeded(&Used { path: PathBuf::from(MAIN), last: MONDAY_NOON + DAY }, MONDAY_NOON);
        assert_eq!(model.file(key(MAIN)).map(|record| record.last_open), Some(MONDAY_NOON), "never from the future");
    }

    #[test]
    fn find_opens_not_counted_twice() {
        let mut model = Model::default();
        model.picked(&pick(CARGO, "car", How::Open, 0), Kind::Text, &at(MONDAY_NOON, None));
        model.seeded(&Used { path: PathBuf::from(CARGO), last: MONDAY_NOON + 3 }, MONDAY_NOON + MINUTE);
        assert_eq!(model.file(key(CARGO)).map(|record| record.opens), Some(1.0));
    }

    #[test]
    fn open_forgives_half_the_skips() {
        let mut model = Model::default();
        model.seeded(&Used { path: PathBuf::from(CARGO), last: MONDAY_NOON - DAY }, MONDAY_NOON);
        (0..4).for_each(|_| model.passed_by(&[key(CARGO)], None, MONDAY_NOON));
        assert!(model.file(key(CARGO)).is_some_and(|record| record.benched()));
        model.picked(&pick(CARGO, "", How::Open, 0), Kind::Text, &at(MONDAY_NOON, None));
        assert!(model.file(key(CARGO)).is_some_and(|record| !record.benched() && record.skips == 2));
    }
}
