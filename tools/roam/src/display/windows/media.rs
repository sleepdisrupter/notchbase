//! What the encoder and decoder share: starting Media Foundation, and samples to and from bytes.

use std::sync::OnceLock;

use windows::Win32::Media::MediaFoundation::{
    IMFSample, MF_VERSION, MFCreateMemoryBuffer, MFCreateSample, MFSTARTUP_FULL, MFStartup,
};
use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx};

/// Starts COM on this thread and Media Foundation for the process.
pub fn start() -> Result<(), String> {
    // SAFETY: joins the thread to the multithreaded apartment; a second call is harmless.
    let _ = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
    static STARTED: OnceLock<Result<(), String>> = OnceLock::new();
    // SAFETY: started once for the whole process, never shut down while Roam runs.
    STARTED.get_or_init(|| unsafe { MFStartup(MF_VERSION, MFSTARTUP_FULL) }.map_err(|err| err.message())).clone()
}

/// Two 32-bit values in one attribute, as Media Foundation keeps sizes and rates.
pub fn pack(high: u32, low: u32) -> u64 {
    (u64::from(high) << 32) | u64::from(low)
}

pub fn unpack(value: u64) -> [u32; 2] {
    [(value >> 32) as u32, value as u32]
}

/// A sample holding a copy of `bytes`.
pub fn sample(bytes: &[u8]) -> Result<IMFSample, String> {
    let err = |err: windows::core::Error| err.message();
    // SAFETY: the buffer is locked only while copied into, for exactly its length.
    unsafe {
        let buffer = MFCreateMemoryBuffer(bytes.len() as u32).map_err(err)?;
        let mut data = std::ptr::null_mut();
        buffer.Lock(&mut data, None, None).map_err(err)?;
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), data, bytes.len());
        buffer.Unlock().map_err(err)?;
        buffer.SetCurrentLength(bytes.len() as u32).map_err(err)?;
        let sample = MFCreateSample().map_err(err)?;
        sample.AddBuffer(&buffer).map_err(err)?;
        Ok(sample)
    }
}

/// What a sample holds, as one run of bytes.
pub fn bytes(sample: &IMFSample) -> Result<Vec<u8>, String> {
    let err = |err: windows::core::Error| err.message();
    // SAFETY: the buffer is locked only while copied out of, for its current length.
    unsafe {
        let buffer = sample.ConvertToContiguousBuffer().map_err(err)?;
        let mut data = std::ptr::null_mut();
        let mut length = 0;
        buffer.Lock(&mut data, None, Some(&mut length)).map_err(err)?;
        let copy = std::slice::from_raw_parts(data, length as usize).to_vec();
        buffer.Unlock().map_err(err)?;
        Ok(copy)
    }
}
