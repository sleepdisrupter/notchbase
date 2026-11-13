use egui::{Pos2, Rect, Shape, TextureId, Vec2, vec2};

use crate::bridge::{Action, Page};
use crate::notch::panel::pages::Notes;
use crate::notch::{Mode, Notch};
use crate::ui::glyph::Glyph;
use crate::ui::picture::Picture;
use crate::whats_new::{Change, Release, Shot};

use super::super::fixtures::{Rig, click, island, shortcuts};
use super::{SETTLING_FRAMES, click_opens, due, frames, macbook, pointer_leaves};

const SCROLL_STEP: f32 = 40.0;
const DONE: Vec2 = vec2(-20.0, 12.0);
const MANY: usize = 14;
const FONT_ATLAS: u64 = 0;
const TITLE: &str = "Sample change";
const CAPTION: &str = "One clause that says what the change does for you";

fn sample(count: usize, picture: Option<&'static Shot>) -> &'static Release {
    let change = |_| Change { glyph: Glyph::Spark, title: TITLE, caption: CAPTION };
    let changes: Vec<Change> = (0..count).map(change).collect();
    Box::leak(Box::new(Release { version: "9.9.9", picture, changes: changes.leak() }))
}

fn shot() -> &'static Shot {
    Box::leak(Box::new(Shot { island: Picture::sample(680, 96), notch: Picture::sample(680, 120) }))
}

fn painted(rig: &Rig, n: &mut Notch) -> (Vec<String>, usize) {
    fn gather(shape: &Shape, words: &mut Vec<String>, pictures: &mut usize) {
        match shape {
            Shape::Vec(shapes) => shapes.iter().for_each(|shape| gather(shape, words, pictures)),
            Shape::Text(text) => words.push(text.galley.text().to_owned()),
            Shape::Mesh(mesh) if matches!(mesh.texture_id, TextureId::Managed(id) if id != FONT_ATLAS) => {
                *pictures += 1
            }
            _ => {}
        }
    }
    let (_, mut output) = rig.run(n, &shortcuts(None), Vec::new(), 1.0 / 60.0);
    output.textures_delta.clear();
    let (mut words, mut pictures) = (Vec::new(), 0);
    output.shapes.iter().for_each(|clipped| gather(&clipped.shape, &mut words, &mut pictures));
    (words, pictures)
}

fn wheel_over(window: Rect, notches: f32) -> Vec<egui::Event> {
    let wheel = egui::Event::MouseWheel {
        unit: egui::MouseWheelUnit::Line,
        delta: vec2(0.0, -notches),
        phase: egui::TouchPhase::Move,
        modifiers: egui::Modifiers::NONE,
    };
    vec![egui::Event::PointerMoved(window.center()), wheel]
}

fn done_at(n: &Notch) -> Pos2 {
    n.content_rect(Page::WhatsNew).right_top() + DONE
}

#[test]
fn no_tab_lit_while_news_shows() {
    let rig = due();
    for mut n in [island(), macbook()] {
        click_opens(&rig, &mut n);
        assert_eq!(n.tab_emphasis(), 0.0);
        let gear = n.header().gear.center();
        click(&rig, &mut n, gear);
        assert_eq!(n.pager.page, Page::Settings);
        click(&rig, &mut n, gear);
        frames(&rig, &mut n, SETTLING_FRAMES);
        assert_eq!((n.pager.page, n.tab_emphasis()), (Page::Tools, 1.0), "the lens comes back with the tab");
    }
}

#[test]
fn done_returns_to_lens_tab() {
    let rig = due();
    for mut n in [island(), macbook()] {
        n.open_page(Page::Stats);
        frames(&rig, &mut n, SETTLING_FRAMES);
        pointer_leaves(&rig, &mut n);
        assert!(n.is_collapsed());
        click_opens(&rig, &mut n);
        assert_eq!(n.visible_page(), Some(Page::WhatsNew));
        let done = done_at(&n);
        assert_eq!(click(&rig, &mut n, done), [Action::ChangesSeen]);
        assert_eq!((n.mode, n.pager.page), (Mode::Expanded, Page::Stats));
        assert_eq!(frames(&rig, &mut n, SETTLING_FRAMES), [], "it is said once");
        assert_eq!(n.visible_page(), Some(Page::Stats));
    }
}

#[test]
fn tall_notes_scroll_and_reopen_at_top() {
    let rig = due();
    for mut n in [island(), macbook()] {
        n.pages.notes = Notes(Some(sample(MANY, None)));
        click_opens(&rig, &mut n);
        let window = n.content_rect(Page::WhatsNew);
        let top = n.overflow(Page::WhatsNew).expect("fourteen changes are taller than the panel");
        assert!(top.natural > window.height() + SCROLL_STEP, "{} in {}", top.natural, window.height());
        assert_eq!((top.offset, top.top_fade(), top.bottom_fade()), (0.0, 0.0, 1.0));
        rig.frame(&mut n, &[], wheel_over(window, 1.0));
        frames(&rig, &mut n, 30);
        let scrolled = n.overflow(Page::WhatsNew).expect("still taller");
        assert_eq!((scrolled.offset, scrolled.top_fade(), scrolled.bottom_fade()), (SCROLL_STEP, 1.0, 1.0));
        assert_eq!(n.content_rect(Page::WhatsNew), window, "scrolling never changes the panel's height");
        pointer_leaves(&rig, &mut n);
        assert!(n.is_collapsed());
        assert!(!n.pages.scroll.contains_key(&Page::WhatsNew), "the next reader starts at the top");
    }
}

#[test]
fn both_styles_paint_everything() {
    let rig = due();
    for mut n in [island(), macbook()] {
        n.pages.notes = Notes(Some(sample(3, Some(shot()))));
        click_opens(&rig, &mut n);
        let (words, pictures) = painted(&rig, &mut n);
        for expected in ["What's New", "notchbase 9.9.9", "Done"] {
            assert!(words.iter().any(|text| text == expected), "{expected} in {words:?}");
        }
        assert_eq!(words.iter().filter(|text| *text == TITLE).count(), 3);
        assert_eq!(words.iter().filter(|text| *text == CAPTION).count(), 3);
        assert_eq!(pictures, 1);
        assert!(n.overflow(Page::WhatsNew).is_none(), "three changes under a picture fit the panel");
    }
}

#[test]
fn this_release_fits_without_scrolling() {
    let rig = due();
    let expected = crate::whats_new::latest().map_or(0, |release| usize::from(release.picture.is_some()));
    for mut n in [island(), macbook()] {
        click_opens(&rig, &mut n);
        assert_eq!(painted(&rig, &mut n).1, expected);
        assert!(n.overflow(Page::WhatsNew).is_none(), "say less before accepting a scroll");
    }
}

#[test]
fn missing_notes_say_so() {
    let (rig, mut n) = (Rig::new(), island());
    n.pages.notes = Notes(None);
    n.open_page(Page::WhatsNew);
    frames(&rig, &mut n, SETTLING_FRAMES);
    let (words, pictures) = painted(&rig, &mut n);
    assert!(words.iter().any(|text| text == "Nothing new in this version"), "{words:?}");
    assert_eq!(pictures, 0);
}
