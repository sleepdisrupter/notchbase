//! Lays out and paints a node tree: rows, columns, frames, cards and lists, handing each leaf to its painter.

use egui::{Id, Rect, pos2, vec2};
use notchbase_sdk::protocol::{Align, Node};

use super::measure::{CARD_PAD, LIST_INSET, fills};
use super::pan::Pan;
use super::{Scene, controls, input, leaves, shortcut};
use crate::ui::glass::{self, Glass};
use crate::ui::list;
use crate::ui::metrics::{CARD_RADIUS, GAP};

pub(super) fn paint(scene: &mut Scene, node: &Node, rect: Rect, id: Id) {
    match node {
        Node::Row { children, spacing, align } => row(scene, children, spacing.unwrap_or(GAP), *align, rect, id),
        Node::Column { children, spacing, align } => column(scene, children, spacing.unwrap_or(GAP), *align, rect, id),
        Node::Frame { child, width, height, align } => {
            let size = vec2(width.unwrap_or(rect.width()), height.unwrap_or(rect.height()));
            let at = align_x(*align, rect, size.x);
            let framed = Rect::from_min_size(pos2(at, rect.center().y - size.y * 0.5), size);
            paint(scene, child, framed, id.with(0));
        }
        Node::Card { child, padding } => {
            glass::paint(scene.canvas, rect, CARD_RADIUS, Glass::TILE);
            let inner = rect.shrink(padding.unwrap_or(CARD_PAD));
            let height = scene.measure().size(child, inner.width()).y;
            paint(scene, child, Rect::from_min_size(inner.min, vec2(inner.width(), height)), id.with(0));
        }
        Node::List { children } => list(scene, children, rect, id),
        Node::Adaptive { notch, island } => {
            let chosen = scene.measure().pick(notch, island);
            paint(scene, chosen, rect, id);
        }
        Node::Keyed { key, child } => paint(scene, child, rect, Id::new(("notchbase.keyed", key.as_str())).with(id)),
        Node::Pan { child, offset, on_pan, on_tap } => {
            super::pan::paint(scene, &Pan { child, offset: *offset, on_pan: *on_pan, on_tap: *on_tap }, rect, id);
        }
        Node::Spacer { .. } => {}
        Node::Shortcut { id: shortcut } => shortcut::paint(scene, shortcut, rect),
        Node::Month(month) => super::month::paint(scene, month, rect, id),
        Node::Week(week) => super::week::paint(scene, week, rect, id),
        Node::Canvas(canvas) => super::drawing::paint(scene, canvas, rect, id),
        Node::Palette { colors, selected, on_select } => {
            super::palette::paint(scene, colors, *selected, *on_select, rect, id);
        }
        Node::TextInput { value, placeholder, on_change, on_submit } => {
            input::paint(scene, rect, id, value, placeholder, *on_change, *on_submit);
        }
        leaf if controls::handles(leaf) => controls::paint(scene, leaf, rect, id),
        leaf => leaves::paint(scene, leaf, rect, id),
    }
}

fn row(scene: &mut Scene, children: &[Node], spacing: f32, align: Align, rect: Rect, id: Id) {
    let widths = scene.measure().row_widths(children, spacing, rect.width());
    let mut x = rect.left();
    for (index, (child, width)) in children.iter().zip(widths).enumerate() {
        let height = scene.measure().size(child, width).y;
        let top = match align {
            Align::Start => rect.top(),
            Align::Center => rect.center().y - height * 0.5,
            Align::End => rect.bottom() - height,
        };
        paint(scene, child, Rect::from_min_size(pos2(x, top), vec2(width, height)), id.with(index));
        x += width + spacing;
    }
}

fn column(scene: &mut Scene, children: &[Node], spacing: f32, align: Align, rect: Rect, id: Id) {
    let mut y = rect.top();
    for (index, child) in children.iter().enumerate() {
        let size = scene.measure().size(child, rect.width());
        let width = if fills(child) { rect.width() } else { size.x };
        let child_rect = Rect::from_min_size(pos2(align_x(align, rect, width), y), vec2(width, size.y));
        paint(scene, child, child_rect, id.with(index));
        y += size.y + spacing;
    }
}

fn list(scene: &mut Scene, children: &[Node], rect: Rect, id: Id) {
    glass::paint(scene.canvas, rect, CARD_RADIUS, Glass::TILE);
    let mut y = rect.top();
    for (index, child) in children.iter().enumerate() {
        let height = scene.measure().list_row(child, rect.width());
        let inner_width = rect.width() - 2.0 * LIST_INSET;
        let child_height = scene.measure().size(child, inner_width).y;
        let top = y + (height - child_height) * 0.5;
        let child_rect = Rect::from_min_size(pos2(rect.left() + LIST_INSET, top), vec2(inner_width, child_height));
        paint(scene, child, child_rect, id.with(index));
        y += height;
        if index + 1 < children.len() {
            let row = Rect::from_min_max(pos2(rect.left(), y - height), pos2(rect.right(), y));
            list::divider(scene.canvas, row, LIST_INSET);
        }
    }
}

