//! The row that shows while the shelf works on its files: a ring, what is being made, and Cancel.

use std::f32::consts::{FRAC_PI_2, TAU};

use egui::{Align2, Id, Rect, Sense, Shape, Stroke, pos2};

use super::buttons::Buttons;
use crate::bridge::Action;
use crate::shelf::{Busy, Item, Op};
use crate::ui::canvas::Canvas;
use crate::ui::color::{self, label, secondary_label, white};
use crate::ui::{controls, font};

const RING: f32 = 22.0;
const RING_INSET: f32 = 4.0;
const RING_W: f32 = 2.5;
const RING_TRACK: f32 = 0.16;
const RING_STEPS: usize = 48;
const TEXT_GAP: f32 = 12.0;
const LINE: f32 = 8.0;
const CANCEL: &str = "Cancel";
const CANCEL_SLOP: f32 = 6.0;
const CANCEL_GAP: f32 = 12.0;

pub(super) fn row(buttons: &mut Buttons, row: Rect, busy: &Busy, ring: f32, items: &[Item], actions: &mut Vec<Action>) {
    let cy = row.center().y;
    let center = pos2(row.left() + RING_INSET + RING * 0.5, cy);
    paint_ring(buttons.canvas, center, ring);

    let width = buttons.canvas.text_width(CANCEL, font::headline());
    let cancel = Rect::from_min_max(pos2(row.right() - width, row.top()), row.max);
    let id = Id::new("notchbase.shelf.cancel");
    let response =
        buttons.live.then(|| buttons.ui.interact(cancel.expand2(egui::vec2(CANCEL_SLOP, 0.0)), id, Sense::click()));
    let hover = controls::animate(buttons.ui, id.with("hover"), response.as_ref().is_some_and(|r| r.hovered()));
    let tint = color::mix(label(), egui::Color32::WHITE, hover);
    buttons.canvas.text(pos2(row.right(), cy), Align2::RIGHT_CENTER, CANCEL, font::headline(), tint);
    if response.is_some_and(|r| r.clicked()) {
        actions.push(Action::ShelfCancel);
    }

    let left = center.x + RING * 0.5 + TEXT_GAP;
    let room = cancel.left() - CANCEL_GAP - left;
    let title = controls::truncate(buttons.canvas, &title(busy.op), font::headline(), room);
    buttons.canvas.text(pos2(left, cy - LINE), Align2::LEFT_CENTER, title, font::headline(), label());
    let detail = controls::truncate(buttons.canvas, &detail(busy, items), font::subheadline(), room);
    buttons.canvas.text(pos2(left, cy + LINE), Align2::LEFT_CENTER, detail, font::subheadline(), secondary_label());
}

fn paint_ring(canvas: &mut Canvas, center: egui::Pos2, progress: f32) {
    let radius = (RING - RING_W) * 0.5;
    canvas.ring(center, radius, Stroke::new(RING_W, white(RING_TRACK)));
    let turn = progress.clamp(0.0, 1.0) * TAU;
    if turn <= 0.01 {
        return;
    }
    let steps = ((RING_STEPS as f32 * progress).ceil() as usize).max(2);
    let arc = (0..=steps).map(|step| {
        let angle = -FRAC_PI_2 + turn * step as f32 / steps as f32;
        center + egui::vec2(angle.cos(), angle.sin()) * radius
    });
    canvas.add(Shape::line(arc.collect(), Stroke::new(RING_W, label())));
}

fn title(op: Op) -> String {
    match op {
        Op::Convert(format) => format!("Converting to {}", format.label()),
        Op::Encode(clip) => format!("Converting to {}", clip.label()),
        Op::Compress => "Compressing".to_owned(),
        Op::Combine => "Combining".to_owned(),
        Op::Split => "Splitting".to_owned(),
    }
}

fn detail(busy: &Busy, items: &[Item]) -> String {
    let count = busy.items.len();
    let name = |id| items.iter().find(|item| item.id == id).map(|item| item.name.clone());
    let first = busy.items.first().and_then(|&id| name(id));
    let percent = format!("{:.0}%", (busy.progress * 100.0).clamp(0.0, 100.0));
    let what = match (busy.op, count, first) {
        (Op::Combine, count, _) => format!("{count} files"),
        (_, 1, Some(first)) => first,
        (_, count, _) => {
            let current = ((busy.progress * count as f32) as usize + 1).min(count);
            format!("{current} of {count}")
        }
    };
    format!("{what} \u{b7} {percent}")
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::time::Instant;

    use super::*;
    use crate::shelf::{Clip, Format, ItemId, Kind, Origin};

    fn item(id: u64) -> Item {
        Item {
            id: ItemId(id),
            path: PathBuf::from(format!("{id}.png")),
            name: format!("{id}.png"),
            kind: Kind::Image,
            ext: "PNG".into(),
            bytes: None,
            pages: None,
            seconds: None,
            dims: None,
            thumb: None,
            thumb_rev: 0,
            added: Instant::now(),
            origin: Origin::Dropped,
        }
    }

    #[test]
    fn title_says_what_is_made() {
        assert_eq!(title(Op::Encode(Clip::Mp3)), "Converting to MP3");
        assert_eq!(title(Op::Compress), "Compressing");
        for format in Format::ALL {
            assert_eq!(title(Op::Convert(format)), format!("Converting to {}", format.label()));
        }
        for clip in [Clip::Mp4, Clip::Mov, Clip::Gif, Clip::M4a, Clip::Mp3, Clip::Wav, Clip::Flac] {
            assert_eq!(title(Op::Encode(clip)), format!("Converting to {}", clip.label()));
        }
    }

    #[test]
    fn line_names_file_and_progress() {
        let items = [item(1), item(2), item(3)];
        let busy = |op, progress, ids: &[u64]| Busy { op, progress, items: ids.iter().map(|&id| ItemId(id)).collect() };
        assert_eq!(detail(&busy(Op::Compress, 0.1, &[2]), &items), "2.png \u{b7} 10%");
        assert_eq!(detail(&busy(Op::Compress, 0.5, &[1, 2, 3]), &items), "2 of 3 \u{b7} 50%");
        assert_eq!(detail(&busy(Op::Compress, 1.0, &[1, 2, 3]), &items), "3 of 3 \u{b7} 100%");
        assert_eq!(detail(&busy(Op::Combine, 0.5, &[1, 2]), &items), "2 files \u{b7} 50%");
    }
}
