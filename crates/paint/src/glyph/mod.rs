//! Hand-drawn line icons. Each one is drawn in a unit box around its center and scaled to `size`.

mod actions;
mod audio;
mod chrome;
mod devices;
mod edit;
mod formats;
mod misc;
mod pen;
mod system;
#[cfg(test)]
mod tests;
mod time;

use egui::{Color32, Pos2};

use crate::canvas::Canvas;
use pen::Pen;

pub use chrome::{checkmark, chevron};
pub use misc::SPARKLES;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Glyph {
    Speaker,
    SpeakerMuted,
    Sun,
    Headphones,
    Display,
    Microphone,
    MicrophoneMuted,
    Device,
    Checkmark,
    ChevronDown,
    ChevronUp,
    ChevronLeft,
    Xmark,
    Tray,
    Exclamation,
    Convert,
    Compress,
    Combine,
    Split,
    Gear,
    Note,
    Play,
    Pause,
    Backward,
    Forward,
    Keyboard,
    Mouse,
    Gamepad,
    Phone,
    Watch,
    Bluetooth,
    ChevronRight,
    Plus,
    Minus,
    Folder,
    Stop,
    Record,
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
    AirDrop,
    Bolt,
    Battery,
    Lock,
    Unlock,
    Download,
    Cup,
    Moon,
    Photo,
    Camera,
    Globe,
    Layers,
    Frames,
    AppIcon,
    Document,
    Film,
    Waveform,
    Bars,
    Undo,
    Redo,
    Search,
    Brush,
    Text,
    Pin,
    Unpin,
    Shield,
}

pub fn chevron_turning(canvas: &mut Canvas, center: Pos2, size: f32, color: Color32, up: f32) {
    chrome::turning(canvas, &Pen::new(canvas, center, size, color), up);
}

pub fn paint(canvas: &mut Canvas, g: Glyph, center: Pos2, size: f32, color: Color32) {
    let pen = Pen::new(canvas, center, size, color);
    match g {
        Glyph::Speaker
        | Glyph::SpeakerMuted
        | Glyph::Headphones
        | Glyph::Microphone
        | Glyph::MicrophoneMuted
        | Glyph::Note
        | Glyph::Play
        | Glyph::Pause
        | Glyph::Backward
        | Glyph::Forward => audio::paint(canvas, g, &pen),
        Glyph::Display
        | Glyph::Device
        | Glyph::Keyboard
        | Glyph::Mouse
        | Glyph::Gamepad
        | Glyph::Phone
        | Glyph::Watch
        | Glyph::Bluetooth => devices::paint(canvas, g, &pen),
        Glyph::Checkmark
        | Glyph::ChevronDown
        | Glyph::ChevronUp
        | Glyph::ChevronLeft
        | Glyph::ChevronRight
        | Glyph::Xmark
        | Glyph::Exclamation
        | Glyph::Plus
        | Glyph::Minus => chrome::paint(canvas, g, &pen),
        Glyph::Tray
        | Glyph::Convert
        | Glyph::Compress
        | Glyph::Combine
        | Glyph::Split
        | Glyph::Gear
        | Glyph::Folder => actions::paint(canvas, g, &pen),
        Glyph::Clock | Glyph::Bell | Glyph::Timer | Glyph::Calendar => time::paint(canvas, g, &pen),
        Glyph::Bolt
        | Glyph::Battery
        | Glyph::Lock
        | Glyph::Unlock
        | Glyph::Download
        | Glyph::Cup
        | Glyph::Moon
        | Glyph::Search
        | Glyph::Shield => system::paint(canvas, g, &pen),
        Glyph::Photo
        | Glyph::Camera
        | Glyph::Globe
        | Glyph::Layers
        | Glyph::Frames
        | Glyph::AppIcon
        | Glyph::Document
        | Glyph::Film
        | Glyph::Waveform
        | Glyph::Bars => formats::paint(canvas, g, &pen),
        Glyph::Undo | Glyph::Redo | Glyph::Brush | Glyph::Text => edit::paint(canvas, g, &pen),
        Glyph::Sun
        | Glyph::Stop
        | Glyph::Record
        | Glyph::Pipette
        | Glyph::Viewfinder
        | Glyph::Star
        | Glyph::Grid
        | Glyph::Video
        | Glyph::Cloud
        | Glyph::Link
        | Glyph::Gauge
        | Glyph::AirDrop
        | Glyph::Spark
        | Glyph::Pin
        | Glyph::Unpin => misc::paint(canvas, g, &pen),
    }
}
