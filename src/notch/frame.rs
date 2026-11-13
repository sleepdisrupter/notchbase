//! Paints the notch's body: the black shape, the glass of the open panel and the shadow under it.

use egui::{Color32, Id, Mesh, Painter, Pos2, Sense, Shadow, Shape};

use crate::bridge::PanelData;
use crate::ui::element::Env;
use crate::ui::spring::smoothstep;
use crate::ui::{erase, shape};

use super::geometry::Silhouette;
use super::moves::transition::Motion;
use super::{Mode, Notch};

const SHADOW_OPACITY: f32 = 0.45;
const GLASS_BOTTOM: f32 = 0.08;
const FADE_FROM: f32 = 0.32;
const GLASS_ROWS: f32 = 24.0;
const GLASS_CELL: f32 = 6.0;
const GLASS_OVERHANG: f32 = 24.0;
const HOUSING_BLACK: f32 = 6.0;
const HOUSING_FADE: f32 = 40.0;
const PEEK_COVER: f32 = 0.4;
const VISIBLE_GLASS: f32 = 0.01;
const OUTLINE_PPP: f32 = 2.0;

impl Notch {
    pub(super) fn advance(&mut self, ctx: &egui::Context, painter: &Painter, data: &PanelData, env: &Env, dt: f32) {
        self.clock.advance(dt);
        self.capturing_keys = data.shortcuts.iter().any(|shortcut| shortcut.recording);
        self.shelf_filled = !data.shelf.items().is_empty();
        self.news_due = data.app.news;
        self.welcome_due = data.welcome.due;
        self.sync_tabs(data.tabs);
        self.upload_artwork(ctx);
        self.measure_message(painter);
        self.measure_notice(painter, env);
        self.measure_ask(painter);
        self.measure_pages(painter, data, env, dt);
        self.step(dt);
    }

    pub(super) fn body(&self) -> Color32 {
        Color32::BLACK
    }

    pub(super) fn glass(&self) -> f32 {
        if crate::platform::BLURS_BEHIND { 1.0 } else { 0.0 }
    }

    fn open_glass(&self) -> f32 {
        self.glass() * self.panel_fade.value.clamp(0.0, 1.0)
    }

    pub(super) fn backdrop_at(&self, y: f32) -> Color32 {
        Color32::BLACK.gamma_multiply(self.blackness(self.silhouette().rect())(y))
    }

    fn blackness(&self, rect: egui::Rect) -> impl Fn(f32) -> f32 {
        let (from, clear) = (self.glass_from(rect), (1.0 - GLASS_BOTTOM) * self.open_glass());
        move |y| 1.0 - clear * smoothstep(from, rect.bottom(), y)
    }

    fn glass_from(&self, rect: egui::Rect) -> f32 {
        let cover = self.header_height().min(rect.height() * PEEK_COVER);
        rect.top() + (rect.height() * FADE_FROM).max(cover)
    }

    fn glass_fade(&self, rect: egui::Rect) -> Option<Mesh> {
        let from = self.glass_from(rect);
        let clear = (1.0 - GLASS_BOTTOM) * self.open_glass();
        if rect.bottom() - from <= 1.0 || clear <= 0.0 {
            return None;
        }
        let area = egui::Rect::from_min_max(
            egui::pos2(rect.left() - GLASS_OVERHANG, from),
            egui::pos2(rect.right() + GLASS_OVERHANG, rect.bottom() + GLASS_OVERHANG),
        );
        let cell = egui::vec2(GLASS_CELL, ((rect.bottom() - from) / GLASS_ROWS).max(GLASS_CELL * 0.5));
        Some(erase::grid(area, cell, |at| clear * smoothstep(from, rect.bottom(), at.y) * self.clear_of_housing(at)))
    }

