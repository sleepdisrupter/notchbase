//! COM and Media Foundation for the recording thread's lifetime, and the performance counter in 100 ns ticks.

use windows::Win32::Foundation::RPC_E_CHANGED_MODE;
use windows::Win32::Media::MediaFoundation::{MF_VERSION, MFSTARTUP_NOSOCKET, MFShutdown, MFStartup};
use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx, CoUninitialize};
use windows::Win32::System::Performance::{QueryPerformanceCounter, QueryPerformanceFrequency};

use super::timeline::TICKS_PER_SECOND;

pub(super) struct Runtime {
    joined: bool,
    media: bool,
}

impl Runtime {
    pub(super) fn start() -> Result<Self, String> {
        let joined = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        // COM already set up in the other mode still works, as long as we don't uninitialize it.
        if joined != RPC_E_CHANGED_MODE {
            joined.ok().map_err(|err| err.message())?;
        }
        let mut runtime = Self { joined: joined.is_ok(), media: false };
        unsafe { MFStartup(MF_VERSION, MFSTARTUP_NOSOCKET) }.map_err(|err| err.message())?;
        runtime.media = true;
        Ok(runtime)
    }
}

impl Drop for Runtime {
    fn drop(&mut self) {
        if self.media {
            let _ = unsafe { MFShutdown() };
        }
        if self.joined {
            unsafe { CoUninitialize() };
        }
    }
}

/// The performance counter in 100 ns ticks, the clock WASAPI gives device positions in.
pub(super) fn performance_ticks() -> i64 {
    let (mut count, mut frequency) = (0i64, 0i64);
    unsafe {
        let _ = QueryPerformanceCounter(&mut count);
        let _ = QueryPerformanceFrequency(&mut frequency);
    }
    (i128::from(count) * i128::from(TICKS_PER_SECOND) / i128::from(frequency.max(1))) as i64
}
