use std::path::PathBuf;

use super::super::{Seen, Usage, Used};
use super::{CARGO, DIARY, MAIN, MONDAY_NOON, README, at, edit, key, lived_in, pick};
use crate::find::context::{DAY, MINUTE};
use crate::find::kind::Kind;
use crate::find::tests::Scratch;
use crate::find::weights::{FILES, PROBATION};

const ACCENTED: &str = "/Users/john/r\u{e9}sum\u{e9}.pdf";
const PLAIN: &str = "/Users/john/resume.pdf";

fn counts(usage: &Usage) -> [usize; 5] {
    usage.read(|model| {
        [
            model.files.len(),
            model.folders.len(),
            model.prefixes.rows().len(),
            model.pairs.rows().len(),
            model.apps.names().len(),
        ]
    })
}

#[test]
fn forgetting_empties_everything() {
    let scratch = Scratch::new("usage-forget");
    let file = scratch.file("usage.bin");
    let usage = Usage::load(Some(&file), MONDAY_NOON);
    let now = lived_in(&usage);
    assert!(counts(&usage).iter().all(|count| *count > 0), "{:?}", counts(&usage));
    assert_eq!(usage.save_if_due(now + DAY).ok(), Some(true));
    std::fs::write(scratch.file("usage.bin.tmp"), b"half a write").unwrap();
    assert!(file.exists());
    usage.forget();
    assert_eq!(counts(&usage), [0; 5]);
    assert!(usage.read(|model| model.is_empty() && model.previous.is_none() && model.removed.is_empty()));
    assert!(!file.exists() && !scratch.file("usage.bin.tmp").exists());
    assert!(usage.matching(&[b"cargo"]).is_empty());
    assert_eq!(usage.save_if_due(now + 2 * DAY).ok(), Some(false), "nothing learned, nothing written");
    assert!(!file.exists());
}

#[test]
fn learning_off_at_launch_removes_file() {
    let scratch = Scratch::new("usage-off-at-launch");
    let file = scratch.file("usage.bin");
    let usage = Usage::load(Some(&file), MONDAY_NOON);
    let now = lived_in(&usage);
    assert_eq!(usage.save_if_due(now + DAY).ok(), Some(true));
    let forgotten = Usage::forgotten(Some(&file), now + DAY);
    assert_eq!(counts(&forgotten), [0; 5]);
    assert!(forgotten.matching(&[b"cargo"]).is_empty() && !file.exists());
}

#[test]
fn excluding_deletes_learning() {
    let usage = Usage::load(None, MONDAY_NOON);
    let now = lived_in(&usage);
    usage.observe(&[edit(DIARY, now)], now);
    usage.picked(&pick(CARGO, "car"), Kind::Text, &at(now, None));
    usage.picked(&pick(DIARY, "di"), Kind::Text, &at(now + MINUTE, None));
    assert!(usage.read(|model| model.pairs.follows(key(CARGO), key(DIARY), now).is_some()));
    usage.purge(&|path| !path.starts_with("/Users/john/private"));
    usage.read(|model| {
        assert!(model.file(key(DIARY)).is_none() && model.folder(key("/Users/john/private")).is_none());
        assert!(model.prefixes.rows().iter().all(|row| row.key != key(DIARY)));
        assert!(model.pairs.rows().iter().all(|row| row.before != key(DIARY) && row.after != key(DIARY)));
        assert!(model.previous.is_none(), "the last pick was the excluded file");
        assert!(model.file(key(CARGO)).is_some() && model.remembered(b"car", key(CARGO), now) > 0.0);
    });
    assert!(usage.matching(&[b"diary"]).is_empty() && usage.matching(&[b"private"]).is_empty());
}

#[test]
fn folded_alike_stay_separate() {
    let usage = Usage::load(None, MONDAY_NOON);
    usage.picked(&pick(ACCENTED, "res"), Kind::Pdf, &at(MONDAY_NOON, None));
    usage.picked(&pick(ACCENTED, "res"), Kind::Pdf, &at(MONDAY_NOON, None));
    usage.picked(&pick(PLAIN, "res"), Kind::Pdf, &at(MONDAY_NOON, None));
    assert_ne!(key(ACCENTED), key(PLAIN));
    usage.read(|model| {
        assert_eq!(model.file(key(ACCENTED)).map(|record| record.opens), Some(2.0));
        assert_eq!(model.file(key(PLAIN)).map(|record| record.opens), Some(1.0));
        assert!(
            model.remembered(b"res", key(ACCENTED), MONDAY_NOON) > model.remembered(b"res", key(PLAIN), MONDAY_NOON)
        );
    });
    usage.observe(&[Seen::Removed { path: PathBuf::from(PLAIN) }], MONDAY_NOON);
    assert!(usage.read(|model| model.known(key(ACCENTED)).is_some() && model.known(key(PLAIN)).is_none()));
}

