use egui::{Rect, TextureId, vec2};

use super::layout::{BULLET_GAP, CAPTION_GAP, CAPTION_ROWS, HEADER_GAP, LIST_GAP, TITLE_SIZE};
use super::*;
use crate::pages::harness::{Harness, PAGE};
use crate::ui::glyph::Glyph;
use crate::ui::picture::Picture;
use crate::whats_new::{Change, PICTURE_WIDTH, Shot};

mod captions;
mod done;

const SHORT: &str = "Press the magnifier";
const NAME: &str = "Find";
const PIXEL: f32 = 1.0;
const WIDER: f32 = 4.0;
const RIM: f32 = 2.0;
const ISLAND_SHOT: [u32; 2] = [680, 96];
const NOTCH_SHOT: [u32; 2] = [680, 120];

fn change(caption: &str) -> Change {
    Change { glyph: Glyph::Search, title: NAME, caption: caption.to_owned().leak() }
}

fn notes(changes: Vec<Change>, picture: Option<&'static Shot>) -> Release {
    Release { version: "9.9.9", picture, changes: changes.leak() }
}

fn list(count: usize, picture: Option<&'static Shot>) -> Release {
    notes((0..count).map(|_| change(SHORT)).collect(), picture)
}

fn shot() -> &'static Shot {
    let picture = |[width, height]: [u32; 2]| Picture::sample(width, height);
    Box::leak(Box::new(Shot { island: picture(ISLAND_SHOT), notch: picture(NOTCH_SHOT) }))
}

fn tall(shot: [u32; 2]) -> f32 {
    shot[1] as f32 * 0.5
}

fn height(harness: &mut Harness, release: Option<&Release>, beside_camera: bool) -> f32 {
    harness.frame(Vec::new(), |ctx| WhatsNewPage::default().height(release, PAGE.width(), beside_camera, ctx.canvas))
}

struct Painted {
    words: Vec<(String, Rect)>,
    pictures: Vec<Rect>,
    marks: Vec<Rect>,
}

impl Painted {
    fn all(&self) -> Rect {
        let rects = self.words.iter().map(|(_, rect)| *rect).chain(self.pictures.clone()).chain(self.marks.clone());
        rects.reduce(|all, next| all.union(next)).expect("something was painted")
    }

    fn said(&self, words: &str) -> Vec<Rect> {
        self.words.iter().filter(|(text, _)| text == words).map(|(_, rect)| *rect).collect()
    }

    fn glyph_beside(&self, title: Rect) -> Rect {
        let beside = self.marks.iter().filter(|mark| title.y_range().contains(mark.center().y));
        beside.copied().reduce(|all, next| all.union(next)).expect("a glyph on the title's line")
    }
}

fn only(rects: &[Rect]) -> Rect {
    let [rect] = rects else { panic!("one of these, not {rects:?}") };
    *rect
}

fn painted_in(harness: &mut Harness, release: Option<&Release>, beside_camera: bool, page: Rect) -> Painted {
    let mut shown = WhatsNewPage::default();
    harness.frame(Vec::new(), |ctx| {
        ctx.rect = page;
        shown.ui(ctx, release, beside_camera);
        let mut painted = Painted { words: Vec::new(), pictures: Vec::new(), marks: Vec::new() };
        for shape in &ctx.canvas.shapes {
            match shape {
                Shape::Text(text) => {
                    let rect = Rect::from_min_size(text.pos, text.galley.size());
                    painted.words.push((text.galley.text().to_owned(), rect));
                }
                Shape::Mesh(mesh) if mesh.texture_id != TextureId::default() => {
                    painted.pictures.push(mesh.calc_bounds());
                }
                other => painted.marks.push(other.visual_bounding_rect()),
            }
        }
        let rim = |mark: &Rect| painted.pictures.iter().any(|frame| frame.expand(RIM).contains_rect(*mark));
        painted.marks = painted.marks.iter().copied().filter(|mark| !rim(mark)).collect();
        painted
    })
}

fn painted(harness: &mut Harness, release: Option<&Release>, beside_camera: bool) -> Painted {
    painted_in(harness, release, beside_camera, PAGE)
}

fn line(harness: &mut Harness, font: egui::FontId) -> f32 {
    harness.frame(Vec::new(), |ctx| ctx.canvas.text_size("Ag", font.clone()).y)
}

fn bullet(harness: &mut Harness) -> f32 {
    (line(harness, font::headline()) + CAPTION_GAP + line(harness, font::subheadline())).ceil()
}

#[test]
fn height_matches_layout() {
    let mut harness = Harness::new();
    let caption = line(&mut harness, font::subheadline());
    for release in [list(2, Some(shot())), list(2, None)] {
        for beside_camera in [false, true] {
            let tall = height(&mut harness, Some(&release), beside_camera);
            let bounds = painted(&mut harness, Some(&release), beside_camera).all();
            let spare = PAGE.top() + tall - bounds.bottom();
            assert!((0.0..caption).contains(&spare), "the page ends with its last caption: {bounds:?} in {tall}");
            assert!(PAGE.expand(PIXEL).contains_rect(bounds), "nothing is painted outside the page: {bounds:?}");
        }
    }
    let (tall, bounds) = (height(&mut harness, None, false), painted(&mut harness, None, false).all());
    assert!(bounds.bottom() < PAGE.top() + tall, "the one dim line sits inside the page: {bounds:?} in {tall}");
}

