//! The live picker: an overlay on every display. Click for the whole display or drag for part of it, hold Shift for
//! a square, nudge with the arrows, and cancel with Escape or a right click.

mod draw;
mod geometry;
mod label;
mod monitor;

use std::time::Instant;

use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::ActiveEventLoop;
use winit::keyboard::{Key, NamedKey};
use winit::window::WindowId;

use super::PickerOutcome;
use crate::capture::overlay::clock::FrameClock;
use crate::capture::overlay::session::{FadeOut, Session};
use draw::Scene;
use geometry::{PixelRect, selection};
use label::SizeLabel;
use monitor::MonitorOverlay;

const FADE: f32 = 0.12;
const CLOSE: FadeOut = FadeOut { hold: 0.0, end: FADE };
/// A drag under this on either side counts as a click on the whole display. Areas to record use `MIN_AREA`.
const MIN_REGION: i64 = 8;
const MIN_AREA: i64 = 32;
/// How long the hidden overlays get to leave the screen before the grab.
#[cfg(not(target_os = "macos"))]
const HIDDEN_SETTLE: std::time::Duration = std::time::Duration::from_millis(40);
const SHOT_HINT: &str = "Click to capture this display, or drag to capture part of it";
const AREA_HINT: &str = "Click to record this display, or drag to record part of it";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Purpose {
    Screenshot,
    Area,
}

impl Purpose {
    fn smallest(self) -> i64 {
        match self {
            Purpose::Screenshot => MIN_REGION,
            Purpose::Area => MIN_AREA,
        }
    }

    fn hint(self) -> &'static str {
        match self {
            Purpose::Screenshot => SHOT_HINT,
            Purpose::Area => AREA_HINT,
        }
    }
}

/// How a closed picker ends: with its outcome, or with a screenshot to take once the overlays are hidden.
enum Exit {
    Done(PickerOutcome),
    #[cfg_attr(target_os = "macos", allow(dead_code))]
    Shoot(usize, PixelRect),
}

#[derive(Clone, Copy)]
struct Drag {
    overlay: usize,
    anchor: [i64; 2],
    current: [i64; 2],
}

pub struct Picker {
    purpose: Purpose,
    overlays: Vec<MonitorOverlay>,
    session: Session<Exit>,
    outcome: Option<PickerOutcome>,
    drag: Option<Drag>,
    shot: Option<(usize, PixelRect)>,
    label: SizeLabel,
    clock: FrameClock,
    shift: bool,
    hovered: Option<usize>,
}

impl Picker {
    pub fn new() -> Self {
        Self::with(Purpose::Screenshot)
    }

    pub fn for_areas() -> Self {
        Self::with(Purpose::Area)
    }

    fn with(purpose: Purpose) -> Self {
        Self {
            purpose,
            overlays: Vec::new(),
            session: Session::new(FADE),
            outcome: None,
            drag: None,
            shot: None,
            label: SizeLabel::new(),
            clock: FrameClock::default(),
            shift: false,
            hovered: None,
        }
    }

    pub fn is_active(&self) -> bool {
        !self.session.is_idle()
    }

    pub fn start(&mut self, event_loop: &ActiveEventLoop) -> Result<(), String> {
        if self.is_active() {
            return Ok(());
        }
        monitor::reopen(&mut self.overlays, event_loop)?;
        for overlay in &mut self.overlays {
            overlay.show();
        }

        self.session.open(Instant::now());
        self.clock.restart();
        self.clear_selection();
        let focus = monitor::locate(&self.overlays).map_or(0, |(i, _)| i);
        self.hovered = Some(focus);
        if let Some([x0, y0, x1, y1]) = preview_selection().filter(|_| self.purpose != Purpose::Area) {
            self.shot = Some((focus, selection([x0, y0], [x1, y1], false, self.overlays[focus].size())));
        }
        self.overlays[focus].focus();
        Ok(())
    }

    pub fn cancel(&mut self) {
        if self.session.is_open() {
            self.close(Exit::Done(PickerOutcome::Cancelled));
        }
    }

    pub fn owns(&self, id: WindowId) -> bool {
        self.overlays.iter().any(|o| o.id() == id)
    }

