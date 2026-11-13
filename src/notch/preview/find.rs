//! The Find page previews and their sample results.

use crate::bridge::{FindView, Page};
use crate::notch::Notch;
use crate::pages::find::samples;

use super::Preview;

const ACTIONS_ROW: usize = 1;

pub(super) fn parse(name: &str) -> Option<Preview> {
    Some(match name {
        "find" | "find-suggestions" => Preview::Find,
        "find-results" => Preview::FindResults,
        "find-actions" | "find-selected" => Preview::FindActions,
        "find-none" | "find-nothing" => Preview::FindNone,
        "find-indexing" | "find-starting" => Preview::FindIndexing,
        "find-access" => Preview::FindAccess,
        "find-off" => Preview::FindOff,
        _ => return None,
    })
}

pub(super) fn page(preview: Preview) -> Option<Page> {
    sample(preview).map(|_| Page::Find)
}

pub(super) fn pin(n: &mut Notch, preview: Preview) {
    let Some(view) = sample(preview) else { return };
    let selected = (preview == Preview::FindActions).then_some(ACTIONS_ROW);
    n.pin_page(Page::Find);
    n.pages.pin_find(view, selected);
}

fn sample(preview: Preview) -> Option<FindView> {
    Some(match preview {
        Preview::Find => samples::suggestions(),
        Preview::FindResults | Preview::FindActions => samples::results(),
        Preview::FindNone => samples::none(),
        Preview::FindIndexing => samples::indexing(),
        Preview::FindAccess => samples::access(),
        Preview::FindOff => samples::off(),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bridge::{Action, FindState};
    use crate::notch::Mode;
    use crate::notch::tests::fixtures::{Rig, shortcuts};
    use crate::platform::Style;

    const NAMES: [(&str, Preview); 7] = [
        ("find", Preview::Find),
        ("find-results", Preview::FindResults),
        ("find-actions", Preview::FindActions),
        ("find-none", Preview::FindNone),
        ("find-indexing", Preview::FindIndexing),
        ("find-access", Preview::FindAccess),
        ("find-off", Preview::FindOff),
    ];

    #[test]
    fn find_previews_leave_keyboard() {
        for (name, preview) in NAMES {
            assert_eq!(Preview::parse(name), Some(preview));
            assert_eq!(preview.page(), Some(Page::Find));
            let rig = Rig::new();
            let mut n = Notch::with_preview(Style::Island, None, Some(preview));
            assert_eq!((n.mode, n.pager.page), (Mode::Expanded, Page::Find));
            let keys = shortcuts(None);
            let actions: Vec<Action> = (0..3).flat_map(|_| rig.frame(&mut n, &keys, Vec::new())).collect();
            assert!(actions.is_empty(), "{name}: {actions:?}");
            assert_eq!(n.pager.page, Page::Find, "{name}: a preview stays on Find");
            assert!(!n.pages.find.armed());
        }
        assert_eq!(page(Preview::Settings), None);
        assert_eq!(sample(Preview::FindOff).map(|view| view.state), Some(FindState::Off));
    }
}
