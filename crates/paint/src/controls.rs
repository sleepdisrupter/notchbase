//! Sliders, device rows, popup buttons, keycaps and text truncation.

use egui::{Align2, Color32, FontId, Id, Pos2, Rect, Response, Sense, pos2, vec2};

use crate::canvas::Canvas;
use crate::color::{self, label, secondary_label, white};
use crate::font;
use crate::glass::{self, Glass};
use crate::glyph::{self, Glyph};
use crate::meter::{self, Reading, Tint};
use crate::metrics;
use crate::spring::{lerp, smoothstep};

const TRACK_GLOW: f32 = 0.55;

pub fn section_header(canvas: &mut Canvas, left_center: Pos2, text: &str) -> Rect {
    canvas.text(left_center, Align2::LEFT_CENTER, text, font::section(), secondary_label())
}

pub fn row_highlight(canvas: &mut Canvas, rect: Rect, t: f32) {
    if t > 0.001 {
        canvas.rect(rect, metrics::CONTROL_RADIUS, white(0.08 * t));
    }
}

/// Eases toward 1 while `on` and back to 0 otherwise, over 140 ms.
pub fn animate(ui: &egui::Ui, id: Id, on: bool) -> f32 {
    ui.ctx().animate_bool_with_time(id, on, 0.14)
}

/// A glass track filled up to `value`. `level` shows a live audio meter inside it and `dimmed`
/// fades it, as for a muted volume.
pub struct Slider {
    pub value: f32,
    pub level: Option<Reading>,
    pub leading: Option<Glyph>,
    pub dimmed: f32,
    pub enabled: bool,
    pub thickness: Option<Thickness>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Thickness {
    pub rest: f32,
    pub hover: f32,
}

impl Thickness {
    fn track(self, area: Rect, hover: f32) -> Rect {
        let h = lerp(self.rest, self.hover, hover).min(area.height());
        Rect::from_center_size(area.center(), vec2(area.width(), h))
    }
}

pub struct SliderResponse {
    pub response: Response,
    pub changed: Option<f32>,
}

impl Slider {
    pub fn show(self, ui: &egui::Ui, canvas: &mut Canvas, area: Rect, id: Id) -> SliderResponse {
        let sense = if self.enabled { Sense::click_and_drag() } else { Sense::hover() };
        let response = ui.interact(area, id, sense);
        let changed = if self.enabled { self.input(ui, &response, area) } else { None };
        let shown = changed.unwrap_or(self.value).clamp(0.0, 1.0);
        let hover = animate(ui, id.with("hover"), response.hovered() || response.dragged());
        let rect = self.thickness.map_or(area, |t| t.track(area, hover));
        glass::paint(canvas, rect, rect.height() * 0.5, Glass::lerp(Glass::CONTROL, Glass::TILE_HOVER, hover * 0.6));
        let lit = 1.0 - self.dimmed.clamp(0.0, 1.0);
        if let Some(reading) = self.level.filter(|_| lit > 0.0) {
            meter::paint(canvas, rect, rect.width(), reading, rect.width(), Tint::LEVEL.faded(TRACK_GLOW * lit));
        }
        if shown > 0.0 {
            self.paint_fill(canvas, rect, shown);
        }
        if let Some(g) = self.leading {
            self.paint_leading(canvas, rect, g, shown > 0.0);
        }
        SliderResponse { response, changed }
    }

    fn input(&self, ui: &egui::Ui, response: &Response, rect: Rect) -> Option<f32> {
        if response.is_pointer_button_down_on() || response.dragged() {
            return response.interact_pointer_pos().map(|pos| ((pos.x - rect.left()) / rect.width()).clamp(0.0, 1.0));
        }
        let notches = if response.hovered() { crate::scroll::across(ui) } else { 0.0 };
        (notches != 0.0).then(|| (self.value + notches * 0.02).clamp(0.0, 1.0)) // 2% per wheel notch
    }

    fn paint_fill(&self, canvas: &mut Canvas, rect: Rect, shown: f32) {
        let h = rect.height();
        let fill_w = (rect.width() * shown).max(h);
        let dimmed = self.dimmed.clamp(0.0, 1.0);
        canvas.rect(Rect::from_min_size(rect.min, vec2(fill_w, h)), h * 0.5, white(lerp(0.92, 0.32, dimmed)));
        if let Some(reading) = self.level.filter(|_| dimmed < 1.0) {
            meter::paint(canvas, rect, rect.width(), reading, fill_w, Tint::LEVEL.faded(1.0 - dimmed));
        }
    }