    pub fn window_event(&mut self, id: WindowId, event: &WindowEvent) {
        let Some(i) = self.overlays.iter().position(|o| o.id() == id) else {
            return;
        };
        match event {
            WindowEvent::RedrawRequested => self.paint(i),
            WindowEvent::CloseRequested => self.cancel(),
            event => self.input(i, event),
        }
    }

    pub fn about_to_wait(&mut self) {
        let now = Instant::now();
        if self.session.is_idle() {
            return;
        }
        if let Some(outcome) = self.session.take_exit(now) {
            self.finish(outcome);
            return;
        }
        if self.session.is_open() {
            self.track_drag();
            self.track_hover();
        }

        let fading = self.session.is_fading(now);
        let focus = self.focus();
        let label_moving = focus.is_some() && self.label.is_animating();
        for (i, overlay) in self.overlays.iter().enumerate() {
            overlay.refresh(fading || (focus == Some(i) && label_moving), self.selection_on(i));
        }
    }

    pub fn take_outcome(&mut self) -> Option<PickerOutcome> {
        self.outcome.take()
    }

    fn input(&mut self, i: usize, event: &WindowEvent) {
        self.overlays[i].observe(event);
        if let WindowEvent::ModifiersChanged(modifiers) = event {
            self.shift = modifiers.state().shift_key();
        }
        if !self.session.is_open() {
            return;
        }
        match event {
            WindowEvent::MouseInput { state, button, .. } => match (*button, *state) {
                (MouseButton::Right, ElementState::Released) => self.cancel(),
                (MouseButton::Left, ElementState::Pressed) => self.begin_drag(i),
                (MouseButton::Left, ElementState::Released) => self.end_drag(),
                _ => {}
            },
            WindowEvent::CursorMoved { .. } => {
                self.track_drag();
                self.track_hover();
            }
            WindowEvent::KeyboardInput { event, is_synthetic: false, .. } if event.state == ElementState::Pressed => {
                self.key(&event.logical_key);
            }
            _ => {}
        }
    }

    fn key(&mut self, key: &Key) {
        let step = if self.shift { 10 } else { 1 };
        match key {
            Key::Named(NamedKey::Escape) => self.cancel(),
            Key::Named(NamedKey::ArrowLeft) => self.nudge(-step, 0),
            Key::Named(NamedKey::ArrowRight) => self.nudge(step, 0),
            Key::Named(NamedKey::ArrowUp) => self.nudge(0, -step),
            Key::Named(NamedKey::ArrowDown) => self.nudge(0, step),
            _ => {}
        }
    }

    fn nudge(&mut self, dx: i64, dy: i64) {
        if let Some((i, pos)) = monitor::locate(&self.overlays) {
            self.overlays[i].nudge(pos, dx, dy);
        }
    }

    fn focus(&self) -> Option<usize> {
        self.drag.map(|d| d.overlay).or(self.shot.map(|(i, _)| i)).or(self.whole_display())
    }

    fn whole_display(&self) -> Option<usize> {
        let idle = self.drag.is_none() && self.shot.is_none();
        self.hovered.filter(|_| idle)
    }

    fn selection_on(&self, i: usize) -> Option<PixelRect> {
        let size = self.overlays.get(i)?.size();
        match (self.drag, self.shot) {
            (Some(d), _) if d.overlay == i => Some(selection(d.anchor, d.current, self.shift, size)),
            (_, Some((j, rect))) if j == i => Some(rect),
            _ if self.whole_display() == Some(i) => Some(PixelRect::full(size)),
            _ => None,
        }
    }

    fn track_hover(&mut self) {
        let hovered = monitor::locate(&self.overlays).map(|(i, _)| i).or(self.hovered);
        if hovered != self.hovered {
            self.hovered = hovered;
            self.label.reset();
        }
    }

    fn clear_selection(&mut self) {
        self.drag = None;
        self.shot = None;
        self.hovered = None;
        self.label.reset();
    }

