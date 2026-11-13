//! Runs a tap's audio callback on its aggregate device.

use std::ffi::c_void;
use std::sync::Arc;

use super::TapError;
use super::aggregate::Aggregate;
use crate::audio::core_audio::buffers::AudioBufferList;
use crate::audio::core_audio::tap_shared::TapShared;
use crate::audio::macos::property::{AudioObjectID, OSStatus};

pub(super) type IoProc = unsafe extern "C" fn(
    device: AudioObjectID,
    now: *const c_void,
    input: *const AudioBufferList,
    input_time: *const c_void,
    output: *mut AudioBufferList,
    output_time: *const c_void,
    client: *mut c_void,
) -> OSStatus;
pub(super) type IoProcId = Option<IoProc>;

#[link(name = "CoreAudio", kind = "framework")]
unsafe extern "C" {
    pub(super) fn AudioDeviceCreateIOProcID(
        device: AudioObjectID,
        io_proc: IoProc,
        client: *mut c_void,
        id: *mut IoProcId,
    ) -> OSStatus;
    pub(super) fn AudioDeviceDestroyIOProcID(device: AudioObjectID, id: IoProcId) -> OSStatus;
    pub(super) fn AudioDeviceStart(device: AudioObjectID, id: IoProcId) -> OSStatus;
    pub(super) fn AudioDeviceStop(device: AudioObjectID, id: IoProcId) -> OSStatus;
}

pub struct RunningIo {
    aggregate: Aggregate,
    id: IoProcId,
    shared: Arc<TapShared>,
}

impl RunningIo {
    pub fn start(aggregate: Aggregate, shared: TapShared) -> Result<Self, TapError> {
        let shared = Arc::new(shared);
        // The callback reads `shared` through this pointer. Drop stops the device first, so it never dangles.
        let client = Arc::as_ptr(&shared).cast_mut().cast::<c_void>();
        let mut id: IoProcId = None;
        let status = unsafe { AudioDeviceCreateIOProcID(aggregate.device(), mix_tap, client, &mut id) };
        if id.is_none() {
            return Err(TapError::Failed);
        }
        let running = Self { aggregate, id, shared };
        if status != 0 || unsafe { AudioDeviceStart(running.aggregate.device(), running.id) } != 0 {
            return Err(TapError::Failed);
        }
        Ok(running)
    }

    pub fn shared(&self) -> &TapShared {
        &self.shared
    }
}

impl Drop for RunningIo {
    fn drop(&mut self) {
        let device = self.aggregate.device();
        unsafe {
            AudioDeviceStop(device, self.id);
            AudioDeviceDestroyIOProcID(device, self.id);
        }
    }
}

unsafe extern "C" fn mix_tap(
    _device: AudioObjectID,
    _now: *const c_void,
    input: *const AudioBufferList,
    _input_time: *const c_void,
    output: *mut AudioBufferList,
    _output_time: *const c_void,
    client: *mut c_void,
) -> OSStatus {
    if let Some(shared) = unsafe { client.cast::<TapShared>().as_ref() } {
        unsafe { shared.process(input, output) };
    }
    0
}
