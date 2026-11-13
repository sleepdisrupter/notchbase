//! The panel's notifications, and which ones have been read.

use std::collections::HashSet;

use crate::bridge::{NotificationView, Page};

use super::App;

#[derive(Default)]
pub(super) struct Seen(HashSet<String>);

impl Seen {
    fn unread(&self, notifications: &[NotificationView]) -> usize {
        notifications.iter().filter(|notification| !self.0.contains(&notification.key)).count()
    }
}

impl App {
    pub(super) fn notifications(&self) -> (Vec<NotificationView>, usize) {
        if let Some(sample) = self.notches.iter().find_map(|nw| nw.notch.previewed_notifications()) {
            let unread = sample.len();
            return (sample, unread);
        }
        let list: Vec<NotificationView> = self
            .update_notification()
            .into_iter()
            .chain(self.permission_notification())
            .chain(self.find_notification())
            .collect();
        let unread = self.seen.unread(&list);
        (list, unread)
    }

    pub(super) fn follow_notifications(&mut self) {
        if self.visible_page() != Some(Page::Notifications) {
            return;
        }
        let (list, unread) = self.notifications();
        if unread > 0 {
            self.seen.0.extend(list.into_iter().map(|notification| notification.key));
            self.redraw_notch();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bridge::NotificationKind;

    fn note(key: &str) -> NotificationView {
        let (title, detail) = (String::new(), String::new());
        NotificationView {
            key: key.into(),
            kind: NotificationKind::Update,
            title,
            detail,
            progress: None,
            action: None,
        }
    }

    #[test]
    fn unread_until_seen() {
        let mut seen = Seen::default();
        assert_eq!(seen.unread(&[]), 0);
        assert_eq!(seen.unread(&[note("update-0.3.0")]), 1);
        seen.0.insert("update-0.3.0".into());
        assert_eq!(seen.unread(&[note("update-0.3.0")]), 0);
        assert_eq!(seen.unread(&[note("update-0.3.0"), note("update-0.4.0")]), 1);
    }
}
