//! What's playing and how loud each band of the music is right now, for the Media page and the notch, and the
//! controls that go back to the player.

#[cfg(any(target_os = "macos", test))]
mod applescript;
mod artwork;
mod follower;
mod groove;
mod listener;
mod track;
mod watcher;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
use self::macos as imp;

#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "windows")]
use self::windows as imp;

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
mod other;
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
use self::other as imp;

pub use notchbase_spectrum::{BANDS, Bands};
pub use track::{Control, NowPlaying, Timeline};
pub use watcher::Media;
