//! The computer's own mouse, keyboard, screens and clipboard: listening to them on the
//! controlling computer, playing input back on the controlled one.

use std::path::PathBuf;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use crate::layout::Rect;

#[cfg(target_os = "macos")]
mod mac;
#[cfg(target_os = "macos")]
mod mac_drag;
#[cfg(windows)]
mod windows;
#[cfg(windows)]
mod windows_drag;

/// Something the person did on this computer's own mouse or keyboard.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Input {
    /// The cursor moved: `at` is where it is now, `delta` how far the mouse moved (both in this
    /// computer's units). While captured, `at` stays put and only `delta` matters.
    Move {
        at: [f64; 2],
        delta: [f64; 2],
    },
    Button {
        button: u8,
        down: bool,
    },
    /// 120 to a wheel notch, positive is up and right.
    Wheel {
        dx: f32,
        dy: f32,
    },
    Key {
        hid: u16,
        down: bool,
    },
    /// A trackpad swipe the system would act on itself, such as Mission Control.
    Swipe(Swipe),
}

/// The system's own trackpad swipes, by what they do on a Mac.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Swipe {
    /// Mission Control.
    Up,
    /// App Exposé.
    Down,
    /// To the space on the left.
    Left,
    /// To the space on the right.
    Right,
}

/// Input to play back as if it came from this computer's own mouse and keyboard.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Play {
    Move([f64; 2]),
    Button { button: u8, down: bool },
    Wheel { dx: f32, dy: f32 },
    Key { hid: u16, down: bool },
}

pub type Sink = Box<dyn Fn(Input) + Send + Sync>;

pub trait Platform: Send {
    /// The displays (in this computer's units) and how many units make a point.
    fn screens(&mut self) -> (Vec<Rect>, f64);
    /// Starts hearing this computer's own mouse and keyboard.
    fn listen(&mut self, sink: Sink) -> Result<(), String>;
    fn stop_listening(&mut self);
    /// While captured, the mouse and keyboard reach only Roam: the cursor is hidden and still.
    fn capture(&mut self, on: bool);
    /// Puts this computer's cursor somewhere.
    fn warp(&mut self, at: [f64; 2]);
    fn play(&mut self, play: Play);
    /// What's missing for Roam to work, in a sentence, if anything.
    fn missing_permission(&mut self) -> Option<String> {
        None
    }
    /// Asks the system for the permission `missing_permission` names.
    fn ask_permission(&mut self) {}
    /// Tunes the network and timers for low latency while a session runs (Windows turns off
    /// Wi-Fi background scans, as game streaming apps do).
    fn fast(&mut self, _on: bool) {}
    /// Keeps the Wi-Fi radio on the network while `on`: called every few seconds while a
    /// session runs with Fast Wi-Fi switched on, and once with `false` when it ends. On a Mac
    /// this keeps AirDrop's link off, which otherwise takes the radio away twice a second.
    fn steady_wifi(&mut self, _on: bool) {}
    /// Roam is still running: called often while the cursor is away. If these stop, the
    /// mouse and keyboard free themselves within a second, so a hang never locks you out.
    fn alive(&mut self) {}
    /// Whether the capture still holds (it lets go by itself if Roam stopped answering).
    fn still_captured(&mut self) -> bool {
        true
    }
    /// Another computer's cursor is arriving here: make sure the cursor can be seen.
    fn arrive(&mut self) {}
    /// Another computer's cursor went back to it: hide the cursor here, as a screen with no
    /// cursor on it shows none, until it comes back or this computer's own mouse moves.
    fn depart(&mut self) {}
    /// Show the cursor again if `depart` hid it.
    fn reveal(&mut self) {}
    /// Called often: a hidden cursor comes back when this computer's own mouse moves.
    fn tick(&mut self) {}
    /// The left button went down or up here, pressed by the person or played: a drag may start.
    fn left_button(&mut self, _down: bool) {}
    /// The files being dragged here now, while the left button is held and the cursor is held
    /// against the edge of the screen at `at`. Windows only learns them by putting a window
    /// under the cursor for the drag to pass over, so the first asks may answer `None`.
    fn dragged_files(&mut self, _at: [f64; 2]) -> Option<Vec<PathBuf>> {
        None
    }
    /// Ends the drag here without dropping anything: the files went across instead.
    fn cancel_drag(&mut self) {}
    /// Picks `paths` up under the cursor, as if they were dragged from there: the left button
    /// goes down here, and letting it go drops them. `played` when the button is played by
    /// another computer, rather than held on this one's own mouse.
    fn drag_in(&mut self, _paths: Vec<PathBuf>, _played: bool) -> bool {
        false
    }
}