    pub(super) fn clear_of_housing(&self, at: Pos2) -> f32 {
        if self.style != crate::platform::Style::Notch {
            return 1.0;
        }
        let half = self.notch.x * 0.5;
        let beside = (at.x - super::geometry::CANVAS.x * 0.5).abs() - half;
        let below = at.y - self.notch.y;
        smoothstep(HOUSING_BLACK, HOUSING_FADE, beside).max(smoothstep(HOUSING_BLACK, HOUSING_FADE, below))
    }

    pub fn backdrop(&self) -> Vec<Vec<egui::Pos2>> {
        let silhouette = self.silhouette();
        let mut outlines = self.sides_backdrop(&silhouette, OUTLINE_PPP);
        if self.glass() > VISIBLE_GLASS {
            outlines.extend(self.body_outline(&silhouette, OUTLINE_PPP).map(|(outline, _)| outline));
        }
        outlines
    }

    fn body_outline(&self, silhouette: &Silhouette, ppp: f32) -> Option<(Vec<Pos2>, Pos2)> {
        if self.flowing() {
            return self.flowing_outline(silhouette, ppp);
        }
        if let Some(surge) = self.surge_outline(silhouette, ppp) {
            return Some(surge);
        }
        let blob = silhouette.blob();
        let outline = shape::outline(&blob, ppp);
        shape::fan_origin(&blob, &outline).map(|origin| (outline, origin))
    }

    pub(super) fn paint_body(&self, painter: &Painter, silhouette: &Silhouette, open: f32) {
        let (rect, ppp) = (silhouette.rect(), painter.pixels_per_point());
        let outline = self.body_outline(silhouette, ppp);
        if let Some((outline, origin)) = &outline {
            painter.add(Shape::mesh(shape::fill_polygon(outline, *origin, self.body(), ppp)));
            painter.extend(self.glass_fade(rect).map(erase::hole));
        }
        let opacity = SHADOW_OPACITY * smoothstep(0.3, 1.0, open);
        if opacity > 0.002 {
            painter.add(shadow(opacity).as_shape(rect, silhouette.corner_radius()));
        }
        if let Some((outline, _)) = &outline {
            self.paint_keyline(painter, outline, silhouette, self.blackness(rect));
        }
    }

    pub(super) fn open_on_click(&mut self, ui: &egui::Ui) {
        if self.locked || matches!(self.mode, Mode::Expanded | Mode::Drop | Mode::Ask) {
            return;
        }
        let response = ui.interact(self.click_zone(), Id::new("notchbase.shape"), Sense::click());
        if response.clicked() && self.preview.is_none() && self.shows_sticky_notice() {
            self.dismiss_clicked_notice(response.interact_pointer_pos());
        } else if response.clicked() && self.preview.is_none() {
            self.opens_on_click();
            self.set_mode(Mode::Expanded, Motion::Open);
        }
    }
}

fn shadow(opacity: f32) -> Shadow {
    Shadow { offset: [0, 8], blur: 30, spread: 0, color: Color32::from_black_alpha((opacity * 255.0).round() as u8) }
}

#[cfg(test)]
mod tests {
    use egui::epaint::Primitive;
    use egui::vec2;

    use crate::bridge::{Page, ToolSlot};
    use crate::notch::activity::ActivityKind;
    use crate::notch::preview::{Preview, sample_copied};
    use crate::notch::tests::fixtures::{Rig, shortcuts};
    use crate::platform::Style;

    use super::*;

    fn fades(rig: &Rig, n: &mut Notch) -> usize {
        let (_, mut output) = rig.run(n, &shortcuts(None), Vec::new(), 1.0 / 60.0);
        output.textures_delta.clear();
        let primitives = rig.ctx.tessellate(std::mem::take(&mut output.shapes), output.pixels_per_point);
        let textures = primitives.iter().filter_map(|clipped| match &clipped.primitive {
            Primitive::Mesh(mesh) => Some(mesh.texture_id),
            Primitive::Callback(_) => None,
        });
        textures.filter(|texture| *texture == erase::TEXTURE).count()
    }

