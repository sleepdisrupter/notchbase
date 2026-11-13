use std::path::{Path, PathBuf};

use ::windows::Win32::Media::MediaFoundation::{
    IMFSample, IMFSinkWriter, MF_MT_DEFAULT_STRIDE, MF_MT_FRAME_RATE, MF_SINK_WRITER_DISABLE_THROTTLING,
    MF_TRANSCODE_CONTAINERTYPE, MFCreateMemoryBuffer, MFCreateSample, MFCreateSinkWriterFromURL,
    MFTranscodeContainerType_MPEG4, MFVideoFormat_RGB32,
};
use ::windows::core::{HSTRING, Result};

use super::super::media_type::{AudioShape, aac, attributes, h264, pack, pcm, uncompressed_video};
use super::super::reader::TICKS_PER_SECOND;
use super::super::runtime::Runtime;

pub(super) const RED: [u8; 4] = [0, 0, 255, 255];
pub(super) const BLUE: [u8; 4] = [255, 0, 0, 255];
const TONE: AudioShape = AudioShape { rate: 48_000, channels: 2 };
const TONE_HZ: f64 = 440.0;
const BAR_WIDTH: u32 = 16;
const GRAIN_FLOOR: u8 = 100;
const GRAIN_SPREAD: u8 = 8;
const GRAIN_DRIFT: usize = 1031;

#[derive(Clone, Copy)]
pub(super) struct Clip {
    pub(super) size: [u32; 2],
    pub(super) frames: u32,
    pub(super) fps: u32,
    pub(super) audio: bool,
    pub(super) rotation: u32,
    pub(super) bitrate: u32,
    pub(super) grain: bool,
}

impl Clip {
    pub(super) const SMALL: Self =
        Self { size: [320, 240], frames: 60, fps: 30, audio: true, rotation: 0, bitrate: 1_500_000, grain: false };

    pub(super) fn save(self, path: PathBuf) -> PathBuf {
        let _runtime = Runtime::start(&path).unwrap();
        write(&path, self).unwrap();
        path
    }
}

fn write(path: &Path, clip: Clip) -> Result<()> {
    let settings = attributes(2)?;
    unsafe {
        settings.SetGUID(&MF_TRANSCODE_CONTAINERTYPE, &MFTranscodeContainerType_MPEG4)?;
        settings.SetUINT32(&MF_SINK_WRITER_DISABLE_THROTTLING, 1)?;
    }
    let writer = unsafe { MFCreateSinkWriterFromURL(&HSTRING::from(path), None, &settings) }?;
    let video = add_video(&writer, clip)?;
    let audio = if clip.audio { Some(add_audio(&writer)?) } else { None };
    unsafe { writer.BeginWriting() }?;
    let frame_ticks = TICKS_PER_SECOND as i64 / i64::from(clip.fps);
    let texture = if clip.grain { grain(clip.size) } else { Vec::new() };
    for index in 0..clip.frames {
        let time = i64::from(index) * frame_ticks;
        let pixels = if clip.grain { shifted(&texture, index) } else { halves(clip.size, index) };
        unsafe { writer.WriteSample(video, &sample(&pixels, time, frame_ticks)?) }?;
        if let Some(audio) = audio {
            let samples = TONE.rate / clip.fps;
            unsafe { writer.WriteSample(audio, &sample(&tone(index * samples, samples), time, frame_ticks)?) }?;
        }
    }
    unsafe { writer.Finalize() }
}

fn add_video(writer: &IMFSinkWriter, clip: Clip) -> Result<u32> {
    let index = unsafe { writer.AddStream(&h264(clip.size, [clip.fps, 1], clip.bitrate, clip.rotation)?) }?;
    let input = uncompressed_video(&MFVideoFormat_RGB32, clip.size)?;
    unsafe {
        input.SetUINT64(&MF_MT_FRAME_RATE, pack([clip.fps, 1]))?;
        input.SetUINT32(&MF_MT_DEFAULT_STRIDE, clip.size[0] * 4)?;
        writer.SetInputMediaType(index, &input, None)?;
    }
    Ok(index)
}

fn add_audio(writer: &IMFSinkWriter) -> Result<u32> {
    let index = unsafe { writer.AddStream(&aac(TONE, 16_000)?) }?;
    unsafe { writer.SetInputMediaType(index, &pcm(TONE)?, None) }?;
    Ok(index)
}

fn sample(bytes: &[u8], time: i64, duration: i64) -> Result<IMFSample> {
    let buffer = unsafe { MFCreateMemoryBuffer(bytes.len() as u32) }?;
    let mut data = std::ptr::null_mut();
    unsafe {
        buffer.Lock(&mut data, None, None)?;
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), data, bytes.len());
        buffer.Unlock()?;
        buffer.SetCurrentLength(bytes.len() as u32)?;
    }
    let sample = unsafe { MFCreateSample() }?;
    unsafe {
        sample.AddBuffer(&buffer)?;
        sample.SetSampleTime(time)?;
        sample.SetSampleDuration(duration)?;
    }
    Ok(sample)
}

fn halves([width, height]: [u32; 2], index: u32) -> Vec<u8> {
    let bar = (index * 4) % width.saturating_sub(BAR_WIDTH).max(1);
    let mut pixels = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
        let color = if y < height / 2 { RED } else { BLUE };
        for x in 0..width {
            let lit = (bar..bar + BAR_WIDTH).contains(&x);
            pixels.extend_from_slice(if lit { &[255, 255, 255, 255] } else { &color });
        }
    }
    pixels
}

fn grain([width, height]: [u32; 2]) -> Vec<u8> {
    let mut state = 0x2545_F491_u32;
    (0..width * height * 4)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            GRAIN_FLOOR + (state >> 24) as u8 / GRAIN_SPREAD
        })
        .collect()
}

fn shifted(texture: &[u8], index: u32) -> Vec<u8> {
    let mut pixels = texture.to_vec();
    let len = pixels.len().max(1);
    pixels.rotate_left(index as usize * 4 * GRAIN_DRIFT % len);
    pixels
}

fn tone(first: u32, count: u32) -> Vec<u8> {
    let mut bytes = Vec::with_capacity((count * TONE.channels * 2) as usize);
    for n in first..first + count {
        let phase = std::f64::consts::TAU * TONE_HZ * f64::from(n) / f64::from(TONE.rate);
        let level = ((phase.sin() * 8000.0) as i16).to_le_bytes();
        for _ in 0..TONE.channels {
            bytes.extend_from_slice(&level);
        }
    }
    bytes
}

pub(super) fn save_wav(path: PathBuf, rate: u32, seconds: u32) -> PathBuf {
    let samples: Vec<u8> =
        (0..rate * seconds).map(|n| ((f64::from(n) * 0.05).sin() * 6000.0) as i16).flat_map(i16::to_le_bytes).collect();
    let mut wav = Vec::with_capacity(44 + samples.len());
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36 + samples.len() as u32).to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    wav.extend_from_slice(&16u32.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&rate.to_le_bytes());
    wav.extend_from_slice(&(rate * 2).to_le_bytes());
    wav.extend_from_slice(&2u16.to_le_bytes());
    wav.extend_from_slice(&16u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&(samples.len() as u32).to_le_bytes());
    wav.extend_from_slice(&samples);
    std::fs::write(&path, wav).unwrap();
    path
}
