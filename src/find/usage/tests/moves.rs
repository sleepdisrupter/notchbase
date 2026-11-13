use std::path::{Path, PathBuf};

use super::super::model::Model;
use super::super::prefix::Typed;
use crate::find::context::{DAY, Stamp};
use crate::find::key::{Key, key_of};
use crate::find::weights::{GONE_KEPT, MOVE_WINDOW};

const NOON: Stamp = 1_704_110_400;
const NOTES: &str = "/Users/john/notes";

fn note(name: &str) -> PathBuf {
    Path::new(NOTES).join(name)
}

fn opened(model: &mut Model, path: &Path) -> Key {
    assert!(model.admitted(path, true, NOON).map(|record| record.open(NOON, 1.0)).is_some());
    model.prefixes.learn(Typed::of(b"plan"), key_of(path), 1.0, NOON);
    key_of(path)
}

#[test]
fn removed_file_kept_thirty_days() {
    let mut model = Model::default();
    let plan = opened(&mut model, &note("plan.md"));
    model.removed(&note("plan.md"), NOON);
    assert!(model.known(plan).is_none() && model.files().next().is_none());
    assert!(model.file(plan).is_some_and(|record| record.gone() && record.opens == 1.0));
    model.expire(NOON + GONE_KEPT);
    assert!(model.file(plan).is_some());
    model.expire(NOON + GONE_KEPT + 2 * DAY);
    assert!(model.file(plan).is_none() && model.prefixes.rows().is_empty());
}

#[test]
fn returning_file_keeps_history() {
    let mut model = Model::default();
    let plan = opened(&mut model, &note("plan.md"));
    model.removed(&note("plan.md"), NOON);
    model.added(&note("plan.md"), 0, NOON + DAY);
    assert_eq!(model.known(plan).map(|record| record.opens), Some(1.0));
    model.expire(NOON + 90 * DAY);
    assert!(model.file(plan).is_some(), "it is no longer missing, so it does not expire");
}

#[test]
fn rename_moves_the_record() {
    let mut model = Model::default();
    let (plan, todo) = (opened(&mut model, &note("plan.md")), opened(&mut model, &note("todo.md")));
    model.pairs.learn(todo, plan, 1.0, NOON);
    model.previous = Some((plan, NOON));
    model.moved(&note("plan.md"), &note("roadmap.md"));
    let roadmap = key_of(&note("roadmap.md"));
    assert!(model.file(plan).is_none());
    assert_eq!(model.known(roadmap).map(|record| (record.key, record.opens)), Some((roadmap, 1.0)));
    assert_eq!(model.path_of(roadmap), Some("/Users/john/notes/roadmap.md"));
    assert!(model.prefixes.strength(Typed::of(b"plan"), roadmap, NOON) > 0.0);
    assert!(model.pairs.follows(todo, roadmap, NOON).is_some());
    assert_eq!(model.previous, Some((roadmap, NOON)));
}

#[test]
fn moving_folder_rewrites_paths() {
    let mut model = Model::default();
    let deep = opened(&mut model, &note("2024/january/plan.md"));
    let outside = opened(&mut model, Path::new("/Users/john/notes-old/plan.md"));
    model.edited(&note("2024/january/plan.md"), NOON, true, NOON);
    model.moved(Path::new(NOTES), Path::new("/Users/john/Documents/journal"));
    let moved = Path::new("/Users/john/Documents/journal/2024/january/plan.md");
    assert!(model.file(deep).is_none() && model.known(key_of(moved)).is_some());
    assert_eq!(model.path_of(key_of(moved)), moved.to_str());
    assert!(model.heat(key_of(Path::new("/Users/john/Documents/journal/2024/january")), NOON) > 0.0);
    assert!(model.folder(key_of(&note("2024/january"))).is_none());
    assert!(model.known(outside).is_some(), "a sibling whose name only starts alike stays put");
}

#[test]
fn empty_move_changes_nothing() {
    let mut model = Model::default();
    let plan = opened(&mut model, &note("plan.md"));
    model.book.dirty = false;
    model.moved(Path::new(""), Path::new("/elsewhere"));
    model.moved(&note("plan.md"), Path::new(""));
    model.moved(&note("plan.md"), &note("plan.md"));
    assert!(model.known(plan).is_some() && !model.book.dirty);
    assert_eq!(model.path_of(plan), Some("/Users/john/notes/plan.md"));
}

