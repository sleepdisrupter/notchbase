//! The gear's Notifications page: an update to install or a missing permission, each with its button.

use std::collections::HashMap;

use egui::{Align2, Color32, Id, Rect, Response, Sense, pos2, vec2};

use super::PageCtx;
use super::settings::{BACK_H, back};
use crate::bridge::{Action, NotificationKind, NotificationView, PanelData};
use crate::ui::canvas::fade;
use crate::ui::color::{self, label, secondary_label, tertiary_label, white};
use crate::ui::glass::{self, Glass};
use crate::ui::glyph::{self, Glyph};
use crate::ui::metrics::{CARD_RADIUS, GAP};
use crate::ui::rows::{self, Rows};
use crate::ui::spring::{Spring, UNIT_PRECISION};
use crate::ui::{controls, font, motion};

const ROW_H: f32 = 58.0;
const EMPTY_H: f32 = 76.0;
const INSET: f32 = 14.0;
const BADGE: f32 = 15.0;
const BADGE_GLYPH: f32 = 13.0;
const TEXT_GAP: f32 = 12.0;
const LINE_OFFSET: f32 = 8.0;
const BUTTON_H: f32 = 26.0;
const BUTTON_PAD: f32 = 12.0;
const BAR_H: f32 = 3.0;
const BAR_BELOW: f32 = 8.0;
const LIVE_FROM: f32 = 0.5;
const BUTTON_SECS: f32 = 0.2;
const NOTHING: &str = "No notifications";
const NOTHING_DETAIL: &str = "Updates and alerts show up here";
const SETTINGS: &str = "Settings";

#[derive(Default)]
pub struct NotificationsPage {
    back: bool,
    rows: Rows<String, NotificationView>,
    progress: HashMap<String, Spring>,
}

impl NotificationsPage {
    pub fn height(&self, data: &PanelData) -> f32 {
        if self.rows.is_empty() {
            return height(data.app.notifications.len());
        }
        BACK_H + GAP + self.list_height().max(EMPTY_H * (1.0 - self.rows.most_shown()))
    }

    fn list_height(&self) -> f32 {
        let pitches = self.rows.iter().enumerate();
        pitches.map(|(index, (_, _, shown))| shown * (ROW_H + if index > 0 { GAP } else { 0.0 })).sum()
    }

    pub fn is_animating(&self) -> bool {
        self.rows.is_animating() || self.progress.values().any(|spring| !spring.is_settled())
    }

    pub fn take_back(&mut self) -> bool {
        std::mem::take(&mut self.back)
    }

    pub fn ui(&mut self, ctx: &mut PageCtx<'_>, data: &PanelData) -> Vec<Action> {
        let live = ctx.alpha > LIVE_FROM;
        self.back |= back(ctx, ctx.rect.top(), SETTINGS, live);
        let first = ctx.rect.top() + BACK_H + GAP;
        let notifications = &data.app.notifications;
        let seen = live && !self.rows.is_empty();
        self.rows.follow(notifications.iter().map(|shown| (shown.key.clone(), shown.clone())), seen);
        self.rows.step(ctx.dt);
        self.follow_progress(notifications, ctx.dt);
        empty(ctx, first, 1.0 - self.rows.most_shown());
        let mut actions = Vec::new();
        let mut top = first;
        let laid: Vec<(NotificationView, f32)> =
            self.rows.iter().map(|(_, notification, shown)| (notification.clone(), shown)).collect();
        for (index, (notification, shown)) in laid.iter().enumerate() {
            if index > 0 {
                top += GAP * shown;
            }
            let rect = Rect::from_min_size(pos2(ctx.rect.left(), top), vec2(ctx.rect.width(), ROW_H));
            let progress = self.progress.get(&notification.key).map(|spring| spring.value);
            let mark = ctx.canvas.mark();
            actions.extend(row(ctx, rect, (notification, progress), live && *shown > 0.99));
            rows::reveal(ctx.canvas, mark, Rect::from_min_size(rect.min, vec2(rect.width(), ROW_H * shown)), *shown);
            top += ROW_H * shown;
        }
        actions
    }

    fn follow_progress(&mut self, notifications: &[NotificationView], dt: f32) {
        self.progress.retain(|key, _| notifications.iter().any(|shown| &shown.key == key && shown.progress.is_some()));
        for notification in notifications {
            let Some(progress) = notification.progress else { continue };
            let spring = self
                .progress
                .entry(notification.key.clone())
                .or_insert_with(|| Spring::new(progress, motion::VALUE, UNIT_PRECISION));
            spring.animate_to(progress, motion::VALUE);
            spring.step(dt);
        }
    }
}

fn height(count: usize) -> f32 {
    BACK_H
        + GAP
        + match count {
            0 => EMPTY_H,
            count => count as f32 * ROW_H + (count - 1) as f32 * GAP,
        }
}

fn empty(ctx: &mut PageCtx<'_>, top: f32, shown: f32) {
    if shown <= 0.01 {
        return;
    }
    let mark = ctx.canvas.mark();
    let center = pos2(ctx.rect.center().x, top + EMPTY_H * 0.5);
    ctx.canvas.text(center - vec2(0.0, LINE_OFFSET), Align2::CENTER_CENTER, NOTHING, font::body(), secondary_label());
    let below = center + vec2(0.0, LINE_OFFSET + 2.0);
    ctx.canvas.text(below, Align2::CENTER_CENTER, NOTHING_DETAIL, font::caption(), tertiary_label());
    for shape in ctx.canvas.since(mark) {
        fade(shape, shown);
    }
}

