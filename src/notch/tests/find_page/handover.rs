use crate::bridge::Action;

use super::super::fixtures::Rig;
use super::opened;

#[test]
fn gone_display_gives_keyboard_back() {
    let rig = Rig::new();
    for mut n in opened(&rig) {
        assert_eq!(n.retire(), [Action::Typing(false), Action::FindShown(false)]);
        assert!(n.retire().is_empty());
    }
}
