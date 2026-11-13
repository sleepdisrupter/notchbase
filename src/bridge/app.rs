//! The app's own rows for the panel: installing, updates and notifications.

use super::Action;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct AppView {
    pub installed: bool,
    pub can_install: bool,
    pub open_at_login: bool,
    pub updates: UpdateView,
    pub notifications: Vec<NotificationView>,
    pub unread: usize,
    pub news: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct NotificationView {
    pub key: String,
    pub kind: NotificationKind,
    pub title: String,
    pub detail: String,
    pub progress: Option<f32>,
    pub action: Option<(String, Action)>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NotificationKind {
    Update,
    Permission,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct UpdateView {
    pub on: bool,
    pub available: bool,
    pub detail: String,
}
