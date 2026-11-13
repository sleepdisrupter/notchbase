//! The Media Foundation sink writer behind a recording: H.264 High profile video from NV12 frames and AAC audio
//! from 16-bit PCM, in an MPEG-4 file.

use std::path::Path;

use notchbase_wasapi::CHANNELS;
use windows::Win32::Foundation::E_POINTER;
use windows::Win32::Media::MediaFoundation::{
    CODECAPI_AVEncCommonMeanBitRate, CODECAPI_AVEncCommonRateControlMode, IMFAttributes, IMFMediaType, IMFSinkWriter,
    MF_MT_AAC_AUDIO_PROFILE_LEVEL_INDICATION, MF_MT_AAC_PAYLOAD_TYPE, MF_MT_ALL_SAMPLES_INDEPENDENT,
    MF_MT_AUDIO_AVG_BYTES_PER_SECOND, MF_MT_AUDIO_BITS_PER_SAMPLE, MF_MT_AUDIO_BLOCK_ALIGNMENT,
    MF_MT_AUDIO_NUM_CHANNELS, MF_MT_AUDIO_SAMPLES_PER_SECOND, MF_MT_AVG_BITRATE, MF_MT_DEFAULT_STRIDE,
    MF_MT_FRAME_RATE, MF_MT_FRAME_SIZE, MF_MT_INTERLACE_MODE, MF_MT_MAJOR_TYPE, MF_MT_MPEG2_PROFILE,
    MF_MT_PIXEL_ASPECT_RATIO, MF_MT_SUBTYPE, MF_MT_TRANSFER_FUNCTION, MF_MT_VIDEO_NOMINAL_RANGE, MF_MT_VIDEO_PRIMARIES,
    MF_MT_YUV_MATRIX, MF_READWRITE_ENABLE_HARDWARE_TRANSFORMS, MF_TRANSCODE_CONTAINERTYPE, MFAudioFormat_AAC,
    MFAudioFormat_PCM, MFCreateAttributes, MFCreateMediaType, MFCreateMemoryBuffer, MFCreateSample,
    MFCreateSinkWriterFromURL, MFMediaType_Audio, MFMediaType_Video, MFNominalRange_16_235,
    MFTranscodeContainerType_MPEG4, MFVideoFormat_H264, MFVideoFormat_NV12, MFVideoInterlace_Progressive,
    MFVideoPrimaries_BT709, MFVideoTransFunc_709, MFVideoTransferMatrix_BT709,
    eAVEncCommonRateControlMode_UnconstrainedVBR, eAVEncH264VProfile_High,
};
use windows::core::{Error, GUID, HSTRING, Result};

/// 192 kbps.
const AAC_BYTES_PER_SECOND: u32 = 24_000;
const AAC_LC: u32 = 0x29;
const PCM_BITS: u32 = 16;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Video {
    pub(super) size: [u32; 2],
    pub(super) fps: u32,
    pub(super) bitrate: u32,
}

pub(super) struct Writer {
    sink: IMFSinkWriter,
    video: u32,
    audio: Option<u32>,
}

impl Writer {
    /// Tries with hardware encoders first, and falls back to software if they can't be set up.
    pub(super) fn create(path: &Path, video: Video, audio_rate: Option<u32>) -> std::result::Result<Self, String> {
        Self::open(path, video, audio_rate, true)
            .or_else(|_| Self::open(path, video, audio_rate, false))
            .map_err(|err| format!("Couldn't set up the video encoder: {}", err.message()))
    }

    pub(super) fn video(&self, nv12: &[u8], time: i64, duration: i64) -> std::result::Result<(), String> {
        self.write(self.video, nv12, time, duration)
    }

    pub(super) fn audio(&self, pcm: &[u8], time: i64, duration: i64) -> std::result::Result<(), String> {
        match self.audio {
            Some(stream) if !pcm.is_empty() => self.write(stream, pcm, time, duration),
            _ => Ok(()),
        }
    }

    pub(super) fn finish(self) -> std::result::Result<(), String> {
        unsafe { self.sink.Finalize() }.map_err(|err| format!("Couldn't finish the recording: {}", err.message()))
    }

    fn open(path: &Path, video: Video, audio_rate: Option<u32>, hardware: bool) -> Result<Self> {
        let settings = attributes(2)?;
        unsafe {
            settings.SetGUID(&MF_TRANSCODE_CONTAINERTYPE, &MFTranscodeContainerType_MPEG4)?;
            settings.SetUINT32(&MF_READWRITE_ENABLE_HARDWARE_TRANSFORMS, u32::from(hardware))?;
        }
        let sink = unsafe { MFCreateSinkWriterFromURL(&HSTRING::from(path), None, &settings) }?;
        let stream = unsafe { sink.AddStream(&h264(video)?) }?;
        unsafe { sink.SetInputMediaType(stream, &nv12(video)?, Some(&rate_control(video.bitrate)?)) }?;
        let audio = match audio_rate {
            Some(rate) => {
                let index = unsafe { sink.AddStream(&aac(rate)?) }?;
                unsafe { sink.SetInputMediaType(index, &pcm16(rate)?, None) }?;
                Some(index)
            }
            None => None,
        };
        unsafe { sink.BeginWriting() }?;
        Ok(Self { sink, video: stream, audio })
    }

    fn write(&self, stream: u32, bytes: &[u8], time: i64, duration: i64) -> std::result::Result<(), String> {
        self.sample(stream, bytes, time, duration).map_err(|err| err.message())
    }