    fn begin_drag(&mut self, clicked: usize) {
        let hit = monitor::locate(&self.overlays).or_else(|| Some((clicked, self.overlays[clicked].cursor()?)));
        let Some((i, pos)) = hit else { return };
        let corner = self.overlays[i].corner(pos);
        self.drag = Some(Drag { overlay: i, anchor: corner, current: corner });
        self.shot = None;
        self.label.reset();
    }

    fn track_drag(&mut self) {
        let Some(drag) = self.drag.as_mut() else { return };
        let overlay = &self.overlays[drag.overlay];
        if let Some(p) = overlay.cursor() {
            drag.current = overlay.corner(p);
        }
    }

    fn end_drag(&mut self) {
        self.track_drag();
        let Some(drag) = self.drag.take() else { return };
        let overlay = &self.overlays[drag.overlay];
        let rect = selection(drag.anchor, drag.current, self.shift, overlay.size());
        let smallest = self.purpose.smallest();
        let rect =
            if rect.width() >= smallest && rect.height() >= smallest { rect } else { PixelRect::full(overlay.size()) };
        let exit = self.exit(drag.overlay, rect);
        self.shot = Some((drag.overlay, rect));
        self.close(exit);
    }

    fn exit(&self, i: usize, rect: PixelRect) -> Exit {
        let overlay = &self.overlays[i];
        match self.purpose {
            Purpose::Area => Exit::Done(PickerOutcome::Area(overlay.region(rect))),
            Purpose::Screenshot => shot(i, overlay, rect),
        }
    }

    fn close(&mut self, exit: Exit) {
        self.session.close(Instant::now(), exit, CLOSE);
    }

    fn finish(&mut self, exit: Exit) {
        for overlay in &mut self.overlays {
            overlay.hide();
        }
        self.clear_selection();
        self.outcome = Some(match exit {
            Exit::Done(outcome) => outcome,
            Exit::Shoot(i, rect) => self.grab_hidden(i, rect),
        });
    }

    #[cfg(not(target_os = "macos"))]
    fn grab_hidden(&self, i: usize, rect: PixelRect) -> PickerOutcome {
        std::thread::sleep(HIDDEN_SETTLE);
        self.overlays
            .get(i)
            .and_then(|overlay| overlay.grab(rect))
            .map_or(PickerOutcome::Cancelled, PickerOutcome::Region)
    }

    #[cfg(target_os = "macos")]
    fn grab_hidden(&self, _i: usize, _rect: PixelRect) -> PickerOutcome {
        PickerOutcome::Cancelled
    }

    fn paint(&mut self, i: usize) {
        if !self.overlays.get(i).is_some_and(MonitorOverlay::is_shown) {
            return;
        }
        let now = Instant::now();
        if self.session.is_open() {
            self.track_drag();
        }
        let dt = if self.focus() == Some(i) { self.clock.tick(now) } else { 0.0 };
        let opacity = self.session.opacity(now);
        let selection = self.selection_on(i);
        let hint = (self.session.is_open() && self.whole_display() == Some(i)).then_some(self.purpose.hint());

        let overlay = &mut self.overlays[i];
        let scene = Scene { view: overlay.view(), size: overlay.size(), dt, opacity, selection, hint };
        let label = &mut self.label;
        overlay.paint(selection, |painter| draw::draw(painter, label, &scene));
    }
}

#[cfg(not(target_os = "macos"))]
fn shot(i: usize, _overlay: &MonitorOverlay, rect: PixelRect) -> Exit {
    Exit::Shoot(i, rect)
}

/// On macOS the wrapper in `region/macos.rs` takes the shot, so it gets the exact area.
#[cfg(target_os = "macos")]
fn shot(_i: usize, overlay: &MonitorOverlay, rect: PixelRect) -> Exit {
    Exit::Done(PickerOutcome::Area(overlay.exact_region(rect)))
}

/// `NOTCHBASE_PREVIEW_SELECTION=x0,y0,x1,y1` opens the picker with a selection drawn, for previews.
fn preview_selection() -> Option<[i64; 4]> {
    let spec = std::env::var("NOTCHBASE_PREVIEW_SELECTION").ok()?;
    let values: Vec<i64> = spec.split(',').map(|v| v.trim().parse().ok()).collect::<Option<_>>()?;
    values.try_into().ok()
}
