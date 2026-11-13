//! Plans the output tracks for a transcode: the input and output type for each stream, and its bitrate.

use std::path::Path;

use ::windows::Win32::Media::MediaFoundation::{
    IMFMediaType, MF_MT_FRAME_RATE, MF_MT_FRAME_SIZE, MF_MT_INTERLACE_MODE, MF_MT_PIXEL_ASPECT_RATIO,
    MFVideoFormat_NV12, MFVideoInterlace_Progressive,
};
use ::windows::core::Error;

use super::diagnostics::{Diagnose, complain};
use super::media_type::{AudioShape, aac, duplicate, h264, mp3, pack, pair, pcm, uncompressed_video};
use super::reader::{Reader, Stream};
use super::settings::{
    MP3_BYTES_PER_SECOND, audio_bytes_per_second, encodable_audio, frame_rate, frame_size, mp3_audio, video_bitrate,
};
use super::tracks::{VideoTrack, audio_shape};
use crate::shelf::video::{Container, MediaError, Quality};

pub(super) struct Track {
    pub(super) stream: Stream,
    pub(super) input: IMFMediaType,
    pub(super) output: IMFMediaType,
    pub(super) bitrate: Option<u32>,
}

pub(super) fn tracks(
    reader: &Reader,
    container: Container,
    quality: Quality,
    source: &Path,
) -> Result<Vec<Track>, MediaError> {
    let video = if container.is_audio_only() {
        None
    } else {
        reader.native(Stream::Video).and_then(|native| VideoTrack::of(&native))
    };
    let audio = reader.native(Stream::Audio).and_then(|native| audio_shape(&native));
    if container.is_audio_only() && audio.is_none() {
        return Err(MediaError::NoAudio);
    }
    let streams: Vec<Stream> =
        [video.map(|_| Stream::Video), audio.map(|_| Stream::Audio)].into_iter().flatten().collect();
    if streams.is_empty() {
        return Err(complain(MediaError::Unsupported, "find a video or audio stream", source, "none present"));
    }
    reader.keep_only(&streams).failed("select streams", source)?;
    let mut tracks = Vec::with_capacity(streams.len());
    if let Some(video) = video {
        tracks.push(video_track(reader, &video, quality).unsupported("decode the video stream", source)?);
    }
    if let Some(audio) = audio {
        tracks.push(audio_track(reader, audio, container, quality).unsupported("decode the audio stream", source)?);
    }
    Ok(tracks)
}

fn video_track(reader: &Reader, video: &VideoTrack, quality: Quality) -> Result<Track, Error> {
    let size = frame_size(quality, video.visible);
    let decoded = uncompressed_video(&MFVideoFormat_NV12, size)?;
    if let Some(rate) = video.frame_rate {
        unsafe { decoded.SetUINT64(&MF_MT_FRAME_RATE, pack(rate)) }?;
    }
    reader.set_output(Stream::Video, &decoded)?;
    let input = duplicate(&reader.output(Stream::Video)?)?;
    let size = pair(&input, &MF_MT_FRAME_SIZE).unwrap_or(size);
    let rate = frame_rate(pair(&input, &MF_MT_FRAME_RATE).or(video.frame_rate));
    unsafe {
        input.SetUINT64(&MF_MT_FRAME_RATE, pack(rate))?;
        input.SetUINT64(&MF_MT_PIXEL_ASPECT_RATIO, pack([1, 1]))?;
        input.SetUINT32(&MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive.0 as u32)?;
    }
    let bitrate = video_bitrate(quality, size, rate);
    let output = h264(size, rate, bitrate, video.turn.rotation_degrees())?;
    Ok(Track { stream: Stream::Video, input, output, bitrate: Some(bitrate) })
}

fn audio_track(reader: &Reader, audio: AudioShape, container: Container, quality: Quality) -> Result<Track, Error> {
    let shape = match container {
        Container::Mp3 => mp3_audio(audio),
        Container::Mp4 | Container::Mov | Container::M4a => encodable_audio(audio),
    };
    reader.set_output(Stream::Audio, &pcm(shape)?)?;
    let input = duplicate(&reader.output(Stream::Audio)?)?;
    let shape = audio_shape(&input).unwrap_or(shape);
    let output = match container {
        Container::Mp3 => mp3(shape, MP3_BYTES_PER_SECOND)?,
        Container::Mp4 | Container::Mov | Container::M4a => aac(shape, audio_bytes_per_second(quality))?,
    };
    Ok(Track { stream: Stream::Audio, input, output, bitrate: None })
}
