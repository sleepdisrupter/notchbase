use egui::Event;

use crate::bridge::Action;
use crate::notch::Notch;

use super::super::fixtures::{Rig, shortcuts};
use super::{found, open, words};

#[cfg(target_os = "macos")]
const PLACEHOLDER: &str = "Search this Mac";
#[cfg(not(target_os = "macos"))]
const PLACEHOLDER: &str = "Search this PC";

fn invites(rig: &Rig, n: &mut Notch) -> bool {
    words(rig, n).iter().any(|word| word == PLACEHOLDER)
}

#[test]
fn empty_field_hint_survives_space() {
    let rig = Rig::new();
    let mut n = open(&rig);
    found(&rig, &mut n);
    assert!(invites(&rig, &mut n));
    let keys = shortcuts(None);
    assert_eq!(rig.frame(&mut n, &keys, vec![Event::Text(" ".into())]), [], "a space alone asks nothing");
    assert!(invites(&rig, &mut n), "and the field is empty in the next frame");
    assert_eq!(rig.frame(&mut n, &keys, vec![Event::Text("c".into())]), [Action::FindQuery("c".into())]);
    assert!(!invites(&rig, &mut n));
}
