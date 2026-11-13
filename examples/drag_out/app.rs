//! The test window: starts a drag once the mouse has moved far enough, and logs what comes back.

use std::path::PathBuf;
use std::time::Instant;

use winit::application::ApplicationHandler;
use winit::dpi::{LogicalSize, PhysicalPosition};
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoopProxy};
use winit::window::{Window, WindowId, WindowLevel};

use crate::filedrag::{self, DragSensor, Sensed};
use crate::outcome::Outcome;

const DRAG_THRESHOLD_PX: f64 = 6.0;

#[derive(Debug)]
#[cfg_attr(not(windows), allow(dead_code))]
pub enum Note {
    SelfTestScriptDone,
}

pub struct App {
    #[cfg_attr(not(windows), allow(dead_code))]
    proxy: EventLoopProxy<Note>,
    files: Vec<PathBuf>,
    self_test: bool,
    window: Option<Window>,
    cursor: PhysicalPosition<f64>,
    pressed_at: Option<PhysicalPosition<f64>>,
    sensor: DragSensor,
    sensed: Sensed,
    outcome: Outcome,
}

impl App {
    pub fn new(proxy: EventLoopProxy<Note>, files: Vec<PathBuf>, self_test: bool) -> Self {
        Self {
            proxy,
            files,
            self_test,
            window: None,
            cursor: PhysicalPosition::default(),
            pressed_at: None,
            sensor: DragSensor::default(),
            sensed: Sensed::Idle,
            outcome: Outcome::default(),
        }
    }

    fn on_left_button(&mut self, state: ElementState) {
        println!("mouse {state:?}");
        match state {
            ElementState::Pressed => self.pressed_at = Some(self.cursor),
            ElementState::Released => {
                self.pressed_at = None;
                self.outcome.release_seen_after_drag |= self.outcome.drag_result.is_some();
            }
        }
    }

    fn on_cursor_moved(&mut self, position: PhysicalPosition<f64>) {
        self.cursor = position;
        let Some(pressed_at) = self.pressed_at else {
            return;
        };
        if (position.x - pressed_at.x).hypot(position.y - pressed_at.y) < DRAG_THRESHOLD_PX {
            return;
        }
        self.pressed_at = None;
        let Some(window) = &self.window else {
            return;
        };
        let started = Instant::now();
        let result = filedrag::start_drag(window, &self.files);
        println!(
            "start_drag -> {result:?} after {:.0?}, dragging_out: {}",
            started.elapsed(),
            filedrag::dragging_out()
        );
        self.outcome.drag_result = Some(result);
    }

    fn watch_sensor(&mut self) {
        let sensed = self.sensor.poll();
        if sensed != self.sensed {
            println!("sensor {sensed:?}");
            self.sensed = sensed;
        }
    }

    fn start_self_test(&self, window: &Window, event_loop: &ActiveEventLoop) {
        #[cfg(windows)]
        {
            let _ = event_loop;
            crate::self_test::start(window, self.proxy.clone());
        }
        #[cfg(not(windows))]
        {
            let _ = window;
            eprintln!("--self-test only runs on Windows");
            event_loop.exit();
        }
    }

    fn window_checks(&self) -> bool {
        #[cfg(windows)]
        if let Some(window) = &self.window {
            return crate::self_test::click_through_hides_the_drop_target(window);
        }
        true
    }
}

impl ApplicationHandler<Note> for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let attributes = Window::default_attributes()
            .with_title("drag_out: press and drag here")
            .with_inner_size(LogicalSize::new(360.0, 240.0))
            .with_window_level(WindowLevel::AlwaysOnTop);
        let window = event_loop.create_window(attributes).expect("create window");
        if self.self_test {
            self.start_self_test(&window, event_loop);
        }
        self.window = Some(window);
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        self.watch_sensor();
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::MouseInput { state, button: MouseButton::Left, .. } => self.on_left_button(state),
            WindowEvent::CursorMoved { position, .. } => self.on_cursor_moved(position),
            WindowEvent::HoveredFile(path) => {
                println!("HoveredFile({}) dragging_out: {}", path.display(), filedrag::dragging_out());
                self.outcome.hovers_seen += 1;
            }
            WindowEvent::HoveredFileCancelled => println!("HoveredFileCancelled"),
            WindowEvent::DroppedFile(path) => {
                println!("DroppedFile({})", path.display());
                self.outcome.dropped.push(path);
            }
            _ => {}
        }
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, note: Note) {
        let Note::SelfTestScriptDone = note;
        let passed = self.outcome.report(&self.files) & self.window_checks();
        println!("self-test {}", if passed { "passed" } else { "FAILED" });
        event_loop.exit();
        if !passed {
            std::process::exit(1);
        }
    }
}
