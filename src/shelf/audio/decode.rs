//! Decodes any audio Symphonia reads into 16 or 24-bit samples, keeping the source's rate and channels.

use std::fs::File;
use std::io;
use std::path::Path;

use symphonia::core::codecs::audio::{AudioDecoder, AudioDecoderOptions};
use symphonia::core::errors::Error;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatOptions, FormatReader, TrackType};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;

use crate::shelf::names::lowercase_extension;
use crate::shelf::video::MediaError;

const PLAIN_BITS: u32 = 16;
const DEEP_BITS: u32 = 24;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Shape {
    pub(super) rate: u32,
    pub(super) channels: usize,
    pub(super) bits: u32,
}

pub(super) struct Decoder {
    packets: Packets,
    pub(super) shape: Shape,
    frames: Option<u64>,
    decoded: u64,
    buffered: bool,
}

struct Packets {
    format: Box<dyn FormatReader>,
    decoder: Box<dyn AudioDecoder>,
    track: u32,
    bits: u32,
    samples: Vec<i32>,
}

impl Decoder {
    pub(super) fn open(path: &Path) -> Result<Self, MediaError> {
        let format = reader(path).ok_or(MediaError::Unsupported)?;
        let track = format.default_track(TrackType::Audio).ok_or(MediaError::NoAudio)?;
        let params = track.codec_params.as_ref().and_then(|params| params.audio()).ok_or(MediaError::Unsupported)?;
        let decoder = symphonia::default::get_codecs()
            .make_audio_decoder(params, &AudioDecoderOptions::default())
            .map_err(|_| MediaError::Unsupported)?;
        let bits = if params.bits_per_sample.is_some_and(|bits| bits > PLAIN_BITS) { DEEP_BITS } else { PLAIN_BITS };
        let (track, frames) = (track.id, track.num_frames);
        let mut packets = Packets { format, decoder, track, bits, samples: Vec::new() };
        let shape = packets.decode_next()?.ok_or(MediaError::Unsupported)?;
        Ok(Self { packets, shape, frames, decoded: 0, buffered: true })
    }

    pub(super) fn next(&mut self) -> Result<Option<&[i32]>, MediaError> {
        if !std::mem::take(&mut self.buffered) {
            match self.packets.decode_next()? {
                Some(shape) if shape == self.shape => {}
                Some(_) => return Err(MediaError::Unsupported),
                None => return Ok(None),
            }
        }
        let samples = &self.packets.samples;
        self.decoded += (samples.len() / self.shape.channels) as u64;
        Ok(Some(samples))
    }

    pub(super) fn progress(&self) -> Option<f32> {
        self.frames.filter(|&frames| frames > 0).map(|frames| (self.decoded as f64 / frames as f64) as f32)
    }
}

impl Packets {
    fn decode_next(&mut self) -> Result<Option<Shape>, MediaError> {
        loop {
            let packet = match self.format.next_packet() {
                Ok(Some(packet)) => packet,
                Ok(None) => return Ok(None),
                Err(Error::IoError(err)) if err.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
                Err(_) => return Err(MediaError::Failed),
            };
            if packet.track_id != self.track {
                continue;
            }
            let buffer = match self.decoder.decode(&packet) {
                Ok(buffer) => buffer,
                Err(Error::DecodeError(_)) => continue,
                Err(_) => return Err(MediaError::Failed),
            };
            if buffer.frames() == 0 {
                continue;
            }
            let spec = buffer.spec();
            let shape = Shape { rate: spec.rate(), channels: spec.channels().count(), bits: self.bits };
            if shape.rate == 0 || shape.channels == 0 {
                return Err(MediaError::Unsupported);
            }
            self.samples.resize(buffer.samples_interleaved(), 0);
            buffer.copy_to_slice_interleaved(&mut self.samples);
            let shift = 32 - self.bits;
            for sample in &mut self.samples {
                *sample >>= shift;
            }
            return Ok(Some(shape));
        }
    }
}

pub(super) fn seconds(path: &Path) -> Option<f64> {
    let format = reader(path)?;
    let track = format.default_track(TrackType::Audio)?;
    let rate = track.codec_params.as_ref()?.audio()?.sample_rate.filter(|&rate| rate > 0)?;
    Some(track.num_frames? as f64 / f64::from(rate))
}

fn reader(path: &Path) -> Option<Box<dyn FormatReader>> {
    let file = File::open(path).ok()?;
    let stream = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    hint.with_extension(&lowercase_extension(path));
    symphonia::default::get_probe().probe(&hint, stream, FormatOptions::default(), MetadataOptions::default()).ok()
}
