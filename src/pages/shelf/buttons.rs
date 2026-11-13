//! The button row under the tiles: what you can do with the selection, the formats to convert to, and AirDrop.

use egui::{Align2, Color32, Id, Rect, Sense, pos2, vec2};

use super::{Feed, ShelfPage, busy};
use crate::bridge::Action;
use crate::pages::PageCtx;
use crate::shelf::{Clip, Format, Item, ItemId, Kind, Op, airdrop, available_ops};
use crate::ui::canvas::{Canvas, fade, scale_about};
use crate::ui::color::{self, label, secondary_label, tertiary_label, white};
use crate::ui::glass::{self, Glass};
use crate::ui::glyph::{self, Glyph};
use crate::ui::spring::lerp;
use crate::ui::{controls, font};

const GAP: f32 = 6.0;
const MAX_ACTION_W: f32 = 118.0;
const MAX_FORMAT_W: f32 = 88.0;
const RADIUS: f32 = 12.0;
const GLYPH: f32 = 18.0;
const GLYPH_Y: f32 = 17.0;
const TITLE_BELOW: f32 = 11.0;
const CLOSE_W: f32 = 30.0;
const PRESS_INSET: f32 = 1.0;
const FLAT_GLYPH: f32 = 0.72;
const FLAT_WASH: f32 = 0.07;
const FLAT_PRESSED: f32 = 0.92;
const LIVE_FROM: f32 = 0.6;
const SLIDE: f32 = 4.0;
const HINT: &str = "Select files to see what you can do with them";
pub(super) const AIRDROP_ID: &str = "notchbase.shelf.airdrop";

#[derive(Clone, Debug, PartialEq)]
pub(super) enum Row {
    Actions { ops: Vec<Op>, airdrop: bool },
    Formats(Vec<Target>),
    Working(Op),
    Nothing,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Target {
    Image(Format),
    Video(Clip),
    Audio(Clip),
}

impl Target {
    fn op(self) -> Op {
        match self {
            Target::Image(format) => Op::Convert(format),
            Target::Video(clip) | Target::Audio(clip) => Op::Encode(clip),
        }
    }

    fn title(self) -> &'static str {
        match self {
            Target::Image(format) => format.label(),
            Target::Video(Clip::M4a) => "Audio",
            Target::Video(clip) | Target::Audio(clip) => clip.label(),
        }
    }

    fn glyph(self) -> Glyph {
        match self {
            Target::Image(Format::Png) => Glyph::Photo,
            Target::Image(Format::Jpeg) => Glyph::Camera,
            Target::Image(Format::Webp) => Glyph::Globe,
            Target::Image(Format::Tiff) => Glyph::Layers,
            Target::Image(Format::Bmp) => Glyph::Grid,
            Target::Image(Format::Gif) => Glyph::Frames,
            Target::Image(Format::Ico) => Glyph::AppIcon,
            Target::Image(Format::Pdf) => Glyph::Document,
            Target::Video(clip) | Target::Audio(clip) => match clip {
                Clip::Mp4 => Glyph::Video,
                Clip::Mov => Glyph::Film,
                Clip::Gif => Glyph::Frames,
                Clip::M4a => Glyph::Note,
                Clip::Mp3 => Glyph::Headphones,
                Clip::Wav => Glyph::Waveform,
                Clip::Flac => Glyph::Bars,
            },
        }
    }
}

pub(super) enum Face<'a> {
    Action(Glyph, &'a str),
    Format(Glyph, &'a str),
    Close,
}

impl Face<'_> {
    fn is_flat(&self) -> bool {
        matches!(self, Face::Format(..) | Face::Close)
    }
}

pub(super) struct Buttons<'a> {
    pub(super) ui: &'a egui::Ui,
    pub(super) canvas: &'a mut Canvas,
    pub(super) live: bool,
}

impl Buttons<'_> {
    pub(super) fn button(&mut self, rect: Rect, id: Id, face: Face) -> bool {
        let response = self.live.then(|| self.ui.interact(rect, id, Sense::click()));
        let hovered = response.as_ref().is_some_and(|r| r.hovered());
        let down = response.as_ref().is_some_and(|r| r.is_pointer_button_down_on());
        let hover = controls::animate(self.ui, id.with("hover"), hovered);
        let center = rect.center();
        let mark = self.canvas.mark();
        if face.is_flat() {
            self.canvas.rect(rect, RADIUS, white(FLAT_WASH * hover));
        } else {
            let drawn = if down { rect.shrink(PRESS_INSET) } else { rect };
            glass::paint(self.canvas, drawn, RADIUS, Glass::lerp(Glass::CONTROL, Glass::TILE_HOVER, hover));
        }
        match face {
            Face::Action(glyph, title) => {
                glyph::paint(self.canvas, glyph, pos2(center.x, rect.top() + GLYPH_Y), GLYPH, label());
                let at = pos2(center.x, rect.bottom() - TITLE_BELOW);
                self.canvas.text(at, Align2::CENTER_CENTER, title, font::subheadline(), secondary_label());
            }
            Face::Format(glyph, title) => {
                let lit = color::mix(white(FLAT_GLYPH), Color32::WHITE, hover);
                glyph::paint(self.canvas, glyph, pos2(center.x, rect.top() + GLYPH_Y), GLYPH, lit);
                let at = pos2(center.x, rect.bottom() - TITLE_BELOW);
                let tint = color::mix(secondary_label(), label(), hover);
                self.canvas.text(at, Align2::CENTER_CENTER, title, font::subheadline(), tint);
            }
            Face::Close => {
                glyph::paint(
                    self.canvas,
                    Glyph::Xmark,
                    center,
                    GLYPH * 0.8,
                    color::mix(secondary_label(), label(), hover),
                );
            }
        }
        if face.is_flat() && down {
            let press = scale_about(center, FLAT_PRESSED);
            for shape in self.canvas.since(mark) {
                shape.transform(press);
            }
        }
        response.is_some_and(|r| r.clicked())
    }
}

