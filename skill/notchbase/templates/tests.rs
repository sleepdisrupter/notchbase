use notchbase_sdk::protocol::Request;
use notchbase_sdk::testing::Harness;

use super::*;

const FIELD: &str = "Add an item";

#[test]
fn typing_and_adding_saves_an_item_and_confirms_it() {
    let mut tool = Harness::<__TYPE__>::start();
    tool.type_text(FIELD, "  first  ");
    let requests = tool.click("Add").to_vec();
    assert_eq!(tool.model().items, ["first"]);
    assert!(tool.shows("first"));
    assert!(matches!(&requests[0], Request::SaveSetting { key, .. } if key == ITEMS));
    assert!(matches!(requests[1], Request::Notice { .. }));
}

#[test]
fn a_blank_item_is_ignored() {
    let mut tool = Harness::<__TYPE__>::start();
    tool.type_text(FIELD, "   ");
    assert!(tool.submit(FIELD).is_empty());
    assert!(tool.shows("Nothing yet"));
}

#[test]
fn removing_an_item() {
    let mut tool = Harness::<__TYPE__>::start();
    tool.type_text(FIELD, "gone soon");
    tool.submit(FIELD);
    tool.click_icon(Icon::Xmark, 0);
    assert!(tool.model().items.is_empty());
}

#[test]
fn the_setting_adds_a_chime() {
    let mut tool = Harness::<__TYPE__>::start_with([(LOUD, true)]);
    tool.type_text(FIELD, "ding");
    assert!(tool.click("Add").contains(&Request::Chime));
}

#[test]
fn the_shortcut_and_tile_open_the_page() {
    let mut tool = Harness::<__TYPE__>::start();
    assert_eq!(tool.shortcut(OPEN), [Request::OpenPage]);
    assert_eq!(tool.press_tile(), [Request::OpenPage]);
}
