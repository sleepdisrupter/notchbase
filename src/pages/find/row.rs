//! One result row: the icon, the name with its matched letters lit, where it lives, and buttons when it is selected.
//! On Windows an app row has one more button, to run it as administrator.

use egui::text::{LayoutJob, TextFormat};
use egui::{Align2, Color32, Id, Pos2, Rect, Sense, Shape, Vec2, pos2};

use super::cut::Cut;
use super::icons::{self, Icons};
use crate::bridge::FindRow;
use crate::pages::PageCtx;
use crate::pages::tools::well;
use crate::ui::canvas::{fade, scale_about};
use crate::ui::color::{self, label, secondary_label, white};
use crate::ui::glass::{self, Glass};
use crate::ui::glyph::{self, Glyph};
use crate::ui::{controls, font};

pub(super) const ROW_H: f32 = 52.0;
pub(super) const INSET: f32 = 6.0;
pub(super) const WELL: f32 = 36.0;
pub(super) const TEXT_GAP: f32 = 12.0;
pub(super) const LINE: f32 = 8.0;
pub(super) const TRAILING_W: f32 = 64.0;
pub(super) const HIGHLIGHT_INSET: f32 = 2.0;
pub(super) const ICON: f32 = 32.0;
const BUTTON: f32 = 26.0;
const BUTTON_GAP: f32 = 6.0;
const BUTTON_GLYPH: f32 = 12.0;
const BUTTONS_LIVE: f32 = 0.5;
const UNMATCHED: f32 = 0.6;
const AT_REST: f32 = 0.86;
const QUIET: f32 = 0.68;
const HINT_GAP: f32 = 8.0;
const ROW_ID: &str = "notchbase.find.row";
const OFFERS_ADMIN: bool = cfg!(windows);

pub(super) struct Face<'a> {
    pub(super) row: &'a FindRow,
    pub(super) detail: &'a str,
    pub(super) hint: &'a str,
    pub(super) cut: Cut,
    pub(super) two_tone: bool,
    pub(super) place_first: bool,
    pub(super) selected: bool,
    pub(super) shelved: bool,
    pub(super) alpha: f32,
    pub(super) live: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Hit {
    Open,
    AsAdmin,
    Reveal,
    Shelve,
    Drag,
    Menu,
}

pub(super) struct Out {
    pub(super) hit: Option<Hit>,
    pub(super) hovered: bool,
}

pub(super) fn id_of(key: u64) -> Id {
    Id::new((ROW_ID, key))
}

pub(super) fn text_room(rect: Rect, row: &FindRow) -> f32 {
    let trailing = if runs_as_admin(row) { TRAILING_W + BUTTON + BUTTON_GAP } else { TRAILING_W };
    rect.width() - 2.0 * INSET - WELL - 2.0 * TEXT_GAP - trailing
}

fn runs_as_admin(row: &FindRow) -> bool {
    OFFERS_ADMIN && row.app && !row.pane()
}

