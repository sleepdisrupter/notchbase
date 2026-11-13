//! Paint helpers for the capture overlays: pixel snapping, rectangle frames and the soft shadow under labels.

use egui::epaint::RectShape;
use egui::{Color32, CornerRadius, Mesh, Pos2, Rect, pos2, vec2};

use crate::ui::canvas::Canvas;

/// How close a label may come to the edge of the screen.
pub(in crate::capture) const EDGE_MARGIN: f32 = 8.0;

pub(in crate::capture) fn snap_pos(p: Pos2, ppp: f32) -> Pos2 {
    pos2((p.x * ppp).round() / ppp, (p.y * ppp).round() / ppp)
}

pub(in crate::capture) fn frame(mesh: &mut Mesh, outer: Rect, width: f32, color: Color32) {
    let inner = outer.shrink(width);
    mesh.add_colored_rect(Rect::from_min_max(outer.min, pos2(outer.max.x, inner.min.y)), color);
    mesh.add_colored_rect(Rect::from_min_max(pos2(outer.min.x, inner.max.y), outer.max), color);
    mesh.add_colored_rect(Rect::from_min_max(pos2(outer.min.x, inner.min.y), pos2(inner.min.x, inner.max.y)), color);
    mesh.add_colored_rect(Rect::from_min_max(pos2(inner.max.x, inner.min.y), pos2(outer.max.x, inner.max.y)), color);
}

pub(in crate::capture) fn label_shadow(canvas: &mut Canvas, rect: Rect) {
    let radius = (rect.height() * 0.5).round() as u8;
    let shadow =
        RectShape::filled(rect.translate(vec2(0.0, 2.0)), CornerRadius::same(radius), Color32::from_black_alpha(64));
    canvas.add(shadow.with_blur_width(12.0));
}
