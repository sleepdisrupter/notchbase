//! Document and folder icons for files without a thumbnail, with the extension on a tag.

use egui::{Align2, Color32, Pos2, Rect, Shape, pos2, vec2};

use crate::shelf::Kind;
use crate::ui::canvas::Canvas;
use crate::ui::color::{self, white};
use crate::ui::{font, shape};

const TAG_CHARS: usize = 4;
const TAG_SIZE: f32 = 8.0;

pub(super) fn document_icon(canvas: &mut Canvas, center: Pos2, size: f32, ext: &str, kind: Kind) {
    let (w, h) = (size * 0.74, size);
    let page = Rect::from_center_size(center, vec2(w, h));
    paint_page(canvas, page, w * 0.3);
    if !ext.is_empty() {
        let tag_center = pos2(center.x, page.bottom() - h * 0.26);
        paint_tag(canvas, tag_center, w * 0.8, extension_tag(ext), tag_color(kind));
    }
}

pub(super) fn folder_icon(canvas: &mut Canvas, center: Pos2, size: f32) {
    let body = Rect::from_center_size(center + vec2(0.0, size * 0.06), vec2(size, size * 0.72));
    let tab = Rect::from_min_size(body.left_top() - vec2(0.0, size * 0.1), vec2(size * 0.42, size * 0.2));
    let back = color::mix(color::BLUE, Color32::BLACK, 0.18);
    canvas.rect(tab, 2.5, back);
    canvas.rect(body, 3.5, back);
    canvas.rect(Rect::from_min_max(body.left_top() + vec2(0.0, size * 0.14), body.max), 3.5, color::BLUE);
}

fn paint_page(canvas: &mut Canvas, page: Rect, fold: f32) {
    let ppp = canvas.pixels_per_point();
    let (fold_left, fold_bottom) = (page.right() - fold, page.top() + fold);
    let sheet = [
        page.left_top(),
        pos2(fold_left, page.top()),
        pos2(page.right(), fold_bottom),
        page.right_bottom(),
        page.left_bottom(),
    ];
    canvas.add(Shape::mesh(shape::fill_polygon(&sheet, page.center(), white(0.9), ppp)));
    let corner = [pos2(fold_left, page.top()), pos2(fold_left, fold_bottom), pos2(page.right(), fold_bottom)];
    let corner_center = pos2(page.right() - fold * 0.66, page.top() + fold * 0.66);
    canvas.add(Shape::mesh(shape::fill_polygon(&corner, corner_center, white(0.6), ppp)));
}

fn paint_tag(canvas: &mut Canvas, center: Pos2, min_width: f32, tag: &str, band: Color32) {
    let text_w = canvas.text_width(tag, font::semibold(TAG_SIZE));
    let rect = Rect::from_center_size(center, vec2((text_w + 6.0).max(min_width), 10.0));
    canvas.rect(rect, 2.5, band);
    canvas.text(rect.center(), Align2::CENTER_CENTER, tag, font::semibold(TAG_SIZE), color::ink_on(band));
}

fn extension_tag(ext: &str) -> &str {
    ext.char_indices().nth(TAG_CHARS).map_or(ext, |(end, _)| &ext[..end])
}

fn tag_color(kind: Kind) -> Color32 {
    match kind {
        Kind::Pdf => color::RED,
        Kind::Image => color::GREEN,
        Kind::Video => color::INK,
        Kind::Audio => color::YELLOW,
        Kind::Folder | Kind::Other => color::GRAY,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tags_keep_four_chars() {
        assert_eq!(extension_tag("PDF"), "PDF");
        assert_eq!(extension_tag("JPEG"), "JPEG");
        assert_eq!(extension_tag("DOCX2"), "DOCX");
        assert_eq!(extension_tag("日本語テキスト"), "日本語テ");
    }
}
