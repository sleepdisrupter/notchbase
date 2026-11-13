//! The app's side of Annotate: opening the drawing window on a screenshot, replacing it and closing it.

mod copying;
mod frame;
mod keyboard;
mod reports;
mod retiring;

use std::sync::Arc;
use std::time::Instant;

use notchbase_sdk::protocol::ImageId;
use winit::event::WindowEvent;
use winit::event_loop::ActiveEventLoop;
use winit::monitor::MonitorHandle;
use winit::window::WindowId;

use crate::annotate::{Annotator, Brush, Copier, Focus, sample};
use crate::capture::Screenshot;
use crate::platform::{self, FrontApp};
use crate::poll::Wake;

use super::App;
use super::tools::requests::NO_IMAGE;
use copying::Copying;
use frame::Heard;
use keyboard::{Farewell, Keyboard};
use reports::Reports;
use retiring::Retiring;

pub(super) use keyboard::Hold;

const NOT_OPENED: &str = "Couldn't open the screenshot to draw on";
const NO_SCREEN: &str = "no screen to show it on";
const PREVIEW_IMAGE: [usize; 2] = [1600, 1000];

pub(super) struct Annotating {
    open: Option<Open>,
    reports: Reports,
    sessions: u64,
    retiring: Retiring,
}

struct Open {
    session: u64,
    window: Annotator,
    copier: Option<Copier>,
    returns_to: Option<FrontApp>,
    copying: Copying,
    keyboard: Keyboard,
}

impl Annotating {
    pub(super) fn new(wake: Wake) -> Self {
        Self { open: None, reports: Reports::new(wake), sessions: 0, retiring: Retiring::default() }
    }

    pub(super) fn owns(&self, id: WindowId) -> bool {
        self.open.as_ref().is_some_and(|open| open.window.owns(id))
    }

    pub(super) fn window_event(&mut self, event: &WindowEvent) {
        let Some(open) = &mut self.open else { return };
        open.window.window_event(event);
        if matches!(event, WindowEvent::Focused(_)) {
            open.keyboard.annotator(open.window.focused());
        }
    }

    pub(super) fn another_window_focused(&mut self, id: WindowId, focused: bool) {
        if let Some(open) = self.open.as_mut().filter(|open| !open.window.owns(id)) {
            open.keyboard.another_window(id, focused);
        }
    }

    pub(super) fn settle_keyboard(&mut self, still_open: impl Fn(WindowId) -> bool) {
        if let Some(open) = &mut self.open {
            open.keyboard.settle(still_open);
        }
    }

    pub(super) fn keyboard(&self) -> Hold {
        self.open.as_ref().map_or(Hold::NoAnnotator, |open| open.keyboard.hold())
    }

    pub(super) fn take_back_the_keyboard(&self) {
        self.focus(Focus::HandedBack);
    }

    fn focus(&self, focus: Focus) {
        if let Some(open) = &self.open {
            open.window.focus(focus);
        }
    }

    pub(super) fn wakeup(&self) -> Option<Instant> {
        let open = self.open.as_ref()?;
        [open.copying.due(), open.window.repaint_at()].into_iter().flatten().min()
    }

    fn keep_copying(&mut self, open: &mut Open) -> Option<Brush> {
        let brush = open.finish_what_is_in_progress();
        open.flush();
        self.retiring.keep(open.session, open.copying.awaited(), open.copier.take());
        brush
    }
}

impl Open {
    fn hand_to_the_clipboard(&mut self) {
        match &self.copier {
            Some(copier) => copier.copy(self.window.job()),
            None => self.window.not_copied(self.window.revision()),
        }
    }

    fn flush(&mut self) {
        if self.copying.take() {
            self.hand_to_the_clipboard();
        }
    }

    fn finish_what_is_in_progress(&mut self) -> Option<Brush> {
        self.window.finish_what_is_in_progress();
        let finished = Heard::of(self.window.take_outcomes());
        finished.arm(&mut self.copying, Instant::now(), false);
        finished.brush
    }