fn row(
    ctx: &mut PageCtx<'_>,
    rect: Rect,
    (notification, progress): (&NotificationView, Option<f32>),
    live: bool,
) -> Option<Action> {
    glass::paint(ctx.canvas, rect, CARD_RADIUS, Glass::TILE);
    let badge = pos2(rect.left() + INSET + BADGE, rect.center().y);
    let (tint, mark) = look(notification.kind);
    ctx.canvas.circle(badge, BADGE, tint);
    glyph::paint(ctx.canvas, mark, badge, BADGE_GLYPH, color::ink_on(tint));
    let left = badge.x + BADGE + TEXT_GAP;
    let mut right = rect.right() - INSET;
    let mut pressed = None;
    let id = Id::new(("notchbase.notification", notification.key.as_str()));
    let offered = ctx.ui.ctx().animate_bool_with_time(id.with("offered"), notification.action.is_some(), BUTTON_SECS);
    if let Some((title, _)) = &notification.action {
        ctx.ui.data_mut(|data| data.insert_temp(id.with("title"), title.clone()));
    }
    let shown_title: Option<String> = notification
        .action
        .as_ref()
        .map(|(title, _)| title.clone())
        .or_else(|| ctx.ui.data(|data| data.get_temp(id.with("title"))));
    if let Some(title) = shown_title.filter(|_| offered > 0.01) {
        let width = ctx.canvas.text_width(title.as_str(), font::callout()) + 2.0 * BUTTON_PAD;
        let button = Rect::from_center_size(pos2(right - width * 0.5, rect.center().y), vec2(width, BUTTON_H));
        let mark = ctx.canvas.mark();
        let live = live && notification.action.is_some() && offered > LIVE_FROM;
        if button_ui(ctx, button, id, &title, live) {
            pressed = notification.action.as_ref().map(|(_, action)| action.clone());
        }
        for shape in ctx.canvas.since(mark) {
            fade(shape, offered);
        }
        right = crate::ui::spring::lerp(right, button.left() - TEXT_GAP, offered);
    }
    let room = (right - left).max(0.0);
    let title = controls::truncate(ctx.canvas, &notification.title, font::body(), room);
    let detail = controls::truncate(ctx.canvas, &notification.detail, font::caption(), room);
    let y = rect.center().y;
    ctx.canvas.text(pos2(left, y - LINE_OFFSET), Align2::LEFT_CENTER, title, font::body(), label());
    ctx.canvas.text(pos2(left, y + LINE_OFFSET), Align2::LEFT_CENTER, detail, font::caption(), secondary_label());
    if let Some(progress) = progress {
        let track = Rect::from_min_size(pos2(left, rect.bottom() - BAR_BELOW - BAR_H), vec2(room, BAR_H));
        ctx.canvas.rect(track, BAR_H * 0.5, white(0.12));
        let fill = Rect::from_min_size(track.min, vec2(track.width() * progress.clamp(0.0, 1.0), BAR_H));
        ctx.canvas.rect(fill, BAR_H * 0.5, white(0.9));
    }
    pressed
}

fn button_ui(ctx: &mut PageCtx<'_>, rect: Rect, id: Id, title: &str, live: bool) -> bool {
    let response = live.then(|| ctx.ui.interact(rect, id, Sense::click()));
    let hover = controls::animate(ctx.ui, id.with("hover"), response.as_ref().is_some_and(Response::hovered));
    ctx.canvas.rect(rect, rect.height() * 0.5, color::mix(color::ACCENT, Color32::WHITE, 0.12 * hover));
    ctx.canvas.text(rect.center(), Align2::CENTER_CENTER, title, font::callout(), color::ink_on(color::ACCENT));
    response.is_some_and(|response| response.clicked())
}

fn look(kind: NotificationKind) -> (Color32, Glyph) {
    match kind {
        NotificationKind::Update => (color::ACCENT, Glyph::ChevronUp),
        NotificationKind::Permission => (color::PURPLE, Glyph::Moon),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn update(progress: Option<f32>) -> NotificationView {
        NotificationView {
            key: "update".into(),
            kind: NotificationKind::Update,
            title: "notchbase 9.9".into(),
            detail: "Downloading".into(),
            progress,
            action: None,
        }
    }

    #[test]
    fn new_notification_grows_in() {
        let mut page = NotificationsPage::default();
        page.rows.follow([("first".to_owned(), update(None))], false);
        assert_eq!(page.list_height(), ROW_H);
        page.rows.follow([("first".to_owned(), update(None)), ("update".to_owned(), update(Some(0.1)))], true);
        page.rows.step(1.0 / 60.0);
        let growing = page.list_height();
        assert!(growing > ROW_H && growing < 2.0 * ROW_H + GAP, "{growing}");
        page.follow_progress(&[update(Some(0.1))], 1.0 / 60.0);
        page.follow_progress(&[update(Some(0.6))], 1.0 / 60.0);
        let bar = page.progress["update"].value;
        assert!(bar > 0.1 && bar < 0.6, "the bar glides instead of stepping: {bar}");
        assert!(page.is_animating());
    }

    #[test]
    fn grows_a_row_per_notification() {
        let back = BACK_H + GAP;
        assert_eq!(height(0), back + EMPTY_H);
        assert_eq!(height(1), back + ROW_H);
        assert_eq!(height(2), back + 2.0 * ROW_H + GAP);
    }
}
