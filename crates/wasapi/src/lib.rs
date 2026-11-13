//! WASAPI capture on Windows: the microphone, or a loopback of what's playing.
//! Only the PCM helpers build elsewhere, so their tests run everywhere.

#[cfg(windows)]
mod apartment;
#[cfg(windows)]
mod audio;
#[cfg(windows)]
mod loopback;
#[cfg(any(windows, test))]
mod pcm;

#[cfg(windows)]
pub use apartment::Apartment;
#[cfg(windows)]
pub use audio::{Audio, Packet};
#[cfg(windows)]
pub use loopback::Loopback;
#[cfg(windows)]
pub use pcm::{CHANNELS, FRAME_BYTES, RATE};
