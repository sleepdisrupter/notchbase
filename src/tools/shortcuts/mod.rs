//! Tool shortcuts as the Settings page shows them, and `keys` for turning a key press into a hotkey.

pub mod keys;

use crate::bridge::ToolSlot;

/// A tool's shortcut, by its slot and its index in the manifest.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ShortcutRef {
    pub tool: ToolSlot,
    pub index: u16,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ShortcutView {
    pub which: ShortcutRef,
    pub id: String,
    pub title: String,
    pub keys: Vec<String>,
    pub recording: bool,
    pub problem: Option<String>,
}

pub fn find<'a>(shortcuts: &'a [ShortcutView], tool: ToolSlot, id: &str) -> Option<&'a ShortcutView> {
    shortcuts.iter().find(|shortcut| shortcut.which.tool == tool && shortcut.id == id)
}
