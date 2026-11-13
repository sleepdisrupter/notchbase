use egui::Id;

use super::copy::{ALLOW, CONTINUE, OPEN_SETTINGS, hint, primary};
use super::*;
use crate::pages::harness::{Harness, press, release};
use crate::system::permissions::Access;

fn step(access: Access, standing: Standing, asked: bool) -> StepView {
    StepView { access, standing, asked }
}

fn tour(stage: WelcomeStage) -> WelcomeView {
    let steps = vec![
        step(Access::Accessibility, Standing::Undecided, false),
        step(Access::Bluetooth, Standing::Undecided, false),
        step(Access::FullDisk, Standing::Undecided, false),
    ];
    WelcomeView { due: true, first_run: true, stage, steps }
}

fn click(harness: &mut Harness, page: &mut WelcomePage, view: &WelcomeView, id: &str) -> Vec<Action> {
    harness.frame(Vec::new(), |ctx| page.ui(ctx, view));
    let at = harness.rect_of(Id::new(id)).center();
    harness.frame(press(at), |ctx| page.ui(ctx, view));
    harness.frame(release(at), |ctx| page.ui(ctx, view))
}

#[test]
fn self_asking_permission_says_allow() {
    assert_eq!(primary(&step(Access::Bluetooth, Standing::Undecided, false)), ALLOW);
    assert_eq!(primary(&step(Access::Bluetooth, Standing::Undecided, true)), ALLOW, "the dialog may still be up");
    assert_eq!(hint(&step(Access::Bluetooth, Standing::Undecided, true)), None);
    assert_eq!(primary(&step(Access::Bluetooth, Standing::Denied, true)), OPEN_SETTINGS, "macOS won't ask twice");
    assert!(hint(&step(Access::Bluetooth, Standing::Denied, true)).is_some());
}

#[test]
fn settings_only_step_opens_settings() {
    assert_eq!(primary(&step(Access::Accessibility, Standing::Undecided, false)), ALLOW, "macOS explains it first");
    assert_eq!(primary(&step(Access::Accessibility, Standing::Undecided, true)), OPEN_SETTINGS);
    assert!(hint(&step(Access::Accessibility, Standing::Undecided, true)).is_some());
    assert_eq!(primary(&step(Access::FullDisk, Standing::Undecided, false)), OPEN_SETTINGS, "it never asks at all");
    assert!(hint(&step(Access::FullDisk, Standing::Undecided, false)).is_some());
}

#[test]
fn allowed_step_carries_on() {
    let allowed = step(Access::ScreenRecording, Standing::Granted, true);
    assert_eq!((primary(&allowed), hint(&allowed)), (CONTINUE, None));
}

#[test]
fn allow_asks_not_now_moves_on() {
    let mut harness = Harness::new();
    let view = tour(WelcomeStage::Step(1));
    let mut page = WelcomePage::default();
    assert_eq!(click(&mut harness, &mut page, &view, PRIMARY_ID), [Action::WelcomeAsk(Access::Bluetooth)]);
    let mut page = WelcomePage::default();
    assert_eq!(click(&mut harness, &mut page, &view, SECONDARY_ID), [Action::WelcomeNext]);
}

#[test]
fn allowed_step_offers_continue() {
    let mut harness = Harness::new();
    let mut view = tour(WelcomeStage::Step(0));
    view.steps[0].standing = Standing::Granted;
    let mut page = WelcomePage::default();
    assert_eq!(click(&mut harness, &mut page, &view, PRIMARY_ID), [Action::WelcomeNext]);
    assert!(!harness.drawn(Id::new(SECONDARY_ID)), "no Not Now for something already on");
}

#[test]
fn hello_starts_or_skips() {
    let mut harness = Harness::new();
    let view = tour(WelcomeStage::Hello);
    assert_eq!(click(&mut harness, &mut WelcomePage::default(), &view, PRIMARY_ID), [Action::WelcomeStart]);
    assert_eq!(click(&mut harness, &mut WelcomePage::default(), &view, SECONDARY_ID), [Action::WelcomeSkip]);
}