pub(super) fn paint(ctx: &mut PageCtx<'_>, rect: Rect, face: &Face<'_>, icons: &mut Icons) -> Out {
    let mark = ctx.canvas.mark();
    let id = id_of(face.row.key);
    let sense = if face.live { Sense::click_and_drag() } else { Sense::hover() };
    let body = ctx.ui.interact(rect, id, sense);
    let hovered = face.live && body.hovered();
    let focus = controls::animate(ctx.ui, id.with("focus"), face.selected);
    let trail = controls::animate(ctx.ui, id.with("trail"), hovered || face.selected);
    leading(ctx, rect, face.row, icons);
    let right = rect.right() - INSET;
    let pressed = trailing(ctx, pos2(right, rect.center().y), id, face, trail);
    let left = rect.left() + INSET + WELL + TEXT_GAP;
    let room = text_room(rect, face.row);
    let named = title(ctx, pos2(left, rect.center().y - LINE * focus), face, room);
    if focus < 1.0 && !face.hint.is_empty() {
        let at = pos2(left + named + HINT_GAP, rect.center().y - LINE * focus);
        let hint = controls::truncate(ctx.canvas, face.hint, font::subheadline(), (room - named - HINT_GAP).max(0.0));
        ctx.canvas.text(
            at,
            Align2::LEFT_CENTER,
            hint,
            font::subheadline(),
            secondary_label().gamma_multiply(1.0 - focus),
        );
    }
    if focus > 0.0 {
        let detail = match face.place_first {
            true => controls::truncate_start(ctx.canvas, face.detail, font::subheadline(), room),
            false => controls::truncate(ctx.canvas, face.detail, font::subheadline(), room),
        };
        let at = pos2(left, rect.center().y + LINE);
        ctx.canvas.text(at, Align2::LEFT_CENTER, detail, font::subheadline(), secondary_label().gamma_multiply(focus));
    }
    let rest = 1.0 - focus;
    let shrink = scale_about(pos2(rect.left() + INSET, rect.center().y), 1.0 - (1.0 - AT_REST) * rest);
    let alpha = face.alpha * (1.0 - (1.0 - QUIET) * rest);
    for shape in ctx.canvas.since(mark) {
        if rest > 0.0 {
            shape.transform(shrink);
        }
        fade(shape, alpha);
    }
    let hit = pressed
        .or((face.live && body.secondary_clicked()).then_some(Hit::Menu))
        .or((face.live && body.drag_started()).then_some(Hit::Drag))
        .or((face.live && body.clicked()).then_some(Hit::Open));
    Out { hit, hovered }
}

fn leading(ctx: &mut PageCtx<'_>, rect: Rect, row: &FindRow, icons: &mut Icons) {
    emblem(ctx, row, icons, emblem_center(rect), 1.0);
}

pub(super) fn emblem_center(rect: Rect) -> Pos2 {
    pos2(rect.left() + INSET + WELL * 0.5, rect.center().y)
}

pub(super) fn emblem(ctx: &mut PageCtx<'_>, row: &FindRow, icons: &mut Icons, center: Pos2, scale: f32) {
    let landed = icons.shown(row.key);
    if landed < 1.0 {
        let mark = ctx.canvas.mark();
        well::paint(ctx.canvas, center, WELL * scale, 0.0);
        well::glyph(ctx.canvas, icons::fallback(row), center, WELL * scale, color::PEARL);
        ctx.canvas.since(mark).iter_mut().for_each(|shape| fade(shape, 1.0 - landed));
    }
    icons.paint(ctx.ui.ctx(), ctx.canvas, row, center, ICON * scale);
}

fn trailing(ctx: &mut PageCtx<'_>, at: Pos2, id: Id, face: &Face<'_>, trail: f32) -> Option<Hit> {
    if trail <= 0.0 || face.row.pane() {
        return None;
    }
    let live = face.live && trail >= BUTTONS_LIVE;
    let shelf = Rect::from_center_size(pos2(at.x - BUTTON * 0.5, at.y), Vec2::splat(BUTTON));
    let reveal = shelf.translate(egui::vec2(-(BUTTON + BUTTON_GAP), 0.0));
    let admin = reveal.translate(egui::vec2(-(BUTTON + BUTTON_GAP), 0.0));
    let mark = ctx.canvas.mark();
    let ran = runs_as_admin(face.row) && button(ctx, admin, id.with("admin"), Glyph::Shield, live);
    let revealed = button(ctx, reveal, id.with("reveal"), Glyph::Folder, live);
    let tray = if face.shelved { Glyph::Checkmark } else { Glyph::Tray };
    let shelved = button(ctx, shelf, id.with("shelve"), tray, live && !face.shelved);
    ctx.canvas.since(mark).iter_mut().for_each(|shape| fade(shape, trail));
    ran.then_some(Hit::AsAdmin).or(revealed.then_some(Hit::Reveal)).or(shelved.then_some(Hit::Shelve))
}