impl ShelfPage {
    pub(super) fn buttons_ui(&mut self, ctx: &mut PageCtx<'_>, row: Rect, feed: &Feed, actions: &mut Vec<Action>) {
        let Feed { items, busy, .. } = *feed;
        let scope = self.scope(items);
        let ops = available_ops(&scope);
        let targets = targets(&scope);
        if ops.is_empty() || targets.is_empty() {
            self.formats_open = false;
        }
        let state = match busy {
            Some(busy) => Row::Working(busy.op),
            None => row_state(ops, targets, airdrop::AVAILABLE && !scope.is_empty(), self.formats_open),
        };
        let shown = self.buttons.show(Some(&state), ctx.dt);
        let ids: Vec<ItemId> = scope.iter().map(|item| item.id).collect();
        if let Some((previous, gone)) = self.buttons.previous() {
            self.paint_row(ctx, row, feed, (&previous, &ids), (gone, false), &mut Vec::new());
        }
        self.paint_row(ctx, row, feed, (&state, &ids), (shown, true), actions);
    }

    pub(super) fn buttons_leave(&mut self, ctx: &mut PageCtx<'_>, row: Rect, feed: &Feed) {
        self.buttons.show(None, ctx.dt);
        if let Some((previous, gone)) = self.buttons.previous() {
            self.paint_row(ctx, row, feed, (&previous, &[]), (gone, false), &mut Vec::new());
        }
    }

    fn paint_row(
        &mut self,
        ctx: &mut PageCtx<'_>,
        row: Rect,
        feed: &Feed,
        (state, ids): (&Row, &[ItemId]),
        (shown, current): (f32, bool),
        actions: &mut Vec<Action>,
    ) {
        if shown <= 0.001 {
            return;
        }
        let live = current && super::interactive(ctx) && shown > LIVE_FROM;
        let mark = ctx.canvas.mark();
        let mut buttons = Buttons { ui: ctx.ui, canvas: ctx.canvas, live };
        match state {
            Row::Actions { ops, airdrop } => self.action_buttons(&mut buttons, row, ops, *airdrop, ids, actions),
            Row::Formats(targets) => self.format_buttons(&mut buttons, row, targets, ids, actions),
            Row::Working(_) => {
                if let Some(busy) = feed.busy {
                    busy::row(&mut buttons, row, busy, self.ring.value, feed.items, actions);
                }
            }
            Row::Nothing => paint_hint(buttons.canvas, row),
        }
        let slide = vec2(0.0, lerp(SLIDE, 0.0, shown));
        for shape in ctx.canvas.since(mark) {
            shape.translate(slide);
            fade(shape, shown);
        }
    }

    fn action_buttons(
        &mut self,
        buttons: &mut Buttons,
        row: Rect,
        ops: &[Op],
        airdrop: bool,
        ids: &[ItemId],
        actions: &mut Vec<Action>,
    ) {
        let slots = slots(row, ops.len() + usize::from(airdrop), MAX_ACTION_W);
        for (&slot, &op) in slots.iter().zip(ops) {
            let (glyph, title) = face(op);
            let pressed = buttons.button(slot, Id::new(("notchbase.shelf.action", title)), Face::Action(glyph, title));
            if pressed {
                match op {
                    Op::Convert(_) | Op::Encode(_) => self.formats_open = true,
                    op => actions.push(Action::ShelfRun(op, ids.to_vec())),
                }
            }
        }
        let (true, Some(&slot)) = (airdrop, slots.last()) else { return };
        if buttons.button(slot, Id::new(AIRDROP_ID), Face::Action(Glyph::AirDrop, "AirDrop")) {
            self.send_airdrop(ids, actions);
        }
    }

    /// Sends what the other buttons would work on: the selection, or every file when nothing is selected.
    fn send_airdrop(&mut self, ids: &[ItemId], actions: &mut Vec<Action>) {
        if ids.is_empty() {
            return;
        }
        self.selected.clear();
        self.formats_open = false;
        self.liftoff.launch(ids);
        actions.push(Action::AirDrop(ids.to_vec()));
    }

