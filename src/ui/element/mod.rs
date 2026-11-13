//! Paints the UI that tools describe with the SDK's nodes, and sends what the user does back to the tool's
//! handlers. A scene that isn't live is a preview and keeps every value to itself.

mod controls;
mod drawer;
mod drawing;
#[cfg(test)]
mod fixture;
mod input;
mod layout;
mod leaves;
mod liquid;
mod measure;
mod month;
mod palette;
mod pan;
mod picker;
mod round;
pub mod shortcut;
pub mod style;
mod swipe;
mod textures;
mod turn;
mod week;

use egui::{Id, Rect};
use notchbase_sdk::protocol::{HandlerId, Node, Surface, UiValue};

use crate::bridge::{Action, ToolSlot};
use crate::platform::Style;
use crate::tools::ImageStore;
use crate::tools::shortcuts::{ShortcutRef, ShortcutView};
use crate::ui::canvas::Canvas;
use crate::ui::field::Focus;

pub use drawer::Drawer;
pub use measure::Measure;
pub use textures::Textures;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Prompt {
    pub shake: f32,
    pub pulse: f32,
    pub hint: Option<(ShortcutRef, &'static str)>,
}

pub struct Env<'a> {
    pub style: Style,
    pub images: &'a ImageStore,
    pub shortcuts: &'a [ShortcutView],
    pub prompt: Prompt,
    pub now_ms: u64,
}

#[derive(Default)]
pub struct SurfaceState {
    pub drawer: Drawer,
    pub focus: Option<Focus>,
    drums: picker::Drums,
    presses: round::Presses,
    counting: bool,
}

impl SurfaceState {
    pub fn begin(&mut self) {
        self.drawer.begin();
        self.counting = false;
    }

    pub fn end(&mut self, ui: &egui::Ui, actions: &mut Vec<Action>) {
        self.drawer.end(ui);
        input::end_frame(self, ui, actions);
    }

    pub fn rest(&mut self, actions: &mut Vec<Action>) {
        self.drawer.close();
        if self.focus.take().is_some() {
            actions.push(Action::Typing(false));
        }
    }

    pub fn step(&mut self, dt: f32) {
        self.drawer.step(dt);
        self.drums.step(dt);
        self.presses.step(dt);
    }

    pub fn snap(&mut self) {
        self.drawer.snap();
    }

    pub fn is_animating(&self) -> bool {
        self.drawer.is_animating() || self.drums.is_animating() || self.presses.is_animating() || self.counting
    }
}

pub struct Scene<'a> {
    pub ui: &'a egui::Ui,
    pub canvas: &'a mut Canvas,
    pub env: &'a Env<'a>,
    pub state: &'a mut SurfaceState,
    pub textures: &'a mut Textures,
    pub actions: &'a mut Vec<Action>,
    pub keys: &'a mut Vec<Rect>,
    pub slot: ToolSlot,
    pub surface: Surface,
    pub live: bool,
}

impl Scene<'_> {
    pub fn measure(&self) -> Measure<'_> {
        Measure { canvas: self.canvas, env: self.env, slot: self.slot }
    }

    pub fn paint(&mut self, node: &Node, rect: Rect, id: Id) {
        layout::paint(self, node, rect, id);
    }

    pub fn emit(&mut self, handler: Option<HandlerId>, value: UiValue) {
        if let Some(handler) = handler.filter(|_| self.live) {
            self.actions.push(Action::Tool(self.slot, self.surface, handler, value));
        }
    }
}

pub fn surface_id(slot: ToolSlot, surface: Surface) -> Id {
    Id::new(("notchbase.tool", slot.0, surface as u8))
}

#[cfg(test)]
mod tests {
    use egui::{Rect, pos2};
    use notchbase_sdk::protocol::{HandlerId, Surface, UiValue};

    use super::fixture::{SLOT, SURFACE, Stage};
    use super::*;
    use crate::ui::field;

    #[test]
    fn live_scene_reaches_the_handler() {
        let mut stage = Stage::live();
        let painted = stage.with(|scene| {
            scene.emit(Some(HandlerId(3)), UiValue::Bool(true));
            scene.emit(None, UiValue::Press);
        });
        assert_eq!(painted.actions, [Action::Tool(SLOT, SURFACE, HandlerId(3), UiValue::Bool(true))]);
    }

    #[test]
    fn preview_scene_keeps_values() {
        let mut stage = Stage::new();
        let painted = stage.with(|scene| scene.emit(Some(HandlerId(3)), UiValue::Press));
        assert!(painted.actions.is_empty());
    }

    #[test]
    fn each_surface_has_its_own_id() {
        let page = surface_id(ToolSlot(1), Surface::Page);
        assert_eq!(page, surface_id(ToolSlot(1), Surface::Page));
        assert_ne!(page, surface_id(ToolSlot(1), Surface::Live));
        assert_ne!(page, surface_id(ToolSlot(2), Surface::Page));
    }

    #[test]
    fn resting_returns_keyboard_once() {
        let mut state = SurfaceState::default();
        let mut actions = Vec::new();
        let rect = Rect::from_min_max(pos2(0.0, 0.0), pos2(100.0, 24.0));
        field::begin(&mut state.focus, Id::new("field"), "draft", rect, &mut actions);
        actions.clear();
        state.rest(&mut actions);
        assert_eq!(actions, [Action::Typing(false)]);
        assert!(state.focus.is_none());
        state.rest(&mut actions);
        assert_eq!(actions, [Action::Typing(false)], "nothing more to give back");
    }

    #[test]
    fn counting_animates_until_next_frame() {
        let mut state = SurfaceState::default();
        assert!(!state.is_animating());
        state.counting = true;
        assert!(state.is_animating());
        state.begin();
        assert!(!state.is_animating());
    }

    #[test]
    fn countdown_ring_keeps_animating() {
        let mut stage = Stage::new();
        let ring =
            |span| Node::Ring { value: 0.5, size: 40.0, tone: Default::default(), span, stroke: None, center: None };
        let rect = Rect::from_min_max(pos2(0.0, 0.0), pos2(40.0, 40.0));
        stage.paint(&ring(None), rect);
        assert!(!stage.state.is_animating());
        stage.paint(&ring(Some(notchbase_sdk::protocol::Span { start: 0, end: 60_000 })), rect);
        assert!(stage.state.is_animating());
    }
}
