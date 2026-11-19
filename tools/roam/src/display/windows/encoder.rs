//! H.264 from NV12 with a Media Foundation encoder: the graphics card's own when it has one
//! (those run asynchronously and are driven by their events), Windows' software encoder
//! otherwise. Tuned for a live picture: low latency, no B pictures, and a key picture only when
//! one is asked for.

use std::time::{Duration, Instant};

use windows::Win32::Media::MediaFoundation::{
    CLSID_MSH264EncoderMFT, CODECAPI_AVEncCommonMeanBitRate, CODECAPI_AVEncMPVDefaultBPictureCount,
    CODECAPI_AVEncMPVGOPSize, CODECAPI_AVEncVideoForceKeyFrame, CODECAPI_AVLowLatencyMode, ICodecAPI, IMFActivate,
    IMFMediaEventGenerator, IMFMediaType, IMFSample, IMFTransform, METransformHaveOutput, METransformNeedInput,
    MF_E_NO_EVENTS_AVAILABLE, MF_E_TRANSFORM_NEED_MORE_INPUT, MF_E_TRANSFORM_STREAM_CHANGE, MF_EVENT_FLAG_NO_WAIT,
    MF_LOW_LATENCY, MF_MT_AVG_BITRATE, MF_MT_FRAME_RATE, MF_MT_FRAME_SIZE, MF_MT_INTERLACE_MODE, MF_MT_MAJOR_TYPE,
    MF_MT_MPEG_SEQUENCE_HEADER, MF_MT_MPEG2_PROFILE, MF_MT_PIXEL_ASPECT_RATIO, MF_MT_SUBTYPE, MF_TRANSFORM_ASYNC,
    MF_TRANSFORM_ASYNC_UNLOCK, MFCreateMediaType, MFCreateMemoryBuffer, MFCreateSample, MFMediaType_Video,
    MFT_CATEGORY_VIDEO_ENCODER, MFT_ENUM_FLAG_HARDWARE, MFT_ENUM_FLAG_SORTANDFILTER, MFT_MESSAGE_COMMAND_FLUSH,
    MFT_MESSAGE_NOTIFY_BEGIN_STREAMING, MFT_MESSAGE_NOTIFY_START_OF_STREAM, MFT_OUTPUT_DATA_BUFFER,
    MFT_OUTPUT_STREAM_PROVIDES_SAMPLES, MFT_REGISTER_TYPE_INFO, MFTEnumEx, MFVideoFormat_H264, MFVideoFormat_NV12,
    MFVideoInterlace_Progressive, eAVEncH264VProfile_Main,
};
use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance, CoTaskMemFree};
use windows::Win32::System::Variant::{VARIANT, VT_UI4};
use windows::core::Interface;

use super::media::{self, pack};
use crate::display::h264;

/// How long an asynchronous encoder may take to hand a picture back before the next goes in.
const OUTPUT_WAIT: Duration = Duration::from_millis(25);

pub struct Encoder {
    transform: IMFTransform,
    codec: Option<ICodecAPI>,
    /// Set for an asynchronous (hardware) encoder.
    events: Option<IMFMediaEventGenerator>,
    /// Inputs an asynchronous encoder has asked for and not been given.
    wanted: u32,
    provides: bool,
    output_size: u32,
    fps: u32,
    /// The parameter sets, put before key pictures that come without them.
    header: Vec<u8>,
    time: i64,
}

impl Encoder {
    pub fn new(size: [u32; 2], fps: u32) -> Result<Self, String> {
        media::start()?;
        let bitrate = bitrate(size, fps);
        let mut why = String::new();
        for activate in hardware_encoders() {
            // SAFETY: plain COM activation; a failure tries the next one.
            let Ok(transform) = (unsafe { activate.ActivateObject::<IMFTransform>() }) else { continue };
            match Self::set_up(transform, size, fps, bitrate) {
                Ok(encoder) => return Ok(encoder),
                Err(err) => {
                    why = err;
                    let _ = unsafe { activate.ShutdownObject() };
                }
            }
        }
        // SAFETY: Windows' own software encoder.
        let transform: IMFTransform = unsafe { CoCreateInstance(&CLSID_MSH264EncoderMFT, None, CLSCTX_INPROC_SERVER) }
            .map_err(|err| format!("No H.264 encoder: {} {why}", err.message()))?;
        Self::set_up(transform, size, fps, bitrate)
    }

