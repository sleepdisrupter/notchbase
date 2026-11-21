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
pub mod udp;

#[cfg(target_os = "macos")]
mod mac;
#[cfg(windows)]
mod windows;

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use crate::layout::Rect;

/// How many encoded pictures may wait to go out before the host stops encoding new ones: a
/// slow network then shows fewer pictures, never older ones.
const MOST_WAITING: usize = 1;

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
    /// The lending computer draws the pointer itself: a host that can sends it with
    /// `Feed::pointer` and leaves it out of the pictures, so it moves without waiting for them.
    pub pointer_apart: bool,
}

/// Where the host's pointer is on its pictures: the top left of its shape, in their pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PointerAt {
    pub at: [i32; 2],
    pub visible: bool,
}

/// What the host's pointer looks like: straight (not premultiplied) RGBA, row by row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PointerShape {
    pub size: [u32; 2],
    pub rgba: Vec<u8>,
}

impl PointerShape {
    /// The biggest pointer that goes, a side.
    pub const MOST: u32 = 512;

    /// Whether the pixels are as many as the size says, and the size is one that goes.
    pub fn sound(&self) -> bool {
        let [w, h] = self.size;
        w > 0 && h > 0 && w <= Self::MOST && h <= Self::MOST && self.rgba.len() == w as usize * h as usize * 4
    }
}

/// Set (to anything) on the host to keep the pointer in the pictures even for a lending
/// computer that could draw it.
pub const POINTER_IN_PICTURE: &str = "NOTCHBASE_ROAM_POINTER_IN_PICTURE";

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
    waiting: Mutex<Waiting>,
    changed: Condvar,
    stopped: AtomicBool,
    key_wanted: AtomicBool,
    pointer_wanted: AtomicBool,
    /// What the capture knows of the pointer, for sending its place the quick way.
    cursor: Mutex<Option<Cursor>>,
    /// The pointer's place goes the quick way, in the session's packets: the picture stream
    /// then only says when it shows or hides.
    quick: AtomicBool,
    /// How much of its full bitrate the encoder may use, in percent; 0 is all of it.
    rate: AtomicU32,
    tally: Mutex<Tally>,
    notes: Mutex<Vec<String>>,
}

/// What waits to go out: pictures in order, and only the latest of the pointer's news.
#[derive(Default)]
struct Waiting {
    pictures: VecDeque<Picture>,
    pointer: Option<PointerAt>,
    shape: Option<PointerShape>,
}

impl Waiting {
    fn any(&self) -> bool {
        !self.pictures.is_empty() || self.pointer.is_some() || self.shape.is_some()
    }
}

/// What the host's capture knows of its pointer: enough for the engine to say where the
/// pointer image goes from where its own mouse hook has the cursor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cursor {
    /// The point of the pointer image that is the cursor's spot, in its pixels.
    pub hotspot: [i32; 2],
    pub visible: bool,
    /// The pictures' size, in pixels.
    pub size: [u32; 2],
}

/// The pointer's news, taken from the feed: a new shape (first), then where it is.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PointerNews {
    pub shape: Option<PointerShape>,
    pub at: Option<PointerAt>,
}

/// How the pictures went since the last look, for the log.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Tally {
    /// Pictures the encoder made.
    pub pictures: u32,
    /// Changes not encoded because the network was still sending.
    pub skipped: u32,
    pub bytes: u64,
    /// Time spent getting pictures ready for the encoder, and encoding them.
    pub convert: Duration,
    pub encode: Duration,
    /// How long pictures took from going into the encoder to coming out, and the longest.
    pub delayed: u32,
    pub delay: Duration,
    pub longest_delay: Duration,
    /// Pictures sent, the time sending took, and the longest one.
    pub sent: u32,
    pub send: Duration,
    pub longest_send: Duration,
}

impl Tally {
    /// One line for the log, over `seconds`.
    pub fn line(&self, seconds: f64) -> String {
        let seconds = seconds.max(0.001);
        let each = |total: Duration, count: u32| total.as_secs_f64() * 1000.0 / f64::from(count.max(1));
        format!(
            "extended display, last {seconds:.0} s: {:.0} pictures a second ({} skipped while the network was busy), {:.1} Mbit/s; getting ready {:.1} ms, handing to the encoder {:.1} ms, in the encoder {:.1} ms (longest {:.0} ms), sending {:.1} ms (longest {:.0} ms)",
            f64::from(self.pictures) / seconds,
            self.skipped,
            self.bytes as f64 * 8.0 / seconds / 1_000_000.0,
            each(self.convert, self.pictures),
            each(self.encode, self.pictures),
            each(self.delay, self.delayed),
            self.longest_delay.as_secs_f64() * 1000.0,
            each(self.send, self.sent),
            self.longest_send.as_secs_f64() * 1000.0,
        )
    }
}

