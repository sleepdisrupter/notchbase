//! The PCM format everything gets converted to, plus downmixing and resampling.

pub const CHANNELS: usize = 2;
pub const RATE: u32 = 48_000;
const SAMPLE_BYTES: usize = 2;
pub const FRAME_BYTES: usize = CHANNELS * SAMPLE_BYTES;
// Center and rear channels are mixed in at about -3 dB.
const SURROUND_MIX: f32 = 0.7;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Encoding {
    Int16,
    Int24,
    Int32,
    Float32,
}

impl Encoding {
    pub(crate) fn from_format(float: bool, bits: u16) -> Option<Self> {
        match (float, bits) {
            (true, 32) => Some(Self::Float32),
            (false, 16) => Some(Self::Int16),
            (false, 24) => Some(Self::Int24),
            (false, 32) => Some(Self::Int32),
            _ => None,
        }
    }

    fn bytes(self) -> usize {
        match self {
            Self::Int16 => 2,
            Self::Int24 => 3,
            Self::Int32 | Self::Float32 => 4,
        }
    }

    fn read(self, bytes: &[u8]) -> f32 {
        match self {
            Self::Int16 => f32::from(i16::from_le_bytes([bytes[0], bytes[1]])) / 32_768.0,
            Self::Int24 => (i32::from_le_bytes([0, bytes[0], bytes[1], bytes[2]]) >> 8) as f32 / 8_388_608.0,
            Self::Int32 => i32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as f32 / 2_147_483_648.0,
            Self::Float32 => f32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Layout {
    pub(crate) encoding: Encoding,
    pub(crate) channels: usize,
    pub(crate) block: usize,
}

impl Layout {
    pub(crate) fn to_stereo(self, input: &[u8], out: &mut Vec<u8>) {
        out.clear();
        let width = self.encoding.bytes();
        if self.channels == 0 || self.block < self.channels * width {
            return;
        }
        for frame in input.chunks_exact(self.block) {
            let sample = |channel: usize| self.encoding.read(&frame[channel * width..(channel + 1) * width]);
            let [left, right] = match self.channels {
                1 => [sample(0); 2],
                channels => {
                    let center = if channels >= 3 { SURROUND_MIX * sample(2) } else { 0.0 };
                    let rear = |channel: usize| if channels >= 6 { SURROUND_MIX * sample(channel) } else { 0.0 };
                    [sample(0) + center + rear(4), sample(1) + center + rear(5)]
                }
            };
            for value in [left, right] {
                out.extend_from_slice(&quantize(value).to_le_bytes());
            }
        }
    }
}

// Linear resampling that carries its position over from one packet to the next.
pub(crate) struct Resampler {
    step: f64,
    position: f64,
    previous: [f32; CHANNELS],
}

impl Resampler {
    pub(crate) fn new(from: u32, to: u32) -> Self {
        Self { step: f64::from(from) / f64::from(to.max(1)), position: 0.0, previous: [0.0; CHANNELS] }
    }

    pub(crate) fn process(&mut self, input: &[u8], out: &mut Vec<u8>) {
        out.clear();
        let frames: Vec<[f32; CHANNELS]> = input
            .as_chunks::<FRAME_BYTES>()
            .0
            .iter()
            .map(|frame| [0, 1].map(|channel| Encoding::Int16.read(&frame[channel * SAMPLE_BYTES..])))
            .collect();
        let Some(&last) = frames.last() else { return };
        let count = frames.len() as f64;
        let at = |index: usize| if index == 0 { self.previous } else { frames[index - 1] };
        while self.position < count {
            let index = self.position.floor() as usize;
            let t = (self.position - index as f64) as f32;
            let (a, b) = (at(index), at(index + 1));
            for channel in 0..CHANNELS {
                out.extend_from_slice(&quantize(a[channel] + (b[channel] - a[channel]) * t).to_le_bytes());
            }
            self.position += self.step;
        }
        self.position -= count;
        self.previous = last;
    }
}

fn quantize(value: f32) -> i16 {
    (value.clamp(-1.0, 1.0) * 32_767.0).round() as i16
}

pub(crate) fn downmix(frame: &[u8]) -> f32 {
    let sum: f32 = frame.as_chunks::<SAMPLE_BYTES>().0.iter().map(|sample| Encoding::Int16.read(sample)).sum();
    sum / CHANNELS as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    fn samples(bytes: &[u8]) -> Vec<i16> {
        bytes.as_chunks::<2>().0.iter().map(|&pair| i16::from_le_bytes(pair)).collect()
    }

    #[test]
    fn float_stereo_becomes_sixteen_bit() {
        let input: Vec<u8> = [0.5f32, -1.0, 2.0, 0.0].iter().flat_map(|v| v.to_le_bytes()).collect();
        let layout = Layout { encoding: Encoding::Float32, channels: 2, block: 8 };
        let mut out = Vec::new();
        layout.to_stereo(&input, &mut out);
        assert_eq!(samples(&out), [16_384, -32_767, 32_767, 0]);
    }

    #[test]
    fn mono_doubles_and_surround_folds() {
        let mut out = Vec::new();
        let mono = Layout { encoding: Encoding::Int16, channels: 1, block: 2 };
        mono.to_stereo(&1000i16.to_le_bytes(), &mut out);
        assert_eq!(samples(&out), [1000, 1000]);

        let frame: Vec<u8> = [0.1f32, 0.2, 0.5, 0.9, 0.0, 0.3].iter().flat_map(|v| v.to_le_bytes()).collect();
        let surround = Layout { encoding: Encoding::Float32, channels: 6, block: 24 };
        surround.to_stereo(&frame, &mut out);
        assert_eq!(samples(&out), [quantize(0.1 + 0.35), quantize(0.2 + 0.35 + 0.21)]);
    }

    #[test]
    fn resampler_keeps_pace_across_packets() {
        let tone: Vec<u8> =
            (0..4410).flat_map(|i: i32| [(i % 100) as i16 * 100; 2]).flat_map(i16::to_le_bytes).collect();
        let mut resampler = Resampler::new(44_100, RATE);
        let (mut out, mut total) = (Vec::new(), 0);
        for packet in tone.chunks(441 * 4) {
            resampler.process(packet, &mut out);
            total += out.len() / FRAME_BYTES;
        }
        assert!((4799..=4801).contains(&total), "{total}");
        let mut same = Resampler::new(RATE, RATE);
        same.process(&tone[..16], &mut out);
        assert_eq!(samples(&out)[2..], samples(&tone[..12])[..]);
    }

    #[test]
    fn wide_integer_formats_are_scaled() {
        let mut out = Vec::new();
        let packed24 = [0x00, 0x00, 0x40, 0x00, 0x00, 0xC0];
        Layout { encoding: Encoding::Int24, channels: 2, block: 6 }.to_stereo(&packed24, &mut out);
        assert_eq!(samples(&out), [16_384, -16_384]);
        let packed32: Vec<u8> = [i32::MAX, i32::MIN].iter().flat_map(|v| v.to_le_bytes()).collect();
        Layout { encoding: Encoding::Int32, channels: 2, block: 8 }.to_stereo(&packed32, &mut out);
        assert_eq!(samples(&out), [32_767, -32_767]);
        assert_eq!(Encoding::from_format(true, 64), None);
    }

    #[test]
    fn stereo_frames_fold_to_mono() {
        let frame = [i16::MAX.to_le_bytes(), i16::MAX.to_le_bytes()].concat();
        assert!((downmix(&frame) - 1.0).abs() < 1e-3);
        let opposite = [16_384i16.to_le_bytes(), (-16_384i16).to_le_bytes()].concat();
        assert_eq!(downmix(&opposite), 0.0);
    }

    #[test]
    fn frame_is_16_bit_stereo_at_48k() {
        assert_eq!((CHANNELS, RATE, FRAME_BYTES), (2, 48_000, 4));
    }

    #[test]
    fn recognises_supported_formats() {
        assert_eq!(Encoding::from_format(false, 16), Some(Encoding::Int16));
        assert_eq!(Encoding::from_format(false, 24), Some(Encoding::Int24));
        assert_eq!(Encoding::from_format(false, 32), Some(Encoding::Int32));
        assert_eq!(Encoding::from_format(true, 32), Some(Encoding::Float32));
        for (float, bits) in [(true, 16), (false, 8), (false, 0), (true, 24)] {
            assert_eq!(Encoding::from_format(float, bits), None, "{float} {bits}");
        }
        let widths = [Encoding::Int16, Encoding::Int24, Encoding::Int32, Encoding::Float32].map(Encoding::bytes);
        assert_eq!(widths, [2, 3, 4, 4]);
    }

    #[test]
    fn bad_layout_yields_nothing() {
        let mut out = vec![1, 2, 3];
        Layout { encoding: Encoding::Int16, channels: 0, block: 4 }.to_stereo(&[0; 8], &mut out);
        assert!(out.is_empty(), "the output is cleared first");
        Layout { encoding: Encoding::Float32, channels: 2, block: 4 }.to_stereo(&[0; 8], &mut out);
        assert!(out.is_empty(), "a block too small for its channels");
    }

    #[test]
    fn drops_partial_last_frame() {
        let mut out = Vec::new();
        let input: Vec<u8> = [100i16, -100, 7].iter().flat_map(|v| v.to_le_bytes()).collect();
        Layout { encoding: Encoding::Int16, channels: 2, block: 4 }.to_stereo(&input, &mut out);
        assert_eq!(samples(&out), [100, -100]);
    }

    #[test]
    fn quantize_clamps() {
        assert_eq!([quantize(2.0), quantize(-2.0), quantize(0.0)], [32_767, -32_767, 0]);
        assert_eq!(quantize(f32::INFINITY), 32_767);
        assert_eq!(quantize(0.5), 16_384);
    }

    #[test]
    fn empty_packet_resamples_to_nothing() {
        let mut resampler = Resampler::new(44_100, RATE);
        let mut out = vec![9, 9];
        resampler.process(&[], &mut out);
        assert!(out.is_empty());
        assert_eq!(resampler.position, 0.0);
        let zero_target = Resampler::new(44_100, 0);
        assert_eq!(zero_target.step, 44_100.0, "a zero target rate counts as one");
    }

    #[test]
    fn downmix_silence_and_one_channel() {
        assert_eq!(downmix(&[0; FRAME_BYTES]), 0.0);
        let full_left = [i16::MIN.to_le_bytes(), 0i16.to_le_bytes()].concat();
        assert_eq!(downmix(&full_left), -0.5);
        assert_eq!(downmix(&[]), 0.0);
    }
}
