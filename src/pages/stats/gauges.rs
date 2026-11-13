//! The four gauges along the top of the page.

use egui::{Align2, Color32, Id, Pos2, Rect, Sense, Stroke, pos2, vec2};

use super::arc::progress_arc;
use super::format::{format_bytes, memory_label, short_gpu_name};
use crate::pages::PageCtx;
use crate::stats::{LISTS_APPS, Resource, Snapshot};
use crate::ui::canvas::Canvas;
use crate::ui::glass::{self, Glass};
use crate::ui::spring::{Spring, UNIT_PRECISION};
use crate::ui::{color, controls, font, metrics, motion};

const COUNT: usize = 4;
const RING: f32 = 46.0;
const RING_WIDTH: f32 = 5.0;
const RADIUS: f32 = (RING - RING_WIDTH) * 0.5;
const VALUE_WIDTH: f32 = RING - 2.0 * RING_WIDTH - 6.0;
const GAUGE_TOP: f32 = 4.0;
const LABEL_GAP: f32 = 7.0;
const LABEL_H: f32 = 13.0;
const SUBLABEL_GAP: f32 = 1.0;
const SUBLABEL_H: f32 = 12.0;
const SUBLABEL_INSET: f32 = 8.0;
const LENS_W: f32 = 76.0;
const LENS_BLEED: f32 = 3.0;
const INTERACTIVE_FROM: f32 = 0.5;
pub(super) const GAUGES_H: f32 = GAUGE_TOP + RING + LABEL_GAP + LABEL_H + SUBLABEL_GAP + SUBLABEL_H;

struct Readout {
    lists: Option<Resource>,
    label: &'static str,
    color: Color32,
    value: Option<f32>,
    detail: Option<String>,
}

fn readouts(stats: &Snapshot) -> [Readout; COUNT] {
    let memory = match (stats.mem_used, stats.mem_total) {
        (Some(used), Some(total)) if total > 0 => Some((used.min(total), total)),
        _ => None,
    };
    let disk_used = match (stats.disk_free, stats.disk_total) {
        (Some(free), Some(total)) if total > 0 => Some(1.0 - (free.min(total) as f64 / total as f64) as f32),
        _ => None,
    };
    let gpu_name =
        stats.gpu_name.as_deref().map(short_gpu_name).filter(|n| !n.is_empty() && !n.eq_ignore_ascii_case("gpu"));
    [
        Readout { lists: Some(Resource::Cpu), label: "CPU", color: color::BLUE, value: stats.cpu, detail: None },
        Readout { lists: None, label: "GPU", color: color::PURPLE, value: stats.gpu, detail: gpu_name },
        Readout {
            lists: Some(Resource::Memory),
            label: "Memory",
            color: color::GREEN,
            value: memory.map(|(used, total)| (used as f64 / total as f64) as f32),
            detail: memory.map(|(used, total)| memory_label(used, total)),
        },
        Readout {
            lists: Some(Resource::Disk),
            label: "Disk",
            color: color::TEAL,
            value: disk_used,
            detail: stats.disk_free.filter(|_| disk_used.is_some()).map(|free| format!("{} free", format_bytes(free))),
        },
    ]
}

struct Gauge {
    spring: Spring,
    shown: bool,
}

impl Gauge {
    fn update(&mut self, value: Option<f32>, dt: f32) {
        match value.filter(|v| v.is_finite()) {
            Some(v) => {
                if !self.shown {
                    self.spring.snap_to(0.0);
                    self.shown = true;
                }
                self.spring.animate_to(v.clamp(0.0, 1.0), motion::VALUE);
                self.spring.step(dt);
            }
            None => {
                self.shown = false;
                self.spring.snap_to(0.0);
            }
        }
    }

