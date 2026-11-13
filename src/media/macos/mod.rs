//! Now playing on a Mac. There's no public API for other apps' songs, so notchbase asks Music and Spotify directly.

mod capture;
mod players;
mod script;
mod source;

pub use capture::Capture;
pub use source::Source;
