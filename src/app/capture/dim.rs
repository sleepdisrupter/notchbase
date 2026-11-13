//! Dims the screen around the area being recorded.

use winit::event::WindowEvent;
use winit::event_loop::ActiveEventLoop;
use winit::window::WindowId;

use crate::app::App;
use crate::capture::RecordingDim;
use crate::platform;
use crate::record::{Display, Region};

#[derive(Default)]
pub(in crate::app) struct DimState {
    wanted: Option<Region>,
    shown: Option<RecordingDim>,
}

impl DimState {
    pub(in crate::app) fn expect(&mut self, region: Region) {
        self.wanted = Some(region);
    }

    pub(in crate::app) fn owns(&self, id: WindowId) -> bool {
        self.shown.as_ref().is_some_and(|dim| dim.owns(id))
    }

    pub(in crate::app) fn window_event(&mut self, event: &WindowEvent) {
        if let Some(dim) = &mut self.shown {
            dim.window_event(event);
        }
    }
}

impl App {
    pub(in crate::app) fn sync_recording_dim(&mut self, event_loop: &ActiveEventLoop) {
        if self.previewed_recording.is_some() {
            return;
        }
        if !self.recorder.is_busy() {
            self.recording_dim = DimState::default();
            return;
        }
        if !self.recorder.is_recording() {
            return;
        }
        let Some(region) = self.recording_dim.wanted.take() else { return };
        self.show_recording_dim(event_loop, region);
    }

    pub(in crate::app) fn preview_recording_dim(&mut self, event_loop: &ActiveEventLoop) {
        let Some(monitor) = event_loop.primary_monitor() else { return };
        let display = Display::of(&monitor);
        let [w, h] = display.size;
        self.show_recording_dim(event_loop, Region::new(display, [w / 4, h / 4], [w / 2, h / 2]));
    }

    fn show_recording_dim(&mut self, event_loop: &ActiveEventLoop, region: Region) {
        match RecordingDim::open(event_loop, region) {
            Ok(dim) => self.recording_dim.shown = dim,
            Err(err) => eprintln!("notchbase: couldn't dim around the recording: {err}"),
        }
        for nw in self.notches.iter() {
            platform::raise_window(nw.window());
        }
    }
}
