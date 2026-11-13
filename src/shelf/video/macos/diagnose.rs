//! Turns AVFoundation errors into the shelf's own: media the system can't handle is unsupported, the rest a failure.

use std::path::Path;

use objc2_av_foundation::{AVError, AVFoundationErrorDomain};
use objc2_foundation::NSError;

use super::super::MediaError;

const UNSUPPORTED_MEDIA: [AVError; 15] = [
    AVError::DecodeFailed,
    AVError::InvalidSourceMedia,
    AVError::FileFormatNotRecognized,
    AVError::FileFailedToParse,
    AVError::ContentIsProtected,
    AVError::DecoderNotFound,
    AVError::EncoderNotFound,
    AVError::ContentIsNotAuthorized,
    AVError::OperationNotSupportedForAsset,
    AVError::IncompatibleAsset,
    AVError::FailedToParse,
    AVError::UndecodableMediaData,
    AVError::FormatUnsupported,
    AVError::NoSourceTrack,
    AVError::OperationNotSupportedForPreset,
];

pub(super) fn failure(action: &str, path: &Path, error: Option<&NSError>) -> MediaError {
    let details = error.map_or_else(|| "no details from AVFoundation".to_owned(), describe);
    eprintln!("notchbase: couldn't {action} {}: {details}", path.display());
    error.map_or(MediaError::Failed, classify)
}

pub(super) fn refusal(action: &str, path: &Path, reason: &str, kind: MediaError) -> MediaError {
    eprintln!("notchbase: couldn't {action} {}: {reason}", path.display());
    kind
}

fn classify(error: &NSError) -> MediaError {
    let domain = error.domain();
    let from_av_foundation = unsafe { AVFoundationErrorDomain }.is_some_and(|av| domain.isEqualToString(av));
    if from_av_foundation { kind_of(AVError(error.code())) } else { MediaError::Failed }
}

fn kind_of(code: AVError) -> MediaError {
    if UNSUPPORTED_MEDIA.contains(&code) { MediaError::Unsupported } else { MediaError::Failed }
}

fn describe(error: &NSError) -> String {
    let summary = summarize(error);
    match error.underlyingErrors().firstObject() {
        Some(cause) => format!("{summary}, caused by {}", summarize(&cause)),
        None => summary,
    }
}

fn summarize(error: &NSError) -> String {
    format!("{} ({} {})", error.localizedDescription(), error.domain(), error.code())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unhandled_media_is_unsupported() {
        assert_eq!(kind_of(AVError::FileFormatNotRecognized), MediaError::Unsupported);
        assert_eq!(kind_of(AVError::DecoderNotFound), MediaError::Unsupported);
        assert_eq!(kind_of(AVError::UndecodableMediaData), MediaError::Unsupported);
    }

    #[test]
    fn everything_else_is_a_plain_failure() {
        assert_eq!(kind_of(AVError::DiskFull), MediaError::Failed);
        assert_eq!(kind_of(AVError::Unknown), MediaError::Failed);
        assert_eq!(kind_of(AVError::FileAlreadyExists), MediaError::Failed);
    }
}