    fn set_up(transform: IMFTransform, size: [u32; 2], fps: u32, bitrate: u32) -> Result<Self, String> {
        let err = |err: windows::core::Error| err.message();
        unsafe {
            let attributes = transform.GetAttributes().ok();
            let asynchronous = attributes.as_ref().is_some_and(|a| a.GetUINT32(&MF_TRANSFORM_ASYNC).unwrap_or(0) != 0);
            if let Some(attributes) = &attributes {
                if asynchronous {
                    attributes.SetUINT32(&MF_TRANSFORM_ASYNC_UNLOCK, 1).map_err(err)?;
                }
                let _ = attributes.SetUINT32(&MF_LOW_LATENCY, 1);
            }
            let codec = transform.cast::<ICodecAPI>().ok();
            if let Some(codec) = &codec {
                let _ = codec.SetValue(&CODECAPI_AVLowLatencyMode, &uint(1));
                let _ = codec.SetValue(&CODECAPI_AVEncMPVDefaultBPictureCount, &uint(0));
                let _ = codec.SetValue(&CODECAPI_AVEncCommonMeanBitRate, &uint(bitrate));
                // A key picture now and then, so a lost one is never lost for long.
                let _ = codec.SetValue(&CODECAPI_AVEncMPVGOPSize, &uint(fps * 4));
            }
            let output: IMFMediaType = MFCreateMediaType().map_err(err)?;
            output.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video).map_err(err)?;
            output.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_H264).map_err(err)?;
            output.SetUINT32(&MF_MT_AVG_BITRATE, bitrate).map_err(err)?;
            output.SetUINT64(&MF_MT_FRAME_SIZE, pack(size[0], size[1])).map_err(err)?;
            output.SetUINT64(&MF_MT_FRAME_RATE, pack(fps, 1)).map_err(err)?;
            output.SetUINT64(&MF_MT_PIXEL_ASPECT_RATIO, pack(1, 1)).map_err(err)?;
            output.SetUINT32(&MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive.0 as u32).map_err(err)?;
            output.SetUINT32(&MF_MT_MPEG2_PROFILE, eAVEncH264VProfile_Main.0 as u32).map_err(err)?;
            transform.SetOutputType(0, &output, 0).map_err(err)?;
            let input: IMFMediaType = MFCreateMediaType().map_err(err)?;
            input.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video).map_err(err)?;
            input.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_NV12).map_err(err)?;
            input.SetUINT64(&MF_MT_FRAME_SIZE, pack(size[0], size[1])).map_err(err)?;
            input.SetUINT64(&MF_MT_FRAME_RATE, pack(fps, 1)).map_err(err)?;
            input.SetUINT64(&MF_MT_PIXEL_ASPECT_RATIO, pack(1, 1)).map_err(err)?;
            input.SetUINT32(&MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive.0 as u32).map_err(err)?;
            transform.SetInputType(0, &input, 0).map_err(err)?;
            let info = transform.GetOutputStreamInfo(0).map_err(err)?;
            let header = sequence_header(&transform);
            let events =
                if asynchronous { Some(transform.cast::<IMFMediaEventGenerator>().map_err(err)?) } else { None };
            transform.ProcessMessage(MFT_MESSAGE_COMMAND_FLUSH, 0).map_err(err)?;
            transform.ProcessMessage(MFT_MESSAGE_NOTIFY_BEGIN_STREAMING, 0).map_err(err)?;
            transform.ProcessMessage(MFT_MESSAGE_NOTIFY_START_OF_STREAM, 0).map_err(err)?;
            Ok(Self {
                transform,
                codec,
                events,
                wanted: 0,
                provides: info.dwFlags & MFT_OUTPUT_STREAM_PROVIDES_SAMPLES.0 as u32 != 0,
                output_size: info.cbSize.max(size[0] * size[1]),
                fps,
                header,
                time: 0,
            })
        }
    }

    /// Encodes one picture, a key picture when `key`. Answers the pictures that came out, in
    /// Annex B: usually this one, sometimes the one before as well, or none yet.
    pub fn encode(&mut self, nv12: &[u8], key: bool) -> Result<Vec<Vec<u8>>, String> {
        let mut out = Vec::new();
        if key && let Some(codec) = &self.codec {
            // SAFETY: a value the encoder documents.
            let _ = unsafe { codec.SetValue(&CODECAPI_AVEncVideoForceKeyFrame, &uint(1)) };
        }
        let sample = self.sample(nv12)?;
        if self.events.is_some() {
            // Wait for the encoder to ask for input, taking what it hands back meanwhile.
            let deadline = Instant::now() + Duration::from_millis(250);
            while self.wanted == 0 {
                if !self.poll(&mut out)? {
                    if Instant::now() >= deadline {
                        return Err("The encoder stopped asking for pictures".into());
                    }
                    std::thread::sleep(Duration::from_millis(1));
                }
            }
            self.wanted -= 1;
            // SAFETY: the encoder asked for this input.
            unsafe { self.transform.ProcessInput(0, &sample, 0) }.map_err(|err| err.message())?;
            let deadline = Instant::now() + OUTPUT_WAIT;
            let before = out.len();
            while out.len() == before && Instant::now() < deadline {
                if !self.poll(&mut out)? {
                    std::thread::sleep(Duration::from_millis(1));
                }
            }
        } else {
            // SAFETY: a synchronous encoder takes input whenever its output has been drained.
            unsafe { self.transform.ProcessInput(0, &sample, 0) }.map_err(|err| err.message())?;
            while let Some(picture) = self.take_output()? {
                out.push(picture);
            }
        }
        Ok(out.into_iter().map(|p| self.with_header(p)).collect())
    }

    /// Handles one waiting event of an asynchronous encoder. Answers whether there was one.
    fn poll(&mut self, out: &mut Vec<Vec<u8>>) -> Result<bool, String> {
        let Some(events) = &self.events else { return Ok(false) };
        // SAFETY: a plain query that doesn't block.
        let event = match unsafe { events.GetEvent(MF_EVENT_FLAG_NO_WAIT) } {
            Ok(event) => event,
            Err(err) if err.code() == MF_E_NO_EVENTS_AVAILABLE => return Ok(false),
            Err(err) => return Err(err.message()),
        };
        // SAFETY: a plain query.
        let kind = unsafe { event.GetType() }.map_err(|err| err.message())?;
        if kind == METransformNeedInput.0 as u32 {
            self.wanted += 1;
        } else if kind == METransformHaveOutput.0 as u32
            && let Some(picture) = self.take_output()?
        {
            out.push(picture);
        }
        Ok(true)
    }

    fn sample(&mut self, nv12: &[u8]) -> Result<IMFSample, String> {
        let duration = 10_000_000 / i64::from(self.fps.max(1));
        let sample = media::sample(nv12)?;
        // SAFETY: plain setters on a sample we own.
        unsafe {
            let _ = sample.SetSampleTime(self.time);
            let _ = sample.SetSampleDuration(duration);
        }
        self.time += duration;
        Ok(sample)
    }

    /// One picture from the encoder, if it has one ready.
    fn take_output(&mut self) -> Result<Option<Vec<u8>>, String> {
        let sample = if self.provides {
            None
        } else {
            // SAFETY: an empty sample with a buffer the size the encoder said it needs.
            unsafe {
                let sample = MFCreateSample().map_err(|err| err.message())?;
                let buffer = MFCreateMemoryBuffer(self.output_size).map_err(|err| err.message())?;
                sample.AddBuffer(&buffer).map_err(|err| err.message())?;
                Some(sample)
            }
        };
        let mut buffers = [MFT_OUTPUT_DATA_BUFFER {
            dwStreamID: 0,
            pSample: std::mem::ManuallyDrop::new(sample),
            dwStatus: 0,
            pEvents: std::mem::ManuallyDrop::new(None),
        }];
        let mut status = 0;
        // SAFETY: one output buffer, as the encoder has one output stream.
        let result = unsafe { self.transform.ProcessOutput(0, &mut buffers, &mut status) };
        let [buffer] = buffers;
        let sample = std::mem::ManuallyDrop::into_inner(buffer.pSample);
        drop(std::mem::ManuallyDrop::into_inner(buffer.pEvents));
        match result {
            Ok(()) => {}
            Err(err) if err.code() == MF_E_TRANSFORM_NEED_MORE_INPUT => return Ok(None),
            Err(err) if err.code() == MF_E_TRANSFORM_STREAM_CHANGE => {
                // SAFETY: takes the encoder's own choice of output type, as it asks.
                unsafe {
                    if let Ok(kind) = self.transform.GetOutputAvailableType(0, 0) {
                        let _ = self.transform.SetOutputType(0, &kind, 0);
                    }
                }
                self.header = sequence_header(&self.transform);
                return Ok(None);
            }
            Err(err) => return Err(err.message()),
        }
        let Some(sample) = sample else { return Ok(None) };
        Ok(Some(media::bytes(&sample)?))
    }

    fn with_header(&mut self, picture: Vec<u8>) -> Vec<u8> {
        if let Some((sps, pps)) = h264::parameter_sets(&picture) {
            self.header = [&[0, 0, 0, 1][..], sps, &[0, 0, 0, 1], pps].concat();
            return picture;
        }
        if h264::is_key(&picture) && !self.header.is_empty() {
            return [self.header.as_slice(), &picture].concat();
        }
        picture
    }
}

