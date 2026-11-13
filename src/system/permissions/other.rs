//! Other systems have no permission prompts, so everything counts as granted.

use super::{Access, Standing};

pub const SUPPORTED: bool = false;

pub fn standing(_access: Access) -> Option<Standing> {
    Some(Standing::Granted)
}

pub fn ask(_access: Access) {}
