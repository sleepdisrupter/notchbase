//! Screen recording on Windows: desktop duplication, a BGRA to NV12 converter, WASAPI audio and an H.264 writer.

mod cursor;
mod dxgi;
mod gdi;
mod mixer;
mod nv12;
mod recording;
mod runtime;
mod screen;
mod session;
#[cfg(test)]
mod tests;
mod timeline;
mod writer;

pub use notchbase_wasapi::Loopback;
pub(super) use session::Session;

pub(super) const EXTENSION: &str = "mp4";