    fn paint_leading(&self, canvas: &mut Canvas, rect: Rect, g: Glyph, on_fill: bool) {
        let radius = rect.height() * 0.5;
        let center = pos2(rect.left() + radius, rect.center().y);
        let dimmed = self.dimmed.clamp(0.0, 1.0);
        let tint = if on_fill { color::mix(Color32::from_black_alpha(165), white(0.75), dimmed) } else { white(0.75) };
        glyph::paint(canvas, g, center, rect.height() * 0.52, tint);
    }
}

pub fn device_row(
    ui: &egui::Ui,
    canvas: &mut Canvas,
    rect: Rect,
    id: Id,
    g: Option<Glyph>,
    title: &str,
    selected: bool,
) -> Response {
    let response = ui.interact(rect, id, Sense::click());
    row_highlight(canvas, rect, animate(ui, id.with("hover"), response.hovered()));
    let sel = animate(ui, id.with("selected"), selected);
    let text_left = match g {
        Some(g) => {
            let well = pos2(rect.left() + 6.0 + metrics::ICON_WELL * 0.5, rect.center().y);
            canvas.circle(well, metrics::ICON_WELL * 0.5, color::mix(white(0.14), color::ACCENT, sel));
            glyph::paint(
                canvas,
                g,
                well,
                metrics::ICON_WELL * 0.56,
                color::mix(white(0.8), color::ink_on(color::ACCENT), sel),
            );
            well.x + metrics::ICON_WELL * 0.5 + 10.0
        }
        None => {
            let dot = pos2(rect.left() + 12.0, rect.center().y);
            canvas.circle(dot, 3.0, color::ACCENT.gamma_multiply(sel));
            dot.x + 12.0
        }
    };
    let max_w = rect.right() - text_left - 8.0;
    let text = truncate(canvas, title, font::body(), max_w);
    canvas.text(pos2(text_left, rect.center().y), Align2::LEFT_CENTER, text, font::body(), label());
    response
}

pub fn popup_button(ui: &egui::Ui, canvas: &mut Canvas, rect: Rect, id: Id, title: &str, open: bool) -> Response {
    let response = ui.interact(rect, id, Sense::click());
    let hover = animate(ui, id.with("hover"), response.hovered());
    glass::paint(canvas, rect, rect.height() * 0.5, Glass::lerp(Glass::CONTROL, Glass::TILE_HOVER, hover));
    let turned = animate(ui, id.with("open"), open);
    let cx = rect.right() - rect.height() * 0.5 - 2.0;
    glyph::chevron_turning(canvas, pos2(cx, rect.center().y), 9.0, secondary_label(), smoothstep(0.0, 1.0, turned));
    let left = rect.left() + rect.height() * 0.5;
    let text = truncate(canvas, title, font::callout(), cx - 10.0 - left);
    canvas.text(pos2(left, rect.center().y), Align2::LEFT_CENTER, text, font::callout(), label());
    response
}

/// Shortens `text` to fit `max_width`, ending it with an ellipsis.
pub fn truncate(canvas: &Canvas, text: &str, font: FontId, max_width: f32) -> String {
    if canvas.text_width(text, font.clone()) <= max_width {
        return text.to_owned();
    }
    let chars: Vec<char> = text.chars().collect();
    let (mut lo, mut hi) = (0usize, chars.len());
    while lo < hi {
        let mid = (lo + hi).div_ceil(2);
        let candidate: String = chars[..mid].iter().collect::<String>().trim_end().to_owned() + "…";
        if canvas.text_width(candidate, font.clone()) <= max_width {
            lo = mid;
        } else {
            hi = mid - 1;
        }
    }
    chars[..lo].iter().collect::<String>().trim_end().to_owned() + "…"
}

/// Like `truncate` but cuts from the start, which suits paths.
pub fn truncate_start(canvas: &Canvas, text: &str, font: FontId, max_width: f32) -> String {
    if canvas.text_width(text, font.clone()) <= max_width {
        return text.to_owned();
    }
    let chars: Vec<char> = text.chars().collect();
    let (mut lo, mut hi) = (0usize, chars.len());
    while lo < hi {
        let mid = (lo + hi) / 2;
        let candidate: String = std::iter::once('…').chain(chars[mid..].iter().copied()).collect();
        if canvas.text_width(candidate, font.clone()) <= max_width {
            hi = mid;
        } else {
            lo = mid + 1;
        }
    }
    std::iter::once('…').chain(chars[lo..].iter().copied()).collect()
}

pub fn keycaps(canvas: &mut Canvas, left_center: Pos2, keys: &[String], alpha: f32) -> Rect {
    let font = font::semibold(10.5);
    let height = 17.0;
    let gap = 3.0;
    let mut x = left_center.x;
    let mut bounds = Rect::NOTHING;
    for key in keys {
        let width = (canvas.text_width(key.clone(), font.clone()) + 9.0).max(height);
        let rect = Rect::from_min_size(pos2(x, left_center.y - height * 0.5), vec2(width, height));
        let mut cap = Glass::CONTROL;
        cap.tint = cap.tint.gamma_multiply(alpha);
        cap.specular *= alpha;
        cap.edge *= alpha;
        glass::paint(canvas, rect, 5.0, cap);
        canvas.text(rect.center(), Align2::CENTER_CENTER, key.clone(), font.clone(), white(0.78 * alpha));
        bounds = bounds.union(rect);
        x += width + gap;
    }
    bounds
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn track_thickens_within_area() {
        let area = Rect::from_min_size(pos2(0.0, 10.0), vec2(100.0, 22.0));
        let thin = Thickness { rest: 14.0, hover: 18.0 };
        assert_eq!(thin.track(area, 0.0), Rect::from_min_size(pos2(0.0, 14.0), vec2(100.0, 14.0)));
        assert_eq!(thin.track(area, 1.0).height(), 18.0);
        assert_eq!(Thickness { rest: 30.0, hover: 40.0 }.track(area, 0.5), area);
    }

    fn with_canvas(check: impl Fn(&Canvas)) {
        let ctx = egui::Context::default();
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| check(&Canvas::new(ui.painter())));
        output.textures_delta.clear();
    }

