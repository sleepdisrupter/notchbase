//! Colors, tones, icons and text styles.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Tone {
    #[default]
    Primary,
    Secondary,
    Tertiary,
    Quaternary,
    White,
    Blue,
    Green,
    Orange,
    Red,
    Purple,
    Teal,
    Yellow,
    Pink,
    Gray,
}

impl Tone {
    pub const fn color(self) -> Color {
        match self {
            Tone::Primary => Color::white(0.85),
            Tone::Secondary => Color::white(0.55),
            Tone::Tertiary => Color::white(0.25),
            Tone::Quaternary => Color::white(0.10),
            Tone::White => Color::rgb(244, 243, 240),
            Tone::Blue => Color::rgb(244, 243, 240),
            Tone::Green => Color::rgb(63, 196, 138),
            Tone::Orange => Color::rgb(242, 193, 78),
            Tone::Red => Color::rgb(236, 84, 90),
            Tone::Purple => Color::rgb(244, 243, 240),
            Tone::Teal => Color::rgb(63, 196, 138),
            Tone::Yellow => Color::rgb(242, 193, 78),
            Tone::Pink => Color::rgb(236, 84, 90),
            Tone::Gray => Color::rgb(140, 139, 136),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }

    pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    pub const fn white(alpha: f32) -> Self {
        let a = if alpha <= 0.0 {
            0
        } else if alpha >= 1.0 {
            255
        } else {
            (alpha * 255.0 + 0.5) as u8
        };
        Self { r: 255, g: 255, b: 255, a }
    }

    pub fn hex(self) -> String {
        format!("#{:02X}{:02X}{:02X}", self.r, self.g, self.b)
    }
}

impl From<Tone> for Color {
    fn from(tone: Tone) -> Self {
        tone.color()
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextStyle {
    Title,
    Headline,
    #[default]
    Body,
    Callout,
    Subheadline,
    Section,
    Caption,
    Value,
    Display,
    Hero,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Align {
    Start,
    #[default]
    Center,
    End,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ButtonStyle {
    #[default]
    Glass,
    Filled,
    Plain,
    Destructive,
    Tinted(Color),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NoticeSize {
    Small,
    #[default]
    Medium,
    Large,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TimerMode {
    #[default]
    Elapsed,
    Remaining,
    Clock,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Icon {
    Speaker,
    SpeakerMuted,
    Headphones,
    Display,
    Microphone,
    MicrophoneMuted,
    Device,
    Checkmark,
    ChevronDown,
    ChevronUp,
    ChevronLeft,
    ChevronRight,
    Xmark,
    Plus,
    Minus,
    Tray,
    Folder,
    Exclamation,
    Convert,
    Compress,
    Combine,
    Split,
    Gear,
    Note,
    Play,
    Pause,
    Stop,
    Record,
    Backward,
    Forward,
    Keyboard,
    Mouse,
    Gamepad,
    Phone,
    Watch,
    Bluetooth,
    Pipette,
    Viewfinder,
    Clock,
    Bell,
    Star,
    Grid,
    Gauge,
    Timer,
    Calendar,
    Video,
    Cloud,
    Link,
    Spark,
    Bolt,
    Battery,
    Lock,
    Unlock,
    Download,
    Cup,
    Moon,
}
