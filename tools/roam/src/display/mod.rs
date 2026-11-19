//! Another computer's screen as an extended display of this one.
//!
//! The computer that borrows the screen (the host) makes a virtual display the size of it,
//! places it where that screen sits on the desk, and streams what's on it as H.264 over the
//! network. The computer that lends its screen (the viewer) covers that screen with a window
//! showing the stream, and sends back what its own mouse and keyboard do in that window.
//!
//! - On a Mac, the virtual display is a `CGVirtualDisplay`, captured with ScreenCaptureKit and
//!   encoded with VideoToolbox; a lent screen shows the stream with an
//!   `AVSampleBufferDisplayLayer`, which decodes in hardware.
//! - On Windows, the virtual display comes from the Virtual Display Driver (an open-source,
//!   signed indirect display driver), captured with desktop duplication and encoded with Media
//!   Foundation; a lent screen decodes with Media Foundation and draws with GDI.
//!
//! The stream is H.264 in Annex B on both sides, so any host works with any viewer.

#[cfg(any(windows, test))]
mod cursor;
pub mod h264;
#[cfg(any(windows, test))]
mod nv12;
pub mod stream;

#[cfg(target_os = "macos")]
mod mac;
#[cfg(windows)]
mod windows;

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

use crate::layout::Rect;

/// How many encoded pictures may wait to go out before the host stops encoding new ones: a
/// slow network then shows fewer pictures, never older ones.
const MOST_WAITING: usize = 2;

/// A screen that will show another computer's picture.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Panel {
    /// Its size in pixels.
    pub pixels: [u32; 2],
    /// How often it refreshes, a second.
    pub hz: u32,
    /// Its pixels per point: 2 on a Retina screen, 1.5 on a PC at 150%.
    pub scale: f32,
}

/// The virtual display a host makes.
#[derive(Clone, Debug, PartialEq)]
pub struct Want {
    /// What the system calls it: the lending computer's name.
    pub name: String,
    pub panel: Panel,
    /// Where its top left goes, in the host's own units.
    pub at: [i32; 2],
}

/// One encoded picture: an H.264 access unit in Annex B, with parameter sets before each key
/// picture.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Picture {
    pub key: bool,
    pub data: Vec<u8>,
}

/// What the lending computer's own mouse and keyboard did over the lent screen.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Remote {
    /// The cursor is here, 0 to 1 across and down the picture.
    Move([f32; 2]),
    Button {
        button: u8,
        down: bool,
    },
    /// 120 to a notch, positive is up and right.
    Wheel {
        dx: f32,
        dy: f32,
    },
    Key {
        hid: u16,
        down: bool,
    },
}

/// Whether this computer can make virtual displays.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Ability {
    Ready,
    /// It can, once the display driver is installed.
    NeedsDriver,
    No(String),
}

impl Default for Ability {
    fn default() -> Self {
        Ability::No(String::new())
    }
}

/// Passes encoded pictures from the encoder to the network, and the network's wishes back.
#[derive(Clone, Default)]
pub struct Feed(Arc<FeedInner>);

#[derive(Default)]
struct FeedInner {
    waiting: Mutex<VecDeque<Picture>>,
    changed: Condvar,
    stopped: AtomicBool,
    key_wanted: AtomicBool,
}

impl Feed {
    pub fn new() -> Self {
        let feed = Self::default();
        // The first picture is always a key picture.
        feed.ask_key();
        feed
    }

    /// The network is behind: skip this picture and take the next.
    pub fn busy(&self) -> bool {
        self.0.waiting.lock().map_or(true, |w| w.len() >= MOST_WAITING)
    }

    pub fn push(&self, picture: Picture) {
        if let Ok(mut waiting) = self.0.waiting.lock() {
            waiting.push_back(picture);
        }
        self.0.changed.notify_all();
    }

    /// Whether the next picture should be a key picture (and forgets the wish).
    pub fn key_wanted(&self) -> bool {
        self.0.key_wanted.swap(false, Ordering::AcqRel)
    }

