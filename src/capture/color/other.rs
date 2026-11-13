//! Other systems have no color sampler.

use winit::event::WindowEvent;
use winit::event_loop::ActiveEventLoop;
use winit::window::WindowId;

use super::SampleOutcome;

pub struct Sampler;

impl Sampler {
    pub fn new() -> Self {
        Self
    }

    pub fn is_active(&self) -> bool {
        false
    }

    pub fn start(&mut self, _event_loop: &ActiveEventLoop) -> Result<(), String> {
        Err("color sampling is not supported on this platform yet".into())
    }

    pub fn cancel(&mut self) {}

    pub fn owns(&self, _id: WindowId) -> bool {
        false
    }

    pub fn window_event(&mut self, _id: WindowId, _event: &WindowEvent) {}

    pub fn about_to_wait(&mut self) {}

    pub fn take_outcome(&mut self) -> Option<SampleOutcome> {
        None
    }
}