    fn truncate_start_by_scanning(canvas: &Canvas, text: &str, font: FontId, max_width: f32) -> String {
        if canvas.text_width(text, font.clone()) <= max_width {
            return text.to_owned();
        }
        let chars: Vec<char> = text.chars().collect();
        (1..chars.len())
            .map(|skip| std::iter::once('…').chain(chars[skip..].iter().copied()).collect::<String>())
            .find(|candidate| canvas.text_width(candidate.as_str(), font.clone()) <= max_width)
            .unwrap_or_else(|| '…'.to_string())
    }

    #[test]
    fn long_paths_keep_their_end() {
        with_canvas(|canvas| {
            let path = "C:/Users/someone/Documents/GitHub/notchbase/target/debug/tools";
            let short = truncate_start(canvas, path, font::caption(), 80.0);
            assert!(short.starts_with('…') && short.ends_with("tools"));
            assert!(canvas.text_width(short, font::caption()) <= 80.0);
            assert_eq!(truncate_start(canvas, "tools", font::caption(), 400.0), "tools");
        });
    }

    #[test]
    fn truncate_start_matches_scanning() {
        with_canvas(|canvas| {
            let texts = [
                "/Applications/notchbase.app",
                r"C:\Users\someone\AppData\Local\Programs\notchbase",
                "Installs to /Users/somebody with a long name/Applications",
                "日本語のフォルダ/とても長い名前のファイル",
                "x",
                "",
            ];
            for text in texts {
                for width in [0.0, 6.0, 12.0, 30.0, 55.0, 80.0, 140.0, 400.0] {
                    let halved = truncate_start(canvas, text, font::caption(), width);
                    let scanned = truncate_start_by_scanning(canvas, text, font::caption(), width);
                    assert_eq!(halved, scanned, "{text:?} at {width}");
                }
            }
        });
    }

    #[test]
    fn truncate_keeps_short_text_whole() {
        with_canvas(|canvas| {
            assert_eq!(truncate(canvas, "Speakers", font::body(), 400.0), "Speakers");
            assert_eq!(truncate(canvas, "", font::body(), 0.0), "");
            let cut = truncate(canvas, "MacBook Pro Speakers and more", font::body(), 70.0);
            assert!(cut.ends_with('…') && cut.starts_with("Mac"), "{cut}");
            assert!(canvas.text_width(cut.as_str(), font::body()) <= 70.0);
            assert_eq!(truncate(canvas, "Speakers", font::body(), 1.0), "…");
        });
    }

    #[test]
    fn no_space_before_ellipsis() {
        with_canvas(|canvas| {
            let text = "one two three four five six";
            for width in [20.0, 40.0, 60.0, 80.0, 100.0] {
                let cut = truncate(canvas, text, font::body(), width);
                assert!(!cut.contains(" …"), "{cut:?} at {width}");
            }
        });
    }

    #[test]
    fn muted_slider_dims_gradually() {
        let ctx = egui::Context::default();
        let mut fills = Vec::new();
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            for dimmed in [0.0, 0.5, 1.0] {
                let mut canvas = Canvas::new(ui.painter());
                let slider = Slider { value: 0.6, level: None, leading: None, dimmed, enabled: false, thickness: None };
                let area = Rect::from_min_size(pos2(10.0, 10.0), vec2(200.0, 12.0));
                slider.show(ui, &mut canvas, area, Id::new(("slider", (dimmed * 10.0) as u8)));
                let fill = canvas.shapes.iter().rev().find_map(|shape| match shape {
                    egui::Shape::Rect(rect) => Some(rect.fill.a()),
                    _ => None,
                });
                fills.push(fill.expect("a fill"));
            }
        });
        output.textures_delta.clear();
        assert!(fills[0] > fills[1] && fills[1] > fills[2], "{fills:?}");
    }
}
