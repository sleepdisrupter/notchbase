//! The right-click menu on a result or a pin.

use crate::bridge::FindRow;
use crate::ui::glyph::Glyph;
use crate::ui::menu::Item;

const PIN: &str = "Pin";
const UNPIN: &str = "Unpin";
const FULL: &str = "Four are pinned";
#[cfg(target_os = "macos")]
const REVEAL: &str = "Show in Finder";
#[cfg(not(target_os = "macos"))]
const REVEAL: &str = "Show in Explorer";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Chose {
    Pin,
    Unpin,
    Reveal,
}

pub(super) fn items(row: &FindRow, pinned: bool, full: bool) -> Vec<Item<Chose>> {
    let pin = match (pinned, full) {
        (true, _) => Item { words: UNPIN, glyph: Glyph::Unpin, choice: Some(Chose::Unpin) },
        (false, true) => Item { words: FULL, glyph: Glyph::Pin, choice: None },
        (false, false) => Item { words: PIN, glyph: Glyph::Pin, choice: Some(Chose::Pin) },
    };
    let reveal = Item { words: REVEAL, glyph: Glyph::Folder, choice: Some(Chose::Reveal) };
    std::iter::once(pin).chain((!row.pane()).then_some(reveal)).collect()
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::sync::Arc;

    use super::*;
    use crate::bridge::Kind;

    fn row(path: &str, app: bool) -> FindRow {
        FindRow {
            key: 1,
            path: Arc::from(Path::new(path)),
            name: "x".into(),
            place: "~".into(),
            marks: Vec::new(),
            kind: Kind::Other,
            app,
            folder: false,
            cloud: false,
            modified: 0,
            why: None,
            predicted: false,
            icon: None,
        }
    }

    #[test]
    fn menu_offers_pin_or_unpin() {
        let offered = |row: &FindRow, pinned, full| {
            items(row, pinned, full).into_iter().map(|item| item.choice).collect::<Vec<_>>()
        };
        let file = row("/a.txt", false);
        assert_eq!(offered(&file, false, false), [Some(Chose::Pin), Some(Chose::Reveal)]);
        assert_eq!(offered(&file, true, true), [Some(Chose::Unpin), Some(Chose::Reveal)]);
        let full = items(&file, false, true);
        assert_eq!((full[0].words, full[0].choice), (FULL, None), "nothing is pushed out to make room");
        let pane = row("/System/Library/ExtensionKit/Extensions/Bluetooth.appex", true);
        assert_eq!(offered(&pane, false, false), [Some(Chose::Pin)], "a settings pane has no file to show");
        let marks = |pinned| items(&file, pinned, false).into_iter().map(|item| item.glyph).collect::<Vec<_>>();
        assert_eq!((marks(false), marks(true)), (vec![Glyph::Pin, Glyph::Folder], vec![Glyph::Unpin, Glyph::Folder]));
    }
}
