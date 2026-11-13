//! A shared-mode capture client that always hands back 16-bit stereo at 48 kHz.

use std::ptr::null_mut;

use windows::Win32::Media::Audio::{
    AUDCLNT_BUFFERFLAGS_SILENT, AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM,
    AUDCLNT_STREAMFLAGS_LOOPBACK, AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY, EDataFlow, IAudioCaptureClient,
    IAudioClient, IMMDevice, IMMDeviceEnumerator, MMDeviceEnumerator, WAVE_FORMAT_PCM, WAVEFORMATEX,
    WAVEFORMATEXTENSIBLE, eCapture, eConsole, eRender,
};
use windows::Win32::System::Com::{CLSCTX_ALL, CoCreateInstance, CoTaskMemFree};
use windows::core::HSTRING;

use crate::pcm::{CHANNELS, Encoding, FRAME_BYTES, Layout, RATE, Resampler};

// One second, in 100 ns units.
const BUFFER_TICKS: i64 = 10_000_000;
const FORMAT_FLOAT: u16 = 3;
const FORMAT_EXTENSIBLE: u16 = 0xFFFE;
const NO_MICROPHONE: &str = "No microphone is connected";
const NO_OUTPUT: &str = "There's no audio output to record";

pub struct Packet<'a> {
    pub pcm: &'a [u8],
    pub starts_at: Option<i64>,
}

pub struct Audio {
    client: IAudioClient,
    capture: IAudioCaptureClient,
    layout: Option<Layout>,
    resampler: Option<Resampler>,
    block: usize,
    origin: i64,
    raw: Vec<u8>,
    stereo: Vec<u8>,
    pcm: Vec<u8>,
    running: bool,
}

impl Audio {
    pub fn microphone(id: Option<&str>) -> Result<Self, String> {
        let device = endpoint(eCapture, id).ok_or(NO_MICROPHONE)?;
        Self::open(&device, false).map_err(|err| format!("Couldn't record from the microphone: {err}"))
    }

    pub fn desktop() -> Result<Self, String> {
        let device = endpoint(eRender, None).ok_or(NO_OUTPUT)?;
        Self::open(&device, true).map_err(|err| format!("Couldn't record desktop audio: {err}"))
    }

    pub fn start(&mut self, origin: i64) -> Result<(), String> {
        self.origin = origin;
        unsafe { self.client.Start() }.map_err(|err| err.message())?;
        self.running = true;
        Ok(())
    }

    pub fn next_packet(&mut self) -> Result<Option<Packet<'_>>, String> {
        let pending = unsafe { self.capture.GetNextPacketSize() }.map_err(|err| err.message())?;
        if pending == 0 {
            return Ok(None);
        }
        let (mut data, mut frames, mut flags, mut position) = (null_mut(), 0u32, 0u32, 0u64);
        unsafe { self.capture.GetBuffer(&mut data, &mut frames, &mut flags, None, Some(&mut position)) }
            .map_err(|err| err.message())?;
        let bytes = frames as usize * self.block;
        self.raw.clear();
        if data.is_null() || flags & AUDCLNT_BUFFERFLAGS_SILENT.0 as u32 != 0 {
            self.raw.resize(bytes, 0);
        } else {
            self.raw.extend_from_slice(unsafe { std::slice::from_raw_parts(data, bytes) });
        }
        unsafe { self.capture.ReleaseBuffer(frames) }.map_err(|err| err.message())?;
        match self.layout {
            Some(layout) => layout.to_stereo(&self.raw, &mut self.stereo),
            None => std::mem::swap(&mut self.raw, &mut self.stereo),
        }
        match &mut self.resampler {
            Some(resampler) => resampler.process(&self.stereo, &mut self.pcm),
            None => std::mem::swap(&mut self.stereo, &mut self.pcm),
        }
        // QPC time of the first frame in 100 ns units, or zero if the driver didn't say.
        let starts_at = (position != 0).then(|| position as i64 - self.origin);
        Ok(Some(Packet { pcm: &self.pcm, starts_at }))
    }

    fn open(device: &IMMDevice, loopback: bool) -> Result<Self, String> {
        let base = if loopback { AUDCLNT_STREAMFLAGS_LOOPBACK } else { 0 };
        // Ask Windows to convert to our format first. Some drivers refuse, so fall back to
        // the mix format and convert it ourselves.
        let converted = activate(device).and_then(|client| {
            let flags = base | AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM | AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY;
            initialize(&client, flags, &stereo(RATE))?;
            Ok((client, None, FRAME_BYTES, RATE))
        });
        let (client, layout, block, rate): (_, Option<Layout>, _, _) = match converted {
            Ok(opened) => opened,
            Err(_) => {
                let client = activate(device)?;
                let mix = MixFormat::of(&client)?;
                let (layout, rate) = mix.describe()?;
                initialize(&client, base, mix.format())?;
                (client, Some(layout), layout.block, rate)
            }
        };
        let capture = unsafe { client.GetService::<IAudioCaptureClient>() }.map_err(|err| err.message())?;
        let resampler = (rate != RATE).then(|| Resampler::new(rate, RATE));
        Ok(Self {
            client,
            capture,
            layout,
            resampler,
            block,
            origin: 0,
            raw: Vec::new(),
            stereo: Vec::new(),
            pcm: Vec::new(),
            running: false,
        })
    }
}

