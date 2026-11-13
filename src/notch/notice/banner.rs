//! The banner layout: an icon with a title and detail stacked beside it, below a camera notch or filling an island.

use egui::{Pos2, Rect, Vec2, pos2, vec2};
use notchbase_sdk::protocol::NoticeNode;

use crate::notch::geometry::Geometry;
use crate::ui::element::Measure;

pub(super) const NOTCH_GAP: f32 = 6.0;
const PAD: f32 = 12.0;
const TEXT_W: f32 = 260.0;
const ICON_GAP: f32 = 10.0;
const LINE_GAP: f32 = 2.0;
const SLACK: f32 = 1.0;
const SIDE_MIN: f32 = 14.0;
const RADIUS_MAX: f32 = 30.0;
const ISLAND_MIN: Vec2 = vec2(180.0, 37.0);
const WING: f32 = 28.0;
const NOTCH_SIDE: f32 = 18.0;
const NOTCH_RADIUS: f32 = 18.0;
const NOTCH_EAR: f32 = 6.0;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(in crate::notch) struct Banner {
    icon: Vec2,
    title: Vec2,
    detail: Option<Vec2>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(in crate::notch) struct Parts {
    pub(in crate::notch) icon: Rect,
    pub(in crate::notch) title: Rect,
    pub(in crate::notch) detail: Option<Rect>,
}

impl Banner {
    pub(in crate::notch) fn measure(measure: &Measure, node: &NoticeNode, room: f32) -> Self {
        Self {
            icon: measure.size(&node.leading, room),
            title: measure.size(&node.trailing, TEXT_W),
            detail: node.below.as_ref().map(|below| measure.size(below, TEXT_W)),
        }
    }

    pub(in crate::notch) fn size(&self) -> Vec2 {
        let column = self.column();
        vec2(self.icon.x + ICON_GAP + column.x, self.icon.y.max(column.y))
    }

    pub(in crate::notch) fn island(&self) -> Geometry {
        let block = self.size();
        let h = (block.y + 2.0 * PAD).max(ISLAND_MIN.y);
        let r = (h * 0.5).min(RADIUS_MAX);
        let side = (r - self.icon.y * 0.5).max(SIDE_MIN);
        Geometry { w: (block.x + 2.0 * side).max(ISLAND_MIN.x), h, r, ear: 0.0 }
    }

    pub(in crate::notch) fn notch(&self, camera: Vec2) -> Geometry {
        let block = self.size();
        let w = (camera.x + 2.0 * WING).max(block.x + 2.0 * NOTCH_SIDE);
        Geometry { w, h: camera.y + NOTCH_GAP + block.y + PAD, r: NOTCH_RADIUS, ear: NOTCH_EAR }
    }

    pub(in crate::notch) fn place(&self, center: Pos2) -> Parts {
        let block = Rect::from_center_size(center, self.size());
        let column = self.column();
        let icon = Rect::from_min_size(pos2(block.left(), block.center().y - self.icon.y * 0.5), self.icon);
        let left = icon.right() + ICON_GAP;
        let width = column.x + SLACK;
        let title = Rect::from_min_size(pos2(left, block.center().y - column.y * 0.5), vec2(width, self.title.y));
        let detail =
            self.detail.map(|detail| Rect::from_min_size(pos2(left, title.bottom() + LINE_GAP), vec2(width, detail.y)));
        Parts { icon, title, detail }
    }

    fn column(&self) -> Vec2 {
        match self.detail {
            Some(detail) => vec2(self.title.x.max(detail.x), self.title.y + LINE_GAP + detail.y),
            None => self.title,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn banner(detail: Option<Vec2>) -> Banner {
        Banner { icon: Vec2::splat(30.0), title: vec2(120.0, 16.0), detail }
    }

    #[test]
    fn title_and_detail_stack() {
        let parts = banner(Some(vec2(200.0, 30.0))).place(pos2(300.0, 100.0));
        let detail = parts.detail.unwrap();
        assert_eq!(parts.title.left(), parts.icon.right() + ICON_GAP);
        assert_eq!(detail.left(), parts.title.left());
        assert_eq!(detail.top(), parts.title.bottom() + LINE_GAP);
        assert!((parts.icon.center().y - 100.0).abs() < 0.01);
        assert!(((parts.title.top() + detail.bottom()) * 0.5 - 100.0).abs() < 0.01);
    }

    #[test]
    fn island_banner_hugs_content() {
        let two = Banner { icon: Vec2::splat(24.0), title: vec2(120.0, 16.0), detail: Some(vec2(200.0, 15.0)) };
        let shape = two.island();
        assert_eq!((shape.h, shape.r), (16.0 + LINE_GAP + 15.0 + 2.0 * PAD, shape.h * 0.5));
        assert_eq!(shape.w, two.size().x + 2.0 * (shape.r - 12.0));
        let tall = banner(Some(vec2(200.0, 80.0))).island();
        assert_eq!(tall.r, RADIUS_MAX);
        let short = Banner { icon: Vec2::splat(20.0), title: vec2(12.0, 16.0), detail: None }.island();
        assert_eq!((short.w, short.h), (ISLAND_MIN.x, ISLAND_MIN.y.max(20.0 + 2.0 * PAD)));
    }

    #[test]
    fn notch_banner_hangs_below() {
        let camera = vec2(185.0, 32.0);
        let small = banner(None).notch(camera);
        assert_eq!((small.w, small.h), (camera.x + 2.0 * WING, camera.y + NOTCH_GAP + 30.0 + PAD));
        let wide = banner(Some(vec2(260.0, 30.0))).notch(camera);
        assert_eq!(wide.w, 30.0 + ICON_GAP + 260.0 + 2.0 * NOTCH_SIDE);
    }
}
