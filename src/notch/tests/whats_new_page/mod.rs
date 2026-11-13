use std::time::Instant;

use egui::vec2;

use crate::bridge::{Action, Page, Tab};
use crate::notch::moves::surge::SURGE;
use crate::notch::{Mode, Notch};
use crate::platform::Style;

use super::fixtures::{AWAY, OVER, Rig, click, draw_out, island, ms, peek, shortcuts, song};

mod leaving;
mod shown;

const SETTLING_FRAMES: usize = 120;

fn frames(rig: &Rig, n: &mut Notch, count: usize) -> Vec<Action> {
    let keys = shortcuts(None);
    (0..count).flat_map(|_| rig.frame(n, &keys, Vec::new())).collect()
}

fn macbook() -> Notch {
    Notch::with_preview(Style::Notch, Some(vec2(185.0, 32.0)), None)
}

fn due() -> Rig {
    let mut rig = Rig::new();
    rig.news = true;
    rig
}

fn filled(mut rig: Rig) -> Rig {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    rig.shelf.add(["Cargo.toml", "Cargo.lock"].map(|name| root.join(name)).to_vec());
    rig
}

fn hover_opens(n: &mut Notch) {
    let rested = peek(n, Instant::now(), OVER);
    let delay = crate::config::current().notch.hover_delay();
    n.tick(rested + delay + SURGE + ms(10), Some(OVER));
}

fn click_opens(rig: &Rig, n: &mut Notch) -> Vec<Action> {
    draw_out(n, Instant::now(), OVER);
    let mut actions = click(rig, n, OVER);
    actions.extend(frames(rig, n, SETTLING_FRAMES));
    actions
}

fn pointer_leaves(rig: &Rig, n: &mut Notch) -> Vec<Action> {
    let t0 = Instant::now();
    n.tick(t0, Some(AWAY));
    n.tick(t0 + ms(300), Some(AWAY));
    frames(rig, n, SETTLING_FRAMES)
}

#[test]
fn hover_open_lands_on_news() {
    for mut n in [island(), macbook()] {
        n.set_now_playing(Some(&song(true, 0.0)));
        n.shelf_filled = true;
        n.news_due = true;
        hover_opens(&mut n);
        assert_eq!((n.mode, n.pager.page), (Mode::Expanded, Page::WhatsNew));
    }
}

#[test]
fn click_open_lands_on_news() {
    let rig = filled(due());
    for mut n in [island(), macbook()] {
        n.set_now_playing(Some(&song(true, 0.0)));
        draw_out(&mut n, Instant::now(), OVER);
        assert!(click(&rig, &mut n, OVER).is_empty());
        assert_eq!((n.mode, n.pager.page), (Mode::Expanded, Page::WhatsNew));
        for _ in 0..SETTLING_FRAMES {
            assert!(frames(&rig, &mut n, 1).is_empty());
            assert_eq!(n.pager.page, Page::WhatsNew, "the frame that opens the panel does not undo it");
        }
        assert_eq!(n.visible_page(), Some(Page::WhatsNew));
    }
}

#[test]
fn no_news_opens_as_usual() {
    let mut n = island();
    n.set_now_playing(Some(&song(true, 0.0)));
    n.shelf_filled = true;
    hover_opens(&mut n);
    assert_eq!((n.mode, n.pager.page), (Mode::Expanded, Page::Shelf));
    let mut n = island();
    n.set_now_playing(Some(&song(true, 0.0)));
    hover_opens(&mut n);
    assert_eq!((n.mode, n.pager.page), (Mode::Expanded, Page::Media));
    let (rig, mut n) = (filled(Rig::new()), island());
    click_opens(&rig, &mut n);
    assert_eq!(n.visible_page(), Some(Page::Shelf));
}

#[test]
fn live_page_and_explicit_open_win() {
    let mut n = island();
    n.news_due = true;
    n.set_activities(vec![crate::notch::live::tests::claude()]);
    n.mode = Mode::Peek;
    n.opens_on_click();
    assert_eq!(n.pager.page, Page::Agents);
    let mut plain = island();
    plain.news_due = true;
    plain.mode = Mode::Peek;
    plain.opens_on_click();
    assert_eq!(plain.pager.page, Page::WhatsNew, "a plain click is where the news waits");
    let mut asked = island();
    asked.news_due = true;
    asked.open_page(Page::Stats);
    assert_eq!((asked.mode, asked.pager.page), (Mode::Expanded, Page::Stats));
}

#[test]
fn news_returns_until_seen() {
    let mut rig = due();
    for mut n in [island(), macbook()] {
        rig.news = true;
        click_opens(&rig, &mut n);
        assert_eq!(n.visible_page(), Some(Page::WhatsNew));
        pointer_leaves(&rig, &mut n);
        assert!(n.is_collapsed());
        assert_eq!(n.pager.page, Page::Tools, "it lets go of the page as it hides");
        click_opens(&rig, &mut n);
        assert_eq!(n.visible_page(), Some(Page::WhatsNew), "unseen, so the next full open shows it again");
        rig.news = false;
        assert_eq!(frames(&rig, &mut n, 2), [], "being seen changes nothing on screen");
        assert_eq!(n.visible_page(), Some(Page::WhatsNew));
        pointer_leaves(&rig, &mut n);
        click_opens(&rig, &mut n);
        assert_eq!(n.visible_page(), Some(Page::Tools), "once seen it never opens by itself");
    }
}

#[test]
fn tab_change_keeps_news() {
    let (rig, mut n) = (due(), island());
    click_opens(&rig, &mut n);
    let mut tabs = Tab::defaults();
    tabs.pop();
    n.sync_tabs(&tabs);
    assert_eq!((n.pager.page, n.pager.from), (Page::WhatsNew, Page::WhatsNew));
    frames(&rig, &mut n, 3);
    assert_eq!(n.visible_page(), Some(Page::WhatsNew), "tools starting up rebuild the tabs right after a relaunch");
}