    fn sample(&self, stream: u32, bytes: &[u8], time: i64, duration: i64) -> Result<()> {
        let length = bytes.len() as u32;
        let buffer = unsafe { MFCreateMemoryBuffer(length) }?;
        let mut data = std::ptr::null_mut();
        unsafe { buffer.Lock(&mut data, None, None) }?;
        if !data.is_null() {
            unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), data, bytes.len()) };
        }
        unsafe {
            buffer.Unlock()?;
            buffer.SetCurrentLength(length)?;
        }
        let sample = unsafe { MFCreateSample() }?;
        unsafe {
            sample.AddBuffer(&buffer)?;
            sample.SetSampleTime(time)?;
            sample.SetSampleDuration(duration)?;
            self.sink.WriteSample(stream, &sample)
        }
    }
}

fn attributes(capacity: u32) -> Result<IMFAttributes> {
    let mut attributes = None;
    unsafe { MFCreateAttributes(&mut attributes, capacity) }?;
    attributes.ok_or_else(|| Error::from(E_POINTER))
}

fn typed(major: &GUID, subtype: &GUID) -> Result<IMFMediaType> {
    let media_type = unsafe { MFCreateMediaType() }?;
    unsafe {
        media_type.SetGUID(&MF_MT_MAJOR_TYPE, major)?;
        media_type.SetGUID(&MF_MT_SUBTYPE, subtype)?;
    }
    Ok(media_type)
}

fn pack([high, low]: [u32; 2]) -> u64 {
    (u64::from(high) << 32) | u64::from(low)
}

fn picture(subtype: &GUID, video: Video) -> Result<IMFMediaType> {
    let media_type = typed(&MFMediaType_Video, subtype)?;
    unsafe {
        media_type.SetUINT64(&MF_MT_FRAME_SIZE, pack(video.size))?;
        media_type.SetUINT64(&MF_MT_FRAME_RATE, pack([video.fps, 1]))?;
        media_type.SetUINT64(&MF_MT_PIXEL_ASPECT_RATIO, pack([1, 1]))?;
        media_type.SetUINT32(&MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive.0 as u32)?;
        media_type.SetUINT32(&MF_MT_YUV_MATRIX, MFVideoTransferMatrix_BT709.0 as u32)?;
        media_type.SetUINT32(&MF_MT_VIDEO_NOMINAL_RANGE, MFNominalRange_16_235.0 as u32)?;
        media_type.SetUINT32(&MF_MT_VIDEO_PRIMARIES, MFVideoPrimaries_BT709.0 as u32)?;
        media_type.SetUINT32(&MF_MT_TRANSFER_FUNCTION, MFVideoTransFunc_709.0 as u32)?;
    }
    Ok(media_type)
}

fn h264(video: Video) -> Result<IMFMediaType> {
    let media_type = picture(&MFVideoFormat_H264, video)?;
    unsafe {
        media_type.SetUINT32(&MF_MT_AVG_BITRATE, video.bitrate)?;
        media_type.SetUINT32(&MF_MT_MPEG2_PROFILE, eAVEncH264VProfile_High.0 as u32)?;
    }
    Ok(media_type)
}

fn nv12(video: Video) -> Result<IMFMediaType> {
    let media_type = picture(&MFVideoFormat_NV12, video)?;
    unsafe { media_type.SetUINT32(&MF_MT_DEFAULT_STRIDE, video.size[0]) }?;
    Ok(media_type)
}

fn rate_control(bitrate: u32) -> Result<IMFAttributes> {
    let encoding = attributes(2)?;
    unsafe {
        encoding
            .SetUINT32(&CODECAPI_AVEncCommonRateControlMode, eAVEncCommonRateControlMode_UnconstrainedVBR.0 as u32)?;
        encoding.SetUINT32(&CODECAPI_AVEncCommonMeanBitRate, bitrate)?;
    }
    Ok(encoding)
}

fn audio_type(subtype: &GUID, rate: u32) -> Result<IMFMediaType> {
    let media_type = typed(&MFMediaType_Audio, subtype)?;
    unsafe {
        media_type.SetUINT32(&MF_MT_AUDIO_BITS_PER_SAMPLE, PCM_BITS)?;
        media_type.SetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND, rate)?;
        media_type.SetUINT32(&MF_MT_AUDIO_NUM_CHANNELS, CHANNELS as u32)?;
    }
    Ok(media_type)
}

fn pcm16(rate: u32) -> Result<IMFMediaType> {
    let media_type = audio_type(&MFAudioFormat_PCM, rate)?;
    let block = CHANNELS as u32 * PCM_BITS / 8;
    unsafe {
        media_type.SetUINT32(&MF_MT_AUDIO_BLOCK_ALIGNMENT, block)?;
        media_type.SetUINT32(&MF_MT_AUDIO_AVG_BYTES_PER_SECOND, block * rate)?;
        media_type.SetUINT32(&MF_MT_ALL_SAMPLES_INDEPENDENT, 1)?;
    }
    Ok(media_type)
}

fn aac(rate: u32) -> Result<IMFMediaType> {
    let media_type = audio_type(&MFAudioFormat_AAC, rate)?;
    unsafe {
        media_type.SetUINT32(&MF_MT_AUDIO_AVG_BYTES_PER_SECOND, AAC_BYTES_PER_SECOND)?;
        media_type.SetUINT32(&MF_MT_AAC_PAYLOAD_TYPE, 0)?;
        media_type.SetUINT32(&MF_MT_AAC_AUDIO_PROFILE_LEVEL_INDICATION, AAC_LC)?;
    }
    Ok(media_type)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pack_puts_high_then_low() {
        assert_eq!(pack([1920, 1080]), (1920u64 << 32) | 1080);
        assert_eq!(pack([0, 0]), 0);
        assert_eq!(pack([u32::MAX, u32::MAX]), u64::MAX);
        assert_eq!(pack([60, 1]) >> 32, 60);
    }
}