#[test]
fn add_with_fresh_id_is_rename() {
    let mut model = Model::default();
    let plan = opened(&mut model, &note("plan.md"));
    model.added(&note("plan.md"), 77, NOON);
    assert_eq!(model.file(plan).map(|record| record.file_id), Some(77));
    model.removed(&note("plan.md"), NOON + 10);
    model.added(Path::new("/Users/john/Desktop/plan-final.md"), 77, NOON + 11);
    let renamed = key_of(Path::new("/Users/john/Desktop/plan-final.md"));
    assert_eq!(model.known(renamed).map(|record| (record.file_id, record.opens)), Some((77, 1.0)));
    assert!(model.file(plan).is_none());
    assert!(model.prefixes.strength(Typed::of(b"plan"), renamed, NOON + 11) > 0.0);
}

#[test]
fn late_add_is_a_new_file() {
    let mut model = Model::default();
    let plan = opened(&mut model, &note("plan.md"));
    model.added(&note("plan.md"), 77, NOON);
    model.removed(&note("plan.md"), NOON + 10);
    model.added(&note("other.md"), 0, NOON + 10);
    model.added(&note("late.md"), 77, NOON + 10 + MOVE_WINDOW + 1);
    assert!(model.file(plan).is_some_and(|record| record.gone()));
    assert!(model.file(key_of(&note("other.md"))).is_none() && model.file(key_of(&note("late.md"))).is_none());
}

#[test]
fn removed_folder_hides_below() {
    let mut model = Model::default();
    let plan = opened(&mut model, &note("2024/plan.md"));
    let kept = opened(&mut model, Path::new("/Users/john/todo.md"));
    model.edited(&note("2024/plan.md"), NOON, true, NOON);
    model.previous = Some((plan, NOON));
    model.removed(Path::new(NOTES), NOON);
    assert!(model.known(plan).is_none() && model.known(kept).is_some() && model.previous.is_none());
    assert!(model.folder(key_of(&note("2024"))).is_none());
}

#[test]
fn purge_clears_excluded_paths() {
    let mut model = Model::default();
    let (secret, kept) =
        (opened(&mut model, Path::new("/Users/john/private/diary.md")), opened(&mut model, &note("a.md")));
    model.pairs.learn(kept, secret, 1.0, NOON);
    model.edited(Path::new("/Users/john/private/diary.md"), NOON, true, NOON);
    model.purge(&|path| !path.starts_with("/Users/john/private"));
    assert!(model.file(secret).is_none() && model.known(kept).is_some());
    assert_eq!(model.prefixes.rows().iter().map(|row| row.key).collect::<Vec<_>>(), [kept]);
    assert!(model.pairs.rows().is_empty());
    assert!(model.folder(key_of(Path::new("/Users/john/private"))).is_none());
    assert!(model.folders().all(|(_, path)| !path.contains("private")));
}

#[test]
fn rename_seen_either_way_round() {
    let mut model = Model::default();
    let plan = opened(&mut model, &note("zebra.md"));
    model.added(&note("zebra.md"), 77, NOON);
    model.added(&note("alpha.md"), 77, NOON + 10);
    assert!(model.known(plan).is_some(), "until the old name goes it may be a second link");
    model.removed(&note("zebra.md"), NOON + 10);
    let renamed = key_of(&note("alpha.md"));
    assert_eq!(model.known(renamed).map(|record| (record.file_id, record.opens)), Some((77, 1.0)));
    assert!(model.file(plan).is_none() && model.arrived.is_empty());
    assert!(model.prefixes.strength(Typed::of(b"plan"), renamed, NOON + 10) > 0.0);
}

#[test]
fn arrivals_remembered_briefly() {
    let mut model = Model::default();
    model.added(&note("first.md"), 5, NOON);
    assert!(model.arrived.is_empty(), "an empty model has nothing to rename");
    let plan = opened(&mut model, &note("plan.md"));
    model.added(&note("plan.md"), 77, NOON);
    (0..100).for_each(|serial| model.added(&note(&format!("new-{serial}.md")), 1_000 + serial, NOON));
    assert_eq!(model.arrived.len(), 32);
    model.added(&note("later.md"), 77, NOON + 1);
    model.removed(&note("plan.md"), NOON + 2 + MOVE_WINDOW);
    assert!(model.file(plan).is_some_and(|record| record.gone()) && model.arrived.is_empty());
}
