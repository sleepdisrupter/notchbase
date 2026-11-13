//! The sound mixer: the output and input devices with their volume, and a row for each app that plays sound.

mod app_row;
mod apps;
mod devices;
mod head;
mod icons;
mod layout;
mod picker;
mod row;
mod slots;
mod thin_slider;

use std::collections::{HashMap, HashSet};

use egui::Id;
use egui::epaint::shape_transform::adjust_colors;

use super::PageCtx;
use crate::audio::{AppId, AudioApp, DeviceKind, Mixer};
use crate::bridge::Action;
use crate::ui::canvas::Canvas;
use crate::ui::motion;
use crate::ui::spring::{Spring, smoothstep};
use head::Disclosure;
use icons::Icons;
use layout::{APP_ROWS, Content, Layout, REASON_H, apps_target};
use picker::Picker;
use row::RowState;
use slots::Slot;

const KINDS: [DeviceKind; 2] = [DeviceKind::Output, DeviceKind::Input];
const SCOPE: &str = "mixer";

pub struct MixerPage {
    scope: Id,
    picker: Picker,
    disclosure: Option<Disclosure>,
    apps_h: Spring,
    slots: Vec<Slot>,
    rows: HashMap<AppId, RowState>,
    icons: Icons,
    dragging: Option<AppId>,
    reason_h: f32,
    initialized: bool,
    animating: bool,
}

impl Default for MixerPage {
    fn default() -> Self {
        Self::scoped(SCOPE)
    }
}

impl MixerPage {
    pub fn scoped(scope: &'static str) -> Self {
        Self {
            scope: Id::new(scope),
            picker: Picker::default(),
            disclosure: None,
            apps_h: Spring::new(0.0, motion::SMOOTH, 0.1),
            slots: Vec::new(),
            rows: HashMap::new(),
            icons: Icons::default(),
            dragging: None,
            reason_h: REASON_H,
            initialized: false,
            animating: false,
        }
    }

    pub fn folded(scope: &'static str) -> Self {
        Self { disclosure: Some(Disclosure::default()), ..Self::scoped(scope) }
    }

    pub fn unfold(&mut self) {
        if let Some(disclosure) = &mut self.disclosure {
            disclosure.open_now();
        }
    }

    pub fn height(&self, mixer: &Mixer, _width: f32) -> f32 {
        self.layout(mixer).height
    }

    pub fn is_animating(&self) -> bool {
        self.animating
    }

    pub fn ui(&mut self, ctx: &mut PageCtx<'_>, mixer: &Mixer) -> Vec<Action> {
        let mut actions = Vec::new();
        let interactive = ctx.alpha >= 0.5;
        if !interactive {
            self.picker.close();
            self.dragging = None;
        }

        let frozen = self.dragging.is_some() || (interactive && ctx.ui.rect_contains_pointer(ctx.rect));
        self.sync_slots(mixer, frozen && self.initialized);
        let content = Content::of(mixer, self.slots.len());
        if !self.initialized {
            self.apps_h.snap_to(apps_target(content.apps, self.reason_h));
        }
        self.picker.close_if_empty(&content);

        let layout = layout::layout(content, self.picker.openness(), self.apps_h.value, self.fold());
        if let Some(fold) = layout.fold {
            self.head_row(ctx, mixer, fold, &mut actions);
        }
        self.rest(ctx, mixer, content, &layout, &mut actions);
        self.picker.close_on_press_outside(ctx.ui);

        self.picker.animate();
        self.apps_h.animate_to(apps_target(content.apps, self.reason_h), motion::SMOOTH);
        self.step(ctx.dt);
        self.initialized = true;
        actions
    }

    fn rest(
        &mut self,
        ctx: &mut PageCtx<'_>,
        mixer: &Mixer,
        content: Content,
        layout: &Layout,
        actions: &mut Vec<Action>,
    ) {
        let reveal = layout.reveal();
        if reveal <= 0.002 {
            return;
        }
        let mut layer = Canvas::new(ctx.ui.painter());
        let alpha = if reveal >= 0.5 { ctx.alpha } else { 0.0 };
        let mut rest =
            PageCtx { ui: &mut *ctx.ui, canvas: &mut layer, rect: ctx.rect, alpha, dt: ctx.dt, now: ctx.now };
        for kind in KINDS {
            if let Some(section) = layout.sections[kind_index(kind)] {
                self.device_section(&mut rest, mixer, kind, section, actions);
            }
        }
        self.apps_section(&mut rest, mixer, content.apps, layout, actions);
        fade_into(ctx.canvas, layer, smoothstep(0.2, 1.0, reveal));
    }

    fn fold(&self) -> Option<f32> {
        self.disclosure.as_ref().map(Disclosure::reveal)
    }

    fn app_row_count(&self, mixer: &Mixer) -> usize {
        if self.initialized { self.slots.len() } else { mixer.apps().len() }
    }

    fn layout(&self, mixer: &Mixer) -> Layout {
        let content = Content::of(mixer, self.app_row_count(mixer));
        let apps_h = if self.initialized { self.apps_h.value } else { apps_target(content.apps, self.reason_h) };
        layout::layout(content, self.picker.openness(), apps_h, self.fold())
    }

    fn sync_slots(&mut self, mixer: &Mixer, frozen: bool) {
        let wanted: Vec<AppId> = mixer.apps().iter().take(APP_ROWS).map(|a| a.id).collect();
        let present: HashSet<AppId> = mixer.apps().iter().map(|a| a.id).collect();
        let rows = &self.rows;
        slots::sync(&mut self.slots, &wanted, &present, frozen, |id| {
            rows.get(&id).is_none_or(|r| r.appear.value <= 0.02)
        });
    }

    fn step(&mut self, dt: f32) {
        let mut animating = self.picker.step(dt);
        if let Some(disclosure) = &mut self.disclosure {
            animating |= disclosure.step(dt);
        }
        self.apps_h.step(dt);
        animating |= !self.apps_h.is_settled();
        self.rows.retain(|_, row| std::mem::take(&mut row.alive));
        for row in self.rows.values_mut() {
            animating |= row.step(dt);
        }
        self.icons.retain(|id| self.rows.contains_key(&id));
        self.animating = animating;
    }

    fn row_state(&mut self, id: AppId, app: Option<&AudioApp>, slot: f32) -> &mut RowState {
        let appear = self.initialized;
        let state = self.rows.entry(id).or_insert_with(|| RowState::new(app, slot, appear));
        state.alive = true;
        state
    }

    fn track_drag(&mut self, id: AppId, down: bool) {
        if down {
            self.dragging = Some(id);
        } else if self.dragging == Some(id) {
            self.dragging = None;
        }
    }
}

fn kind_index(kind: DeviceKind) -> usize {
    match kind {
        DeviceKind::Output => 0,
        DeviceKind::Input => 1,
    }
}

fn channel(mixer: &Mixer, kind: DeviceKind) -> Option<&AudioApp> {
    match kind {
        DeviceKind::Output => mixer.master(),
        DeviceKind::Input => mixer.input(),
    }
}

fn fade_into(dst: &mut Canvas, src: Canvas, alpha: f32) {
    let alpha = alpha.clamp(0.0, 1.0);
    if alpha <= 0.002 {
        return;
    }
    for mut shape in src.shapes {
        if alpha < 0.999 {
            adjust_colors(&mut shape, move |c| *c = c.gamma_multiply(alpha));
        }
        dst.add(shape);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds_index_their_own_slots() {
        assert_eq!(KINDS.map(kind_index), [0, 1]);
    }
}
