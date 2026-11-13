//! Lays out the header, the picture and a bullet for each change.

use std::sync::Arc;

use egui::{Galley, Rangef, Rect, pos2};

use crate::ui::canvas::Canvas;
use crate::ui::color::{label, secondary_label};
use crate::ui::font;
use crate::ui::picture::Picture;
use crate::update::current_version;
use crate::whats_new::{Change, PICTURE_WIDTH, Release};

pub(super) const GLYPH: f32 = 15.0;
pub(super) const TEXT_GAP: f32 = 12.0;
pub(super) const VERSION_GAP: f32 = 8.0;
pub(super) const CAPTION_ROWS: usize = 2;
pub(super) const CAPTION_GAP: f32 = 1.0;
pub(super) const HEADER_GAP: f32 = 18.0;
pub(super) const LIST_GAP: f32 = 20.0;
pub(super) const BULLET_GAP: f32 = 18.0;
pub(super) const EMPTY_H: f32 = 44.0;
pub(super) const TITLE_SIZE: f32 = 17.0;
pub(super) const EVEN_SLACK: f32 = 16.0;
const PRODUCT: &str = "notchbase";
const LINE: &str = "Ag";

pub(super) struct Plan {
    pub(super) column: Rangef,
    pub(super) header: Header,
    pub(super) picture: Option<(&'static Picture, Rect)>,
    pub(super) list_top: f32,
    pub(super) bullets: Vec<Bullet>,
    pub(super) height: f32,
}

pub(super) struct Header {
    pub(super) title: Arc<Galley>,
    pub(super) version: Arc<Galley>,
    pub(super) height: f32,
}

pub(super) struct Bullet {
    pub(super) change: &'static Change,
    pub(super) top: f32,
    pub(super) line: f32,
    pub(super) caption: Arc<Galley>,
}

impl Header {
    fn of(release: Option<&Release>, canvas: &Canvas) -> Self {
        let running = current_version().to_string();
        let version = format!("{PRODUCT} {}", release.map_or(running.as_str(), |release| release.version));
        let title = canvas.wrapped(super::TITLE, font::semibold(TITLE_SIZE), label(), f32::INFINITY);
        let version = canvas.wrapped(version, font::subheadline(), secondary_label(), f32::INFINITY);
        let height = title.size().y.ceil();
        Self { title, version, height }
    }
}

impl Bullet {
    fn of(canvas: &Canvas, change: &'static Change, top: f32, room: f32) -> Self {
        let line = canvas.text_size(LINE, font::headline()).y;
        Self { change, top, line, caption: caption(canvas, change.caption, room) }
    }

    pub(super) fn caption_top(&self) -> f32 {
        self.top + self.line + CAPTION_GAP
    }

    fn bottom(&self) -> f32 {
        (self.caption_top() + self.caption.size().y).ceil()
    }
}

pub(super) fn caption(canvas: &Canvas, words: &str, room: f32) -> Arc<Galley> {
    let wrapped = |width: f32| canvas.clamped(words, font::subheadline(), secondary_label(), width, CAPTION_ROWS);
    let full = wrapped(room);
    let lines = full.rows.len();
    if lines < 2 || full.elided {
        return full;
    }
    let natural = canvas.text_width(words, font::subheadline());
    let even = wrapped((natural / lines as f32 + EVEN_SLACK).min(room));
    if even.rows.len() == lines && !even.elided { even } else { full }
}

pub(super) fn text_indent() -> f32 {
    GLYPH + TEXT_GAP
}

pub(super) fn baseline(galley: &Galley) -> f32 {
    let first = galley.rows.first();
    first.and_then(|row| row.row.glyphs.first().map(|glyph| row.pos.y + glyph.pos.y)).unwrap_or(galley.size().y)
}

fn column(width: f32) -> Rangef {
    let wide = width.clamp(0.0, PICTURE_WIDTH);
    Rangef::new((width - wide) * 0.5, (width + wide) * 0.5)
}

fn framed(shot: &'static Picture, column: Rangef, top: f32) -> (&'static Picture, Rect) {
    let size = shot.fitted(column.span());
    (shot, Rect::from_min_size(pos2(column.center() - size.x * 0.5, top), size))
}

pub(super) fn plan(release: Option<&Release>, width: f32, beside_camera: bool, canvas: &Canvas) -> Plan {
    let column = column(width);
    let header = Header::of(release, canvas);
    let shot = release.and_then(|release| release.picture);
    let picture = shot.map(|shot| framed(shot.beside(beside_camera), column, header.height + HEADER_GAP));
    let list_top = picture.map_or(header.height, |(_, frame)| frame.bottom().ceil());
    let changes = release.map_or(&[][..], |release| release.changes);
    let room = (column.span() - text_indent()).max(1.0);
    let mut bullets: Vec<Bullet> = Vec::with_capacity(changes.len());
    for change in changes {
        let top = bullets.last().map_or(list_top + LIST_GAP, |above| above.bottom() + BULLET_GAP);
        bullets.push(Bullet::of(canvas, change, top, room));
    }
    let height = bullets.last().map_or(list_top + EMPTY_H, Bullet::bottom);
    Plan { column, header, picture, list_top, bullets, height }
}