fn button(ctx: &mut PageCtx<'_>, rect: Rect, id: Id, glyph: Glyph, live: bool) -> bool {
    let response = ctx.ui.interact(rect, id, if live { Sense::click() } else { Sense::hover() });
    let hover = controls::animate(ctx.ui, id.with("hover"), live && response.hovered());
    glass::paint(ctx.canvas, rect, rect.height() * 0.5, Glass::lerp(Glass::CONTROL, Glass::TILE_HOVER, hover));
    glyph::paint(ctx.canvas, glyph, rect.center(), BUTTON_GLYPH, color::mix(label(), color::PEARL, hover));
    live && response.clicked()
}

fn title(ctx: &mut PageCtx<'_>, left_center: Pos2, face: &Face<'_>, room: f32) -> f32 {
    let text = face.cut.text(&face.row.name);
    if !face.two_tone {
        let width = ctx.canvas.text_width(&text, font::headline());
        ctx.canvas.text(left_center, Align2::LEFT_CENTER, text, font::headline(), label());
        return width;
    }
    let lit = lit_letters(face, text.chars().count());
    let format = |on: bool| TextFormat {
        font_id: font::headline(),
        color: if on { color::PEARL } else { white(UNMATCHED) },
        ..Default::default()
    };
    let mut job = LayoutJob::default();
    let mut run = String::new();
    let mut run_lit = lit.first().copied().unwrap_or(false);
    for (letter, on) in text.chars().zip(lit) {
        if on != run_lit {
            job.append(&std::mem::take(&mut run), 0.0, format(run_lit));
            run_lit = on;
        }
        run.push(letter);
    }
    job.append(&run, 0.0, format(run_lit));
    job.wrap.max_width = room.max(1.0);
    job.wrap.max_rows = 1;
    let galley = ctx.canvas.painter().layout_job(job);
    let rect = Align2::LEFT_CENTER.anchor_size(left_center, galley.size());
    let width = galley.size().x;
    ctx.canvas.add(Shape::galley(rect.min, galley, Color32::WHITE));
    width
}

fn lit_letters(face: &Face<'_>, count: usize) -> Vec<bool> {
    let mut lit = vec![false; count];
    for mark in &face.row.marks {
        for index in mark.clone() {
            if let Some(shown) = face.cut.place(usize::from(index)).filter(|&shown| shown < count) {
                lit[shown] = true;
            }
        }
    }
    lit
}

#[cfg(test)]
mod tests {
    use std::ops::Range;
    use std::path::Path;
    use std::sync::Arc;

    use super::super::cut;
    use super::*;
    use crate::bridge::Kind;
    use crate::pages::harness::{Harness, press, release};

    const NAME: &str = "a-very-long-holiday-photo.png";
    const PATH: &str = "~/Pictures/Holidays/2026/Lisbon/Rooftops";
    const RECT: Rect = Rect::from_min_max(pos2(16.0, 60.0), pos2(356.0, 112.0));

    fn found(name: &str, marks: Vec<Range<u16>>) -> FindRow {
        FindRow {
            key: 11,
            path: Arc::from(Path::new("/Users/john").join(name).as_path()),
            name: name.into(),
            place: "~".into(),
            marks,
            kind: Kind::Image,
            app: false,
            folder: false,
            cloud: false,
            modified: 0,
            why: None,
            predicted: false,
            icon: None,
        }
    }

