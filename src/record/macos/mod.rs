//! Screen recording on macOS with ScreenCaptureKit, which captures, encodes and writes the movie itself.

mod delegate;
mod session;
mod shared;
mod stream;

pub(super) use session::Session;

pub(super) const EXTENSION: &str = "mov";
