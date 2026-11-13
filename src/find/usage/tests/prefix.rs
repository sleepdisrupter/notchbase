use super::super::prefix::{Memory, Prefixes, Typed};
use crate::find::context::{Context, DAY, Stamp};
use crate::find::key::Key;
use crate::find::model::fold_query;
use crate::find::query::parse;
use crate::find::weights::{PREFIX_BYTES, PREFIX_EXTENDED, PREFIX_HALF_LIFE, PREFIXES};

const NOON: Stamp = 1_704_110_400;
const CARGO: Key = Key(1);
const CALENDAR: Key = Key(2);
const CHROME: Key = Key(3);
const CLOSE: f32 = 1e-3;

fn typed(text: &str) -> Typed {
    Typed::of(text.as_bytes())
}

fn taught(picks: &[(&str, Key)]) -> Prefixes {
    let mut prefixes = Prefixes::default();
    picks.iter().for_each(|(text, key)| prefixes.learn(typed(text), *key, 1.0, NOON));
    prefixes
}

#[test]
fn typed_text_is_trimmed_to_twelve() {
    assert_eq!(typed("  Car ").text(), b"car");
    assert_eq!(typed("notchbase readme").text(), b"notchbaserea", "twelve bytes of letters, not of spaces");
    assert_eq!(typed("my-file_v1.2\tfinal").text(), b"myfilev1.2fi", "a dot is a letter of the name");
    assert!(typed("   ").is_empty() && typed(" -_ - ").is_empty());
    assert!(typed("ca") < typed("car") && typed("car") < typed("cb"));
    assert_eq!(Typed::of(typed("Google  Chrome").text()), typed("Google  Chrome"), "said twice, it is the same");
}

#[test]
fn spaces_make_no_difference() {
    let mut prefixes = taught(&[("google chrome", CHROME)]);
    let spellings =
        ["google chrome", "google  chrome", "googlechrome", "Google-Chrome", "google_chrome ", " goo gle chrome"];
    for text in spellings {
        assert!((prefixes.strength(typed(text), CHROME, NOON) - 0.5).abs() < CLOSE, "{text}");
        assert_eq!(typed(text), typed("googlechrome"), "{text}");
    }
    assert!((prefixes.strength(typed("goo gl"), CHROME, NOON) - 0.5).abs() < CLOSE, "a start of it, spaced anyhow");
    spellings.iter().for_each(|text| prefixes.learn(typed(text), CHROME, 1.0, NOON));
    assert_eq!(prefixes.rows().len(), 1, "one memory, however it was spaced");
    assert_eq!(prefixes.strength(typed("google chrome"), CHROME, NOON), 1.0);
    assert_eq!(prefixes.strength(typed("google.chrome"), CHROME, NOON), 0.0, "a dot is not a space");
}

#[test]
fn memory_keyed_by_phrase_letters() {
    let context = Context::at(NOON, 0, None);
    for text in ["TreeSize  Free ", " tree size free", "my-file", "my_file v1.2", "a b c", "Google-Chrome"] {
        let phrase = parse(text, &context).phrase.expect("plain words make a phrase");
        assert_eq!(Typed::of(&fold_query(text)).text(), &phrase[..phrase.len().min(PREFIX_BYTES)], "{text}");
    }
}

#[test]
fn same_memories_merge() {
    let memory = |text: &[u8], key, weight, last| Memory { typed: Typed::of(text), key, weight, last };
    let later = NOON + PREFIX_HALF_LIFE;
    let rows = vec![
        memory(b"my file", CARGO, 2.0, NOON),
        memory(b"myfile", CARGO, 1.0, later),
        memory(b"my-file", CALENDAR, 1.0, NOON),
        memory(b"  ", CARGO, 9.0, NOON),
    ];
    let prefixes = Prefixes::from_rows(rows);
    let kept: Vec<(&[u8], Key, f32, Stamp)> =
        prefixes.rows().iter().map(|row| (row.typed.text(), row.key, row.weight, row.last)).collect();
    assert_eq!(kept.len(), 2, "{kept:?}");
    assert_eq!((kept[0].0, kept[0].1, kept[0].3), (&b"myfile"[..], CARGO, later));
    assert!((kept[0].2 - 2.0).abs() < CLOSE, "the older one faded to the day of the newer, then both count");
    assert_eq!(kept[1], (&b"myfile"[..], CALENDAR, 1.0, NOON), "another file keeps its own memory");
}