/// Lets a captured mouse and keyboard go by themselves when Roam stops checking in.
pub struct Watchdog(AtomicU64);

/// How long Roam may stay silent before the capture lets go.
const PATIENCE_MS: u64 = 1_000;

fn millis() -> u64 {
    static START: OnceLock<Instant> = OnceLock::new();
    START.get_or_init(Instant::now).elapsed().as_millis() as u64 + 1
}

impl Watchdog {
    pub const fn new() -> Self {
        Self(AtomicU64::new(0))
    }

    pub fn pet(&self) {
        self.0.store(millis(), Ordering::Release);
    }

    pub fn stale(&self) -> bool {
        millis().saturating_sub(self.0.load(Ordering::Acquire)) > PATIENCE_MS
    }
}

impl Default for Watchdog {
    fn default() -> Self {
        Self::new()
    }
}

/// The platform this computer runs.
pub fn native() -> Box<dyn Platform> {
    #[cfg(target_os = "macos")]
    return Box::new(mac::Mac::new());
    #[cfg(windows)]
    return Box::new(windows::Windows::new());
    #[cfg(not(any(target_os = "macos", windows)))]
    return Box::new(Unsupported);
}

/// Gives the calling thread the priority of input handling, so it runs the moment a packet or
/// event is waiting.
pub fn urgent_thread() {
    #[cfg(target_os = "macos")]
    mac::urgent_thread();
    #[cfg(windows)]
    windows::urgent_thread();
}

/// A computer Roam can't drive: it can be seen but not controlled.
pub struct Unsupported;

impl Platform for Unsupported {
    fn screens(&mut self) -> (Vec<Rect>, f64) {
        (vec![Rect::new(0.0, 0.0, 1920.0, 1080.0)], 1.0)
    }

    fn listen(&mut self, _sink: Sink) -> Result<(), String> {
        Err("Roam works on macOS and Windows".into())
    }

    fn stop_listening(&mut self) {}
    fn capture(&mut self, _on: bool) {}
    fn warp(&mut self, _at: [f64; 2]) {}
    fn play(&mut self, _play: Play) {}

    fn missing_permission(&mut self) -> Option<String> {
        Some("Roam works on macOS and Windows".into())
    }
}

/// What is on the clipboard, of the kinds Roam shares.
#[derive(Clone, Debug, PartialEq)]
pub enum Copied {
    Text(String),
    /// Straight RGBA, 8 bits a channel.
    Image {
        width: usize,
        height: usize,
        rgba: Vec<u8>,
    },
    /// Files and folders copied in Finder or Explorer.
    Files(Vec<PathBuf>),
}

impl Copied {
    /// Tells one clipboard from another, so what came over isn't sent straight back.
    pub fn digest(&self) -> [u8; 32] {
        match self {
            Copied::Text(text) => crate::crypto::hash(&[text.as_bytes()]),
            Copied::Image { width, height, rgba } => {
                crate::crypto::hash(&[b"image", &width.to_le_bytes(), &height.to_le_bytes(), rgba])
            }
            Copied::Files(paths) => {
                let names: Vec<String> = paths.iter().map(|p| p.to_string_lossy().into_owned()).collect();
                crate::crypto::hash(&[b"files", names.join("\n").as_bytes()])
            }
        }
    }
}

/// The clipboard: copied files first (Finder also puts their names there as text), then text,
/// then an image. An image wins over text that is only a link: "Copy Image" in a browser puts
/// the picture's address there too. Text from a document comes with a picture of itself, and
/// that text wins.
pub fn clipboard() -> Option<Copied> {
    let mut clipboard = arboard::Clipboard::new().ok()?;
    if let Ok(files) = clipboard.get().file_list()
        && !files.is_empty()
    {
        return Some(Copied::Files(files));
    }
    let text = clipboard.get_text().ok().filter(|text| !text.is_empty());
    if let Some(text) = &text
        && !just_a_link(text)
    {
        return Some(Copied::Text(text.clone()));
    }
    let image = clipboard
        .get_image()
        .ok()
        .filter(|image| image.width > 0 && image.height > 0 && image.bytes.len() == image.width * image.height * 4);
    match image {
        Some(image) => Some(Copied::Image { width: image.width, height: image.height, rgba: image.bytes.into_owned() }),
        None => text.map(Copied::Text),
    }
}

fn just_a_link(text: &str) -> bool {
    let text = text.trim();
    !text.contains(char::is_whitespace)
        && ["http://", "https://", "data:", "blob:", "file:"].iter().any(|start| text.starts_with(start))
}

