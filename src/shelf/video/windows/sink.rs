//! The sink writer that encodes the output file, with hardware encoders first and software if those fail.

use std::path::Path;

use ::windows::Win32::Media::MediaFoundation::{
    CODECAPI_AVEncCommonMeanBitRate, CODECAPI_AVEncCommonRateControlMode, IMFAttributes, IMFSample, IMFSinkWriter,
    MF_READWRITE_ENABLE_HARDWARE_TRANSFORMS, MF_SINK_WRITER_DISABLE_THROTTLING, MF_TRANSCODE_CONTAINERTYPE,
    MFCreateSinkWriterFromURL, MFTranscodeContainerType_MP3, MFTranscodeContainerType_MPEG4,
    eAVEncCommonRateControlMode_UnconstrainedVBR,
};
use ::windows::core::{Error, HSTRING};

use super::diagnostics::Diagnose;
use super::media_type::attributes;
use super::plan::Track;
use super::reader::Stream;
use crate::shelf::video::{Container, MediaError};

pub(super) struct Sink {
    writer: IMFSinkWriter,
    lanes: Vec<(Stream, u32)>,
}

impl Sink {
    pub(super) fn open(
        target: &Path,
        container: Container,
        tracks: &[Track],
        source: &Path,
    ) -> Result<Self, MediaError> {
        Self::open_with(target, container, tracks, true)
            .or_else(|_| Self::open_with(target, container, tracks, false))
            .failed("set up the encoder", source)
    }

    pub(super) fn lanes(&self) -> &[(Stream, u32)] {
        &self.lanes
    }

    pub(super) fn write(&self, index: u32, sample: &IMFSample) -> Result<(), Error> {
        unsafe { self.writer.WriteSample(index, sample) }
    }

    pub(super) fn finish(self) -> Result<(), Error> {
        unsafe { self.writer.Finalize() }
    }

    fn open_with(target: &Path, container: Container, tracks: &[Track], hardware: bool) -> Result<Self, Error> {
        let settings = attributes(3)?;
        let container_type = match container {
            Container::Mp3 => &MFTranscodeContainerType_MP3,
            Container::Mp4 | Container::Mov | Container::M4a => &MFTranscodeContainerType_MPEG4,
        };
        unsafe {
            settings.SetGUID(&MF_TRANSCODE_CONTAINERTYPE, container_type)?;
            settings.SetUINT32(&MF_SINK_WRITER_DISABLE_THROTTLING, 1)?;
            settings.SetUINT32(&MF_READWRITE_ENABLE_HARDWARE_TRANSFORMS, u32::from(hardware))?;
        }
        let writer = unsafe { MFCreateSinkWriterFromURL(&HSTRING::from(target), None, &settings) }?;
        let mut lanes = Vec::with_capacity(tracks.len());
        for track in tracks {
            let index = unsafe { writer.AddStream(&track.output) }?;
            let encoding = track.bitrate.map(rate_control).transpose()?;
            unsafe { writer.SetInputMediaType(index, &track.input, encoding.as_ref()) }?;
            lanes.push((track.stream, index));
        }
        unsafe { writer.BeginWriting() }?;
        Ok(Self { writer, lanes })
    }
}

fn rate_control(bitrate: u32) -> Result<IMFAttributes, Error> {
    let encoding = attributes(2)?;
    unsafe {
        encoding
            .SetUINT32(&CODECAPI_AVEncCommonRateControlMode, eAVEncCommonRateControlMode_UnconstrainedVBR.0 as u32)?;
        encoding.SetUINT32(&CODECAPI_AVEncCommonMeanBitRate, bitrate)?;
    }
    Ok(encoding)
}
