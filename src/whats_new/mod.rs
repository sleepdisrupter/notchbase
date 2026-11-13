//! What changed in each release, built into the app so the notes work offline and always match the copy.

mod notes;

use crate::ui::glyph::Glyph;
use crate::ui::picture::Picture;
use crate::update::{Version, current_version};

/// The width a release's picture is drawn at on the page.
pub const PICTURE_WIDTH: f32 = 340.0;

/// One version's notes: a few changes and at most one picture.
pub struct Release {
    pub version: &'static str,
    pub picture: Option<&'static Shot>,
    pub changes: &'static [Change],
}

/// One change: a glyph, a title and the line under it.
pub struct Change {
    pub glyph: Glyph,
    pub title: &'static str,
    pub caption: &'static str,
}

/// A picture in two versions, for an island and for a notch beside a camera.
pub struct Shot {
    pub island: Picture,
    pub notch: Picture,
}

impl Release {
    fn number(&self) -> Option<Version> {
        Version::parse(self.version)
    }

    /// An empty or unreadable `seen` counts as never seen.
    fn is_newer_than(&self, seen: &str) -> bool {
        self.number() > Some(Version::parse(seen).unwrap_or_default())
    }
}

impl Shot {
    pub fn beside(&self, camera: bool) -> &Picture {
        if camera { &self.notch } else { &self.island }
    }
}

/// The newest notes that aren't newer than the running version.
pub fn latest() -> Option<&'static Release> {
    newest(notes::NOTES, current_version())
}

/// Whether the latest notes are newer than the version whose notes were last seen.
pub fn unseen(seen: &str) -> bool {
    latest().is_some_and(|release| release.is_newer_than(seen))
}

fn newest(notes: &'static [Release], running: Version) -> Option<&'static Release> {
    let shipped = notes.iter().filter(|release| release.number().is_some_and(|number| number <= running));
    shipped.max_by_key(|release| release.number())
}

#[cfg(test)]
mod tests;