/// Puts `copied` on the clipboard. What the clipboard then holds, as `digest` sees it: the
/// system may change an image a little on the way in.
pub fn set_clipboard(copied: &Copied) -> Option<[u8; 32]> {
    let mut board = arboard::Clipboard::new().ok()?;
    match copied {
        Copied::Text(text) => board.set_text(text.as_str()).ok()?,
        Copied::Image { width, height, rgba } => board
            .set_image(arboard::ImageData {
                width: *width,
                height: *height,
                bytes: std::borrow::Cow::Borrowed(rgba.as_slice()),
            })
            .ok()?,
        Copied::Files(paths) => board.set().file_list(paths).ok()?,
    }
    drop(board);
    Some(clipboard().map_or_else(|| copied.digest(), |now| now.digest()))
}

/// An image as a PNG, quickly made: screenshots are big.
pub fn png(width: usize, height: usize, rgba: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(&mut out, u32::try_from(width).ok()?, u32::try_from(height).ok()?);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.set_compression(png::Compression::Fast);
    let mut writer = encoder.write_header().ok()?;
    writer.write_image_data(rgba).ok()?;
    writer.finish().ok()?;
    Some(out)
}

/// A PNG made by `png`, back as RGBA.
pub fn unpng(bytes: &[u8]) -> Option<Copied> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::EXPAND);
    let mut reader = decoder.read_info().ok()?;
    let (width, height) = (reader.info().width as usize, reader.info().height as usize);
    if width.checked_mul(height)? > 1 << 28 {
        return None;
    }
    let mut rgba = vec![0; reader.output_buffer_size()?];
    let frame = reader.next_frame(&mut rgba).ok()?;
    if frame.color_type != png::ColorType::Rgba || frame.bit_depth != png::BitDepth::Eight {
        return None;
    }
    rgba.truncate(frame.buffer_size());
    (rgba.len() == width * height * 4).then_some(Copied::Image { width, height, rgba })
}

/// A platform for tests: records what it was asked to do.
#[cfg(test)]
mod tests {
    #[test]
    fn a_link_alone_gives_way_to_an_image() {
        assert!(super::just_a_link("https://example.com/cat.png"));
        assert!(super::just_a_link("  data:image/png;base64,AAAA\n"));
        assert!(!super::just_a_link("see https://example.com/cat.png"));
        assert!(!super::just_a_link("Quarterly plan"));
    }
}

#[cfg(test)]
pub mod fake {
    use std::sync::{Arc, Mutex};

    use super::*;

    #[derive(Default)]
    pub struct State {
        pub screens: Vec<Rect>,
        pub ppu: f64,
        pub sink: Option<Sink>,
        pub captured: bool,
        pub warped: Vec<[f64; 2]>,
        pub played: Vec<Play>,
        /// What a drag under way here carries.
        pub dragging: Option<Vec<PathBuf>>,
        pub cancelled: u32,
        /// Files picked up here, and whether the button was played.
        pub picked: Vec<(Vec<PathBuf>, bool)>,
    }

    #[derive(Clone, Default)]
    pub struct Fake(pub Arc<Mutex<State>>);

    impl Fake {
        pub fn new(screens: Vec<Rect>, ppu: f64) -> Self {
            Self(Arc::new(Mutex::new(State { screens, ppu, ..State::default() })))
        }

        /// The person moves the mouse or types.
        pub fn input(&self, input: Input) -> bool {
            let state = self.0.lock().unwrap();
            match &state.sink {
                Some(sink) => {
                    sink(input);
                    true
                }
                None => false,
            }
        }

        pub fn state(&self) -> std::sync::MutexGuard<'_, State> {
            self.0.lock().unwrap()
        }
    }

    impl Platform for Fake {
        fn screens(&mut self) -> (Vec<Rect>, f64) {
            let state = self.state();
            (state.screens.clone(), state.ppu)
        }
        fn listen(&mut self, sink: Sink) -> Result<(), String> {
            self.state().sink = Some(sink);
            Ok(())
        }
        fn stop_listening(&mut self) {
            self.state().sink = None;
        }
        fn capture(&mut self, on: bool) {
            self.state().captured = on;
        }
        fn warp(&mut self, at: [f64; 2]) {
            self.state().warped.push(at);
        }
        fn play(&mut self, play: Play) {
            self.state().played.push(play);
        }
        fn dragged_files(&mut self, _at: [f64; 2]) -> Option<Vec<PathBuf>> {
            self.state().dragging.clone()
        }
        fn cancel_drag(&mut self) {
            let mut state = self.state();
            state.cancelled += 1;
            state.dragging = None;
        }
        fn drag_in(&mut self, paths: Vec<PathBuf>, played: bool) -> bool {
            self.state().picked.push((paths, played));
            true
        }
    }
}
