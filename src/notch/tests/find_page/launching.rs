use std::path::Path;
use std::sync::Arc;

use egui::{Key, Modifiers};

use super::super::fixtures::{Rig, island, key, notch, shortcuts};
use super::{SETTLING_FRAMES, answered, found, frames, open, row};
use crate::bridge::{Action, FindRow, FindView, How, Kind};
use crate::notch::Notch;
use crate::notch::geometry::CANVAS;
use crate::ui::snapshot::{Atlas, folder, rasterize};

const DT: f32 = 1.0 / 60.0;
const SCALE: f32 = 2.0;
const ICON_PX: u32 = 64;
const CLOSES_AT: usize = 14;
const SHOT_AT: [usize; 8] = [0, 4, 8, 12, 16, 20, 26, 34];
const APPS: [&str; 4] = [
    "/Applications/Zen.app",
    "/Applications/Spotify.app",
    "/System/Applications/Notes.app",
    "/System/Applications/Calendar.app",
];

fn app(path: &str, key: u64) -> FindRow {
    let name = Path::new(path).file_stem().unwrap_or_default().to_string_lossy();
    FindRow { key, name: name.into(), app: true, folder: true, kind: Kind::App, ..row(path) }
}

fn apps(query: &str) -> FindView {
    let rows: Vec<FindRow> = APPS.iter().zip(1..).map(|(path, key)| app(path, key)).collect();
    FindView { rows: rows.into(), ..answered(query, &[]) }
}

fn picks(actions: &[Action]) -> usize {
    actions.iter().filter(|action| matches!(action, Action::FindPick(pick) if pick.how == How::Open)).count()
}

#[test]
fn opening_app_lifts_it_once() {
    let mut rig = Rig::new();
    let mut n = open(&rig);
    found(&rig, &mut n);
    rig.find = apps("");
    frames(&rig, &mut n, 4);
    let keys = shortcuts(None);
    let enter = || vec![key(Key::Enter, Modifiers::NONE)];
    assert_eq!(picks(&rig.frame(&mut n, &keys, enter())), 1, "the app is opened at once, not after the effect");
    assert!(n.is_animating(), "and its icon is on its way");
    assert_eq!(picks(&rig.frame(&mut n, &keys, enter())), 0, "a second Enter does not open it twice");
    let down = vec![key(Key::ArrowDown, Modifiers::NONE)];
    rig.frame(&mut n, &keys, down);
    assert_eq!(picks(&rig.frame(&mut n, &keys, enter())), 0, "nor does it open the row below");
    frames(&rig, &mut n, SETTLING_FRAMES);
    assert_eq!(picks(&rig.frame(&mut n, &keys, enter())), 1, "a launch that never closed the panel gives it back");
}

#[test]
fn file_opens_plainly() {
    let mut rig = Rig::new();
    let mut n = open(&rig);
    found(&rig, &mut n);
    rig.find = answered("", &["/Users/john/Cargo.toml", "/Users/john/Car.png"]);
    frames(&rig, &mut n, 4);
    let keys = shortcuts(None);
    let enter = || vec![key(Key::Enter, Modifiers::NONE)];
    assert_eq!(picks(&rig.frame(&mut n, &keys, enter())), 1);
    assert_eq!(picks(&rig.frame(&mut n, &keys, enter())), 1, "the page is as live as it was");
}

fn pictured(view: FindView) -> FindView {
    let icon = |row: &FindRow| crate::filedrag::icon(&row.path, ICON_PX).map(Arc::new);
    let rows: Vec<FindRow> = view.rows.iter().map(|row| FindRow { icon: icon(row), ..row.clone() }).collect();
    FindView { rows: rows.into(), ..view }
}

fn drawn(rig: &Rig, n: &mut Notch, atlas: &mut Atlas, events: Vec<egui::Event>) -> Vec<egui::ClippedPrimitive> {
    let (_, mut output) = rig.run(n, &shortcuts(None), events, DT);
    atlas.keep(&mut output);
    rig.ctx.tessellate(std::mem::take(&mut output.shapes), output.pixels_per_point)
}

#[test]
#[ignore = "writes one sheet per notch style of an app launch from Find, frame by frame, to target/snapshots"]
fn launch_frames() {
    for (mut n, style) in [(island(), "island"), (notch(), "notch")] {
        let mut rig = Rig::new();
        rig.ctx.set_pixels_per_point(SCALE);
        let mut atlas = Atlas::default();
        n.summon_find();
        for _ in 0..SETTLING_FRAMES {
            drawn(&rig, &mut n, &mut atlas, Vec::new());
        }
        n.pages.find.browse();
        rig.find = pictured(apps(""));
        for _ in 0..SETTLING_FRAMES {
            drawn(&rig, &mut n, &mut atlas, Vec::new());
        }
        let mut kept = Vec::new();
        for frame in 0..=*SHOT_AT.last().expect("frames to take") {
            let events = if frame == 0 { vec![key(Key::Enter, Modifiers::NONE)] } else { Vec::new() };
            if frame == CLOSES_AT {
                n.collapse();
                n.swallow();
            }
            let primitives = drawn(&rig, &mut n, &mut atlas, events);
            if SHOT_AT.contains(&frame) {
                kept.push(primitives);
            }
        }
        let taken: Vec<image::RgbImage> =
            kept.iter().map(|primitives| rasterize(primitives, &atlas, CANVAS, SCALE).over_desk()).collect();
        let (width, height) = (taken[0].width(), taken[0].height() / 2);
        let mut sheet = image::RgbImage::new(width * 4 / 2, height * 2 / 2);
        for (at, frame) in taken.iter().enumerate() {
            let top = image::imageops::crop_imm(frame, 0, 0, width, height).to_image();
            let small = image::imageops::resize(&top, width / 2, height / 2, image::imageops::FilterType::Triangle);
            let (column, line) = ((at % 4) as i64, (at / 4) as i64);
            image::imageops::replace(&mut sheet, &small, column * i64::from(width / 2), line * i64::from(height / 2));
        }
        sheet.save(folder().join(format!("find-launch-{style}.png"))).expect("a written sheet");
    }
}

