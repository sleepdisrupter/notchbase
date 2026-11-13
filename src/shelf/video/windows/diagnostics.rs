//! Turns Windows errors into the shelf's own, and logs which step failed on which file.

use std::path::Path;

use ::windows::core::Error;

use crate::shelf::names::file_name;
use crate::shelf::video::MediaError;

pub(super) trait Diagnose<T> {
    fn failed(self, step: &str, source: &Path) -> Result<T, MediaError>;
    fn unsupported(self, step: &str, source: &Path) -> Result<T, MediaError>;
}

impl<T> Diagnose<T> for ::windows::core::Result<T> {
    fn failed(self, step: &str, source: &Path) -> Result<T, MediaError> {
        self.map_err(|error| complain(MediaError::Failed, step, source, &describe(&error)))
    }

    fn unsupported(self, step: &str, source: &Path) -> Result<T, MediaError> {
        self.map_err(|error| complain(MediaError::Unsupported, step, source, &describe(&error)))
    }
}

pub(super) fn complain(kind: MediaError, step: &str, source: &Path, detail: &str) -> MediaError {
    eprintln!("notchbase: couldn't {step} for video {}: {detail}", file_name(source));
    kind
}

fn describe(error: &Error) -> String {
    let code = error.code().0 as u32;
    match error.message().trim() {
        "" => format!("HRESULT {code:#010x}"),
        message => format!("{message} (HRESULT {code:#010x})"),
    }
}