    fn copied(&mut self, revision: u64) {
        self.copying.copied(revision);
        self.window.copied(revision);
    }

    fn app_to_return_to(&mut self) -> Option<FrontApp> {
        match self.keyboard.farewell()? {
            Farewell::WhereItCameFrom => self.returns_to.take(),
            Farewell::TheAppInFront => platform::remember_front_app(),
        }
    }

    fn app_to_hand_the_keyboard_to(&mut self) -> Option<FrontApp> {
        if self.keyboard.hold() == Hold::Lent { None } else { self.app_to_return_to() }
    }
}

impl App {
    pub(super) fn open_annotator(&mut self, event_loop: &ActiveEventLoop, image: ImageId) {
        match self.tools.images.full(image) {
            Some(shot) => self.annotate(event_loop, shot),
            None => self.notify(NO_IMAGE),
        }
    }

    pub(super) fn preview_annotator(&mut self, event_loop: &ActiveEventLoop) {
        let image = self.tools.images.insert(sample(PREVIEW_IMAGE));
        self.open_annotator(event_loop, image);
    }

    pub(super) fn finish_annotating(&mut self) {
        if let Some(open) = self.annotating.open.take() {
            self.let_go_of(open);
        }
        self.annotating.retiring.finish();
    }

    fn annotate(&mut self, event_loop: &ActiveEventLoop, shot: Arc<Screenshot>) {
        if self.annotating.open.as_ref().is_some_and(|open| open.window.shows(&shot)) {
            self.return_to_annotator();
            return;
        }
        let Some(monitor) = self.monitor_in_use(event_loop) else {
            self.report(NOT_OPENED, NO_SCREEN);
            return;
        };
        let (carried, in_front) = (self.retire_annotator(), platform::remember_front_app());
        let brush = Brush::from(&self.config.config().annotate);
        match Annotator::open(event_loop, &monitor, shot, brush) {
            Ok(window) => {
                let returns_to = carried.or(self.lend_hovers_to_the_annotator()).or(in_front);
                self.keep_annotator(window, returns_to);
            }
            Err(err) => self.report(NOT_OPENED, err),
        }
    }

    fn return_to_annotator(&mut self) {
        self.lend_hovers_to_the_annotator();
        self.annotating.focus(Focus::Asked);
    }

    fn keep_annotator(&mut self, window: Annotator, returns_to: Option<FrontApp>) {
        self.annotating.sessions += 1;
        let session = self.annotating.sessions;
        let copier = match self.annotating.reports.copier(session) {
            Ok(copier) => Some(copier),
            Err(err) => {
                self.screenshot_not_copied(&err.to_string());
                None
            }
        };
        let (copying, keyboard) = (Copying::default(), Keyboard::default());
        self.annotating.open = Some(Open { session, window, copier, returns_to, copying, keyboard });
    }

    fn close_annotator(&mut self) {
        let Some(open) = self.annotating.open.take() else { return };
        let mut open = self.let_go_of(open);
        if let Some(app) = open.app_to_hand_the_keyboard_to() {
            platform::restore_front_app(app);
        }
    }

    fn retire_annotator(&mut self) -> Option<FrontApp> {
        let open = self.annotating.open.take()?;
        self.let_go_of(open).app_to_return_to()
    }

    fn let_go_of(&mut self, mut open: Open) -> Open {
        if let Some(brush) = self.annotating.keep_copying(&mut open) {
            self.remember_brush(brush);
        }
        open
    }

    fn lend_hovers_to_the_annotator(&mut self) -> Option<FrontApp> {
        let hovered: Vec<FrontApp> =
            self.notches.windows_mut().filter_map(|(_, nw)| nw.lend_hover_to_the_annotator()).collect();
        hovered.into_iter().next()
    }

    fn monitor_in_use(&self, event_loop: &ActiveEventLoop) -> Option<MonitorHandle> {
        let clicked = self.notches.active().map(|nw| nw.placement().monitor.clone());
        clicked.or_else(|| event_loop.primary_monitor()).or_else(|| event_loop.available_monitors().next())
    }
}