#[test]
fn header_and_a_bullet_per_change() {
    let mut harness = Harness::new();
    let header = line(&mut harness, font::semibold(TITLE_SIZE)).ceil();
    let bullet = bullet(&mut harness);
    let one = height(&mut harness, Some(&list(1, None)), false);
    assert_eq!(one, header + LIST_GAP + bullet);
    assert_eq!(height(&mut harness, Some(&list(3, None)), false), one + 2.0 * (BULLET_GAP + bullet));
}

#[test]
fn picture_sits_under_header() {
    let mut harness = Harness::new();
    let (plain, pictured) = (list(3, None), list(3, Some(shot())));
    let bare = height(&mut harness, Some(&plain), false);
    assert_eq!(height(&mut harness, Some(&pictured), false), bare + HEADER_GAP + tall(ISLAND_SHOT));
    assert_eq!(height(&mut harness, Some(&pictured), true), bare + HEADER_GAP + tall(NOTCH_SHOT));
    let header = line(&mut harness, font::semibold(TITLE_SIZE)).ceil();
    let shown = painted(&mut harness, Some(&pictured), false);
    let frame = only(&shown.pictures);
    assert!((frame.top() - (PAGE.top() + header + HEADER_GAP)).abs() < PIXEL, "{frame:?}");
    assert!(shown.said(NAME).iter().all(|title| title.top() >= frame.bottom() + LIST_GAP - PIXEL), "{frame:?}");
}

#[test]
fn parts_share_both_edges() {
    let mut harness = Harness::new();
    let release = list(3, Some(shot()));
    for page in [PAGE, Rect::from_min_size(PAGE.min, PAGE.size() + vec2(WIDER, 0.0))] {
        let shown = painted_in(&mut harness, Some(&release), true, page);
        let (left, right) = (page.center().x - PICTURE_WIDTH * 0.5, page.center().x + PICTURE_WIDTH * 0.5);
        let (title, done, frame) = (only(&shown.said(TITLE)), only(&shown.said(DONE)), only(&shown.pictures));
        assert!((title.left() - left).abs() < 0.01 && (done.right() - right).abs() < 0.01, "{title:?} {done:?}");
        assert!((frame.left() - left).abs() < PIXEL && (frame.right() - right).abs() < PIXEL, "{frame:?}");
        let indent = left + GLYPH;
        for mark in &shown.marks {
            assert!(mark.left() > left - PIXEL && mark.right() < indent + PIXEL, "a glyph at {mark:?}");
        }
        let (titles, captions) = (shown.said(NAME), shown.said(SHORT));
        assert_eq!((titles.len(), captions.len()), (3, 3));
        for text in titles.iter().chain(&captions) {
            assert_eq!(text.left(), left + text_indent(), "titles and captions start together, past the glyph");
            assert!(text.right() <= right);
        }
    }
}

#[test]
fn bullet_glyph_on_title_line() {
    let mut harness = Harness::new();
    let shown = painted(&mut harness, Some(&list(2, None)), false);
    let (titles, captions) = (shown.said(NAME), shown.said(SHORT));
    assert_eq!((titles.len(), captions.len()), (2, 2));
    for title in &titles {
        let mark = shown.glyph_beside(*title);
        assert!(mark.width() <= GLYPH + PIXEL && mark.height() <= title.height(), "no well around {mark:?}");
        assert!((mark.center().y - title.center().y).abs() <= PIXEL, "{mark:?} beside {title:?}");
    }
    assert!(shown.marks.iter().all(|mark| titles.iter().any(|title| title.y_range().contains(mark.center().y))));
    let within = captions[0].top() - titles[0].bottom();
    let between = titles[1].top() - captions[0].bottom();
    assert!((within - CAPTION_GAP).abs() < 0.01 && between >= BULLET_GAP, "{within} then {between}");
    assert!(between > 4.0 * within.max(PIXEL), "a new bullet is plainly further off than a caption");
}

#[test]
fn caption_grows_to_two_lines() {
    let mut harness = Harness::new();
    let tall = |harness: &mut Harness, caption: &str| height(harness, Some(&notes(vec![change(caption)], None)), false);
    let short = tall(&mut harness, SHORT);
    let long = tall(&mut harness, &"A caption that keeps on going. ".repeat(3));
    let longer = tall(&mut harness, &"A caption that keeps on going. ".repeat(40));
    let endless = tall(&mut harness, &"A caption that keeps on going. ".repeat(80));
    assert!(long > short, "{short} to {long}");
    assert_eq!(longer, endless, "the ellipsis ends it");
    let caption = line(&mut harness, font::subheadline());
    assert!(longer >= long && longer - short <= ((CAPTION_ROWS - 1) as f32 * caption).ceil(), "{short}, {longer}");
}

#[test]
fn captions_fit_one_line_on_island() {
    let Some(release) = crate::whats_new::latest() else { return };
    let mut harness = Harness::new();
    let shown = painted(&mut harness, Some(release), false);
    let one = line(&mut harness, font::subheadline());
    for change in release.changes {
        let caption = only(&shown.said(change.caption));
        assert!(caption.height() <= one + PIXEL, "{:?} wraps: say it in fewer words", change.caption);
        assert_eq!(shown.said(change.title).len(), 1, "{:?} is cut short", change.title);
    }
}
