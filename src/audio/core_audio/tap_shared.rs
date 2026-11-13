//! What a tap's audio callback shares with the app: the gain to ramp to and the loudest peak since the last read.

use std::sync::atomic::{AtomicU32, Ordering};

use super::buffers::AudioBufferList;
use super::render::render;

pub struct TapShared {
    target: AtomicU32,
    current: AtomicU32,
    peak: AtomicU32,
    skip: usize,
}

impl TapShared {
    pub fn new(gain: f32, skip: usize) -> Self {
        Self {
            target: AtomicU32::new(gain.to_bits()),
            current: AtomicU32::new(gain.to_bits()),
            peak: AtomicU32::new(0),
            skip,
        }
    }

    pub fn set_gain(&self, gain: f32) {
        self.target.store(gain.clamp(0.0, 1.0).to_bits(), Ordering::Relaxed);
    }

    pub fn take_peak(&self) -> f32 {
        f32::from_bits(self.peak.swap(0, Ordering::Relaxed)).clamp(0.0, 1.0)
    }

    pub unsafe fn process(&self, input: *const AudioBufferList, output: *mut AudioBufferList) {
        let (inputs, outputs) = unsafe { (AudioBufferList::buffers(input), AudioBufferList::buffers(output)) };
        // The output device's own inputs, like a headset mic, come first. Skipping them keeps the mic out.
        let tapped = &inputs[self.skip.min(inputs.len())..];
        let from = f32::from_bits(self.current.load(Ordering::Relaxed));
        let to = f32::from_bits(self.target.load(Ordering::Relaxed));
        let peak = unsafe { render(tapped, outputs, from, to) };
        self.current.store(to.to_bits(), Ordering::Relaxed);
        if peak.is_finite() {
            self.peak.fetch_max(peak.to_bits(), Ordering::Relaxed);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::core_audio::buffers::AudioBuffer;

    #[repr(C)]
    struct TwoBufferList {
        count: u32,
        buffers: [AudioBuffer; 2],
    }

    #[test]
    fn skips_device_inputs_tracks_peak() {
        let mut mic = [0.9f32, 0.9];
        let mut tap = [0.5f32, -0.5, 0.5, -0.5];
        let mut output = [0.0f32; 4];
        let input =
            TwoBufferList { count: 2, buffers: [AudioBuffer::over(&mut mic, 1), AudioBuffer::over(&mut tap, 2)] };
        let input = (&input as *const TwoBufferList).cast::<AudioBufferList>();
        let mut out = AudioBufferList { count: 1, buffers: [AudioBuffer::over(&mut output, 2)] };
        let shared = TapShared::new(0.5, 1);
        unsafe { shared.process(input, &mut out) };
        assert_eq!(output, [0.25, -0.25, 0.25, -0.25], "mic must never reach the speakers");
        assert_eq!(shared.take_peak(), 0.5);
        assert_eq!(shared.take_peak(), 0.0, "reading resets the peak");

        shared.set_gain(1.0);
        unsafe { shared.process(input, &mut out) };
        assert_eq!(output, [0.375, -0.375, 0.5, -0.5]);
    }
}
