//! A tap on all the Mac's output, mixed down to mono for the music levels. Opening one is also how macOS is made to
//! show the System Audio Recording prompt.

use std::ffi::c_void;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use super::aggregate::Aggregate;
use super::api::TapApi;
use super::description::TapDescription;
use super::io_proc::{
    AudioDeviceCreateIOProcID, AudioDeviceDestroyIOProcID, AudioDeviceStart, AudioDeviceStop, IoProcId,
};
use super::process_tap::ProcessTap;
use crate::audio::core_audio::buffers::AudioBufferList;
use crate::audio::macos::devices::default_output;
use crate::audio::macos::permission::{Permission, audio_capture_permission};
use crate::audio::macos::property::{AudioObjectID, OSStatus, PropertyAddress, read};
use crate::audio::macos::selectors::{DEVICE_NOMINAL_RATE, SCOPE_INPUT};
use crate::audio::macos::streams::{buffer_count, outputs_float32};

const BACKLOG: usize = 48_000;
const ANSWER_WAIT: Duration = Duration::from_secs(120);
const ANSWER_POLL: Duration = Duration::from_millis(250);
const FALLBACK_RATE: u32 = 48_000;
const NOT_ALLOWED: &str = "System audio recording isn't allowed";
const UNAVAILABLE: &str = "System audio can't be tapped";

struct Pending {
    skip: usize,
    samples: Mutex<Vec<f32>>,
}

pub struct Listener {
    aggregate: Aggregate,
    id: IoProcId,
    pending: Arc<Pending>,
    rate: u32,
}

unsafe impl Send for Listener {}

impl Listener {
    pub fn open() -> Result<Self, String> {
        if audio_capture_permission() != Permission::Granted {
            return Err(NOT_ALLOWED.into());
        }
        Self::start()
    }

    pub fn supported() -> bool {
        TapApi::load().is_some()
    }

    pub fn ask_for_access() {
        let held = Self::start().ok();
        let asked = Instant::now();
        while audio_capture_permission() == Permission::Unknown && asked.elapsed() < ANSWER_WAIT {
            std::thread::sleep(ANSWER_POLL);
        }
        drop(held);
    }

    fn start() -> Result<Self, String> {
        let api = TapApi::load().ok_or(UNAVAILABLE)?;
        let (device, uid) = default_output().ok_or(UNAVAILABLE)?;
        if !outputs_float32(device) {
            return Err(UNAVAILABLE.into());
        }
        let description = TapDescription::everything(api.description_class()).ok_or(UNAVAILABLE)?;
        let tap = ProcessTap::create(api, &description).map_err(|_| UNAVAILABLE)?;
        let aggregate = Aggregate::create(&uid, tap).map_err(|_| UNAVAILABLE)?;
        let rate = read::<f64>(aggregate.device(), PropertyAddress::global(DEVICE_NOMINAL_RATE))
            .filter(|rate| rate.is_finite() && *rate >= 8_000.0)
            .map_or(FALLBACK_RATE, |rate| rate.round() as u32);
        let pending = Arc::new(Pending { skip: buffer_count(device, SCOPE_INPUT), samples: Mutex::new(Vec::new()) });
        let client = Arc::as_ptr(&pending).cast_mut().cast::<c_void>();
        let mut id: IoProcId = None;
        let status = unsafe { AudioDeviceCreateIOProcID(aggregate.device(), collect, client, &mut id) };
        if id.is_none() {
            return Err(UNAVAILABLE.into());
        }
        let listener = Self { aggregate, id, pending, rate };
        if status != 0 || unsafe { AudioDeviceStart(listener.aggregate.device(), listener.id) } != 0 {
            return Err(UNAVAILABLE.into());
        }
        Ok(listener)
    }

    pub fn rate(&self) -> u32 {
        self.rate
    }

    pub fn read(&mut self, mono: &mut Vec<f32>) {
        if let Ok(mut samples) = self.pending.samples.lock() {
            mono.append(&mut samples);
        }
    }
}

impl Drop for Listener {
    fn drop(&mut self) {
        let device = self.aggregate.device();
        unsafe {
            AudioDeviceStop(device, self.id);
            AudioDeviceDestroyIOProcID(device, self.id);
        }
    }
}

unsafe extern "C" fn collect(
    _device: AudioObjectID,
    _now: *const c_void,
    input: *const AudioBufferList,
    _input_time: *const c_void,
    _output: *mut AudioBufferList,
    _output_time: *const c_void,
    client: *mut c_void,
) -> OSStatus {
    let Some(pending) = (unsafe { client.cast::<Pending>().as_ref() }) else { return 0 };
    let buffers = unsafe { AudioBufferList::buffers(input) };
    let Some(tapped) = buffers.get(pending.skip) else { return 0 };
    let channels = tapped.channels.max(1) as usize;
    if tapped.data.is_null() {
        return 0;
    }
    let count = tapped.bytes as usize / size_of::<f32>();
    let samples = unsafe { std::slice::from_raw_parts(tapped.data.cast::<f32>(), count) };
    if let Ok(mut out) = pending.samples.try_lock()
        && out.len() < BACKLOG
    {
        out.extend(samples.chunks_exact(channels).map(|frame| frame.iter().sum::<f32>() / channels as f32));
    }
    0
}