    fn format_buttons(
        &mut self,
        buttons: &mut Buttons,
        row: Rect,
        targets: &[Target],
        ids: &[ItemId],
        actions: &mut Vec<Action>,
    ) {
        let (formats, close) = format_row(row);
        if buttons.button(close, Id::new("notchbase.shelf.back"), Face::Close) {
            self.formats_open = false;
        }
        for (slot, &target) in slots(formats, targets.len(), MAX_FORMAT_W).into_iter().zip(targets) {
            let id = Id::new(("notchbase.shelf.format", target.title()));
            if buttons.button(slot, id, Face::Format(target.glyph(), target.title())) {
                actions.push(Action::ShelfRun(target.op(), ids.to_vec()));
                self.formats_open = false;
            }
        }
    }
}

fn row_state(ops: Vec<Op>, targets: Vec<Target>, airdrop: bool, formats_open: bool) -> Row {
    if ops.is_empty() && !airdrop {
        Row::Nothing
    } else if formats_open && !ops.is_empty() && !targets.is_empty() {
        Row::Formats(targets)
    } else {
        Row::Actions { ops, airdrop }
    }
}

fn targets(scope: &[&Item]) -> Vec<Target> {
    let all = |kind: Kind| !scope.is_empty() && scope.iter().all(|item| item.kind == kind);
    if all(Kind::Image) {
        Format::ALL.into_iter().map(Target::Image).collect()
    } else if all(Kind::Video) {
        Clip::for_video().into_iter().map(Target::Video).collect()
    } else if all(Kind::Audio) {
        Clip::for_audio().into_iter().map(Target::Audio).collect()
    } else {
        Vec::new()
    }
}

fn face(op: Op) -> (Glyph, &'static str) {
    match op {
        Op::Convert(_) | Op::Encode(_) => (Glyph::Convert, "Convert"),
        Op::Compress => (Glyph::Compress, "Compress"),
        Op::Combine => (Glyph::Combine, "Combine"),
        Op::Split => (Glyph::Split, "Split"),
    }
}

fn slots(row: Rect, count: usize, max_width: f32) -> Vec<Rect> {
    if count == 0 {
        return Vec::new();
    }
    let available = row.width();
    let width = ((available - GAP * (count - 1) as f32) / count as f32).min(max_width);
    let total = width * count as f32 + GAP * (count - 1) as f32;
    let left = row.left() + (available - total) * 0.5;
    (0..count)
        .map(|i| Rect::from_min_size(pos2(left + i as f32 * (width + GAP), row.top()), vec2(width, row.height())))
        .collect()
}

fn format_row(row: Rect) -> (Rect, Rect) {
    let close = Rect::from_min_max(pos2(row.right() - CLOSE_W, row.top()), row.max);
    (Rect::from_min_max(row.min, pos2(close.left() - GAP, row.bottom())), close)
}

fn paint_hint(canvas: &mut Canvas, row: Rect) {
    let hint = controls::truncate(canvas, HINT, font::callout(), row.width());
    canvas.text(row.center(), Align2::CENTER_CENTER, hint, font::callout(), tertiary_label());
}

#[cfg(test)]
mod tests {
    use egui::Pos2;

    use super::*;

    #[test]
    fn few_buttons_stay_centered() {
        let row = Rect::from_min_size(Pos2::ZERO, vec2(340.0, 46.0));
        let two = slots(row, 2, MAX_ACTION_W);
        assert_eq!(two[0].width(), MAX_ACTION_W);
        assert!((two[0].left() - (340.0 - two[1].right())).abs() < 1e-3);
        let five = slots(row, 5, MAX_ACTION_W);
        assert!((five[4].right() - 340.0).abs() < 1e-3 && five[0].left().abs() < 1e-3);
        assert!(slots(row, 0, MAX_ACTION_W).is_empty());
    }

    #[test]
    fn formats_spread_leave_close() {
        let row = Rect::from_min_size(Pos2::ZERO, vec2(340.0, 46.0));
        let (formats, close) = format_row(row);
        assert_eq!((close.right(), close.width()), (340.0, CLOSE_W));
        let slots = slots(formats, Format::ALL.len(), MAX_FORMAT_W);
        assert!(slots[0].left().abs() < 1e-3);
        assert!((slots.last().unwrap().right() - (close.left() - GAP)).abs() < 1e-3);
    }

    #[test]
    fn every_format_has_an_icon() {
        let rows = [
            Format::ALL.into_iter().map(Target::Image).collect::<Vec<_>>(),
            [Clip::Mp4, Clip::Mov, Clip::Gif, Clip::M4a].map(Target::Video).to_vec(),
            [Clip::M4a, Clip::Mp3, Clip::Wav, Clip::Flac].map(Target::Audio).to_vec(),
        ];
        for row in rows {
            for (i, target) in row.iter().enumerate() {
                assert!(row[..i].iter().all(|other| other.glyph() != target.glyph()), "{target:?}");
            }
        }
        assert_eq!(Target::Video(Clip::M4a).title(), "Audio");
    }
}
