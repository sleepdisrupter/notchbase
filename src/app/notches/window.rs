//! A notch window: its winit window, GL surface and egui state, and drawing one frame of it.

use std::time::{Duration, Instant};

use egui::{Pos2, Rect};
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::ActiveEventLoop;
use winit::window::Window;

use crate::app::annotate::Hold;
use crate::app::return_to::ReturnTo;
use crate::bridge::{Action, PanelData};
use crate::filedrag::{self, DropSite, Inbox};
use crate::notch::Notch;
use crate::platform::{self, FrontApp, Placement};
use crate::ui::gl_window::GlWindow;

const NOMINAL_DT: f32 = 1.0 / 60.0;
const LONGEST_DT: f32 = 1.0 / 20.0;
const STALLED_AFTER: f32 = 0.1;
const REPAINT_HORIZON: Duration = Duration::from_secs(1);

pub(in crate::app) enum InputSwitch {
    ToNotch,
    ToDesktop,
}

pub(in crate::app) struct NotchWindow {
    _drops: Option<DropSite>,
    pub(in crate::app) notch: Notch,
    gl: GlWindow,
    backdrop: platform::Backdrop,
    placement: Placement,
    click_through: bool,
    hit_testing_suspended: bool,
    on_lock_screen: bool,
    spent: bool,
    last_cursor: Option<Pos2>,
    hover_app: Option<ReturnTo>,
    last_frame: Instant,
    repaint_at: Option<Instant>,
}

impl NotchWindow {
    pub(in crate::app) fn open(
        event_loop: &ActiveEventLoop,
        placement: Placement,
        data: &PanelData,
        inbox: &Inbox,
        prime: impl FnOnce(&mut Notch),
    ) -> Result<Self, String> {
        let mut notch = Notch::new(placement.style, placement.notch_size);
        prime(&mut notch);
        let canvas = notch.canvas_size();
        let mut gl = GlWindow::new(event_loop, platform::notch_window_attributes(&placement, canvas))?;
        platform::configure_notch_window(&gl.window, &placement, canvas);
        gl.paint(|ui| {
            notch.ui(ui, data, NOMINAL_DT);
        });
        if notch.settle_in() {
            gl.paint(|ui| {
                notch.ui(ui, data, NOMINAL_DT);
            });
        }
        gl.window.set_visible(true);
        platform::configure_notch_window(&gl.window, &placement, canvas);
        platform::set_click_through(&gl.window, true);
        let drops = filedrag::accept_drops(&gl.window, inbox);
        let backdrop = platform::Backdrop::new(&gl.window);
        let now = Instant::now();
        Ok(Self {
            _drops: drops,
            notch,
            gl,
            backdrop,
            placement,
            click_through: true,
            hit_testing_suspended: false,
            on_lock_screen: false,
            spent: false,
            last_cursor: None,
            hover_app: None,
            last_frame: now,
            repaint_at: None,
        })
    }

    pub(in crate::app) fn window(&self) -> &Window {
        &self.gl.window
    }

    pub(in crate::app) fn redraw(&self) {
        self.gl.window.request_redraw();
    }

    pub(in crate::app) fn cursor(&self) -> Option<Pos2> {
        platform::cursor_in_window(&self.gl.window)
    }

    pub(in crate::app) fn display(&self) -> Option<Rect> {
        let window = &self.gl.window;
        let scale = window.scale_factor() as f32;
        let origin = window.inner_position().ok()?;
        let (at, size) = (self.placement.monitor.position(), self.placement.monitor.size());
        let corner = egui::pos2((at.x - origin.x) as f32 / scale, (at.y - origin.y) as f32 / scale);
        Some(Rect::from_min_size(corner, egui::vec2(size.width as f32 / scale, size.height as f32 / scale)))
    }

    pub(in crate::app) fn on_event(&mut self, event: &WindowEvent) {
        if let WindowEvent::MouseInput { state, button, .. } = event {
            self.press_before_any_cursor_move(*state, *button);
        }
        if self.gl.on_event(event) {
            self.redraw();
        }
    }

    fn press_before_any_cursor_move(&mut self, state: ElementState, button: MouseButton) {
        if self.gl.knows_pointer() {
            return;
        }
        if let Some(event) = self.last_cursor.and_then(|pos| pointer_button(pos, state, button)) {
            self.gl.push_input(event);
        }
    }

    pub(in crate::app) fn paint(&mut self, now: Instant, data: &PanelData) -> Vec<Action> {
        let dt = self.advance_clock(now);
        let mut actions = Vec::new();
        let delay = self.gl.paint(|ui| actions = self.notch.ui(ui, data, dt));
        self.backdrop.shape(self.notch.backdrop(), self.gl.panes());
        self.repaint_at = (delay < REPAINT_HORIZON).then(|| now + delay);
        actions
    }

    fn advance_clock(&mut self, now: Instant) -> f32 {
        let elapsed = now.duration_since(self.last_frame).as_secs_f32();
        self.last_frame = now;
        frame_dt(elapsed)
    }

    pub(in crate::app) fn set_wants_input(&mut self, wants_input: bool) -> Option<InputSwitch> {
        let click_through = !wants_input;
        if click_through == self.click_through {
            return None;
        }
        self.click_through = click_through;
        self.apply_hit_testing();
        if wants_input {
            return Some(InputSwitch::ToNotch);
        }
        self.gl.push_input(egui::Event::PointerGone);
        self.last_cursor = None;
        Some(InputSwitch::ToDesktop)
    }