    /// Whether a key picture is wanted, keeping the wish.
    pub fn key_pending(&self) -> bool {
        self.0.key_wanted.load(Ordering::Acquire)
    }

    pub fn ask_key(&self) {
        self.0.key_wanted.store(true, Ordering::Release);
    }

    pub fn stopped(&self) -> bool {
        self.0.stopped.load(Ordering::Acquire)
    }

    pub fn stop(&self) {
        self.0.stopped.store(true, Ordering::Release);
        self.0.changed.notify_all();
    }

    /// The next picture to send, waiting at most `wait`.
    pub fn next(&self, wait: Duration) -> Option<Picture> {
        let waiting = self.0.waiting.lock().ok()?;
        let (mut waiting, _) =
            self.0.changed.wait_timeout_while(waiting, wait, |w| w.is_empty() && !self.stopped()).ok()?;
        waiting.pop_front()
    }
}

/// A virtual display being shown elsewhere. Dropping it takes the display away.
pub trait Hosted: Send {
    /// Where it is now, in this computer's units: the system may have moved it to touch the
    /// other displays.
    fn rect(&mut self) -> Option<Rect>;
    /// Moves it, its top left to `at` in this computer's units.
    fn place(&mut self, at: [i32; 2]);
    /// The size of the pictures it sends, in pixels.
    fn size(&self) -> [u32; 2];
    /// Why it stopped sending pictures on its own, if it did.
    fn failed(&self) -> Option<String> {
        None
    }
}

/// A lent screen showing another computer's picture. Dropping it gives the screen back.
pub trait Showing: Send {
    /// Shows the next picture. An error asks the host for a key picture.
    fn picture(&mut self, picture: &Picture) -> Result<(), String>;
}

pub type RemoteSink = Box<dyn Fn(Remote) + Send + Sync>;

/// What a computer does with displays: make virtual ones, and lend its own.
pub trait Displays: Send + Sync {
    fn ability(&self) -> Ability;
    /// Whether this computer can lend its screens to another.
    fn lends(&self) -> bool {
        lends()
    }
    /// Installs what making virtual displays needs, asking the system for permission. Blocks
    /// until done.
    fn install(&self) -> Result<(), String> {
        Err("There's nothing to install on this computer".into())
    }
    /// Makes a virtual display and starts sending what's on it to `feed`.
    fn host(&self, want: &Want, feed: Feed) -> Result<Box<dyn Hosted>, String>;
    /// What the display at `rect` (in this computer's units) is like, to lend it.
    fn panel(&self, rect: Rect) -> Option<Panel>;
    /// Covers the display at `rect` with pictures `size` pixels big. What the person does
    /// over them goes to `remote`.
    fn show(&self, rect: Rect, size: [u32; 2], remote: RemoteSink) -> Result<Box<dyn Showing>, String>;
}

/// The displays of the computer this runs on.
pub fn native() -> Arc<dyn Displays> {
    #[cfg(target_os = "macos")]
    return Arc::new(mac::Mac);
    #[cfg(windows)]
    return Arc::new(windows::Windows::new());
    #[cfg(not(any(target_os = "macos", windows)))]
    return Arc::new(Unsupported);
}

/// Whether this computer can lend its screens.
pub fn lends() -> bool {
    cfg!(any(target_os = "macos", windows))
}

/// A computer that can neither make virtual displays nor lend its own.
pub struct Unsupported;

impl Displays for Unsupported {
    fn ability(&self) -> Ability {
        Ability::No("Extended displays work on macOS and Windows".into())
    }

    fn host(&self, _want: &Want, _feed: Feed) -> Result<Box<dyn Hosted>, String> {
        Err("Extended displays work on macOS and Windows".into())
    }

    fn panel(&self, _rect: Rect) -> Option<Panel> {
        None
    }

    fn show(&self, _rect: Rect, _size: [u32; 2], _remote: RemoteSink) -> Result<Box<dyn Showing>, String> {
        Err("Extended displays work on macOS and Windows".into())
    }
}

