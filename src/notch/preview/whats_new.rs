//! The What's New page and Updated notice previews.

use crate::bridge::Page;
use crate::notch::Notch;
use crate::notch::notice::whats_new::updated;
use crate::update::current_version;

use super::Preview;

pub(super) fn parse(name: &str) -> Option<Preview> {
    Some(match name {
        "whats-new" | "changes" => Preview::WhatsNew,
        "updated" => Preview::Updated,
        _ => return None,
    })
}

pub(super) fn page(preview: Preview) -> Option<Page> {
    (preview == Preview::WhatsNew).then_some(Page::WhatsNew)
}

pub(super) fn pin(n: &mut Notch, preview: Preview) {
    match preview {
        Preview::WhatsNew => n.pin_page(Page::WhatsNew),
        Preview::Updated => n.pin_notice(updated(&current_version().to_string())),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use egui::vec2;

    use super::*;
    use crate::bridge::{Action, Tab};
    use crate::notch::Mode;
    use crate::notch::activity::ActivityKind;
    use crate::notch::tests::fixtures::{Rig, shortcuts};
    use crate::platform::Style;

    const NAMES: [&str; 3] = ["whats-new", "changes", " Whats-New "];

    #[test]
    fn whats_new_preview_asks_nothing() {
        for name in NAMES {
            assert_eq!(Preview::parse(name), Some(Preview::WhatsNew), "{name}");
        }
        assert_eq!(Preview::WhatsNew.page(), Some(Page::WhatsNew));
        for style in [Style::Island, Style::Notch] {
            let rig = Rig::new();
            let mut n = Notch::with_preview(style, Some(vec2(185.0, 32.0)), Some(Preview::WhatsNew));
            assert_eq!((n.mode, n.pager.page), (Mode::Expanded, Page::WhatsNew));
            let keys = shortcuts(None);
            let actions: Vec<Action> = (0..3).flat_map(|_| rig.frame(&mut n, &keys, Vec::new())).collect();
            assert!(actions.is_empty(), "{actions:?}");
            assert_eq!(n.visible_page(), Some(Page::WhatsNew), "a preview stays on the page");
            let mut tabs = Tab::defaults();
            tabs.pop();
            n.sync_tabs(&tabs);
            assert_eq!(n.pager.page, Page::WhatsNew, "and outlives a tab change");
        }
    }

    #[test]
    fn updated_preview_pins_posted_notice() {
        assert_eq!(Preview::parse(" Updated "), Some(Preview::Updated));
        assert_eq!(Preview::Updated.page(), None);
        let posted = updated(&current_version().to_string());
        for style in [Style::Island, Style::Notch] {
            let rig = Rig::new();
            let mut n = Notch::with_preview(style, Some(vec2(185.0, 32.0)), Some(Preview::Updated));
            assert_eq!(n.mode, Mode::Activity);
            let keys = shortcuts(None);
            let actions: Vec<Action> = (0..40).flat_map(|_| rig.frame(&mut n, &keys, Vec::new())).collect();
            assert!(actions.is_empty(), "{actions:?}");
            let Some(ActivityKind::Tool(notice)) = &n.activity.kind else { panic!("a pinned notice") };
            assert_eq!(*notice.node, posted, "the preview cannot drift from the real notice");
            assert!(notice.widths.is_some() && notice.controls.x > 0.0, "measured, button and all");
            assert_eq!((n.mode, n.visible_page()), (Mode::Activity, None));
        }
    }

    #[test]
    fn other_previews_are_not_this_page() {
        assert_eq!(page(Preview::Settings), None);
        assert_eq!(parse("update"), None, "that name is the Notifications preview");
        assert_eq!(Preview::parse("update"), Some(Preview::Notifications));
    }
}
