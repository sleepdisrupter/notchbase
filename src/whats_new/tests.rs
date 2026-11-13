use super::notes::NOTES;
use super::*;
use crate::ui::picture::LARGEST_SIDE;

const RUNNING: &str = env!("CARGO_PKG_VERSION");
const SECOND_CLAUSE: &str = ", and ";

static SAMPLE: &[Release] = &[
    Release { version: "2.0.0", picture: None, changes: &[] },
    Release { version: "soon", picture: None, changes: &[] },
    Release { version: "1.5.0", picture: None, changes: &[] },
    Release { version: "1.4.2", picture: None, changes: &[] },
];

fn numbers() -> Vec<Version> {
    NOTES.iter().map(|release| release.number().expect("a version that parses")).collect()
}

fn changes() -> impl Iterator<Item = &'static Change> {
    NOTES.iter().flat_map(|release| release.changes)
}

fn at(version: &str) -> Version {
    Version::parse(version).expect("a version")
}

#[test]
fn newest_notes_arent_from_the_future() {
    assert!(numbers().into_iter().max().is_some_and(|newest| newest <= crate::update::current_version()));
    assert!(latest().is_some_and(|release| release.version == NOTES[0].version));
}

#[test]
fn notes_are_newest_first_and_nonempty() {
    assert!(numbers().windows(2).all(|pair| pair[0] > pair[1]));
    assert!(NOTES.iter().all(|release| !release.changes.is_empty()));
}

#[test]
fn latest_is_newest_at_or_below_running() {
    let shown = |running: &str| newest(SAMPLE, at(running)).map(|release| release.version);
    assert_eq!(shown("1.5.0"), Some("1.5.0"));
    assert_eq!(shown("1.9.3"), Some("1.5.0"));
    assert_eq!(shown("3.0.0"), Some("2.0.0"));
    assert_eq!(shown("1.4.9"), Some("1.4.2"));
    assert_eq!(shown("1.0.0"), None);
}

#[test]
fn unseen_until_their_version_is_seen() {
    assert!(unseen(""));
    assert!(unseen("0.13.0"));
    assert!(unseen("nonsense"));
    assert!(!unseen(RUNNING));
    assert!(!unseen("999.0.0"));
}

#[test]
fn newer_only_than_an_older_marker() {
    let release = &SAMPLE[2];
    assert!(release.is_newer_than("") && release.is_newer_than("1.4.2"));
    assert!(!release.is_newer_than("1.5.0") && !release.is_newer_than("2.0.0"));
    assert!(!SAMPLE[1].is_newer_than(""));
}

#[test]
fn every_line_is_one_plain_clause() {
    for change in changes() {
        assert!(!change.title.is_empty() && !change.caption.is_empty());
        for words in [change.title, change.caption] {
            assert!(!words.ends_with('.') && !words.ends_with('!') && words.trim() == words, "{words:?}");
            assert!(!words.contains(SECOND_CLAUSE), "{words:?} says two things: give the second its own change");
        }
    }
}

#[test]
fn pictures_decode_at_page_width() {
    for shot in NOTES.iter().filter_map(|release| release.picture) {
        for camera in [false, true] {
            let picture = shot.beside(camera);
            let image = picture.decode().expect("a png that decodes");
            assert!(image.size.iter().all(|side| *side <= LARGEST_SIDE as usize), "{:?}", image.size);
            assert_eq!(picture.size().x, PICTURE_WIDTH, "{:?}", picture.size());
        }
    }
}

#[test]
fn shot_has_a_picture_per_style() {
    let shot = Shot { island: Picture::sample(680, 200), notch: Picture::sample(686, 240) };
    assert_eq!(shot.beside(false).size().y, 100.0);
    assert_eq!(shot.beside(true).size().y, 120.0);
}
