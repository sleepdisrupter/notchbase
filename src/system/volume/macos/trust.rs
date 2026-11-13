//! The keys need Accessibility, so this asks for it once when asking is allowed.

use crate::system::permissions::macos::{accessibility, prompt_accessibility_once};
use crate::system::permissions::{Access, may_ask};

pub fn may_watch_keys() -> bool {
    if accessibility::trusted() {
        return true;
    }
    if may_ask(Access::Accessibility) {
        prompt_accessibility_once();
    }
    false
}
