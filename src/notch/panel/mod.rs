//! The open panel: the header, the page under it and the switch from one page to the next. A page taller than the
//! screen scrolls inside the panel and fades out at its edges.

pub(super) mod header;
mod opening;
mod overflow;
pub(super) mod pager;
pub(super) mod pages;
mod welcome;

use std::time::Instant;

use egui::{Painter, Pos2, Rect, pos2, vec2};

use crate::bridge::{Action, Page, PanelData};
use crate::pages::PageCtx;
use crate::pages::tools::Frame;
use crate::ui::canvas::Canvas;
use crate::ui::element::{Env, Measure};
use crate::ui::metrics::PAD;
use crate::ui::spring::{lerp, smoothstep};

use super::geometry::CANVAS;
use super::moves::transition::Motion;
use super::{Mode, Notch};
use header::Badges;
use overflow::Overflow;
use pager::Heights;
use pages::LAYERS;

const REVEAL_BEFORE: f32 = 40.0;
const REVEAL_AFTER: f32 = 12.0;
const RISE: f32 = 6.0;
const GROW_FROM: f32 = 0.92;
const PAGE_SLIDE: f32 = 12.0;
const BACKGROUND_ALPHA: f32 = 0.2;
const VISIBLE: f32 = 0.001;
const SHADOW_ROOM: f32 = 34.0;
const SCROLL_STEP: f32 = 40.0;
const SCROLLS_FROM: f32 = 0.5;
const FOLLOWS_UP_TO: f32 = 2000.0;
const CRAMPED_SLACK: f32 = 0.5;
const SHORTEST_FRAME: f32 = 1.0 / 120.0;
const PANEL_GONE_BY: f32 = 0.12;

#[derive(Clone, Copy)]
struct PageLayer {
    page: Page,
    reveal: f32,
    opacity: f32,
    slide: f32,
    active: bool,
}

impl PageLayer {
    fn is_visible(&self) -> bool {
        self.opacity > VISIBLE
    }

    fn input_alpha(&self) -> f32 {
        if self.active { self.opacity } else { self.opacity.min(BACKGROUND_ALPHA) }
    }
}

struct Stage {
    clip: Painter,
    anchor: Pos2,
    fit: f32,
    lean: f32,
}

impl Stage {
    fn present(&self, canvas: Canvas, reveal: f32, opacity: f32, slide: f32, window: Option<Rect>) {
        let offset = vec2(self.lean + slide, lerp(-RISE, 0.0, reveal));
        let clip = match window {
            Some(window) => self.clip.with_clip_rect(self.clip.clip_rect().intersect(window)),
            None => self.clip.clone(),
        };
        canvas.paint(&clip, self.anchor, lerp(GROW_FROM, 1.0, reveal).min(self.fit), offset, opacity);
    }
}

impl Notch {
    pub(super) fn content_rect(&self, page: Page) -> Rect {
        let panel = self.body_rect(self.expanded_geometry());
        let size = vec2(self.content_width(), self.pager.height(page));
        Rect::from_min_size(pos2(panel.left() + PAD, self.header().body_top), size)
    }

    fn max_page_height(&self) -> f32 {
        CANVAS.y - self.header().body_top - PAD - SHADOW_ROOM - self.download_room()
    }

    pub(super) fn measure_pages(&mut self, painter: &Painter, data: &PanelData, env: &Env, dt: f32) {
        let canvas = Canvas::new(painter);
        let measure = Measure { canvas: &canvas, env, slot: crate::bridge::ToolSlot(0) };
        let natural = self.pages.measure(data, self.content_width(), &measure);
        let limit = self.max_page_height();
        let shown: Heights = natural.iter().map(|(page, height)| (*page, height.min(limit))).collect();
        self.pages.natural = natural;
        let before = self.pager.height(self.pager.page);
        if self.pager.remeasure(shown) && self.mode == Mode::Expanded {
            let jump = (self.pager.height(self.pager.page) - before).abs();
            self.retarget(self.page_motion(jump, dt));
        }
    }

