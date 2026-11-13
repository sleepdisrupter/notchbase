//! The source reader: opens a file, picks its first video and audio streams, and decodes them to the types asked for.

use std::path::Path;

use ::windows::Win32::Foundation::E_FAIL;
use ::windows::Win32::Media::MediaFoundation::{
    IMFMediaType, IMFSample, IMFSourceReader, IMFSourceReaderEx, IMFTransform, IMFVideoProcessorControl,
    MF_E_END_OF_STREAM, MF_PD_DURATION, MF_SOURCE_READER_ALL_STREAMS,
    MF_SOURCE_READER_ENABLE_ADVANCED_VIDEO_PROCESSING, MF_SOURCE_READER_FIRST_AUDIO_STREAM,
    MF_SOURCE_READER_FIRST_VIDEO_STREAM, MF_SOURCE_READER_MEDIASOURCE, MF_SOURCE_READERF_ENDOFSTREAM,
    MF_SOURCE_READERF_ERROR, MFCreateSourceReaderFromURL, ROTATION_NONE,
};
use ::windows::Win32::System::Com::StructuredStorage::PropVariantClear;
use ::windows::Win32::System::Variant::VT_UI8;
use ::windows::core::{Error, HSTRING, Interface, Result};

use super::media_type::attributes;

pub(super) const TICKS_PER_SECOND: f64 = 10_000_000.0;
const MAX_TRANSFORMS: u32 = 16;
const MAX_EMPTY_READS: u32 = 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Stream {
    Video,
    Audio,
}

impl Stream {
    fn index(self) -> u32 {
        match self {
            Self::Video => MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32,
            Self::Audio => MF_SOURCE_READER_FIRST_AUDIO_STREAM.0 as u32,
        }
    }
}

pub(super) struct Decoded {
    pub(super) sample: IMFSample,
    pub(super) time: i64,
}

impl Decoded {
    pub(super) fn duration_seconds(&self) -> Option<f64> {
        let ticks = unsafe { self.sample.GetSampleDuration() }.ok()?;
        (ticks > 0).then(|| ticks as f64 / TICKS_PER_SECOND)
    }
}

pub(super) struct Reader {
    reader: IMFSourceReader,
}

impl Reader {
    pub(super) fn open(path: &Path) -> Result<Self> {
        let attributes = attributes(1)?;
        unsafe { attributes.SetUINT32(&MF_SOURCE_READER_ENABLE_ADVANCED_VIDEO_PROCESSING, 1) }?;
        let reader = unsafe { MFCreateSourceReaderFromURL(&HSTRING::from(path), &attributes) }?;
        Ok(Self { reader })
    }

    pub(super) fn duration_seconds(&self) -> Option<f64> {
        let source = MF_SOURCE_READER_MEDIASOURCE.0 as u32;
        let mut value = unsafe { self.reader.GetPresentationAttribute(source, &MF_PD_DURATION) }.ok()?;
        let ticks = unsafe {
            let fields = &value.Anonymous.Anonymous;
            (fields.vt == VT_UI8).then_some(fields.Anonymous.uhVal)
        };
        let _ = unsafe { PropVariantClear(&mut value) };
        ticks.filter(|ticks| *ticks > 0).map(|ticks| ticks as f64 / TICKS_PER_SECOND)
    }

    pub(super) fn native(&self, stream: Stream) -> Option<IMFMediaType> {
        unsafe { self.reader.GetNativeMediaType(stream.index(), 0) }.ok()
    }

    pub(super) fn keep_only(&self, streams: &[Stream]) -> Result<()> {
        unsafe { self.reader.SetStreamSelection(MF_SOURCE_READER_ALL_STREAMS.0 as u32, false) }?;
        for stream in streams {
            unsafe { self.reader.SetStreamSelection(stream.index(), true) }?;
        }
        Ok(())
    }

    pub(super) fn set_output(&self, stream: Stream, media_type: &IMFMediaType) -> Result<()> {
        unsafe { self.reader.SetCurrentMediaType(stream.index(), None, media_type) }?;
        match stream {
            Stream::Video => self.keep_stored_orientation(),
            Stream::Audio => Ok(()),
        }
    }

    fn keep_stored_orientation(&self) -> Result<()> {
        let Ok(extended) = self.reader.cast::<IMFSourceReaderEx>() else { return Ok(()) };
        for index in 0..MAX_TRANSFORMS {
            let mut transform: Option<IMFTransform> = None;
            let found = unsafe { extended.GetTransformForStream(Stream::Video.index(), index, None, &mut transform) };
            if found.is_err() {
                break;
            }
            if let Some(processor) = transform.and_then(|transform| transform.cast::<IMFVideoProcessorControl>().ok()) {
                unsafe { processor.SetRotation(ROTATION_NONE) }?;
            }
        }
        Ok(())
    }

    pub(super) fn output(&self, stream: Stream) -> Result<IMFMediaType> {
        unsafe { self.reader.GetCurrentMediaType(stream.index()) }
    }

    pub(super) fn next(&self, stream: Stream) -> Result<Option<Decoded>> {
        for _ in 0..MAX_EMPTY_READS {
            let mut flags = 0u32;
            let mut time = 0i64;
            let mut sample: Option<IMFSample> = None;
            let read = unsafe {
                self.reader.ReadSample(
                    stream.index(),
                    0,
                    None,
                    Some(&raw mut flags),
                    Some(&raw mut time),
                    Some(&raw mut sample),
                )
            };
            match read {
                Err(error) if error.code() == MF_E_END_OF_STREAM => return Ok(None),
                Err(error) => return Err(error),
                Ok(()) if flags & MF_SOURCE_READERF_ERROR.0 as u32 != 0 => return Err(Error::from(E_FAIL)),
                Ok(()) => {}
            }
            if let Some(sample) = sample {
                return Ok(Some(Decoded { sample, time }));
            }
            if flags & MF_SOURCE_READERF_ENDOFSTREAM.0 as u32 != 0 {
                return Ok(None);
            }
        }
        Err(Error::from(E_FAIL))
    }
}
