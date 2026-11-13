//! Writes WAV with hound.

use std::fs::File;
use std::io::BufWriter;
use std::path::Path;

use hound::{SampleFormat, WavSpec, WavWriter};

use super::decode::Shape;
use crate::shelf::video::MediaError;

pub(super) struct Writer {
    wav: WavWriter<BufWriter<File>>,
}

impl Writer {
    pub(super) fn create(target: &Path, shape: Shape) -> Result<Self, MediaError> {
        let spec = WavSpec {
            channels: u16::try_from(shape.channels).map_err(|_| MediaError::Unsupported)?,
            sample_rate: shape.rate,
            bits_per_sample: shape.bits as u16,
            sample_format: SampleFormat::Int,
        };
        let wav = WavWriter::create(target, spec).map_err(|_| MediaError::Failed)?;
        Ok(Self { wav })
    }

    pub(super) fn write(&mut self, samples: &[i32]) -> Result<(), MediaError> {
        for &sample in samples {
            self.wav.write_sample(sample).map_err(|_| MediaError::Failed)?;
        }
        Ok(())
    }

    pub(super) fn finish(self) -> Result<(), MediaError> {
        self.wav.finalize().map_err(|_| MediaError::Failed)
    }
}
