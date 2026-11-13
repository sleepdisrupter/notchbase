use notchbase_sdk::protocol::Request;
use notchbase_sdk::testing::Harness;

use super::*;

const FIELD: &str = "Write a note";

fn with_notes(notes: &[&str]) -> Harness<Notes> {
    let mut notes_tool = Harness::<Notes>::start();
    for note in notes.iter().rev() {
        notes_tool.type_text(FIELD, note);
        notes_tool.click("Add");
    }
    notes_tool
}

fn scratch(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("notchbase-notes-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

#[test]
fn enter_adds_note_on_top() {
    let mut notes = with_notes(&["older"]);
    notes.type_text(FIELD, "  buy oat milk  ");
    notes.submit(FIELD);
    assert_eq!(notes.model().notes, ["buy oat milk", "older"]);
    assert_eq!(notes.saved(NOTES), Some(&Value::from(vec!["buy oat milk", "older"])));
    assert_eq!(notes.model().draft, "", "the field clears");
    assert!(notes.shows("buy oat milk"));
}

#[test]
fn blank_note_is_ignored() {
    let mut notes = with_notes(&[]);
    notes.type_text(FIELD, "   ");
    assert!(notes.submit(FIELD).is_empty());
    assert!(notes.model().notes.is_empty());
    assert!(notes.shows("Nothing yet"));
}

#[test]
fn pin_follows_its_note() {
    let mut notes = with_notes(&["first", "second"]);
    assert!(notes.live().is_none());
    notes.click_nth("Pin", 1);
    assert_eq!(notes.model().pinned, Some(1));
    assert!(notes.live().is_some());
    notes.type_text(FIELD, "newest");
    notes.click("Add");
    assert_eq!(notes.model().pinned_note().map(String::as_str), Some("second"));
    notes.click_icon(Icon::Xmark, 0);
    assert_eq!(notes.model().pinned, Some(1), "removing a note above keeps the same note pinned");
    notes.click_nth("Pin", 1);
    assert!(notes.live().is_none(), "pinning it again unpins it");
}

#[test]
fn re_adding_keeps_the_same_pin() {
    let mut notes = with_notes(&["a", "b", "c"]);
    notes.click_nth("Pin", 2);
    notes.type_text(FIELD, "a");
    notes.click("Add");
    assert_eq!(notes.model().pinned_note().map(String::as_str), Some("c"));
    notes.type_text(FIELD, "c");
    notes.click("Add");
    assert_eq!(notes.model().notes, ["c", "a", "b"]);
    assert_eq!(notes.model().pinned, Some(0), "the pinned note moves to the top and stays pinned");
    assert_eq!(notes.saved(PINNED), Some(&Value::from(0_usize)));
}

#[test]
fn removing_pinned_clears_pin() {
    let mut notes = with_notes(&["only"]);
    notes.click("Pin");
    notes.click_icon(Icon::Xmark, 0);
    assert_eq!(notes.model().pinned, None);
    assert_eq!(notes.saved(PINNED), Some(&Value::Null));
}

#[test]
fn copy_uses_the_clipboard() {
    let mut notes = with_notes(&["copy me"]);
    let requests = notes.click_icon(Icon::Note, 0);
    assert!(matches!(&requests[0], Request::CopyText { text } if text == "copy me"));
    assert!(matches!(requests[1], Request::Notice { .. }));
}

#[test]
fn shortcut_and_tile_open_page() {
    let mut notes = with_notes(&[]);
    assert_eq!(notes.shortcut(OPEN), [Request::OpenPage]);
    assert_eq!(notes.press_tile(), [Request::OpenPage]);
}

#[test]
fn keep_open_holds_then_releases() {
    let mut notes = with_notes(&[]);
    assert_eq!(notes.set_toggle(0, true), [Request::Hold { held: true }]);
    assert!(notes.model().holding);
    assert_eq!(notes.set_toggle(0, false), [Request::Hold { held: false }]);
}

#[test]
fn page_describes_notch_states() {
    let mut notes = with_notes(&[]);
    let halfway = NotchState {
        phase: NotchPhase::Open,
        style: NotchStyle::Notch,
        expansion: 0.5,
        hug: 0.25,
        width: 300.0,
        height: 120.0,
        ..NotchState::default()
    };
    notes.notch(halfway);
    assert!(notes.shows("Open notch \u{b7} 300\u{d7}120 pt \u{b7} open 50% \u{b7} hugging 25%"));
    notes.change_settings([(prefs::DETAILS, false)]);
    assert!(!notes.shows("hugging"));
}

#[test]
fn limit_keeps_the_newest() {
    let mut notes = Harness::<Notes>::start_with([(prefs::LIMIT, 5)]);
    for n in 0..7 {
        notes.type_text(FIELD, &format!("note {n}"));
        notes.click("Add");
    }
    assert_eq!(notes.model().notes.len(), 5);
    assert_eq!(notes.model().notes[0], "note 6");
}

#[test]
fn file_gets_markdown() {
    let folder = scratch("export");
    let mut notes = with_notes(&["line one\nline two", "second"]);
    notes.change_settings([
        (prefs::FOLDER, Value::from(folder.to_string_lossy().into_owned())),
        (prefs::EXPORT, true.into()),
    ]);
    let file = folder.join(export::FILE);
    notes.wait_for(|_| file.exists());
    let written = std::fs::read_to_string(&file).unwrap();
    let _ = std::fs::remove_dir_all(&folder);
    assert_eq!(written, "# Notes\n\n- line one\n  line two\n- second\n");
}

#[test]
fn folder_shows_while_saving() {
    let manifest = Notes::manifest();
    let folder = manifest.settings.iter().find(|setting| setting.key == prefs::FOLDER).unwrap();
    assert_eq!(folder.shown_when.as_deref(), Some(prefs::EXPORT));
    assert!(matches!(folder.kind, notchbase_sdk::protocol::SettingKind::Folder { .. }));
}

#[test]
fn long_notes_show_first_line() {
    assert_eq!(page::preview("short", 10), "short");
    assert_eq!(page::preview("a much longer note", 6), "a much\u{2026}");
    assert_eq!(page::preview("first\nsecond", 20), "first\u{2026}");
}