    pub(super) fn clear_of_the_panel(&self) -> f32 {
        1.0 - smoothstep(0.0, PANEL_GONE_BY, self.panel_fade.value)
    }

    fn page_motion(&self, jump: f32, dt: f32) -> Motion {
        let follow = self.spring_configs(Motion::Follow).1;
        let resting = self.morph.h.is_settled() || self.morph.h.config == follow;
        let gliding = jump <= FOLLOWS_UP_TO * dt.max(SHORTEST_FRAME);
        if resting && gliding && !self.pager.switching() { Motion::Follow } else { Motion::Resize }
    }

    pub(super) fn panel_ui(
        &mut self,
        ui: &mut egui::Ui,
        painter: &Painter,
        data: &PanelData,
        env: &Env,
        shape: Rect,
        dt: f32,
    ) -> Vec<Action> {
        let fade = self.panel_fade.value.clamp(0.0, 1.0);
        let alpha = fade * revealed(self.header().body_top, shape);
        self.panel_alpha = alpha;
        self.never_reopen_into_a_passing_page(alpha <= VISIBLE);
        if alpha <= VISIBLE && !self.summoned() {
            return self.pages.hide();
        }
        let interactive = self.mode == Mode::Expanded && alpha > 0.5;
        let capturing = interactive && self.capturing_keys;
        let mut actions = Vec::new();
        if capturing {
            actions.extend(self.pages.capture.capture(ui, data.shortcuts));
        }

        let stage = self.stage(painter, shape);
        let reading = self.reads_news();
        let header = self.header_ui(ui, painter, interactive, Badges::of(data));
        stage.present(header, alpha, alpha, 0.0, None);
        let finding = self.pager.page == Page::Find && self.mode == Mode::Expanded;
        self.pages.find.present(finding, &mut actions);

        self.pages.keep_settings_scroll();
        let layers = self.page_layers(fade, shape, interactive);
        self.pages.drawn = layers.map(|layer| layer.map(|layer| layer.page));
        let mut keys = Vec::new();
        let mut textures = std::mem::take(&mut self.textures);
        for layer in layers.into_iter().flatten() {
            let frame = Frame { env, textures: &mut textures, keys: &mut keys };
            let (canvas, window, page_actions) = self.page_ui(ui, painter, data, &layer, dt, frame);
            actions.extend(page_actions);
            let window = match (window, self.cramped(layer.page, shape)) {
                (Some(window), Some(room)) => Some(window.intersect(room)),
                (window, room) => window.or(room),
            };
            stage.present(canvas, layer.reveal, layer.opacity, layer.slide, window);
        }
        self.textures = textures;
        let reveal = layers[1].map_or(0.0, |layer| layer.reveal);
        let (downloads, cancelled) = self.downloading_ui(ui, painter, shape, interactive && reveal > 0.5);
        stage.present(downloads, reveal, reveal, 0.0, None);
        actions.extend(cancelled);
        if let Some(page) = self.pages.route() {
            self.switch_to(page);
        }
        actions.extend(self.walked_away_from_news(reading));
        self.leave_changes_when_done();
        self.leave_welcome_when_done();
        if interactive {
            self.swipe_tabs(ui, shape);
        }

        if capturing && self.pages.capture.clicked_outside(ui, &keys) {
            actions.push(Action::CancelRecording);
        }
        actions
    }

    fn cramped(&self, page: Page, shape: Rect) -> Option<Rect> {
        let floor = shape.bottom() - PAD - self.download_room();
        let room = Rect::from_min_max(shape.min - vec2(1.0, 1.0), pos2(shape.right() + 1.0, floor));
        (self.content_rect(page).bottom() > floor + CRAMPED_SLACK).then_some(room)
    }

    fn summoned(&self) -> bool {
        let find = &self.pages.find;
        self.mode == Mode::Expanded && self.pager.page == Page::Find && (find.armed() || find.visiting())
    }