#[test]
fn nothing_to_ask_one_way_on() {
    let mut harness = Harness::new();
    let view = WelcomeView { steps: Vec::new(), ..tour(WelcomeStage::Hello) };
    assert_eq!(click(&mut harness, &mut WelcomePage::default(), &view, PRIMARY_ID), [Action::WelcomeStart]);
    assert!(!harness.drawn(Id::new(SECONDARY_ID)));
}

#[test]
fn first_run_done_goes_to_tabs() {
    let mut harness = Harness::new();
    let mut page = WelcomePage::default();
    assert_eq!(click(&mut harness, &mut page, &tour(WelcomeStage::AllSet), PRIMARY_ID), [Action::WelcomeDone]);
    assert_eq!(page.take_done(), Some(false));
}

#[test]
fn done_returns_to_settings() {
    let mut harness = Harness::new();
    let view = WelcomeView { first_run: false, ..tour(WelcomeStage::AllSet) };
    let mut page = WelcomePage::default();
    assert_eq!(click(&mut harness, &mut page, &view, PRIMARY_ID), [Action::WelcomeDone]);
    assert_eq!(page.take_done(), Some(true));
    assert_eq!(page.take_done(), None, "once");
}

#[test]
fn new_step_resists_double_click() {
    let mut harness = Harness::new();
    let mut page = WelcomePage::default();
    harness.frame(Vec::new(), |ctx| page.ui(ctx, &tour(WelcomeStage::Step(0))));
    assert!(!page.is_animating(), "the first page arrives with the panel");
    let next = tour(WelcomeStage::Step(1));
    harness.frame(Vec::new(), |ctx| page.ui(ctx, &next));
    assert!(page.is_animating());
    let at = harness.rect_of(Id::new(PRIMARY_ID)).center();
    harness.frame(press(at), |ctx| page.ui(ctx, &next));
    assert!(harness.frame(release(at), |ctx| page.ui(ctx, &next)).is_empty());
}

fn words_on(harness: &mut Harness, page: &mut WelcomePage, view: &WelcomeView) -> Vec<String> {
    harness.frame(Vec::new(), |ctx| {
        page.ui(ctx, view);
        ctx.canvas
            .shapes
            .iter()
            .filter_map(|shape| match shape {
                egui::Shape::Text(text) if text.opacity_factor > 0.0 => Some(text.galley.text().to_owned()),
                _ => None,
            })
            .collect()
    })
}

#[test]
fn pinned_preview_wins() {
    let mut harness = Harness::new();
    let mut page = WelcomePage::default();
    page.pin(tour(WelcomeStage::AllSet));
    let live = tour(WelcomeStage::Hello);
    let shows =
        |words: Vec<String>| words.iter().any(|text| text == ALL_SET) && !words.iter().any(|text| text == WELCOME);
    assert!(shows(words_on(&mut harness, &mut page, &live)));
    page.rest();
    assert!(shows(words_on(&mut harness, &mut page, &live)), "resting keeps the sample");
}

#[test]
fn granted_step_eases_into_continue() {
    let mut harness = Harness::new();
    let mut view = tour(WelcomeStage::Step(1));
    let mut page = WelcomePage::default();
    for _ in 0..30 {
        words_on(&mut harness, &mut page, &view);
    }
    view.steps[1].standing = Standing::Granted;
    let has = |words: &[String], word: &str| words.iter().any(|text| text == word);
    let first = words_on(&mut harness, &mut page, &view);
    assert!(has(&first, ALLOW) && has(&first, NOT_NOW), "{first:?}");
    assert!(!has(&first, CONTINUE), "the new label waits for the old one to leave: {first:?}");
    for _ in 0..9 {
        words_on(&mut harness, &mut page, &view);
    }
    let between = words_on(&mut harness, &mut page, &view);
    assert!(has(&between, CONTINUE) && has(&between, NOT_NOW) && !has(&between, ALLOW), "{between:?}");
    for _ in 0..40 {
        words_on(&mut harness, &mut page, &view);
    }
    let after = words_on(&mut harness, &mut page, &view);
    assert!(after.iter().any(|text| text == CONTINUE) && !after.iter().any(|text| text == ALLOW || text == NOT_NOW));
}
