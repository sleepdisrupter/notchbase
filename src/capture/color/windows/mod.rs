//! The color sampler on Windows: a loupe window that follows the cursor, fed by a small screen grab and steered by
//! input hooks. Picking presses the loupe and shows a check before it fades.

mod grabber;
mod hooks;
mod native;
mod screen;
mod window;

use std::time::Instant;

use egui::Color32;
use winit::event::WindowEvent;
use winit::event_loop::ActiveEventLoop;
use winit::window::WindowId;

use super::SampleOutcome;
use super::loupe::{GRAB, Loupe, Scene, lens_center, monitor_bounds};
use crate::capture::overlay::clock::FrameClock;
use crate::capture::overlay::session::{FadeOut, Session};
use crate::platform::Capture;
use crate::ui::spring::smoothstep;
use grabber::Grabber;
use hooks::{HookEvent, Hooks};
use screen::{cursor_pos, monitor_rect, nudge_cursor};
use window::LoupeWindow;

const FADE_IN: f32 = 0.12;
// A pick holds for a moment to show the check, while a cancel fades right away.
const CONFIRM: FadeOut = FadeOut { hold: 0.04, end: 0.16 };
const CANCEL: FadeOut = FadeOut { hold: 0.0, end: 0.12 };

pub struct Sampler {
    window: Option<LoupeWindow>,
    grabber: Option<Grabber>,
    hooks: Option<Hooks>,
    session: Session<SampleOutcome>,
    outcome: Option<SampleOutcome>,
    loupe: Loupe,
    cursor: [i32; 2],
    grab: Option<Capture>,
    needs_upload: bool,
    clock: FrameClock,
}

impl Sampler {
    pub fn new() -> Self {
        Self {
            window: None,
            grabber: None,
            hooks: None,
            session: Session::new(FADE_IN),
            outcome: None,
            loupe: Loupe::new(),
            cursor: [0, 0],
            grab: None,
            needs_upload: false,
            clock: FrameClock::default(),
        }
    }

    pub fn is_active(&self) -> bool {
        !self.session.is_idle()
    }

    pub fn start(&mut self, event_loop: &ActiveEventLoop) -> Result<(), String> {
        if self.is_active() {
            return Ok(());
        }
        let cursor = cursor_pos().ok_or("couldn't read the cursor position")?;
        if self.grabber.is_none() {
            self.grabber = Some(Grabber::new()?);
        }
        let grab = self.grab_at(cursor).ok_or("couldn't read the screen")?;
        if self.window.is_none() {
            self.window = Some(LoupeWindow::new(event_loop)?);
        }
        self.hooks = Some(Hooks::install()?);

        let now = Instant::now();
        self.session.open(now);
        self.cursor = cursor;
        self.grab = Some(grab);
        self.needs_upload = true;
        self.clock.restart();
        if let Some(window) = self.window.as_mut() {
            window.place(cursor, now);
            self.loupe.open(window.scale());
        }
        self.paint();
        if let Some(window) = self.window.as_mut() {
            window.show();
        }
        Ok(())
    }

    pub fn cancel(&mut self) {
        if !self.session.is_open() {
            return;
        }
        self.session.close(Instant::now(), SampleOutcome::Cancelled, CANCEL);
        self.loupe.dismiss();
    }

    pub fn owns(&self, id: WindowId) -> bool {
        self.window.as_ref().is_some_and(|w| w.id() == id)
    }

    pub fn window_event(&mut self, id: WindowId, event: &WindowEvent) {
        if !self.owns(id) {
            return;
        }
        match event {
            WindowEvent::RedrawRequested => self.paint(),
            WindowEvent::CloseRequested => self.cancel(),
            event => {
                if let Some(window) = self.window.as_mut() {
                    window.on_event(event);
                }
            }
        }
    }

    pub fn about_to_wait(&mut self) {
        if self.session.is_idle() {
            return;
        }
        let events = self.hooks.as_ref().map(Hooks::drain).unwrap_or_default();
        for event in events {
            self.handle(event);
        }
        if let Some(outcome) = self.session.take_exit(Instant::now()) {
            self.finish(outcome);
            return;
        }
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }

    pub fn take_outcome(&mut self) -> Option<SampleOutcome> {
        self.outcome.take()
    }

    fn handle(&mut self, event: HookEvent) {
        match event {
            HookEvent::Cancel => self.cancel(),
            _ if !self.session.is_open() => {}
            HookEvent::Pick => self.pick(),
            HookEvent::Zoom(notches) => self.loupe.zoom_by(notches, self.scale()),
            HookEvent::Nudge(dx, dy) => nudge_cursor(dx, dy),
        }
    }

    fn scale(&self) -> f32 {
        self.window.as_ref().map_or(1.0, LoupeWindow::scale)
    }

    fn grab_at(&self, at: [i32; 2]) -> Option<Capture> {
        self.grabber.as_ref().and_then(|g| g.grab(at))
    }

    fn follow_cursor(&mut self) {
        if let Some(at) = cursor_pos() {
            self.cursor = at;
        }
        if let Some(grab) = self.grab_at(self.cursor) {
            self.grab = Some(grab);
            self.needs_upload = true;
        }
    }

    fn pick(&mut self) {
        let now = Instant::now();
        self.follow_cursor();
        let Some(grab) = &self.grab else { return };
        let middle = (GRAB / 2) as i64;
        let color = grab.srgb(middle, middle);
        if let Some(window) = self.window.as_mut() {
            window.place(self.cursor, now);
        }
        self.session.close(now, SampleOutcome::Picked(color), CONFIRM);
        self.loupe.press();
    }

    fn finish(&mut self, outcome: SampleOutcome) {
        self.hooks = None;
        if let Some(window) = self.window.as_mut() {
            window.hide();
        }
        self.grab = None;
        self.outcome = Some(outcome);
    }

    fn confirmation(&self, now: Instant) -> (f32, Option<Color32>) {
        match self.session.closing(now) {
            Some((SampleOutcome::Picked(color), since)) => (smoothstep(0.0, CONFIRM.hold, since), Some(*color)),
            _ => (0.0, None),
        }
    }

    fn paint(&mut self) {
        if self.window.is_none() {
            return;
        }
        let now = Instant::now();
        let live = self.session.is_open();
        if live {
            self.follow_cursor();
        }
        let opacity = self.session.opacity(now);
        let (check, picked) = self.confirmation(now);
        let dt = self.clock.tick(now);
        let (Some(window), Some(grab)) = (self.window.as_mut(), self.grab.as_ref()) else { return };
        let cursor = self.cursor;
        let origin = if live { window.place(cursor, now) } else { window.origin().unwrap_or(cursor) };
        let scale = window.scale();
        let bounds = monitor_bounds(monitor_rect(cursor), origin, scale, window.view());
        let center = lens_center(cursor, origin, scale, window.is_excluded());
        let upload = std::mem::take(&mut self.needs_upload);

        let loupe = &mut self.loupe;
        window.paint(grab, upload, |painter, texture| {
            let scene = Scene { texture, grab, center, bounds, dt, opacity, check, picked };
            loupe.draw(painter, &scene);
        });
    }
}
