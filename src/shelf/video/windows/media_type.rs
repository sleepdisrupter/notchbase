//! Reading and building Media Foundation media types: size pairs, rates, and the audio types the encoders take.

use ::windows::Win32::Foundation::E_POINTER;
use ::windows::Win32::Media::MediaFoundation::{
    IMFAttributes, IMFMediaType, MF_MT_AAC_AUDIO_PROFILE_LEVEL_INDICATION, MF_MT_AAC_PAYLOAD_TYPE,
    MF_MT_ALL_SAMPLES_INDEPENDENT, MF_MT_AUDIO_AVG_BYTES_PER_SECOND, MF_MT_AUDIO_BITS_PER_SAMPLE,
    MF_MT_AUDIO_BLOCK_ALIGNMENT, MF_MT_AUDIO_NUM_CHANNELS, MF_MT_AUDIO_SAMPLES_PER_SECOND, MF_MT_AVG_BITRATE,
    MF_MT_FRAME_RATE, MF_MT_FRAME_SIZE, MF_MT_INTERLACE_MODE, MF_MT_MAJOR_TYPE, MF_MT_MPEG2_PROFILE,
    MF_MT_PIXEL_ASPECT_RATIO, MF_MT_SUBTYPE, MF_MT_VIDEO_ROTATION, MFAudioFormat_AAC, MFAudioFormat_MP3,
    MFAudioFormat_PCM, MFCreateAttributes, MFCreateMediaType, MFMediaType_Audio, MFMediaType_Video, MFVideoFormat_H264,
    MFVideoInterlace_Progressive, eAVEncH264VProfile_High,
};
use ::windows::core::{Error, GUID, Result};

const AAC_LC_PROFILE_LEVEL: u32 = 0x29;
const PCM_BITS: u32 = 16;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct AudioShape {
    pub(super) rate: u32,
    pub(super) channels: u32,
}

pub(super) fn pack([high, low]: [u32; 2]) -> u64 {
    (u64::from(high) << 32) | u64::from(low)
}

pub(super) fn unpack(value: u64) -> [u32; 2] {
    [(value >> 32) as u32, value as u32]
}

pub(super) fn pair(media_type: &IMFMediaType, key: &GUID) -> Option<[u32; 2]> {
    let value = unsafe { media_type.GetUINT64(key) }.ok()?;
    Some(unpack(value)).filter(|[first, second]| *first > 0 && *second > 0)
}

pub(super) fn number(media_type: &IMFMediaType, key: &GUID) -> Option<u32> {
    unsafe { media_type.GetUINT32(key) }.ok()
}

pub(super) fn attributes(capacity: u32) -> Result<IMFAttributes> {
    let mut attributes: Option<IMFAttributes> = None;
    unsafe { MFCreateAttributes(&mut attributes, capacity) }?;
    attributes.ok_or_else(|| Error::from(E_POINTER))
}

pub(super) fn duplicate(media_type: &IMFMediaType) -> Result<IMFMediaType> {
    let copy = unsafe { MFCreateMediaType() }?;
    unsafe { media_type.CopyAllItems(&copy) }?;
    Ok(copy)
}

pub(super) fn uncompressed_video(subtype: &GUID, size: [u32; 2]) -> Result<IMFMediaType> {
    let media_type = typed(&MFMediaType_Video, subtype)?;
    unsafe {
        media_type.SetUINT64(&MF_MT_FRAME_SIZE, pack(size))?;
        media_type.SetUINT64(&MF_MT_PIXEL_ASPECT_RATIO, pack([1, 1]))?;
        media_type.SetUINT32(&MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive.0 as u32)?;
    }
    Ok(media_type)
}