#[test]
fn two_picks_make_full_strength() {
    let mut prefixes = taught(&[("car", CARGO)]);
    assert!((prefixes.strength(typed("car"), CARGO, NOON) - 0.5).abs() < CLOSE);
    prefixes.learn(typed("car"), CARGO, 1.0, NOON);
    assert!((prefixes.strength(typed("car"), CARGO, NOON) - 1.0).abs() < CLOSE);
    assert_eq!(prefixes.rows().len(), 1);
}

#[test]
fn shorter_text_recalls_longer() {
    let prefixes = taught(&[("car", CARGO)]);
    assert!((prefixes.strength(typed("c"), CARGO, NOON) - 0.5).abs() < CLOSE);
    assert_eq!(prefixes.strength(typed("x"), CARGO, NOON), 0.0);
    assert_eq!(prefixes.strength(typed("c"), CALENDAR, NOON), 0.0);
    assert_eq!(prefixes.strength(typed(""), CARGO, NOON), 0.0);
}

#[test]
fn longer_recalls_at_seven_tenths() {
    let prefixes = taught(&[("car", CARGO)]);
    let extended = prefixes.strength(typed("cargo"), CARGO, NOON);
    assert!((extended - 0.5 * PREFIX_EXTENDED).abs() < CLOSE, "{extended}");
    assert_eq!(prefixes.strength(typed("cat"), CARGO, NOON), 0.0);
}

#[test]
fn same_text_shares_strength() {
    let prefixes = taught(&[("ca", CARGO), ("ca", CARGO), ("ca", CARGO), ("ca", CARGO), ("cal", CALENDAR)]);
    assert!((prefixes.strength(typed("ca"), CARGO, NOON) - 0.8).abs() < CLOSE);
    assert!((prefixes.strength(typed("ca"), CALENDAR, NOON) - 0.2 * 0.5).abs() < CLOSE);
    assert!((prefixes.strength(typed("cal"), CALENDAR, NOON) - 0.2 * 0.5).abs() < CLOSE);
}

#[test]
fn memory_fades_with_half_life() {
    let prefixes = taught(&[("car", CARGO), ("car", CARGO), ("car", CARGO), ("car", CARGO)]);
    assert_eq!(prefixes.strength(typed("car"), CARGO, NOON), 1.0);
    let later = prefixes.strength(typed("car"), CARGO, NOON + 2 * PREFIX_HALF_LIFE);
    assert!((later - 0.5).abs() < CLOSE, "{later}");
}

#[test]
fn answer_follows_clock_and_picks() {
    let mut prefixes = taught(&[("car", CARGO)]);
    let first = prefixes.strength(typed("car"), CARGO, NOON);
    assert!(prefixes.strength(typed("car"), CARGO, NOON + 30 * DAY) < first);
    prefixes.learn(typed("car"), CALENDAR, 1.0, NOON);
    assert!(prefixes.strength(typed("car"), CARGO, NOON) < first);
}

#[test]
fn passing_over_halves_memory() {
    let mut prefixes = taught(&[("car", CARGO), ("car", CARGO), ("x", Key(9))]);
    prefixes.pass_over(typed("ca"), CALENDAR, NOON);
    assert_eq!(prefixes.rows().iter().map(|row| row.weight).collect::<Vec<_>>(), [1.0, 1.0]);
    prefixes.pass_over(typed("ca"), CARGO, NOON);
    assert_eq!(prefixes.rows().iter().map(|row| row.weight).collect::<Vec<_>>(), [1.0, 1.0]);
}

#[test]
fn weakest_memory_makes_room() {
    let mut prefixes = Prefixes::default();
    for serial in 0..PREFIXES as u64 {
        prefixes.learn(typed(&format!("t{serial}")), Key(serial), 2.0, NOON);
    }
    prefixes.learn(typed("t7"), Key(7), -1.5, NOON);
    prefixes.learn(typed("new"), Key(99_999), 0.1, NOON);
    assert_eq!(prefixes.rows().len(), PREFIXES);
    assert!(prefixes.rows().iter().any(|row| row.key == Key(99_999)));
    assert!(prefixes.rows().iter().all(|row| row.key != Key(7)));
}

#[test]
fn forget_and_rename_follow_file() {
    let mut prefixes = taught(&[("car", CARGO), ("cal", CALENDAR)]);
    prefixes.rekey(|key| (key == CARGO).then_some(Key(5)));
    assert!(prefixes.strength(typed("car"), Key(5), NOON) > 0.0);
    prefixes.forget(|key| key == Key(5));
    assert_eq!(prefixes.rows().iter().map(|row| row.key).collect::<Vec<_>>(), [CALENDAR]);
    assert_eq!(prefixes.strength(typed("car"), Key(5), NOON), 0.0);
}
