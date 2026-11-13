//! Wraps messages to the row's width, up to six lines, and keeps the result for each run and width.

use std::collections::HashMap;

use egui::Color32;

use crate::ui::canvas::Canvas;
use crate::ui::font;

pub(super) const MAX_LINES: usize = 6;

#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct Wrapped {
    pub(super) lines: Vec<String>,
    pub(super) line_h: f32,
}

impl Wrapped {
    pub(super) fn extra(&self) -> f32 {
        self.lines.len().saturating_sub(1) as f32 * self.line_h
    }
}

#[derive(Default)]
pub(super) struct Wraps(HashMap<(u64, u32), Wrapped>);

impl Wraps {
    pub(super) fn get(&mut self, id: u64, width: f32, measure: impl FnOnce() -> Wrapped) -> &Wrapped {
        self.0.entry((id, key(width))).or_insert_with(measure)
    }

    pub(super) fn keep(&mut self, ids: &[u64], width: f32) {
        let width = key(width);
        self.0.retain(|(id, at), _| *at == width && ids.contains(id));
    }
}

fn key(width: f32) -> u32 {
    (width.max(0.0) * 2.0).round() as u32
}

pub(super) fn measure(canvas: &Canvas, text: &str, width: f32) -> Wrapped {
    if text.is_empty() {
        return Wrapped::default();
    }
    let galley = canvas.clamped(text, font::caption(), Color32::WHITE, width, MAX_LINES);
    let lines: Vec<String> = galley.rows.iter().map(|row| row.row.text().trim_end().to_owned()).collect();
    let line_h = galley.size().y / lines.len().max(1) as f32;
    Wrapped { lines, line_h }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use super::*;

    fn wrapped(lines: usize) -> Wrapped {
        Wrapped { lines: vec!["line".to_owned(); lines], line_h: 12.0 }
    }

    #[test]
    fn extra_lines_add_height() {
        assert_eq!(Wrapped::default().extra(), 0.0);
        assert_eq!(wrapped(1).extra(), 0.0);
        assert_eq!(wrapped(3).extra(), 24.0);
    }

    #[test]
    fn cache_keys_by_id_and_width() {
        let measured = Cell::new(0);
        let mut wraps = Wraps::default();
        let mut get = |id, width| {
            wraps.get(id, width, || {
                measured.set(measured.get() + 1);
                wrapped(2)
            });
        };
        get(1, 300.0);
        get(1, 300.0);
        assert_eq!(measured.get(), 1, "the same row at the same width is measured once");
        get(1, 280.0);
        get(2, 300.0);
        assert_eq!(measured.get(), 3, "a new width or a new row measures again");
        wraps.keep(&[1], 300.0);
        assert_eq!(wraps.0.len(), 1);
        assert!(wraps.0.contains_key(&(1, key(300.0))));
    }

    #[test]
    fn long_messages_cap_at_six_lines() {
        let ctx = egui::Context::default();
        font::install(&ctx);
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            let canvas = Canvas::new(ui.painter());
            let short = measure(&canvas, "Shipped the fix", 300.0);
            assert_eq!(short.lines, ["Shipped the fix"]);
            assert_eq!(short.extra(), 0.0);
            assert_eq!(measure(&canvas, "", 300.0), Wrapped::default());
            let long = "Shipped the fix and the tests are green again after a long afternoon. ".repeat(8);
            let capped = measure(&canvas, &long, 300.0);
            assert_eq!(capped.lines.len(), MAX_LINES);
            assert!(capped.lines[MAX_LINES - 1].ends_with('\u{2026}'), "{:?}", capped.lines);
            assert!(capped.line_h > 8.0 && capped.line_h < 20.0);
            assert!(capped.lines.iter().all(|line| canvas.text_width(line.as_str(), font::caption()) <= 300.5));
            let two = measure(&canvas, "Done\nAll green", 300.0);
            assert_eq!(two.lines, ["Done", "All green"]);
        });
        output.textures_delta.clear();
    }
}
