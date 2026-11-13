//! Starts COM and Media Foundation for one job and shuts them down after.

use std::path::Path;

use ::windows::Win32::Foundation::RPC_E_CHANGED_MODE;
use ::windows::Win32::Media::MediaFoundation::{MF_VERSION, MFSTARTUP_NOSOCKET, MFShutdown, MFStartup};
use ::windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx, CoUninitialize};

use super::diagnostics::Diagnose;
use crate::shelf::video::MediaError;

pub(super) struct Runtime {
    joined_apartment: bool,
    started_media: bool,
}

impl Runtime {
    pub(super) fn start(source: &Path) -> Result<Self, MediaError> {
        let joined = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        if joined != RPC_E_CHANGED_MODE {
            joined.ok().failed("start COM", source)?;
        }
        let mut runtime = Self { joined_apartment: joined.is_ok(), started_media: false };
        unsafe { MFStartup(MF_VERSION, MFSTARTUP_NOSOCKET) }.failed("start Media Foundation", source)?;
        runtime.started_media = true;
        Ok(runtime)
    }
}

impl Drop for Runtime {
    fn drop(&mut self) {
        if self.started_media {
            let _ = unsafe { MFShutdown() };
        }
        if self.joined_apartment {
            unsafe { CoUninitialize() };
        }
    }
}