    #[test]
    fn only_open_panel_turns_glass() {
        let closed = [
            Preview::Idle,
            Preview::Peek,
            Preview::Working,
            Preview::Sides,
            Preview::Volume,
            Preview::Copied,
            Preview::Playing,
            Preview::Permission,
            Preview::Drop,
        ];
        for style in [Style::Island, Style::Notch] {
            let size = Some(vec2(185.0, 32.0));
            for preview in closed {
                let mut n = Notch::with_preview(style, size, Some(preview));
                assert_eq!(fades(&Rig::new(), &mut n), 0, "{preview:?} is black all the way down");
                assert_eq!(n.backdrop_at(n.silhouette().rect().bottom()), Color32::BLACK);
            }
            let mut n = Notch::with_preview(style, size, Some(Preview::Expanded));
            assert_eq!(fades(&Rig::new(), &mut n) == 1, crate::platform::BLURS_BEHIND);
            let bottom = n.backdrop_at(n.silhouette().rect().bottom());
            assert_eq!(bottom.is_opaque(), !crate::platform::BLURS_BEHIND);
            assert_eq!(n.backdrop_at(n.silhouette().rect().top() + 40.0), Color32::BLACK, "the header stays black");
        }
    }

    #[test]
    fn paints_every_state_headless() {
        let keys = shortcuts(None);
        for style in [Style::Island, Style::Notch] {
            let size = Some(vec2(185.0, 32.0));
            let previews = [
                Preview::Idle,
                Preview::Peek,
                Preview::Expanded,
                Preview::Sound,
                Preview::Stats,
                Preview::StatsHistory,
                Preview::StatsApps,
                Preview::StatsDisk,
                Preview::Copied,
                Preview::Shot,
                Preview::Message,
                Preview::Shelf,
                Preview::ShelfConvert,
                Preview::Drop,
                Preview::Recorded,
                Preview::Recording,
                Preview::Alert,
                Preview::Playing,
                Preview::Docked,
                Preview::Loud,
                Preview::Media,
                Preview::MediaQuiet,
                Preview::MediaMixer,
                Preview::Volume,
                Preview::Brightness,
                Preview::Device,
                Preview::DeviceGone,
                Preview::Settings,
                Preview::Notifications,
                Preview::AssistantSettings,
                Preview::DockSettings,
                Preview::Catalog,
                Preview::Permission,
                Preview::Question,
                Preview::Agents,
                Preview::AgentsExpanded,
                Preview::AgentsWaiting,
                Preview::AgentsEmpty,
                Preview::VideoDrop,
                Preview::VideoLayouts,
                Preview::VideoFinding,
                Preview::VideoDownloading,
                Preview::VideoSaved,
                Preview::VideoSides,
                Preview::Find,
                Preview::FindResults,
                Preview::FindActions,
                Preview::FindNone,
                Preview::FindIndexing,
                Preview::FindAccess,
                Preview::FindOff,
                Preview::WhatsNew,
                Preview::Updated,
            ];
            for preview in previews {
                let rig = Rig::new();
                let mut n = Notch::with_preview(style, size, Some(preview));
                for _ in 0..2 {
                    rig.frame(&mut n, &keys, Vec::new());
                }
            }
            let rig = Rig::new();
            let mut n = Notch::with_preview(style, size, None);
            n.set_mode(Mode::Expanded, Motion::Open);
            for _ in 0..20 {
                rig.frame(&mut n, &keys, Vec::new());
            }
            assert_eq!(n.visible_page(), Some(Page::Tools));
            n.switch_to(Page::Stats);
            for _ in 0..10 {
                rig.frame(&mut n, &keys, Vec::new());
            }
            n.show_notice(ToolSlot(0), sample_copied());
            n.set_mode(Mode::Activity, Motion::Close);
            for _ in 0..40 {
                rig.frame(&mut n, &keys, Vec::new());
            }
            let Some(ActivityKind::Tool(notice)) = &n.activity.kind else { panic!("a tool notice") };
            assert!(notice.widths.is_some_and(|[leading, trailing]| leading > 0.0 && trailing > 0.0));
            assert_eq!(n.visible_page(), None);
        }
    }
}