    fn content_width(&self) -> f32 {
        self.panel_width() - 2.0 * PAD
    }

    fn stage(&self, painter: &Painter, shape: Rect) -> Stage {
        let panel = self.body_rect(self.expanded_geometry());
        Stage {
            clip: painter.with_clip_rect(shape.expand(1.0)),
            anchor: pos2(panel.center().x, panel.top()),
            fit: (shape.width() / panel.width()).min(1.0),
            lean: self.lean.value + self.jolt.value + self.flee.value + self.dodge.value + self.morph.shift.value,
        }
    }

    fn page_layers(&mut self, fade: f32, shape: Rect, interactive: bool) -> [Option<PageLayer>; LAYERS] {
        let (page, from) = (self.pager.page, self.pager.from);
        let (progress, direction) = (self.pager.progress(), self.pager.direction());
        let exposed = if self.pager.switching() { 1.0 } else { revealed(self.content_rect(page).bottom(), shape) };
        let reveal = fade * self.pager.hold_reveal(page, self.mode == Mode::Expanded, exposed);

        let outgoing = self.pager.switching().then(|| {
            let reveal = fade * revealed(self.content_rect(from).bottom(), shape);
            PageLayer {
                page: from,
                reveal,
                opacity: reveal * pager::outgoing(progress),
                slide: -direction * PAGE_SLIDE * progress,
                active: false,
            }
        });
        let opacity = reveal * pager::incoming(progress);
        let incoming = PageLayer {
            page,
            reveal,
            opacity,
            slide: direction * PAGE_SLIDE * (1.0 - progress),
            active: interactive && opacity > 0.5,
        };
        let unseen_yet = self.summoned();
        let drawn = |layer: &PageLayer| layer.is_visible() || (unseen_yet && layer.page == Page::Find);
        [outgoing, Some(incoming)].map(|layer| layer.filter(drawn))
    }

    fn page_ui(
        &mut self,
        ui: &mut egui::Ui,
        painter: &Painter,
        data: &PanelData,
        layer: &PageLayer,
        dt: f32,
        frame: Frame<'_>,
    ) -> (Canvas, Option<Rect>, Vec<Action>) {
        let mut canvas = Canvas::new(painter);
        let overflow = self.overflow(layer.page);
        crate::ui::scroll::set_page_scrolls(ui, overflow.is_some());
        let Some(overflow) = overflow else {
            let rect = self.content_rect(layer.page);
            let mut ctx =
                PageCtx { ui, canvas: &mut canvas, rect, alpha: layer.input_alpha(), dt, now: Instant::now() };
            let actions = self.pages.ui(layer.page, &mut ctx, data, frame);
            return (canvas, None, actions);
        };
        let window = overflow.window;
        let mut clipped = ui.new_child(egui::UiBuilder::new().max_rect(window));
        clipped.set_clip_rect(window);
        let actions = {
            let mut ctx = PageCtx {
                ui: &mut clipped,
                canvas: &mut canvas,
                rect: overflow.content(),
                alpha: layer.input_alpha(),
                dt,
                now: Instant::now(),
            };
            self.pages.ui(layer.page, &mut ctx, data, frame)
        };
        if layer.active && ui.rect_contains_pointer(window) && !crate::ui::scroll::claimed(ui) {
            let wheel = crate::ui::scroll::read(ui);
            self.scroll_by(layer.page, wheel.down() * SCROLL_STEP, overflow.spare(), wheel.smooth);
        }
        let edges = [self.backdrop_at(window.top()), self.backdrop_at(window.bottom())];
        overflow.paint(&mut canvas, edges);
        (canvas, Some(overflow.clip()), actions)
    }

    pub(super) fn overflow(&mut self, page: Page) -> Option<Overflow> {
        let window = self.content_rect(page);
        let natural = self.pages.natural(page, window.height());
        let spare = natural - window.height();
        (spare > SCROLLS_FROM).then(|| Overflow { window, natural, offset: self.scroll_offset(page, spare) })
    }