#[test]
fn summoned_find_open_from_first_frame() {
    for mut n in [island(), notch()] {
        let mut rig = Rig::new();
        let keys = shortcuts(None);
        n.summon_find();
        let mut unseen = 0;
        for frame in 0..90 {
            if frame == 20 {
                rig.find = apps("");
            }
            rig.frame(&mut n, &keys, Vec::new());
            assert!(n.expanded(), "frame {frame}: the keyboard is held for a panel that is open");
            unseen += usize::from(n.visible_page().is_none());
        }
        assert!(unseen > 0, "while it fades in its page is not yet in sight, which once gave the keyboard back");
        assert!(n.visible_page().is_some());
    }
}

#[test]
#[ignore = "writes a picture of Find as it opens, one bar and nothing else, to target/snapshots"]
fn resting_bar() {
    for (mut n, style) in [(island(), "island"), (notch(), "notch")] {
        let mut rig = Rig::new();
        rig.ctx.set_pixels_per_point(SCALE);
        rig.find = apps("");
        let mut atlas = Atlas::default();
        n.summon_find();
        let mut last = Vec::new();
        for _ in 0..SETTLING_FRAMES {
            last = drawn(&rig, &mut n, &mut atlas, Vec::new());
        }
        let shot = rasterize(&last, &atlas, CANVAS, SCALE).over_desk();
        shot.save(folder().join(format!("find-rest-{style}.png"))).expect("a written picture");
    }
}

#[test]
#[ignore = "writes pictures of the pinned strip under the resting bar, and of its menu, to target/snapshots"]
fn pinned_strip() {
    use crate::bridge::Page;

    for (mut n, style) in [(island(), "island"), (notch(), "notch")] {
        let mut rig = Rig::new();
        rig.ctx.set_pixels_per_point(SCALE);
        let home = std::env::var("HOME").unwrap_or_default();
        let downloads = FindRow { key: 9, name: "Downloads".into(), folder: true, ..row(&format!("{home}/Downloads")) };
        let pins: Vec<FindRow> = apps("").rows.iter().take(3).cloned().chain([downloads]).collect();
        rig.find = FindView { pins: pictured(FindView { rows: pins.into(), ..apps("") }).rows, ..apps("") };
        let mut atlas = Atlas::default();
        n.summon_find();
        let mut last = Vec::new();
        for _ in 0..SETTLING_FRAMES {
            last = drawn(&rig, &mut n, &mut atlas, Vec::new());
        }
        let shot = rasterize(&last, &atlas, CANVAS, SCALE).over_desk();
        shot.save(folder().join(format!("find-pins-{style}.png"))).expect("a written picture");
        let page = n.content_rect(Page::Find);
        let at = egui::pos2(page.left() + page.width() / 4.0 * 1.5, page.top() + 46.0 + 8.0 + 30.0);
        for _ in 0..30 {
            last = drawn(&rig, &mut n, &mut atlas, vec![egui::Event::PointerMoved(at)]);
        }
        let shot = rasterize(&last, &atlas, CANVAS, SCALE).over_desk();
        shot.save(folder().join(format!("find-pins-hover-{style}.png"))).expect("a written picture");
        let button = |pressed| egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Secondary,
            pressed,
            modifiers: Modifiers::NONE,
        };
        drawn(&rig, &mut n, &mut atlas, vec![egui::Event::PointerMoved(at), button(true)]);
        drawn(&rig, &mut n, &mut atlas, vec![button(false)]);
        for _ in 0..60 {
            last = drawn(&rig, &mut n, &mut atlas, Vec::new());
        }
        let shot = rasterize(&last, &atlas, CANVAS, SCALE).over_desk();
        shot.save(folder().join(format!("find-pins-menu-{style}.png"))).expect("a written picture");
        let over = at + egui::vec2(70.0, 50.0);
        for _ in 0..40 {
            last = drawn(&rig, &mut n, &mut atlas, vec![egui::Event::PointerMoved(over)]);
        }
        let shot = rasterize(&last, &atlas, CANVAS, SCALE).over_desk();
        shot.save(folder().join(format!("find-pins-menu-lit-{style}.png"))).expect("a written picture");
    }
}