#[test]
fn index_changes_reach_model() {
    let usage = Usage::load(None, MONDAY_NOON);
    let now = lived_in(&usage);
    assert!(usage.read(|model| model.file(key(README)).is_some_and(|record| record.gone() && record.file_id == 4_242)));
    let renamed = "/Users/john/GitHub/notchbase/READ-ME.md";
    let found_by_a_scan = Seen::Added { path: PathBuf::from(renamed), file_id: 4_242, created: now, plain: true };
    usage.observe(&[found_by_a_scan], now + 1);
    assert!(usage.read(|model| model.known(key(renamed)).is_some() && model.file(key(README)).is_none()));
    let moved = Seen::Moved { from: PathBuf::from("/Users/john/GitHub"), to: PathBuf::from("/Users/john/Code") };
    usage.observe(&[moved], now + 2);
    let cargo = "/Users/john/Code/notchbase/Cargo.toml";
    usage.read(|model| {
        assert!(model.known(key(cargo)).is_some() && model.file(key(CARGO)).is_none());
        assert!(model.remembered(b"car", key(cargo), now) > 0.0);
        assert!(model.heat(key("/Users/john/Code/notchbase/src"), now) > 0.0);
    });
    assert_eq!(usage.matching(&[b"code", b"cargo"]), [(key(cargo), cargo.into())]);
    assert!(usage.matching(&[b"github"]).is_empty());
}

#[test]
fn edit_flood_stays_on_probation() {
    let usage = Usage::load(None, MONDAY_NOON);
    lived_in(&usage);
    let flood: Vec<Seen> =
        (0..3 * PROBATION).map(|serial| edit(&format!("/Users/john/export/{serial}.csv"), MONDAY_NOON + DAY)).collect();
    usage.observe(&flood, MONDAY_NOON + DAY);
    usage.read(|model| {
        assert_eq!(model.files.rows().iter().filter(|record| !record.opened()).count(), PROBATION);
        assert!(model.files.len() < FILES && model.known(key(CARGO)).is_some());
    });
    let build: Vec<Seen> = (0..500)
        .map(|serial| Seen::Edited {
            path: format!("/Users/john/target/{serial}.o").into(),
            at: MONDAY_NOON + DAY,
            plain: false,
        })
        .collect();
    usage.observe(&build, MONDAY_NOON + DAY);
    assert!(usage.read(|model| model.folder(key("/Users/john/target")).is_none()));
}

#[test]
fn pick_saves_in_two_seconds() {
    let scratch = Scratch::new("usage-save");
    let file = scratch.file("find").join("usage.bin");
    let usage = Usage::load(Some(&file), MONDAY_NOON);
    assert_eq!(usage.save_if_due(MONDAY_NOON + DAY).ok(), Some(false), "a clean model is never written");
    usage.observe(&[edit(MAIN, MONDAY_NOON)], MONDAY_NOON);
    assert_eq!(usage.save_if_due(MONDAY_NOON + 9 * MINUTE).ok(), Some(false));
    assert_eq!(usage.save_if_due(MONDAY_NOON + 10 * MINUTE).ok(), Some(true));
    assert_eq!(usage.save_if_due(MONDAY_NOON + 11 * MINUTE).ok(), Some(false));
    let picked_at = MONDAY_NOON + 12 * MINUTE;
    usage.picked(&pick(CARGO, "car"), Kind::Text, &at(picked_at, None));
    assert_eq!(usage.save_if_due(picked_at + 1).ok(), Some(false));
    assert_eq!(usage.save_if_due(picked_at + 2).ok(), Some(true));
    assert_eq!(usage.save_if_due(picked_at + 3).ok(), Some(false));
    let again = Usage::load(Some(&file), picked_at + 5);
    assert!(again.read(|model| model.known(key(CARGO)).is_some() && model.known(key(MAIN)).is_some()));
    assert!(!scratch.file("find").join("usage.bin.tmp").exists());
}

#[test]
fn failed_write_is_retried() {
    let scratch = Scratch::new("usage-blocked");
    std::fs::write(scratch.file("in-the-way"), b"a file where the folder should be").unwrap();
    let usage = Usage::load(Some(&scratch.file("in-the-way").join("usage.bin")), MONDAY_NOON);
    usage.picked(&pick(CARGO, "car"), Kind::Text, &at(MONDAY_NOON, None));
    assert!(usage.save_if_due(MONDAY_NOON + 5).is_err());
    assert!(usage.read(|model| model.book.dirty && model.known(key(CARGO)).is_some()));
    assert!(
        usage.save_if_due(MONDAY_NOON + 6).ok() == Some(false) && usage.save_if_due(MONDAY_NOON + 11 * MINUTE).is_err()
    );
}

#[test]
fn no_file_never_writes() {
    let usage = Usage::load(None, MONDAY_NOON);
    let now = lived_in(&usage);
    assert_eq!(usage.save_if_due(now + DAY).ok(), Some(false));
    let shared = usage.clone();
    shared.seed(&[Used { path: PathBuf::from("/Users/john/todo.txt"), last: now }], now);
    assert!(usage.read(|model| model.known(key("/Users/john/todo.txt")).is_some()), "a clone is the same model");
    usage.forget();
    assert!(shared.read(|model| model.is_empty()));
}