/// The picture size for a panel: its own pixels, kept within what H.264 levels and a home
/// network carry well (4K), and even on both sides as the encoders need.
pub fn picture_size(panel: Panel) -> [u32; 2] {
    const MOST: [u32; 2] = [3840, 2160];
    let [w, h] = panel.pixels.map(|side| side.max(64));
    let fit = (f64::from(MOST[0]) / f64::from(w)).min(f64::from(MOST[1]) / f64::from(h)).min(1.0);
    [w, h].map(|side| ((f64::from(side) * fit) as u32) & !1)
}

/// Of the sizes a display offers, the one to show a `want`-sized screen at: the same shape as
/// near as there is, then the nearest in size. Only even sizes up to 4K, which the encoders take.
pub fn closest_size(offered: &[[u32; 2]], want: [u32; 2]) -> Option<[u32; 2]> {
    let shape = |[w, h]: [u32; 2]| (f64::from(w) / f64::from(h.max(1))).ln();
    let area = |[w, h]: [u32; 2]| (f64::from(w) * f64::from(h)).max(1.0).ln();
    let fits = |&&[w, h]: &&[u32; 2]| w > 0 && h > 0 && w % 2 == 0 && h % 2 == 0 && w <= 3840 && h <= 2160;
    offered.iter().filter(fits).copied().min_by_key(|&size| {
        // Shapes within about 2% count as the same.
        let off_shape = ((shape(size) - shape(want)).abs() * 50.0).round() as u64;
        let off_size = ((area(size) - area(want)).abs() * 1000.0).round() as u64;
        (off_shape, off_size)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_screen_takes_the_nearest_size_of_the_same_shape() {
        let offered = [[1920, 1080], [2560, 1440], [3840, 2160], [1280, 800], [1920, 1200]];
        assert_eq!(closest_size(&offered, [2560, 1440]), Some([2560, 1440]), "an exact size");
        assert_eq!(closest_size(&offered, [2940, 1912]), Some([1920, 1200]), "a MacBook's shape, 16:10");
        assert_eq!(closest_size(&offered, [3024, 1964]), Some([1920, 1200]));
        assert_eq!(closest_size(&offered, [3440, 1440]), Some([2560, 1440]), "the widest shape there is, nearest size");
        assert_eq!(closest_size(&offered, [2048, 1152]), Some([1920, 1080]));
        assert_eq!(closest_size(&[[1365, 767], [5120, 2880]], [1366, 768]), None, "odd and too big are out");
        assert_eq!(closest_size(&[], [1920, 1080]), None);
    }

    #[test]
    fn pictures_stay_within_4k_and_even() {
        let panel = |pixels| Panel { pixels, hz: 60, scale: 1.0 };
        assert_eq!(picture_size(panel([2560, 1440])), [2560, 1440]);
        assert_eq!(picture_size(panel([5120, 2880])), [3840, 2160]);
        assert_eq!(picture_size(panel([1365, 767])), [1364, 766]);
        assert_eq!(picture_size(panel([6016, 3384])), [3840, 2160]);
    }

    #[test]
    fn the_feed_holds_back_when_the_network_is_behind() {
        let feed = Feed::new();
        assert!(feed.key_wanted(), "the first picture is a key picture");
        assert!(!feed.key_wanted());
        assert!(!feed.busy());
        feed.push(Picture { key: true, data: vec![1] });
        feed.push(Picture { key: false, data: vec![2] });
        assert!(feed.busy());
        assert_eq!(feed.next(Duration::ZERO).map(|p| p.data), Some(vec![1]));
        assert!(!feed.busy());
        feed.stop();
        assert!(feed.stopped());
        assert_eq!(feed.next(Duration::from_secs(5)).map(|p| p.data), Some(vec![2]));
        assert_eq!(feed.next(Duration::from_secs(5)), None, "stopped and empty: no wait");
    }
}