impl Drop for Encoder {
    fn drop(&mut self) {
        // SAFETY: tells the encoder the stream is over before it goes.
        unsafe {
            let _ = self.transform.ProcessMessage(MFT_MESSAGE_COMMAND_FLUSH, 0);
        }
    }
}

/// Enough for text to stay sharp: about 0.12 bits a pixel at 60 a second, from 8 to 40 Mbit/s.
fn bitrate([width, height]: [u32; 2], fps: u32) -> u32 {
    let wanted = f64::from(width) * f64::from(height) * f64::from(fps) * 0.12;
    wanted.clamp(8_000_000.0, 40_000_000.0) as u32
}

fn uint(value: u32) -> VARIANT {
    let mut variant = VARIANT::default();
    // SAFETY: a VT_UI4 variant keeps its value in `ulVal`.
    unsafe {
        let inner = &mut *variant.Anonymous.Anonymous;
        inner.vt = VT_UI4;
        inner.Anonymous.ulVal = value;
    }
    variant
}

/// The parameter sets the encoder gives on its output type, once it has them.
fn sequence_header(transform: &IMFTransform) -> Vec<u8> {
    // SAFETY: plain queries on the current output type.
    unsafe {
        let Ok(kind) = transform.GetOutputCurrentType(0) else { return Vec::new() };
        let Ok(size) = kind.GetBlobSize(&MF_MT_MPEG_SEQUENCE_HEADER) else { return Vec::new() };
        let mut blob = vec![0u8; size as usize];
        if kind.GetBlob(&MF_MT_MPEG_SEQUENCE_HEADER, &mut blob, None).is_err() {
            return Vec::new();
        }
        blob
    }
}