    fn paint(&self, c: &mut Canvas, center: Pos2, color: Color32) {
        if !self.shown {
            c.ring(center, RADIUS, Stroke::new(RING_WIDTH, color::quaternary_label().gamma_multiply(0.6)));
            c.text(center, Align2::CENTER_CENTER, "—", font::headline(), color::tertiary_label());
            return;
        }
        let shown = self.spring.value.clamp(0.0, 1.0);
        c.ring(center, RADIUS, Stroke::new(RING_WIDTH, color::quaternary_label()));
        progress_arc(c, center, RADIUS, RING_WIDTH, shown, color);
        let text = format!("{}%", (shown * 100.0).round() as i32);
        let too_wide = c.text_width(text.as_str(), font::headline()) > VALUE_WIDTH;
        let font = if too_wide { font::section() } else { font::headline() };
        c.text(center, Align2::CENTER_CENTER, text, font, color::label());
    }
}

pub(super) struct Gauges([Gauge; COUNT]);

impl Default for Gauges {
    fn default() -> Self {
        Self(std::array::from_fn(|_| Gauge { spring: Spring::new(0.0, motion::VALUE, UNIT_PRECISION), shown: false }))
    }
}

impl Gauges {
    pub(super) fn is_animating(&self) -> bool {
        self.0.iter().any(|g| !g.spring.is_settled())
    }

    pub(super) fn ui(&mut self, ctx: &mut PageCtx<'_>, stats: &Snapshot, area: Rect) -> Option<Resource> {
        let col_w = area.width() / COUNT as f32;
        let mut picked = None;
        for (i, (gauge, readout)) in self.0.iter_mut().zip(readouts(stats)).enumerate() {
            gauge.update(readout.value, ctx.dt);
            let cx = area.left() + col_w * (i as f32 + 0.5);
            if let Some(lists) = readout.lists.filter(|_| LISTS_APPS) {
                let lens = Rect::from_center_size(pos2(cx, area.center().y), vec2(LENS_W.min(col_w), area.height()));
                let lens = lens.with_max_y(lens.bottom() + LENS_BLEED);
                if lens_ui(ctx, lens, lists, stats.listing == Some(lists)) {
                    picked = Some(lists);
                }
            }
            gauge.paint(ctx.canvas, pos2(cx, area.top() + GAUGE_TOP + RING * 0.5), readout.color);
            paint_caption(ctx.canvas, pos2(cx, area.top()), col_w, &readout);
        }
        picked
    }
}

fn lens_ui(ctx: &mut PageCtx<'_>, lens: Rect, lists: Resource, chosen: bool) -> bool {
    let id = Id::new("notchbase.stats.gauge").with(lists as u8);
    let response = (ctx.alpha >= INTERACTIVE_FROM).then(|| ctx.ui.interact(lens, id, Sense::click()));
    let hover = controls::animate(ctx.ui, id.with("hover"), response.as_ref().is_some_and(|r| r.hovered()));
    let lit = controls::animate(ctx.ui, id.with("chosen"), chosen);
    if lit > 0.01 {
        let mark = ctx.canvas.mark();
        glass::paint(ctx.canvas, lens, metrics::CARD_RADIUS, Glass::lerp(Glass::CONTROL, Glass::TILE_HOVER, hover));
        ctx.canvas.since(mark).iter_mut().for_each(|shape| crate::ui::canvas::fade(shape, lit));
    }
    if hover > 0.01 && lit < 0.99 {
        ctx.canvas.rect(lens, metrics::CARD_RADIUS, color::white(0.06 * hover * (1.0 - lit)));
    }
    response.is_some_and(|r| r.clicked())
}

fn paint_caption(c: &mut Canvas, top_center: Pos2, width: f32, readout: &Readout) {
    let label_y = top_center.y + GAUGE_TOP + RING + LABEL_GAP + LABEL_H * 0.5;
    let label = pos2(top_center.x, label_y);
    c.text(label, Align2::CENTER_CENTER, readout.label, font::subheadline(), color::secondary_label());
    if let Some(detail) = &readout.detail {
        let detail = controls::truncate(c, detail, font::caption(), width - SUBLABEL_INSET);
        let y = label_y + LABEL_H * 0.5 + SUBLABEL_GAP + SUBLABEL_H * 0.5;
        c.text(pos2(top_center.x, y), Align2::CENTER_CENTER, detail, font::caption(), color::tertiary_label());
    }
}
