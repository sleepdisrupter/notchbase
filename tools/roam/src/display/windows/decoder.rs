//! H.264 to NV12 with Windows' own decoder, set for low latency: each picture comes out as
//! soon as it goes in.

use windows::Win32::Media::MediaFoundation::{
    CLSID_MSH264DecoderMFT, IMFMediaType, IMFSample, IMFTransform, MF_E_TRANSFORM_NEED_MORE_INPUT,
    MF_E_TRANSFORM_STREAM_CHANGE, MF_LOW_LATENCY, MF_MT_DEFAULT_STRIDE, MF_MT_FRAME_SIZE, MF_MT_MAJOR_TYPE,
    MF_MT_SUBTYPE, MFCreateMediaType, MFCreateMemoryBuffer, MFCreateSample, MFMediaType_Video,
    MFT_MESSAGE_NOTIFY_BEGIN_STREAMING, MFT_MESSAGE_NOTIFY_START_OF_STREAM, MFT_OUTPUT_DATA_BUFFER,
    MFT_OUTPUT_STREAM_PROVIDES_SAMPLES, MFVideoFormat_H264, MFVideoFormat_NV12,
};
use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance};

use super::media::{self, pack, unpack};

/// A decoded picture: NV12, `stride` bytes a row, the luma plane `rows` tall.
pub struct Decoded {
    pub nv12: Vec<u8>,
    pub stride: usize,
    pub rows: usize,
}

pub struct Decoder {
    transform: IMFTransform,
    provides: bool,
    output_size: u32,
    stride: usize,
    rows: usize,
}

impl Decoder {
    pub fn new(size: [u32; 2]) -> Result<Self, String> {
        media::start()?;
        let err = |err: windows::core::Error| err.message();
        // SAFETY: Windows' own decoder, set up with plain attribute calls.
        unsafe {
            let transform: IMFTransform =
                CoCreateInstance(&CLSID_MSH264DecoderMFT, None, CLSCTX_INPROC_SERVER).map_err(err)?;
            if let Ok(attributes) = transform.GetAttributes() {
                let _ = attributes.SetUINT32(&MF_LOW_LATENCY, 1);
            }
            let input: IMFMediaType = MFCreateMediaType().map_err(err)?;
            input.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video).map_err(err)?;
            input.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_H264).map_err(err)?;
            input.SetUINT64(&MF_MT_FRAME_SIZE, pack(size[0], size[1])).map_err(err)?;
            transform.SetInputType(0, &input, 0).map_err(err)?;
            let mut decoder =
                Self { transform, provides: false, output_size: 0, stride: size[0] as usize, rows: size[1] as usize };
            decoder.choose_output()?;
            decoder.transform.ProcessMessage(MFT_MESSAGE_NOTIFY_BEGIN_STREAMING, 0).map_err(err)?;
            decoder.transform.ProcessMessage(MFT_MESSAGE_NOTIFY_START_OF_STREAM, 0).map_err(err)?;
            Ok(decoder)
        }
    }

    /// Picks NV12 among the outputs the decoder offers, and learns its layout.
    fn choose_output(&mut self) -> Result<(), String> {
        let err = |err: windows::core::Error| err.message();
        // SAFETY: plain queries and setters on the decoder.
        unsafe {
            for index in 0.. {
                let Ok(kind) = self.transform.GetOutputAvailableType(0, index) else { break };
                if kind.GetGUID(&MF_MT_SUBTYPE).ok() != Some(MFVideoFormat_NV12) {
                    continue;
                }
                self.transform.SetOutputType(0, &kind, 0).map_err(err)?;
                let [width, height] = kind.GetUINT64(&MF_MT_FRAME_SIZE).map(unpack).unwrap_or([0, 0]);
                let stride = kind.GetUINT32(&MF_MT_DEFAULT_STRIDE).map_or(width, |s| s as i32 as u32).max(width);
                self.stride = stride as usize;
                self.rows = height as usize;
                let info = self.transform.GetOutputStreamInfo(0).map_err(err)?;
                self.provides = info.dwFlags & MFT_OUTPUT_STREAM_PROVIDES_SAMPLES.0 as u32 != 0;
                self.output_size = info.cbSize.max(stride * height * 3 / 2);
                return Ok(());
            }
        }
        Err("The decoder offers no NV12 output".into())
    }

    /// Decodes one Annex B access unit. Answers the newest picture that came out, if any.
    pub fn decode(&mut self, data: &[u8]) -> Result<Option<Decoded>, String> {
        let sample = media::sample(data)?;
        // SAFETY: a synchronous decoder takes input once its output is drained.
        unsafe { self.transform.ProcessInput(0, &sample, 0) }.map_err(|err| err.message())?;
        let mut newest = None;
        loop {
            match self.take()? {
                Taken::Picture(picture) => newest = Some(picture),
                Taken::Changed => self.choose_output()?,
                Taken::Nothing => break,
            }
        }
        Ok(newest)
    }

    fn take(&mut self) -> Result<Taken, String> {
        let sample: Option<IMFSample> = if self.provides {
            None
        } else {
            // SAFETY: an empty sample with a buffer the size the decoder said it needs.
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
        // SAFETY: one output buffer, as the decoder has one output stream.
        let result = unsafe { self.transform.ProcessOutput(0, &mut buffers, &mut status) };
        let [buffer] = buffers;
        let sample = std::mem::ManuallyDrop::into_inner(buffer.pSample);
        drop(std::mem::ManuallyDrop::into_inner(buffer.pEvents));
        match result {
            Ok(()) => {}
            Err(err) if err.code() == MF_E_TRANSFORM_NEED_MORE_INPUT => return Ok(Taken::Nothing),
            Err(err) if err.code() == MF_E_TRANSFORM_STREAM_CHANGE => return Ok(Taken::Changed),
            Err(err) => return Err(err.message()),
        }
        let Some(sample) = sample else { return Ok(Taken::Nothing) };
        Ok(Taken::Picture(Decoded { nv12: media::bytes(&sample)?, stride: self.stride, rows: self.rows }))
    }
}

enum Taken {
    Picture(Decoded),
    Changed,
    Nothing,
}