/// When the capture loop may encode: as soon as the screen changes, but no more than `every`
/// apart. Waiting for a fixed tick instead adds up to a whole frame to every change.
#[derive(Clone, Copy, Debug)]
pub struct Pace {
    every: Duration,
    next: Instant,
}

impl Pace {
    /// How long to wait for the screen when nothing is due: the loop still comes round this
    /// often to send what the encoder finishes, and the occasional refresh.
    const IDLE: Duration = Duration::from_millis(50);
    /// How often to look for a picture the encoder is still working on.
    const POLL: Duration = Duration::from_millis(2);

    pub fn new(every: Duration, now: Instant) -> Self {
        Self { every, next: now }
    }

    /// Until when to wait for the screen: until a picture may go again, else for the next
    /// change (returning at once when one comes), or only briefly while something is waiting:
    /// a picture in the encoder, or a change the network held back.
    pub fn wait_until(&self, now: Instant, waiting: bool) -> Instant {
        if now < self.next { self.next } else { now + if waiting { Self::POLL } else { Self::IDLE } }
    }

    pub fn may_encode(&self, now: Instant) -> bool {
        now >= self.next
    }

    /// A picture went to the encoder at `at`.
    pub fn encoded(&mut self, at: Instant) {
        self.next = at + self.every;
    }
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
        self.0.waiting.lock().map_or(true, |w| w.pictures.len() >= MOST_WAITING)
    }

    pub fn push(&self, picture: Picture) {
        self.count(|t| t.bytes += picture.data.len() as u64);
        if let Ok(mut waiting) = self.0.waiting.lock() {
            waiting.pictures.push_back(picture);
        }
        self.0.changed.notify_all();
    }

    /// The pointer moved or changed shape: it goes out ahead of any picture, and news not yet
    /// sent gives way to newer.
    pub fn pointer(&self, at: Option<PointerAt>, shape: Option<PointerShape>) {
        if at.is_none() && shape.is_none() {
            return;
        }
        if let Ok(mut waiting) = self.0.waiting.lock() {
            if at.is_some() {
                waiting.pointer = at;
            }
            if shape.is_some() {
                waiting.shape = shape;
            }
        }
        self.0.changed.notify_all();
    }

    /// The capture's latest word on the pointer.
    pub fn set_cursor(&self, cursor: Cursor) {
        if let Ok(mut slot) = self.0.cursor.lock() {
            *slot = Some(cursor);
        }
    }

    pub fn cursor(&self) -> Option<Cursor> {
        self.0.cursor.lock().ok().and_then(|c| *c)
    }

    /// The engine sends the pointer's place the quick way from now on.
    pub fn go_quick(&self) {
        self.0.quick.store(true, Ordering::Release);
    }

    pub fn quick(&self) -> bool {
        self.0.quick.load(Ordering::Acquire)
    }

    /// The network is losing pictures: the encoder should use this much of its full bitrate.
    pub fn set_rate(&self, percent: u32) {
        self.0.rate.store(percent.clamp(1, 100), Ordering::Release);
    }

    /// How much of its full bitrate the encoder should use, in percent.
    pub fn rate(&self) -> u32 {
        match self.0.rate.load(Ordering::Acquire) {
            0 => 100,
            percent => percent,
        }
    }

    /// The pointer's news since the last call.
    pub fn take_pointer(&self) -> PointerNews {
        self.0
            .waiting
            .lock()
            .map(|mut w| PointerNews { shape: w.shape.take(), at: w.pointer.take() })
            .unwrap_or_default()
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

    /// A new stream: the pointer's place and shape should go again, though they haven't changed.
    pub fn ask_pointer(&self) {
        self.0.pointer_wanted.store(true, Ordering::Release);
    }

    /// Whether the pointer should go again (and forgets the wish).
    pub fn pointer_wanted(&self) -> bool {
        self.0.pointer_wanted.swap(false, Ordering::AcqRel)
    }

    /// Forgets pictures not yet sent: a stream starting again has no use for them.
    pub fn clear_pictures(&self) {
        if let Ok(mut waiting) = self.0.waiting.lock() {
            waiting.pictures.clear();
        }
    }

    pub fn stopped(&self) -> bool {
        self.0.stopped.load(Ordering::Acquire)
    }

    pub fn stop(&self) {
        self.0.stopped.store(true, Ordering::Release);
        self.0.changed.notify_all();
    }

    fn count(&self, change: impl FnOnce(&mut Tally)) {
        if let Ok(mut tally) = self.0.tally.lock() {
            change(&mut tally);
        }
    }

    /// A picture was got ready and encoded, taking these long.
    pub fn count_encoded(&self, convert: Duration, encode: Duration) {
        self.count(|t| {
            t.pictures += 1;
            t.convert += convert;
            t.encode += encode;
        });
    }

    /// Pictures came out of the encoder, these long after going in.
    pub fn count_delays(&self, delays: &[Duration]) {
        self.count(|t| {
            for &delay in delays {
                t.delayed += 1;
                t.delay += delay;
                t.longest_delay = t.longest_delay.max(delay);
            }
        });
    }

    /// A change wasn't encoded: the network was still sending.
    pub fn count_skipped(&self) {
        self.count(|t| t.skipped += 1);
    }

    /// A picture went out, taking this long to hand to the network.
    pub fn count_sent(&self, took: Duration) {
        self.count(|t| {
            t.sent += 1;
            t.send += took;
            t.longest_send = t.longest_send.max(took);
        });
    }

    /// A line for the log, from the thread making the pictures.
    pub fn note(&self, text: String) {
        if let Ok(mut notes) = self.0.notes.lock() {
            notes.push(text);
        }
    }

    pub fn take_notes(&self) -> Vec<String> {
        self.0.notes.lock().map(|mut notes| std::mem::take(&mut *notes)).unwrap_or_default()
    }

    /// What happened since the last call.
    pub fn take_tally(&self) -> Tally {
        self.0.tally.lock().map(|mut t| std::mem::take(&mut *t)).unwrap_or_default()
    }

    /// The next picture to send, waiting at most `wait`, and coming back early (perhaps with
    /// no picture) when the pointer has news.
    pub fn next(&self, wait: Duration) -> Option<Picture> {
        let waiting = self.0.waiting.lock().ok()?;
        let (mut waiting, _) =
            self.0.changed.wait_timeout_while(waiting, wait, |w| !w.any() && !self.stopped()).ok()?;
        waiting.pictures.pop_front()
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
    /// The host's pointer moved, for a screen that draws it itself.
    fn pointer(&mut self, _at: PointerAt) {}
    /// The host's pointer changed shape.
    fn shape(&mut self, _shape: &PointerShape) {}
    /// The longest the pointer waited to be drawn since the last call.
    fn take_pointer_lag(&mut self) -> Option<Duration> {
        None
    }
}

pub type RemoteSink = Box<dyn Fn(Remote) + Send + Sync>;

/// What a computer does with displays: make virtual ones, and lend its own.
pub trait Displays: Send + Sync {
    fn ability(&self) -> Ability;
    /// Whether this computer can lend its screens to another.
    fn lends(&self) -> bool {
        lends()
    }
    /// Whether a screen this computer lends draws the host's pointer itself.
    fn draws_pointer(&self) -> bool {
        false
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
    /// Whether this computer shows line-up guides over its screens.
    fn aligns(&self) -> bool {
        cfg!(any(target_os = "macos", windows))
    }
    /// Puts line-up guides over this computer's screens, in place of what was there: none takes
    /// them away.
    fn guide(&self, shown: Vec<crate::align::Shown>) {
        #[cfg(any(target_os = "macos", windows))]
        crate::align::show_here(shown);
        #[cfg(not(any(target_os = "macos", windows)))]
        drop(shown);
    }
    /// Puts the cursor's marks over this computer's screens, in place of the ones there.
    fn mark(&self, marks: Vec<crate::align::Marked>) {
        #[cfg(any(target_os = "macos", windows))]
        crate::align::mark_here(marks);
        #[cfg(not(any(target_os = "macos", windows)))]
        drop(marks);
    }
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
        assert!(feed.busy(), "one picture waiting is enough: the next one waits for the network");
        feed.push(Picture { key: false, data: vec![2] });
        assert_eq!(feed.next(Duration::ZERO).map(|p| p.data), Some(vec![1]));
        assert!(feed.busy());
        assert_eq!(feed.take_tally().bytes, 2);
        assert_eq!(feed.take_tally(), Tally::default(), "taking the tally starts it again");
        feed.stop();
        assert!(feed.stopped());
        assert_eq!(feed.next(Duration::from_secs(5)).map(|p| p.data), Some(vec![2]));
        assert!(!feed.busy());
        assert_eq!(feed.next(Duration::from_secs(5)), None, "stopped and empty: no wait");
    }

    #[test]
    fn a_change_is_encoded_at_once_but_never_faster_than_the_rate() {
        let every = Duration::from_millis(16);
        let start = Instant::now();
        let mut pace = Pace::new(every, start);
        assert!(pace.may_encode(start));
        // Nothing due: wait for the next change, briefly while the encoder is busy.
        assert_eq!(pace.wait_until(start, false), start + Pace::IDLE);
        assert_eq!(pace.wait_until(start, true), start + Pace::POLL);
        pace.encoded(start);
        // Just after one, the next waits out the rest of the frame, gathering changes.
        let soon = start + Duration::from_millis(5);
        assert!(!pace.may_encode(soon));
        assert_eq!(pace.wait_until(soon, false), start + every);
        assert!(pace.may_encode(start + every));
    }

    #[test]
    fn pointer_news_goes_ahead_and_only_the_latest() {
        let feed = Feed::new();
        assert_eq!(feed.take_pointer(), PointerNews::default());
        let start = Instant::now();
        assert_eq!(feed.next(Duration::from_millis(30)), None);
        assert!(start.elapsed() >= Duration::from_millis(25), "nothing to send: it waits");
        feed.push(Picture { key: true, data: vec![1] });
        let shape = PointerShape { size: [1, 1], rgba: vec![1, 2, 3, 4] };
        feed.pointer(None, Some(shape.clone()));
        feed.pointer(Some(PointerAt { at: [1, 2], visible: true }), None);
        feed.pointer(Some(PointerAt { at: [3, 4], visible: false }), None);
        feed.pointer(None, None);
        assert!(feed.busy(), "the pointer's news doesn't hold up pictures, nor count as one");
        let news = feed.take_pointer();
        assert_eq!(news, PointerNews { shape: Some(shape), at: Some(PointerAt { at: [3, 4], visible: false }) });
        assert_eq!(feed.take_pointer(), PointerNews::default(), "taken once");
        // News wakes a wait at once, even with no picture.
        assert_eq!(feed.next(Duration::ZERO).map(|p| p.data), Some(vec![1]));
        let waker = feed.clone();
        let woke = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(20));
            waker.pointer(Some(PointerAt { at: [0, 0], visible: true }), None);
        });
        let start = Instant::now();
        assert_eq!(feed.next(Duration::from_secs(5)), None);
        assert!(start.elapsed() < Duration::from_secs(4), "woken by the pointer");
        woke.join().unwrap();
        assert!(feed.take_pointer().at.is_some());
    }

    #[test]
    fn a_pointer_shape_must_hold_its_pixels() {
        let shape = |size: [u32; 2], len: usize| PointerShape { size, rgba: vec![0; len] };
        assert!(shape([32, 32], 32 * 32 * 4).sound());
        assert!(!shape([32, 32], 32 * 32 * 4 - 1).sound(), "short");
        assert!(!shape([0, 32], 0).sound(), "empty");
        assert!(!shape([513, 1], 513 * 4).sound(), "too big");
    }

    #[test]
    fn the_tally_reads_as_one_line() {
        let tally = Tally {
            pictures: 300,
            skipped: 4,
            bytes: 7_500_000,
            convert: Duration::from_millis(600),
            encode: Duration::from_millis(900),
            delayed: 300,
            delay: Duration::from_millis(2400),
            longest_delay: Duration::from_millis(14),
            sent: 300,
            send: Duration::from_millis(300),
            longest_send: Duration::from_millis(21),
        };
        assert_eq!(
            tally.line(5.0),
            "extended display, last 5 s: 60 pictures a second (4 skipped while the network was busy), 12.0 Mbit/s; getting ready 2.0 ms, handing to the encoder 3.0 ms, in the encoder 8.0 ms (longest 14 ms), sending 1.0 ms (longest 21 ms)"
        );
    }

    #[test]
    fn the_feed_tally_is_unused_until_asked() {
        let feed = Feed::new();
        feed.count_encoded(Duration::from_millis(2), Duration::from_millis(3));
        feed.count_skipped();
        feed.count_sent(Duration::from_millis(9));
        feed.count_sent(Duration::from_millis(4));
        feed.count_delays(&[Duration::from_millis(6), Duration::from_millis(10)]);
        let tally = feed.take_tally();
        assert_eq!((tally.pictures, tally.skipped, tally.sent), (1, 1, 2));
        feed.note("encoder: software".into());
        assert_eq!(feed.take_notes(), ["encoder: software"]);
        assert!(feed.take_notes().is_empty());
        assert_eq!(tally.longest_send, Duration::from_millis(9));
        assert_eq!(
            (tally.delayed, tally.delay, tally.longest_delay),
            (2, Duration::from_millis(16), Duration::from_millis(10))
        );
    }
}
