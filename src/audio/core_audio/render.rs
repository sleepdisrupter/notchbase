//! Copies tapped audio to the output with the gain ramped across the buffer, and measures the peak on the way.

use super::buffers::AudioBuffer;

struct Source<'a> {
    samples: &'a [f32],
    stride: usize,
    offset: usize,
}

impl<'a> Source<'a> {
    unsafe fn channel(buffers: &'a [AudioBuffer], index: usize) -> Option<Self> {
        let mut first = 0;
        for buffer in buffers {
            let channels = buffer.channels as usize;
            if index < first + channels {
                if buffer.data.is_null() || channels == 0 {
                    return None;
                }
                let len = buffer.bytes as usize / size_of::<f32>();
                let samples = unsafe { std::slice::from_raw_parts(buffer.data.cast::<f32>(), len) };
                return Some(Self { samples, stride: channels, offset: index - first });
            }
            first += channels;
        }
        None
    }

    fn sample(&self, frame: usize) -> f32 {
        self.samples.get(frame * self.stride + self.offset).copied().unwrap_or(0.0)
    }
}

struct Layout<'a> {
    input: &'a [AudioBuffer],
    in_channels: usize,
    out_channels: usize,
}

struct Feed<'a> {
    first: Option<Source<'a>>,
    second: Option<Source<'a>>,
    weight: f32,
}

impl<'a> Feed<'a> {
    unsafe fn for_channel(layout: &Layout<'a>, channel: usize) -> Self {
        let input = layout.input;
        unsafe {
            if layout.out_channels == 1 && layout.in_channels >= 2 {
                Self { first: Source::channel(input, 0), second: Source::channel(input, 1), weight: 0.5 }
            } else if channel < layout.in_channels {
                Self { first: Source::channel(input, channel), second: None, weight: 1.0 }
            } else {
                Self { first: None, second: None, weight: 0.0 }
            }
        }
    }

    fn sample(&self, frame: usize) -> f32 {
        let read = |source: &Option<Source<'_>>| source.as_ref().map_or(0.0, |s| s.sample(frame));
        (read(&self.first) + read(&self.second)) * self.weight
    }
}

fn channel_count(buffers: &[AudioBuffer]) -> usize {
    buffers.iter().map(|b| b.channels as usize).sum()
}

fn ramp(from: f32, to: f32, frame: usize, frames: usize) -> f32 {
    from + (to - from) * ((frame + 1) as f32 / frames as f32)
}

pub unsafe fn render(input: &[AudioBuffer], output: &[AudioBuffer], from: f32, to: f32) -> f32 {
    let layout = Layout { input, in_channels: channel_count(input), out_channels: channel_count(output) };
    let mut peak = 0.0f32;
    let mut first_channel = 0;
    for buffer in output {
        let channels = buffer.channels as usize;
        if channels > 0 && !buffer.data.is_null() {
            let frames = buffer.bytes as usize / size_of::<f32>() / channels;
            let samples = unsafe { std::slice::from_raw_parts_mut(buffer.data.cast::<f32>(), frames * channels) };
            for c in 0..channels {
                let feed = unsafe { Feed::for_channel(&layout, first_channel + c) };
                for f in 0..frames {
                    let sample = feed.sample(f);
                    peak = peak.max(sample.abs());
                    samples[f * channels + c] = sample * ramp(from, to, f, frames);
                }
            }
        }
        first_channel += channels;
    }
    peak
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stereo_passes_with_gain_ramp() {
        let mut input = [1.0, -1.0, 1.0, -1.0, 1.0, -1.0, 1.0, -1.0];
        let mut output = [9.0f32; 8];
        let (i, o) = ([AudioBuffer::over(&mut input, 2)], [AudioBuffer::over(&mut output, 2)]);
        let peak = unsafe { render(&i, &o, 0.0, 1.0) };
        assert_eq!(peak, 1.0);
        assert_eq!(output, [0.25, -0.25, 0.5, -0.5, 0.75, -0.75, 1.0, -1.0]);
    }

    #[test]
    fn planar_to_interleaved_extras_silent() {
        let (mut left, mut right) = ([0.5f32, 0.5], [-0.25f32, -0.25]);
        let mut output = [9.0f32; 8];
        let i = [AudioBuffer::over(&mut left, 1), AudioBuffer::over(&mut right, 1)];
        let o = [AudioBuffer::over(&mut output, 4)];
        unsafe { render(&i, &o, 1.0, 1.0) };
        assert_eq!(output, [0.5, -0.25, 0.0, 0.0, 0.5, -0.25, 0.0, 0.0]);
    }

    #[test]
    fn mono_folds_down_missing_is_silent() {
        let mut input = [1.0f32, 0.0, 0.5, 0.5];
        let mut output = [9.0f32; 3];
        let (i, o) = ([AudioBuffer::over(&mut input, 2)], [AudioBuffer::over(&mut output, 1)]);
        unsafe { render(&i, &o, 1.0, 1.0) };
        assert_eq!(output, [0.5, 0.5, 0.0]);

        let mut output = [9.0f32; 4];
        let o = [AudioBuffer::over(&mut output, 2)];
        assert_eq!(unsafe { render(&[], &o, 1.0, 1.0) }, 0.0);
        assert_eq!(output, [0.0; 4]);
    }
}
