//! Other systems: no key, no microphone. Lets the tool build and its tests run on Linux.

use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use super::{Cue, Hotkey, KeySink, Platform, Problem, Recording, Surroundings, Tape, Typed};

#[derive(Default)]
pub struct Native;

impl Platform for Native {
    fn watch(&mut self, _hotkey: Hotkey, _keys: KeySink, _listening: Arc<AtomicBool>) -> Result<(), Problem> {
        Err(unsupported())
    }

    fn hotkey_down(&self, _hotkey: Hotkey) -> Option<bool> {
        None
    }

    fn listen(&mut self, _tape: Tape) -> Result<Box<dyn Recording>, String> {
        Err("Dictation needs macOS or Windows".into())
    }

    fn surroundings(&self, _read_text: bool) -> Surroundings {
        Surroundings::default()
    }

    fn type_text(&self, _text: &str, _reread: bool) -> Result<Typed, String> {
        Err("Dictation needs macOS or Windows".into())
    }

    fn cue(&self, _cue: Cue) {}

    fn problem(&self, _hotkey: Hotkey) -> Option<Problem> {
        Some(unsupported())
    }
}

fn unsupported() -> Problem {
    Problem { says: "Dictation needs macOS or Windows".into(), fix: None, button: "", blocks: true }
}