/// The graphics card's H.264 encoders, best first.
fn hardware_encoders() -> Vec<IMFActivate> {
    let input = MFT_REGISTER_TYPE_INFO { guidMajorType: MFMediaType_Video, guidSubtype: MFVideoFormat_NV12 };
    let output = MFT_REGISTER_TYPE_INFO { guidMajorType: MFMediaType_Video, guidSubtype: MFVideoFormat_H264 };
    let mut list: *mut Option<IMFActivate> = std::ptr::null_mut();
    let mut count = 0u32;
    // SAFETY: Media Foundation fills the list, which is freed below.
    let found = unsafe {
        MFTEnumEx(
            MFT_CATEGORY_VIDEO_ENCODER,
            MFT_ENUM_FLAG_HARDWARE | MFT_ENUM_FLAG_SORTANDFILTER,
            Some(&input),
            Some(&output),
            &mut list,
            &mut count,
        )
    };
    if found.is_err() || list.is_null() {
        return Vec::new();
    }
    // SAFETY: `count` entries, each owned by us now.
    unsafe {
        let entries = std::slice::from_raw_parts_mut(list, count as usize);
        let taken: Vec<IMFActivate> = entries.iter_mut().filter_map(Option::take).collect();
        CoTaskMemFree(Some(list.cast()));
        taken
    }
}