    pub(in crate::app) fn follow_pointer(&mut self, cursor: Option<Pos2>) -> bool {
        if self.click_through || cursor == self.last_cursor {
            return false;
        }
        self.last_cursor = cursor;
        let Some(pos) = cursor else { return false };
        self.gl.push_input(egui::Event::PointerMoved(pos));
        true
    }

    pub(in crate::app) fn pointer_held(&self) -> bool {
        self.gl.ctx.input(|input| input.pointer.any_down())
    }

    pub(in crate::app) fn release_primary_button(&mut self) {
        let (held, press_origin) =
            self.gl.ctx.input(|input| (input.pointer.primary_down(), input.pointer.press_origin()));
        if !held {
            return;
        }
        let Some(pos) = self.cursor().or(self.last_cursor).or(press_origin) else { return };
        self.gl.push_input(egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        });
        if self.click_through {
            self.gl.push_input(egui::Event::PointerGone);
        }
    }

    pub(in crate::app) fn suspend_hit_testing(&mut self) {
        self.hit_testing_suspended = true;
        self.apply_hit_testing();
    }

    pub(in crate::app) fn resume_hit_testing(&mut self) {
        if std::mem::take(&mut self.hit_testing_suspended) {
            self.apply_hit_testing();
        }
    }

    fn apply_hit_testing(&self) {
        platform::set_click_through(&self.gl.window, self.click_through || self.hit_testing_suspended);
    }

    pub(in crate::app) fn remember_hover_app(&mut self, annotator: Hold) {
        self.hover_app = ReturnTo::remembered(annotator, platform::remember_front_app);
    }

    pub(in crate::app) fn forget_hover_app(&mut self) {
        self.hover_app = None;
    }

    pub(in crate::app) fn lend_hover_to_the_annotator(&mut self) -> Option<FrontApp> {
        let remembered = self.hover_app.take()?;
        self.hover_app = Some(ReturnTo::Annotator);
        remembered.app()
    }

    pub(in crate::app) fn leave_hover(&mut self) -> Option<ReturnTo> {
        self.hover_app.take()
    }

    pub(in crate::app) fn placement(&self) -> &Placement {
        &self.placement
    }

    pub(in crate::app) fn follow_lock(&mut self, shown: bool) {
        if shown && !self.on_lock_screen {
            self.on_lock_screen = platform::show_on_lock_screen(&self.gl.window);
        } else if !shown && std::mem::take(&mut self.on_lock_screen) {
            platform::leave_lock_screen(&self.gl.window);
            self.spent = true;
        }
    }

    pub(in crate::app) fn spent(&self) -> bool {
        self.spent
    }

    pub(in crate::app) fn place(&mut self, placement: Placement) {
        if placement != self.placement {
            self.notch.restyle(placement.style, placement.notch_size);
            self.placement = placement;
            self.redraw();
        }
        platform::configure_notch_window(&self.gl.window, &self.placement, self.notch.canvas_size());
    }

    pub(in crate::app) fn take_due_repaint(&mut self, now: Instant) -> bool {
        let due = self.repaint_at.is_some_and(|at| at <= now);
        if due {
            self.repaint_at = None;
        }
        due
    }

    pub(in crate::app) fn wake_deadline(&self, now: Instant) -> Option<Instant> {
        [self.repaint_at, self.notch.next_wakeup()].into_iter().flatten().filter(|&at| at > now).min()
    }
}

fn pointer_button(pos: Pos2, state: ElementState, button: MouseButton) -> Option<egui::Event> {
    let button = match button {
        MouseButton::Left => egui::PointerButton::Primary,
        MouseButton::Right => egui::PointerButton::Secondary,
        MouseButton::Middle => egui::PointerButton::Middle,
        _ => return None,
    };
    let pressed = state == ElementState::Pressed;
    Some(egui::Event::PointerButton { pos, button, pressed, modifiers: egui::Modifiers::NONE })
}

fn frame_dt(elapsed: f32) -> f32 {
    if elapsed > STALLED_AFTER { NOMINAL_DT } else { elapsed.min(LONGEST_DT) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dt_follows_clock_within_bounds() {
        assert_eq!(frame_dt(0.008), 0.008);
        assert_eq!(frame_dt(0.07), LONGEST_DT);
    }

    #[test]
    fn blind_press_lands_at_last_cursor() {
        let at = egui::pos2(340.0, 20.0);
        let press = pointer_button(at, ElementState::Pressed, MouseButton::Left);
        let primary = egui::PointerButton::Primary;
        assert!(
            matches!(press, Some(egui::Event::PointerButton { pos, button, pressed: true, .. }) if pos == at && button == primary)
        );
        let release = pointer_button(at, ElementState::Released, MouseButton::Left);
        assert!(matches!(release, Some(egui::Event::PointerButton { pressed: false, .. })));
        assert!(pointer_button(at, ElementState::Pressed, MouseButton::Back).is_none());
    }

    #[test]
    fn dt_resets_after_stall() {
        assert_eq!(frame_dt(STALLED_AFTER + 0.01), NOMINAL_DT);
        assert_eq!(frame_dt(5.0), NOMINAL_DT);
    }
}
