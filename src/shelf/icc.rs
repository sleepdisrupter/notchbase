//! Checks that an ICC profile still describes the pixels: an RGB profile for color, a gray one for gray.

use std::ops::Range;

const HEADER_BYTES: usize = 128;
const COLOR_SPACE_FIELD: Range<usize> = 16..20;

pub(super) fn matches(profile: &[u8], has_color: bool) -> bool {
    let expected: &[u8] = if has_color { b"RGB " } else { b"GRAY" };
    profile.len() >= HEADER_BYTES && profile.get(COLOR_SPACE_FIELD) == Some(expected)
}
