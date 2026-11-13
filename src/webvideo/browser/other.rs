//! Other systems have no way to read the browser in front.

use super::Browser;

pub(super) fn frontmost() -> Option<Browser> {
    None
}

pub(super) fn addresses(_: &Browser, _: &dyn Fn(&str) -> bool) -> Vec<String> {
    Vec::new()
}
