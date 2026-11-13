//! Other systems have no summon key.

use super::Pressed;

pub struct KeyTap;

impl KeyTap {
    pub fn install(_pressed: Pressed) -> Result<Self, String> {
        Err("no system search key on this system".into())
    }
}
