use egui::Id;

use super::{line, list, shot};
use crate::bridge::Action;
use crate::pages::harness::{Harness, PAGE, press, release};
use crate::pages::whats_new::layout::TITLE_SIZE;
use crate::pages::whats_new::{DONE_ID, LIVE_FROM, WhatsNewPage};
use crate::ui::font;

const FADING: f32 = 0.4;

fn draw(harness: &mut Harness, page: &mut WhatsNewPage, alpha: f32, events: Vec<egui::Event>) -> Vec<Action> {
    let release = list(1, None);
    harness.frame(events, |ctx| {
        ctx.alpha = alpha;
        page.ui(ctx, Some(&release), false)
    })
}

fn click_done(harness: &mut Harness, page: &mut WhatsNewPage, alpha: f32) -> Vec<Action> {
    draw(harness, page, alpha, Vec::new());
    let at = harness.rect_of(Id::new(DONE_ID)).center();
    let mut actions = draw(harness, page, alpha, press(at));
    actions.extend(draw(harness, page, alpha, release(at)));
    actions
}

#[test]
fn done_tells_app_once() {
    let (mut harness, mut page) = (Harness::new(), WhatsNewPage::default());
    assert_eq!(click_done(&mut harness, &mut page, 1.0), [Action::ChangesSeen]);
    assert!(page.take_done());
    assert!(!page.take_done());
    assert_eq!(click_done(&mut harness, &mut page, LIVE_FROM), [Action::ChangesSeen], "half way in is live");
}

#[test]
fn fading_page_takes_no_clicks() {
    let (mut harness, mut page) = (Harness::new(), WhatsNewPage::default());
    assert!(click_done(&mut harness, &mut page, FADING).is_empty());
    assert!(!page.take_done());
}

#[test]
fn done_sits_right_of_header() {
    let (mut harness, mut page) = (Harness::new(), WhatsNewPage::default());
    draw(&mut harness, &mut page, 1.0, Vec::new());
    let done = harness.rect_of(Id::new(DONE_ID));
    let header = line(&mut harness, font::semibold(TITLE_SIZE));
    assert!(done.right() >= PAGE.right() && done.center().x > PAGE.center().x);
    assert!(done.center().y > PAGE.top() && done.center().y < PAGE.top() + header);
}

#[test]
fn resting_forgets_picture_and_done() {
    let (mut harness, mut page) = (Harness::new(), WhatsNewPage::default());
    let pictured = list(1, Some(shot()));
    let textures =
        |harness: &mut Harness| harness.frame(Vec::new(), |ctx| ctx.ui.ctx().tex_manager().read().num_allocated());
    let before = textures(&mut harness);
    harness.frame(Vec::new(), |ctx| page.ui(ctx, Some(&pictured), false));
    assert_eq!(textures(&mut harness), before + 1);
    page.done = true;
    page.rest();
    page.rest();
    assert_eq!(textures(&mut harness), before);
    assert!(!page.take_done() && !page.is_animating());
}