impl Drop for Audio {
    fn drop(&mut self) {
        if self.running {
            let _ = unsafe { self.client.Stop() };
        }
    }
}

fn endpoint(flow: EDataFlow, id: Option<&str>) -> Option<IMMDevice> {
    let enumerator: IMMDeviceEnumerator = unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL) }.ok()?;
    id.and_then(|id| unsafe { enumerator.GetDevice(&HSTRING::from(id)) }.ok())
        .or_else(|| unsafe { enumerator.GetDefaultAudioEndpoint(flow, eConsole) }.ok())
}

fn activate(device: &IMMDevice) -> Result<IAudioClient, String> {
    unsafe { device.Activate::<IAudioClient>(CLSCTX_ALL, None) }.map_err(|err| err.message())
}

fn initialize(client: &IAudioClient, flags: u32, format: &WAVEFORMATEX) -> Result<(), String> {
    unsafe { client.Initialize(AUDCLNT_SHAREMODE_SHARED, flags, BUFFER_TICKS, 0, format, None) }
        .map_err(|err| err.message())
}

fn stereo(rate: u32) -> WAVEFORMATEX {
    let block = FRAME_BYTES as u16;
    WAVEFORMATEX {
        wFormatTag: WAVE_FORMAT_PCM as u16,
        nChannels: CHANNELS as u16,
        nSamplesPerSec: rate,
        nAvgBytesPerSec: rate * u32::from(block),
        nBlockAlign: block,
        wBitsPerSample: 16,
        cbSize: 0,
    }
}

struct MixFormat(*mut WAVEFORMATEX);

impl MixFormat {
    fn of(client: &IAudioClient) -> Result<Self, String> {
        let format = unsafe { client.GetMixFormat() }.map_err(|err| err.message())?;
        if format.is_null() { Err("the device has no format".into()) } else { Ok(Self(format)) }
    }

    fn format(&self) -> &WAVEFORMATEX {
        unsafe { &*self.0 }
    }

    fn describe(&self) -> Result<(Layout, u32), String> {
        let format = self.format();
        let tag = format.wFormatTag;
        let float = match tag {
            FORMAT_EXTENSIBLE => {
                let extensible = unsafe { self.0.cast::<WAVEFORMATEXTENSIBLE>().read_unaligned() };
                extensible.SubFormat.data1 == u32::from(FORMAT_FLOAT)
            }
            _ => tag == FORMAT_FLOAT,
        };
        let (rate, bits, channels, block) =
            (format.nSamplesPerSec, format.wBitsPerSample, format.nChannels, format.nBlockAlign);
        let encoding = Encoding::from_format(float, bits).ok_or("the device's audio format isn't supported")?;
        if rate == 0 {
            return Err("the device reports no sample rate".into());
        }
        Ok((Layout { encoding, channels: usize::from(channels), block: usize::from(block) }, rate))
    }
}

impl Drop for MixFormat {
    fn drop(&mut self) {
        unsafe { CoTaskMemFree(Some(self.0 as *const _)) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stereo_format_is_16_bit_pcm() {
        let format = stereo(RATE);
        let (tag, channels, rate, bits) =
            (format.wFormatTag, format.nChannels, format.nSamplesPerSec, format.wBitsPerSample);
        assert_eq!((tag, channels, rate, bits), (WAVE_FORMAT_PCM as u16, 2, RATE, 16));
        let (block, per_second, extra) = (format.nBlockAlign, format.nAvgBytesPerSec, format.cbSize);
        assert_eq!((block, per_second, extra), (4, RATE * 4, 0));
        let slow = stereo(8_000);
        let slow_per_second = slow.nAvgBytesPerSec;
        assert_eq!(slow_per_second, 32_000);
    }
}
