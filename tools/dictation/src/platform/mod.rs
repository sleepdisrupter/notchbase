//! What differs between macOS and Windows: hearing the dictation key, the microphone, reading
//! the text around the cursor, typing into the focused app, and the little sounds.

#[cfg(target_os = "macos")]
mod macos;
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
mod other;
#[cfg(target_os = "windows")]
mod windows;

#[cfg(target_os = "macos")]
use macos as imp;
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
use other as imp;
#[cfg(target_os = "windows")]
use windows as imp;

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use crate::trigger::Key;

/// The key that starts dictation.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Hotkey {
    #[cfg_attr(not(windows), default)]
    Fn,
    RightOption,
    RightCommand,
    #[cfg_attr(windows, default)]
    CtrlWin,
    RightCtrl,
    RightAlt,
}

impl Hotkey {
    pub const CHOICES: [(&str, &str); 3] = if cfg!(windows) {
        [("ctrl_win", "Ctrl + Win"), ("right_ctrl", "Right Ctrl"), ("right_alt", "Right Alt")]
    } else {
        [("fn", "fn 🌐"), ("right_option", "Right ⌥"), ("right_command", "Right ⌘")]
    };

    pub fn named(name: &str) -> Self {
        match name {
            "fn" => Hotkey::Fn,
            "right_option" => Hotkey::RightOption,
            "right_command" => Hotkey::RightCommand,
            "ctrl_win" => Hotkey::CtrlWin,
            "right_ctrl" => Hotkey::RightCtrl,
            "right_alt" => Hotkey::RightAlt,
            _ => Hotkey::default(),
        }
    }

    /// How it's written in the notch.
    pub fn caps(self) -> &'static [&'static str] {
        match self {
            Hotkey::Fn => &["fn"],
            Hotkey::RightOption => &["right ⌥"],
            Hotkey::RightCommand => &["right ⌘"],
            Hotkey::CtrlWin => &["Ctrl", "Win"],
            Hotkey::RightCtrl => &["Right Ctrl"],
            Hotkey::RightAlt => &["Right Alt"],
        }
    }
}

/// Something that keeps dictation from working, and where it's fixed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Problem {
    pub says: String,
    /// The settings page that fixes it, and the button that opens it.
    pub fix: Option<String>,
    pub button: &'static str,
    /// Whether dictation can't work at all until it's fixed.
    pub blocks: bool,
}

/// Where the cursor is when dictation starts.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Surroundings {
    pub app: String,
    /// The text before and after the cursor in the focused field, when the system lets it be read.
    pub before: Option<String>,
    pub after: Option<String>,
    /// The window's title and other text on screen, for picking out terms.
    pub visible: String,
}

/// Words that were typed, and how to read the field again to see what the user changed.
pub struct Typed {
    pub reread: Option<Box<dyn FnMut() -> Option<String> + Send>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cue {
    Start,
    Stop,
}

/// Where the microphone's samples go: 16 kHz mono.
#[derive(Clone, Default)]
pub struct Tape {
    samples: Arc<Mutex<Vec<i16>>>,
    /// The loudest recent level, 0 to 1, as bits.
    level: Arc<AtomicU32>,
}

impl Tape {
    pub fn push(&self, samples: &[i16]) {
        self.samples.lock().unwrap_or_else(PoisonError::into_inner).extend_from_slice(samples);
    }

    pub fn count(&self) -> usize {
        self.samples.lock().unwrap_or_else(PoisonError::into_inner).len()
    }

    pub fn copy(&self, from: usize, to: usize) -> Vec<i16> {
        let samples = self.samples.lock().unwrap_or_else(PoisonError::into_inner);
        let to = to.min(samples.len());
        samples.get(from.min(to)..to).map(<[i16]>::to_vec).unwrap_or_default()
    }

    pub fn all(&self) -> Vec<i16> {
        self.samples.lock().unwrap_or_else(PoisonError::into_inner).clone()
    }

    pub fn tail(&self, count: usize) -> Vec<i16> {
        let samples = self.samples.lock().unwrap_or_else(PoisonError::into_inner);
        samples[samples.len().saturating_sub(count)..].to_vec()
    }

    pub fn set_level(&self, level: f32) {
        self.level.store(level.to_bits(), Ordering::Relaxed);
    }

    pub fn level(&self) -> f32 {
        f32::from_bits(self.level.load(Ordering::Relaxed))
    }
}

/// A running microphone. Dropping it stops it.
pub trait Recording: Send {}

pub type KeySink = Arc<dyn Fn(Key) + Send + Sync>;

pub trait Platform: Send + 'static {
    /// Starts hearing `hotkey`, replacing the key heard before. `listening` says when Escape
    /// should be kept from the focused app.
    fn watch(&mut self, hotkey: Hotkey, keys: KeySink, listening: Arc<AtomicBool>) -> Result<(), Problem>;
    /// Whether the dictation key is down right now, when the system can say.
    fn hotkey_down(&self, hotkey: Hotkey) -> Option<bool>;
    fn listen(&mut self, tape: Tape) -> Result<Box<dyn Recording>, String>;
    fn surroundings(&self, read_text: bool) -> Surroundings;
    fn type_text(&self, text: &str, reread: bool) -> Result<Typed, String>;
    fn cue(&self, cue: Cue);
    /// What's in the way right now, such as a missing permission.
    fn problem(&self, hotkey: Hotkey) -> Option<Problem>;
}

pub fn native() -> Box<dyn Platform> {
    Box::<imp::Native>::default()
}