    fn scroll_by(&mut self, page: Page, moved: f32, spare: f32, smooth: bool) {
        if moved == 0.0 {
            return;
        }
        let kept = self.pages.scroll.entry(page).or_insert(0.0);
        let before = *kept;
        *kept = (before - moved).clamp(0.0, spare);
        let target = *kept;
        if smooth || self.preview.is_some() {
            self.pages.stop_gliding(page);
        } else {
            let from = self.pages.shown_scroll(page, before);
            self.pages.glide_to(page, from, target);
        }
    }

    fn scroll_offset(&mut self, page: Page, spare: f32) -> f32 {
        let offset = self.pages.scroll.entry(page).or_insert(0.0);
        *offset = offset.clamp(0.0, spare);
        let target = *offset;
        self.pages.shown_scroll(page, target).clamp(0.0, spare)
    }
}

fn revealed(bottom: f32, shape: Rect) -> f32 {
    smoothstep(bottom - REVEAL_BEFORE, bottom + REVEAL_AFTER, shape.bottom())
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use global_hotkey::hotkey::{Code, HotKey, Modifiers};

    use super::*;
    use crate::notch::preview::Preview;
    use crate::notch::tests::fixtures::{AWAY, Rig, island, key, ms, shortcuts};
    use crate::platform::Style;
    use crate::shelf::Notice;

    fn frames(rig: &Rig, n: &mut Notch, count: usize) {
        let keys = shortcuts(None);
        for _ in 0..count {
            rig.frame(n, &keys, Vec::new());
        }
    }

    #[test]
    fn tools_page_fits_panel() {
        let rig = Rig::new();
        let notch = Notch::with_preview(Style::Notch, Some(vec2(200.0, 38.0)), None);
        for mut n in [island(), notch] {
            frames(&rig, &mut n, 1);
            let panel = n.body_rect(n.expanded_geometry());
            let content = n.content_rect(Page::Tools);
            assert!(panel.shrink(PAD - 0.01).contains_rect(content));
            assert_eq!(content.height(), n.pages.tools.height(&rig.tools));
            assert!(n.header().bar.bottom() < content.top());
        }
    }

    #[test]
    fn find_slides_in_from_left() {
        let rig = Rig::new();
        let mut n = island();
        n.open_page(Page::Tools);
        frames(&rig, &mut n, 120);
        n.toggle_find();
        let shape = n.body_rect(n.expanded_geometry());
        let both = (0..30).find_map(|_| {
            frames(&rig, &mut n, 1);
            match n.page_layers(1.0, shape, true) {
                [Some(outgoing), Some(incoming)] => Some((outgoing, incoming)),
                _ => None,
            }
        });
        let (outgoing, incoming) = both.expect("a frame where both pages show");
        assert_eq!((outgoing.page, incoming.page), (Page::Tools, Page::Find));
        assert!(incoming.slide < 0.0 && outgoing.slide > 0.0, "{} {}", incoming.slide, outgoing.slide);
    }

    #[test]
    fn taller_page_shows_while_growing() {
        let rig = Rig::new();
        let mut n = island();
        n.open_page(Page::Tools);
        frames(&rig, &mut n, 120);
        n.switch_to(Page::Settings);
        let mut dimmest = 1.0f32;
        for frame in 0..30 {
            frames(&rig, &mut n, 1);
            let shape = n.silhouette().rect();
            if frame < 6 {
                let room = n.cramped(Page::Settings, shape).expect("a page still taller than the panel");
                assert_eq!(room.bottom(), shape.bottom() - PAD, "it stops short of the rounded corners");
            }
            let shown: f32 =
                n.page_layers(1.0, shape, true).iter().flatten().map(|layer| layer.opacity).fold(0.0, f32::max);
            dimmest = dimmest.min(shown);
        }
        assert!(dimmest > 0.2, "a page is always showing during the switch, never an empty panel: {dimmest}");
        assert!(n.cramped(Page::Settings, n.silhouette().rect()).is_none(), "once it fits, nothing is held back");
    }

    #[test]
    fn records_shortcut_from_keys() {
        let rig = Rig::new();
        let mut n = Notch::with_preview(Style::Island, None, Some(Preview::Expanded));
        let recording = shortcuts(Some(1));
        rig.frame(&mut n, &recording, Vec::new());
        assert!(n.is_animating());
        assert!(!n.pages.capture.is_animating());

        let actions = rig.frame(&mut n, &recording, vec![key(egui::Key::F35, egui::Modifiers::NONE)]);
        assert!(actions.is_empty());
        assert!(n.pages.capture.is_animating());

        let ctrl_alt = egui::Modifiers { ctrl: true, alt: true, ..Default::default() };
        let actions = rig.frame(&mut n, &recording, vec![key(egui::Key::X, ctrl_alt)]);
        let expected = HotKey::new(Some(Modifiers::CONTROL | Modifiers::ALT), Code::KeyX);
        assert_eq!(actions, vec![Action::SetShortcut(recording[1].which, expected)]);

        let actions = rig.frame(&mut n, &recording, vec![key(egui::Key::Escape, egui::Modifiers::NONE)]);
        assert_eq!(actions, vec![Action::CancelRecording]);
    }

    #[test]
    fn collapsing_stops_redraws() {
        let mut rig = Rig::new();
        rig.shelf.add(vec![Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml")]);
        let mut n = island();
        n.open_page(Page::Shelf);
        frames(&rig, &mut n, 120);
        assert_eq!(n.visible_page(), Some(Page::Shelf));
        assert!(!n.is_animating());

        rig.notice = Some(Notice { text: "Saved".into(), ok: true });
        frames(&rig, &mut n, 1);
        assert!(n.is_animating());

        let t0 = Instant::now();
        n.tick(t0, Some(AWAY));
        n.tick(t0 + ms(300), Some(AWAY));
        assert_eq!(n.mode, Mode::Idle);
        frames(&rig, &mut n, 120);
        assert!(n.is_collapsed());
        assert!(!n.is_animating());
    }

    #[test]
    fn prompt_animates_only_when_drawn() {
        let rig = Rig::new();
        let recording = shortcuts(Some(0));
        let mut n = island();
        for _ in 0..30 {
            rig.frame(&mut n, &recording, Vec::new());
        }
        assert!(!n.is_animating());
        n.set_pinned(true);
        for _ in 0..120 {
            rig.frame(&mut n, &recording, Vec::new());
        }
        assert_eq!(n.visible_page(), Some(Page::Tools));
        assert!(n.is_animating());
    }

    #[test]
    fn tall_page_scrolls_inside() {
        let rig = Rig::new();
        let mut n = island();
        n.open_page(Page::Settings);
        frames(&rig, &mut n, 1);
        n.pages.natural.insert(Page::Settings, 2000.0);
        let limit = n.max_page_height();
        n.pager.remeasure(Heights::from([(Page::Settings, limit)]));
        assert!(n.content_rect(Page::Settings).bottom() <= CANVAS.y - PAD);
        assert_eq!(n.pages.natural(Page::Settings, limit), 2000.0);
    }

    #[test]
    fn settings_views_keep_scroll() {
        let rig = Rig::new();
        let mut n = island();
        n.open_page(Page::Settings);
        frames(&rig, &mut n, 60);
        n.pages.scroll.insert(Page::Settings, 120.0);
        n.pages.show_assistant_settings();
        frames(&rig, &mut n, 1);
        assert_eq!(n.pages.scroll.get(&Page::Settings).copied().unwrap_or(0.0), 0.0, "Assistant opens at its top");
        assert_eq!(
            n.morph.h.config,
            n.spring_configs(Motion::Resize).1,
            "a view that jumps in size gets the soft spring"
        );
        n.pages.hide();
        frames(&rig, &mut n, 1);
        assert_eq!(n.pages.scroll.get(&Page::Settings).copied(), Some(120.0), "home comes back where it was left");
    }

    fn wheel_over(window: Rect, notches: f32) -> Vec<egui::Event> {
        let wheel = egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Line,
            delta: vec2(0.0, -notches),
            phase: egui::TouchPhase::Move,
            modifiers: egui::Modifiers::NONE,
        };
        vec![egui::Event::PointerMoved(window.center()), wheel]
    }

    #[test]
    fn words_ride_with_sliding_shape() {
        let mut n = island();
        n.open_page(Page::Tools);
        n.dodge.value = 24.0;
        n.flee.value = -4.0;
        let painter = Painter::new(egui::Context::default(), egui::LayerId::background(), Rect::EVERYTHING);
        let stage = n.stage(&painter, n.silhouette().rect());
        assert_eq!(stage.lean, 20.0, "the content moves with the black shape it sits in");
    }

    #[test]
    fn wheel_glides_trackpad_follows() {
        let rig = Rig::new();
        let mut n = island();
        n.open_page(Page::Settings);
        frames(&rig, &mut n, 60);
        n.pages.search_settings("e");
        frames(&rig, &mut n, 60);
        let window = n.content_rect(Page::Settings);
        assert!(n.overflow(Page::Settings).is_some(), "a search finding most settings is taller than the panel");
        rig.frame(&mut n, &[], wheel_over(window, 1.0));
        frames(&rig, &mut n, 1);
        let started = n.overflow(Page::Settings).expect("scrolls").offset;
        assert!(started > 0.0 && started < SCROLL_STEP * 0.6, "a notch eases in: {started}");
        assert_eq!(n.pages.scroll.get(&Page::Settings).copied(), Some(SCROLL_STEP), "toward one notch down");
        assert!(n.is_animating());
        frames(&rig, &mut n, 40);
        let rested = n.overflow(Page::Settings).expect("scrolls").offset;
        assert!((rested - SCROLL_STEP).abs() < 0.1, "{rested}");
        let fingers = egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: vec2(0.0, -12.0),
            phase: egui::TouchPhase::Move,
            modifiers: egui::Modifiers::NONE,
        };
        rig.frame(&mut n, &[], vec![egui::Event::PointerMoved(window.center()), fingers]);
        let followed = n.overflow(Page::Settings).expect("scrolls").offset;
        assert!((followed - SCROLL_STEP - 12.0).abs() < 0.01, "a trackpad follows the fingers: {followed}");
    }

    #[test]
    fn long_agents_list_scrolls_and_fades() {
        let rig = Rig::new();
        let mut n = Notch::with_preview(Style::Island, None, Some(Preview::AgentsMany));
        frames(&rig, &mut n, 3);
        let window = n.content_rect(Page::Agents);
        let top = n.overflow(Page::Agents).expect("eleven rows are taller than the panel");
        assert_eq!(window.height(), n.max_page_height());
        assert!(top.natural > window.height() + SCROLL_STEP, "{} in {}", top.natural, window.height());
        assert_eq!((top.window, top.offset), (window, 0.0));
        assert_eq!((top.top_fade(), top.bottom_fade()), (0.0, 1.0), "only the bottom fades before scrolling");
        assert!(top.clip().contains_rect(window) && top.clip().contains_rect(top.scrollbar()));

        rig.frame(&mut n, &[], wheel_over(window, 1.0));
        let scrolled = n.overflow(Page::Agents).unwrap();
        assert_eq!(scrolled.offset, SCROLL_STEP);
        assert_eq!((scrolled.top_fade(), scrolled.bottom_fade()), (1.0, 1.0), "both edges fade mid-list");

        rig.frame(&mut n, &[], wheel_over(window, 50.0));
        let end = n.overflow(Page::Agents).unwrap();
        assert_eq!(end.offset, end.spare());
        assert_eq!((end.top_fade(), end.bottom_fade()), (1.0, 0.0), "the last row rests clear of the fade");
        assert_eq!(n.content_rect(Page::Agents), window, "scrolling never changes the panel's height");
    }
}
