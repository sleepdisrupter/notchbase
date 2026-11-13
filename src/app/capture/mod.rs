//! Screenshots and recordings: asking for an area or a window, then handing the result to whoever asked.

pub(super) mod dim;
mod recording;

use std::time::Instant;

use notchbase_sdk::protocol::{Capture, Color, Event, Reply};
use winit::event::WindowEvent;
use winit::event_loop::ActiveEventLoop;
use winit::window::WindowId;

use crate::bridge::ToolSlot;
use crate::capture::color::SampleOutcome;
use crate::capture::region::PickerOutcome;
use crate::capture::{CaptureMode, Screenshot};
use crate::notch::Notch;
use crate::{config, platform};

use super::App;
use super::tools::requests::empty_reply;

const SCREEN_RECORDING_NEEDED: &str = "Allow Screen Recording for notchbase in System Settings, then reopen it";
const SCREENSHOT_NOT_COPIED: &str = "Couldn't copy the screenshot";

#[derive(Clone, Copy)]
pub(super) struct PendingCapture {
    mode: CaptureMode,
    at: Instant,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct CaptureReply {
    pub(super) slot: ToolSlot,
    pub(super) id: u64,
    pub(super) mode: CaptureMode,
}

impl App {
    pub(super) fn capturing(&self) -> bool {
        self.picker.is_active() || self.sampler.is_active() || self.framer.is_active()
    }

    pub(super) fn choosing_area(&self) -> bool {
        self.framer.is_active() || self.pending_capture.is_some_and(|pending| pending.mode == CaptureMode::Area)
    }

    pub(super) fn capture_requested(&self) -> bool {
        self.capturing() || self.pending_capture.is_some()
    }

    pub(super) fn pending_capture_deadline(&self) -> Option<Instant> {
        self.pending_capture.map(|pending| pending.at)
    }

    pub(super) fn cancel_capture(&mut self) {
        self.pending_capture = None;
        self.picker.cancel();
        self.sampler.cancel();
        self.framer.cancel();
        self.recording_options = None;
        if let Some(pending) = self.capture_reply {
            self.answer_capture(empty_reply(pending.mode));
        }
    }

    pub(super) fn request_capture(&mut self, event_loop: &ActiveEventLoop, mode: CaptureMode) {
        if self.capture_requested() {
            return;
        }
        self.stop_recording();
        self.release_typing();
        if self.notches.all_collapsed() {
            self.start_capture(event_loop, mode);
        } else {
            self.update_notch(Notch::collapse);
            let at = Instant::now() + config::current().notch.collapse_before_capture();
            self.pending_capture = Some(PendingCapture { mode, at });
        }
    }

    pub(super) fn start_pending_capture(&mut self, event_loop: &ActiveEventLoop, now: Instant) {
        let Some(pending) = self.pending_capture else { return };
        if self.notches.all_collapsed() || now >= pending.at {
            self.pending_capture = None;
            self.start_capture(event_loop, pending.mode);
        }
    }

    pub(super) fn capture_window_event(&mut self, id: WindowId, event: &WindowEvent) {
        if self.picker.owns(id) {
            self.picker.window_event(id, event);
            self.collect_region();
        } else if self.framer.owns(id) {
            self.framer.window_event(id, event);
            self.collect_area();
        } else if self.sampler.owns(id) {
            self.sampler.window_event(id, event);
            self.collect_sample();
        }
    }

    pub(super) fn advance_captures(&mut self) {
        self.picker.about_to_wait();
        self.collect_region();
        self.framer.about_to_wait();
        self.collect_area();
        self.sampler.about_to_wait();
        self.collect_sample();
    }

    fn start_capture(&mut self, event_loop: &ActiveEventLoop, mode: CaptureMode) {
        if mode != CaptureMode::Color && !platform::screen_capture_access(true) {
            self.notify(SCREEN_RECORDING_NEEDED);
            self.cancel_capture();
            return;
        }
        self.remember_front_app();
        let started = match mode {
            CaptureMode::Color => self.sampler.start(event_loop),
            CaptureMode::Region => self.picker.start(event_loop),
            CaptureMode::Area => self.framer.start(event_loop),
        };
        if let Err(err) = started {
            self.front_app = None;
            self.notify(&err);
            self.cancel_capture();
        } else if mode != CaptureMode::Color {
            self.grab_escape();
        }
    }

    fn grab_escape(&mut self) {
        if let Err(err) = self.hotkeys.grab_escape() {
            eprintln!("notchbase: Escape won't cancel the capture: {err}");
        }
    }

    fn collect_region(&mut self) {
        if let Some(outcome) = self.picker.take_outcome() {
            self.finish_region(outcome);
        }
    }

    fn collect_area(&mut self) {
        if let Some(outcome) = self.framer.take_outcome() {
            self.finish_region(outcome);
        }
    }

    fn collect_sample(&mut self) {
        if let Some(outcome) = self.sampler.take_outcome() {
            self.finish_sample(outcome);
        }
    }

    fn finish_region(&mut self, outcome: PickerOutcome) {
        match outcome {
            PickerOutcome::Region(shot) => self.captured(shot),
            PickerOutcome::Area(region) => {
                self.end_capture();
                self.begin_screen_recording(region);
                return;
            }
            PickerOutcome::Cancelled => self.cancel_capture(),
        }
        self.end_capture();
    }

    fn finish_sample(&mut self, outcome: SampleOutcome) {
        match outcome {
            SampleOutcome::Picked(color) => {
                let color = Some(Color::rgba(color.r(), color.g(), color.b(), color.a()));
                self.answer_capture(Reply::Color { color });
            }
            _ => self.cancel_capture(),
        }
        self.end_capture();
    }

    fn captured(&mut self, shot: Screenshot) {
        let (width, height) = (shot.width as u32, shot.height as u32);
        let image = self.tools.images.insert(shot);
        self.answer_capture(Reply::Capture { capture: Some(Capture { image, width, height, on_clipboard: false }) });
    }

    fn answer_capture(&mut self, reply: Reply) {
        if let Some(pending) = self.capture_reply.take() {
            self.tools.send(pending.slot, Event::Reply { id: pending.id, reply });
        }
    }

    fn end_capture(&mut self) {
        self.hotkeys.release_escape();
        self.restore_front_app();
    }

    pub(super) fn screenshot_not_copied(&mut self, err: &str) {
        self.report(SCREENSHOT_NOT_COPIED, err);
    }
}