    fn face(row: &FindRow) -> Face<'_> {
        Face {
            row,
            detail: PATH,
            hint: "",
            cut: Cut::whole(&row.name),
            two_tone: false,
            place_first: false,
            selected: false,
            shelved: false,
            alpha: 1.0,
            live: true,
        }
    }

    fn texts(shapes: &[Shape]) -> Vec<String> {
        let text = |shape: &Shape| match shape {
            Shape::Text(text) => Some(text.galley.text().to_owned()),
            _ => None,
        };
        shapes.iter().filter_map(text).collect()
    }

    fn painted(harness: &mut Harness, face: &Face<'_>, rect: Rect) -> (Out, Vec<Shape>) {
        harness.frame(Vec::new(), |ctx| {
            let out = paint(ctx, rect, face, &mut Icons::default());
            (out, std::mem::take(&mut ctx.canvas.shapes))
        })
    }

    #[test]
    fn text_gets_leftover_width() {
        let file = found("notes.txt", Vec::new());
        assert_eq!(text_room(RECT, &file), 340.0 - 2.0 * INSET - WELL - 2.0 * TEXT_GAP - TRAILING_W);
        assert_eq!(text_room(RECT, &file), 204.0);
        let app = FindRow { app: true, ..found("Notepad++.lnk", Vec::new()) };
        let room = if cfg!(windows) { 204.0 - BUTTON - BUTTON_GAP } else { 204.0 };
        assert_eq!(text_room(RECT, &app), room, "the administrator button takes its own room");
    }

    #[test]
    fn row_known_by_key() {
        assert_eq!(id_of(5), id_of(5));
        assert_ne!(id_of(5), id_of(6));
    }

    #[test]
    fn matched_letters_light_up() {
        let row = found("report.pdf", vec![0..3, 7..10]);
        let lit = lit_letters(&face(&row), 10);
        assert_eq!(lit, [true, true, true, false, false, false, false, true, true, true]);
    }

    #[test]
    fn cut_letters_stay_dark() {
        let row = found(NAME, vec![2..4, 12..19, 26..29, 40..42]);
        let short = cut::middle(NAME, |text| text.chars().count() <= 10);
        assert_eq!(short.text(NAME), "a-ver\u{2026}.png");
        let lit = lit_letters(&Face { cut: short, ..face(&row) }, 10);
        assert_eq!(lit, [false, false, true, true, false, false, false, true, true, true]);
    }

    #[test]
    fn unmarked_name_is_dark() {
        let row = found(NAME, Vec::new());
        assert!(lit_letters(&face(&row), NAME.chars().count()).iter().all(|on| !on));
        assert!(lit_letters(&face(&row), 0).is_empty());
    }

    #[test]
    fn resting_row_shows_name_and_folder() {
        let row = found("notes.txt", Vec::new());
        let (out, shapes) = painted(&mut Harness::new(), &Face { detail: "~/Notes", ..face(&row) }, RECT);
        assert_eq!(texts(&shapes), ["notes.txt"], "no place, no date, nothing to read past");
        assert_eq!(out.hit, None);
        let twin = Face { detail: "~/Notes", hint: "Notes", ..face(&row) };
        assert_eq!(texts(&painted(&mut Harness::new(), &twin, RECT).1), ["notes.txt", "Notes"]);
    }

    #[test]
    fn resting_row_is_smaller_and_dimmer() {
        let row = found("notes.txt", Vec::new());
        let title = |shapes: &[Shape]| {
            shapes.iter().find_map(|shape| match shape {
                Shape::Text(text) if text.galley.text() == "notes.txt" => {
                    Some((text.visual_bounding_rect().height(), text.opacity_factor))
                }
                _ => None,
            })
        };
        let (_, resting) = painted(&mut Harness::new(), &face(&row), RECT);
        let (_, selected) = painted(&mut Harness::new(), &Face { selected: true, ..face(&row) }, RECT);
        let (small, dim) = title(&resting).expect("the title is painted");
        let (full, bright) = title(&selected).expect("the title is painted");
        assert!((small / full - AT_REST).abs() < 0.02, "{small} beside {full}");
        assert!((dim - QUIET).abs() < 1e-3 && (bright - 1.0).abs() < 1e-3, "{dim} beside {bright}");
    }

    #[test]
    fn selected_row_shows_place_and_buttons() {
        let row = found("notes.txt", Vec::new());
        let mut harness = Harness::new();
        let (_, shapes) = painted(&mut harness, &Face { selected: true, hint: "Notes", ..face(&row) }, RECT);
        assert_eq!(texts(&shapes), ["notes.txt", PATH], "its own line says where, so the hint is not needed");
        assert!(harness.drawn(id_of(row.key).with("reveal")));
        assert!(harness.drawn(id_of(row.key).with("shelve")));
        assert!(!harness.drawn(id_of(row.key).with("admin")), "a file is not run as administrator");
    }

    #[test]
    fn windows_app_row_runs_as_admin() {
        let row = FindRow { app: true, ..found("Notepad++.lnk", Vec::new()) };
        let face = Face { selected: true, ..face(&row) };
        let mut harness = Harness::new();
        painted(&mut harness, &face, RECT);
        let admin = id_of(row.key).with("admin");
        assert_eq!(harness.drawn(admin), cfg!(windows));
        if cfg!(windows) {
            for _ in 0..30 {
                painted(&mut harness, &face, RECT);
            }
            let at = harness.rect_of(admin).center();
            harness.frame(press(at), |ctx| paint(ctx, RECT, &face, &mut Icons::default()));
            let out = harness.frame(release(at), |ctx| paint(ctx, RECT, &face, &mut Icons::default()));
            assert_eq!(out.hit, Some(Hit::AsAdmin));
        }
    }

    #[test]
    fn long_place_keeps_its_end() {
        let row = found("notes.txt", Vec::new());
        let narrow = Rect::from_min_size(RECT.min, egui::vec2(260.0, ROW_H));
        let selected = Face { selected: true, ..face(&row) };
        let (_, kept_end) = painted(&mut Harness::new(), &Face { place_first: true, ..face(&row) }, narrow);
        assert_eq!(texts(&kept_end), ["notes.txt"], "a resting row has no place to cut");
        let (_, kept_end) = painted(&mut Harness::new(), &Face { place_first: true, ..selected }, narrow);
        let detail = texts(&kept_end).pop().unwrap_or_default();
        assert!(detail.starts_with('\u{2026}') && detail.ends_with("Rooftops"), "{detail}");
        let selected = Face { selected: true, ..face(&row) };
        let (_, kept_start) = painted(&mut Harness::new(), &selected, narrow);
        let detail = texts(&kept_start).pop().unwrap_or_default();
        assert!(detail.starts_with("~/Pictures") && detail.ends_with('\u{2026}'), "{detail}");
    }

    #[test]
    fn two_tone_title_lights_matches() {
        let row = found("report.pdf", std::iter::once(0..3).collect());
        let (_, shapes) = painted(&mut Harness::new(), &Face { two_tone: true, ..face(&row) }, RECT);
        let title = shapes.iter().find_map(|shape| match shape {
            Shape::Text(text) if text.galley.text() == "report.pdf" => Some(text.galley.clone()),
            _ => None,
        });
        let title = title.expect("the title is painted");
        let runs: Vec<(&str, Color32)> = title
            .job
            .sections
            .iter()
            .map(|section| {
                (&title.job.text[section.byte_range.start.0..section.byte_range.end.0], section.format.color)
            })
            .collect();
        assert_eq!(runs, [("rep", color::PEARL), ("ort.pdf", white(UNMATCHED))]);
    }

    #[test]
    fn click_opens_live_rows_only() {
        for (live, expected) in [(true, Some(Hit::Open)), (false, None)] {
            let row = found("notes.txt", Vec::new());
            let face = Face { live, ..face(&row) };
            let mut harness = Harness::new();
            let at = pos2(RECT.left() + 120.0, RECT.center().y);
            painted(&mut harness, &face, RECT);
            harness.frame(press(at), |ctx| paint(ctx, RECT, &face, &mut Icons::default()));
            let out = harness.frame(release(at), |ctx| paint(ctx, RECT, &face, &mut Icons::default()));
            assert_eq!(out.hit, expected, "live: {live}");
        }
    }
}
