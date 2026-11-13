//! What a key or a toolbar control asks the editor to do.

use super::tool::Tool;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Command {
    Pick(usize),
    Resize(f32),
    Settle,
    Nudge(f32),
    Use(Tool),
    Undo,
    Redo,
    Cancel,
    Finish,
    Close,
}