fn align_x(align: Align, rect: Rect, width: f32) -> f32 {
    match align {
        Align::Start => rect.left(),
        Align::Center => rect.center().x - width * 0.5,
        Align::End => rect.right() - width,
    }
}

#[cfg(test)]
mod tests {
    use notchbase_sdk::protocol::Color;

    use super::super::fixture::Stage;
    use super::super::measure::LIST_ROW_MIN;
    use super::*;

    const RED: Color = Color { r: 255, g: 0, b: 0, a: 255 };

    fn dot(size: f32) -> Node {
        Node::Dot { color: RED, size }
    }

    fn area(left: f32, top: f32, width: f32, height: f32) -> Rect {
        Rect::from_min_size(pos2(left, top), vec2(width, height))
    }

    #[test]
    fn child_aligns_start_middle_or_end() {
        let rect = area(10.0, 0.0, 100.0, 20.0);
        assert_eq!(align_x(Align::Start, rect, 30.0), 10.0);
        assert_eq!(align_x(Align::Center, rect, 30.0), 45.0);
        assert_eq!(align_x(Align::End, rect, 30.0), 80.0);
        assert_eq!(align_x(Align::End, rect, 130.0), -20.0, "a child wider than the room overhangs the start");
    }

    #[test]
    fn row_spaces_children_side_by_side() {
        let row = Node::Row { children: vec![dot(10.0), dot(20.0)], spacing: Some(5.0), align: Align::Start };
        let painted = Stage::new().paint(&row, area(100.0, 50.0, 200.0, 40.0));
        let centers: Vec<_> = painted.circles().iter().map(|circle| circle.center).collect();
        assert_eq!(centers, [pos2(105.0, 55.0), pos2(125.0, 60.0)]);
    }

    #[test]
    fn row_aligns_children_by_height() {
        let rect = area(0.0, 0.0, 200.0, 40.0);
        let middle = |align| {
            let row = Node::Row { children: vec![dot(10.0)], spacing: None, align };
            Stage::new().paint(&row, rect).circles()[0].center.y
        };
        assert_eq!(middle(Align::Start), 5.0);
        assert_eq!(middle(Align::Center), 20.0);
        assert_eq!(middle(Align::End), 35.0);
    }

    #[test]
    fn column_stacks_and_aligns_children() {
        let column = Node::Column { children: vec![dot(10.0), dot(20.0)], spacing: Some(4.0), align: Align::End };
        let painted = Stage::new().paint(&column, area(0.0, 100.0, 80.0, 200.0));
        let centers: Vec<_> = painted.circles().iter().map(|circle| circle.center).collect();
        assert_eq!(centers, [pos2(75.0, 105.0), pos2(70.0, 124.0)]);
    }

    #[test]
    fn frame_centres_and_aligns_child() {
        let frame = |align| Node::Frame { child: Box::new(dot(10.0)), width: Some(40.0), height: Some(40.0), align };
        let rect = area(0.0, 0.0, 100.0, 60.0);
        let center = |align| Stage::new().paint(&frame(align), rect).circles()[0].center;
        assert_eq!(center(Align::Start), pos2(20.0, 30.0));
        assert_eq!(center(Align::Center), pos2(50.0, 30.0));
        assert_eq!(center(Align::End), pos2(80.0, 30.0));
    }

    #[test]
    fn list_rows_part_with_inset_hairlines() {
        let list = Node::List { children: vec![dot(10.0), dot(10.0), dot(10.0)] };
        let rect = area(0.0, 0.0, 200.0, 3.0 * LIST_ROW_MIN);
        let painted = Stage::new().paint(&list, rect);
        let hairlines: Vec<Rect> = painted
            .rects()
            .iter()
            .filter(|shape| shape.fill == crate::ui::color::white(list::HAIRLINE))
            .map(|shape| shape.rect)
            .collect();
        assert_eq!(hairlines.len(), 2);
        for (index, line) in hairlines.iter().enumerate() {
            assert_eq!(line.left(), LIST_INSET);
            assert_eq!(line.right(), 200.0);
            assert_eq!(line.center().y, (index + 1) as f32 * LIST_ROW_MIN);
        }
        let rows: Vec<f32> = painted.circles().iter().map(|circle| circle.center.y).collect();
        assert_eq!(rows, [0.5, 1.5, 2.5].map(|row| row * LIST_ROW_MIN));
    }

    #[test]
    fn adaptive_node_picks_island_child() {
        let adaptive = Node::Adaptive { notch: Box::new(dot(10.0)), island: Box::new(dot(20.0)) };
        let painted = Stage::new().paint(&adaptive, area(0.0, 0.0, 40.0, 40.0));
        assert_eq!(painted.circles().len(), 1);
        assert_eq!(painted.circles()[0].radius, 10.0);
    }

    #[test]
    fn spacer_paints_nothing() {
        let painted = Stage::new().paint(&Node::Spacer { size: Some(12.0) }, area(0.0, 0.0, 40.0, 40.0));
        assert!(painted.shapes.is_empty());
    }
}
