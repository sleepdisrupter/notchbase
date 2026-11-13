//! The color sampler on macOS is the system's `NSColorSampler`, which answers through a channel when it closes. We
//! can't close it from here, so cancelling only stops listening.

use std::sync::mpsc::{self, Receiver, TryRecvError};

use block2::RcBlock;
use egui::Color32;
use objc2_app_kit::{NSColor, NSColorSampler, NSColorSpace};
use winit::event::WindowEvent;
use winit::event_loop::ActiveEventLoop;
use winit::window::WindowId;

use super::SampleOutcome;

pub struct Sampler {
    showing: Option<Showing>,
    outcome: Option<SampleOutcome>,
}

struct Showing {
    selection: Receiver<SampleOutcome>,
    /// False once cancelled. The sampler stays up until the user closes it, and its late answer is dropped.
    wanted: bool,
}

impl Sampler {
    pub fn new() -> Self {
        Self { showing: None, outcome: None }
    }

    pub fn is_active(&self) -> bool {
        self.showing.as_ref().is_some_and(|showing| showing.wanted)
    }

    pub fn start(&mut self, _event_loop: &ActiveEventLoop) -> Result<(), String> {
        self.show();
        Ok(())
    }

    pub fn cancel(&mut self) {
        if let Some(showing) = self.showing.as_mut().filter(|showing| showing.wanted) {
            showing.wanted = false;
            self.outcome = Some(SampleOutcome::Cancelled);
        }
    }

    pub fn owns(&self, _id: WindowId) -> bool {
        false
    }

    pub fn window_event(&mut self, _id: WindowId, _event: &WindowEvent) {}

    pub fn about_to_wait(&mut self) {
        let Some(showing) = &self.showing else { return };
        let selected = match showing.selection.try_recv() {
            Ok(outcome) => outcome,
            Err(TryRecvError::Empty) => return,
            Err(TryRecvError::Disconnected) => SampleOutcome::Cancelled,
        };
        if showing.wanted {
            self.outcome = Some(selected);
        }
        self.showing = None;
    }

    pub fn take_outcome(&mut self) -> Option<SampleOutcome> {
        self.outcome.take()
    }

    fn show(&mut self) {
        // A sampler left up by a cancelled run is taken back rather than opening a second one.
        self.showing.get_or_insert_with(show_system_sampler).wanted = true;
    }
}

fn show_system_sampler() -> Showing {
    let (sender, selection) = mpsc::channel();
    let handler = RcBlock::new(move |color: *mut NSColor| {
        let color = unsafe { color.as_ref() };
        let _ = sender.send(color.and_then(to_srgb).map_or(SampleOutcome::Cancelled, SampleOutcome::Picked));
    });
    unsafe { NSColorSampler::new().showSamplerWithSelectionHandler(&handler) };
    Showing { selection, wanted: false }
}

fn to_srgb(color: &NSColor) -> Option<Color32> {
    let srgb = color.colorUsingColorSpace(&NSColorSpace::sRGBColorSpace())?;
    let channel = |v: f64| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    Some(Color32::from_rgb(channel(srgb.redComponent()), channel(srgb.greenComponent()), channel(srgb.blueComponent())))
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc::Sender;

    use super::*;

    fn showing() -> (Sender<SampleOutcome>, Sampler) {
        let (sender, selection) = mpsc::channel();
        let sampler = Sampler { showing: Some(Showing { selection, wanted: true }), outcome: None };
        (sender, sampler)
    }

    fn picked(sampler: &mut Sampler) -> Option<Color32> {
        match sampler.take_outcome() {
            Some(SampleOutcome::Picked(color)) => Some(color),
            _ => None,
        }
    }

    #[test]
    fn pick_arrives_when_sampler_closes() {
        let (sender, mut sampler) = showing();
        sampler.about_to_wait();
        assert!(sampler.is_active());
        assert!(sampler.take_outcome().is_none());
        sender.send(SampleOutcome::Picked(Color32::RED)).unwrap();
        sampler.about_to_wait();
        assert!(!sampler.is_active());
        assert_eq!(picked(&mut sampler), Some(Color32::RED));
    }

    #[test]
    fn cancel_reports_now_drops_late_pick() {
        let (sender, mut sampler) = showing();
        sampler.cancel();
        assert!(!sampler.is_active());
        assert!(matches!(sampler.take_outcome(), Some(SampleOutcome::Cancelled)));
        sampler.cancel();
        assert!(sampler.take_outcome().is_none());
        sender.send(SampleOutcome::Picked(Color32::RED)).unwrap();
        sampler.about_to_wait();
        assert!(sampler.take_outcome().is_none() && sampler.showing.is_none());
    }

    #[test]
    fn restart_takes_back_open_sampler() {
        let (sender, mut sampler) = showing();
        sampler.cancel();
        sampler.take_outcome();
        sampler.show();
        assert!(sampler.is_active());
        sender.send(SampleOutcome::Picked(Color32::BLUE)).unwrap();
        sampler.about_to_wait();
        assert_eq!(picked(&mut sampler), Some(Color32::BLUE));
    }

    #[test]
    fn sampler_gone_counts_as_cancelled() {
        let (sender, mut sampler) = showing();
        drop(sender);
        sampler.about_to_wait();
        assert!(!sampler.is_active());
        assert!(matches!(sampler.take_outcome(), Some(SampleOutcome::Cancelled)));
    }
}