pub(super) fn h264(size: [u32; 2], frame_rate: [u32; 2], bitrate: u32, rotation_degrees: u32) -> Result<IMFMediaType> {
    let media_type = typed(&MFMediaType_Video, &MFVideoFormat_H264)?;
    unsafe {
        media_type.SetUINT64(&MF_MT_FRAME_SIZE, pack(size))?;
        media_type.SetUINT64(&MF_MT_FRAME_RATE, pack(frame_rate))?;
        media_type.SetUINT64(&MF_MT_PIXEL_ASPECT_RATIO, pack([1, 1]))?;
        media_type.SetUINT32(&MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive.0 as u32)?;
        media_type.SetUINT32(&MF_MT_AVG_BITRATE, bitrate)?;
        media_type.SetUINT32(&MF_MT_MPEG2_PROFILE, eAVEncH264VProfile_High.0 as u32)?;
        if rotation_degrees != 0 {
            media_type.SetUINT32(&MF_MT_VIDEO_ROTATION, rotation_degrees)?;
        }
    }
    Ok(media_type)
}

pub(super) fn pcm(shape: AudioShape) -> Result<IMFMediaType> {
    let media_type = typed(&MFMediaType_Audio, &MFAudioFormat_PCM)?;
    let block = shape.channels * PCM_BITS / 8;
    unsafe {
        media_type.SetUINT32(&MF_MT_AUDIO_BITS_PER_SAMPLE, PCM_BITS)?;
        media_type.SetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND, shape.rate)?;
        media_type.SetUINT32(&MF_MT_AUDIO_NUM_CHANNELS, shape.channels)?;
        media_type.SetUINT32(&MF_MT_AUDIO_BLOCK_ALIGNMENT, block)?;
        media_type.SetUINT32(&MF_MT_AUDIO_AVG_BYTES_PER_SECOND, block * shape.rate)?;
        media_type.SetUINT32(&MF_MT_ALL_SAMPLES_INDEPENDENT, 1)?;
    }
    Ok(media_type)
}

pub(super) fn aac(shape: AudioShape, bytes_per_second: u32) -> Result<IMFMediaType> {
    let media_type = typed(&MFMediaType_Audio, &MFAudioFormat_AAC)?;
    unsafe {
        media_type.SetUINT32(&MF_MT_AUDIO_BITS_PER_SAMPLE, PCM_BITS)?;
        media_type.SetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND, shape.rate)?;
        media_type.SetUINT32(&MF_MT_AUDIO_NUM_CHANNELS, shape.channels)?;
        media_type.SetUINT32(&MF_MT_AUDIO_AVG_BYTES_PER_SECOND, bytes_per_second)?;
        media_type.SetUINT32(&MF_MT_AAC_PAYLOAD_TYPE, 0)?;
        media_type.SetUINT32(&MF_MT_AAC_AUDIO_PROFILE_LEVEL_INDICATION, AAC_LC_PROFILE_LEVEL)?;
    }
    Ok(media_type)
}

pub(super) fn mp3(shape: AudioShape, bytes_per_second: u32) -> Result<IMFMediaType> {
    let media_type = typed(&MFMediaType_Audio, &MFAudioFormat_MP3)?;
    unsafe {
        media_type.SetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND, shape.rate)?;
        media_type.SetUINT32(&MF_MT_AUDIO_NUM_CHANNELS, shape.channels)?;
        media_type.SetUINT32(&MF_MT_AUDIO_AVG_BYTES_PER_SECOND, bytes_per_second)?;
        media_type.SetUINT32(&MF_MT_AUDIO_BLOCK_ALIGNMENT, 1)?;
    }
    Ok(media_type)
}

fn typed(major: &GUID, subtype: &GUID) -> Result<IMFMediaType> {
    let media_type = unsafe { MFCreateMediaType() }?;
    unsafe {
        media_type.SetGUID(&MF_MT_MAJOR_TYPE, major)?;
        media_type.SetGUID(&MF_MT_SUBTYPE, subtype)?;
    }
    Ok(media_type)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn size_pairs_round_trip() {
        assert_eq!(pack([1920, 1080]), (1920 << 32) | 1080);
        assert_eq!(unpack(pack([30000, 1001])), [30000, 1001]);
    }
}
