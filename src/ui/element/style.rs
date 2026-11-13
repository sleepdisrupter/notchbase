//! Maps the SDK's tones, colors, text styles and icons onto the app's own, so tools look like the rest of the notch.

use egui::{Color32, FontId};
use notchbase_sdk::protocol::{Color, Icon, TextStyle, Tone};

use crate::ui::color::{self, label, quaternary_label, secondary_label, tertiary_label};
use crate::ui::font;
use crate::ui::glyph::Glyph;

pub fn tone(tone: Tone) -> Color32 {
    match tone {
        Tone::Primary => label(),
        Tone::Secondary => secondary_label(),
        Tone::Tertiary => tertiary_label(),
        Tone::Quaternary => quaternary_label(),
        other => rgba(other.color()),
    }
}

pub fn rgba(color: Color) -> Color32 {
    Color32::from_rgba_unmultiplied(color.r, color.g, color.b, color.a)
}

pub fn ink(color: Color) -> Color32 {
    color::harmonize(rgba(color))
}

pub fn text_font(style: TextStyle) -> FontId {
    match style {
        TextStyle::Title => font::semibold(17.0),
        TextStyle::Headline => font::headline(),
        TextStyle::Body => font::body(),
        TextStyle::Callout => font::callout(),
        TextStyle::Subheadline => font::subheadline(),
        TextStyle::Section => font::section(),
        TextStyle::Caption => font::caption(),
        TextStyle::Value => font::value(),
        TextStyle::Display => font::mono(18.0),
        TextStyle::Hero => font::regular(30.0),
    }
}

pub fn accent() -> Color32 {
    color::ACCENT
}

pub fn glyph(icon: Icon) -> Glyph {
    match icon {
        Icon::Speaker => Glyph::Speaker,
        Icon::SpeakerMuted => Glyph::SpeakerMuted,
        Icon::Headphones => Glyph::Headphones,
        Icon::Display => Glyph::Display,
        Icon::Microphone => Glyph::Microphone,
        Icon::MicrophoneMuted => Glyph::MicrophoneMuted,
        Icon::Device => Glyph::Device,
        Icon::Checkmark => Glyph::Checkmark,
        Icon::ChevronDown => Glyph::ChevronDown,
        Icon::ChevronUp => Glyph::ChevronUp,
        Icon::ChevronLeft => Glyph::ChevronLeft,
        Icon::ChevronRight => Glyph::ChevronRight,
        Icon::Xmark => Glyph::Xmark,
        Icon::Plus => Glyph::Plus,
        Icon::Minus => Glyph::Minus,
        Icon::Tray => Glyph::Tray,
        Icon::Folder => Glyph::Folder,
        Icon::Exclamation => Glyph::Exclamation,
        Icon::Convert => Glyph::Convert,
        Icon::Compress => Glyph::Compress,
        Icon::Combine => Glyph::Combine,
        Icon::Split => Glyph::Split,
        Icon::Gear => Glyph::Gear,
        Icon::Note => Glyph::Note,
        Icon::Play => Glyph::Play,
        Icon::Pause => Glyph::Pause,
        Icon::Stop => Glyph::Stop,
        Icon::Record => Glyph::Record,
        Icon::Backward => Glyph::Backward,
        Icon::Forward => Glyph::Forward,
        Icon::Keyboard => Glyph::Keyboard,
        Icon::Mouse => Glyph::Mouse,
        Icon::Gamepad => Glyph::Gamepad,
        Icon::Phone => Glyph::Phone,
        Icon::Watch => Glyph::Watch,
        Icon::Bluetooth => Glyph::Bluetooth,
        Icon::Pipette => Glyph::Pipette,
        Icon::Viewfinder => Glyph::Viewfinder,
        Icon::Clock => Glyph::Clock,
        Icon::Bell => Glyph::Bell,
        Icon::Star => Glyph::Star,
        Icon::Grid => Glyph::Grid,
        Icon::Gauge => Glyph::Gauge,
        Icon::Timer => Glyph::Timer,
        Icon::Calendar => Glyph::Calendar,
        Icon::Video => Glyph::Video,
        Icon::Cloud => Glyph::Cloud,
        Icon::Link => Glyph::Link,
        Icon::Spark => Glyph::Spark,
        Icon::Bolt => Glyph::Bolt,
        Icon::Battery => Glyph::Battery,
        Icon::Lock => Glyph::Lock,
        Icon::Unlock => Glyph::Unlock,
        Icon::Download => Glyph::Download,
        Icon::Cup => Glyph::Cup,
        Icon::Moon => Glyph::Moon,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notch_and_tools_share_a_palette() {
        let pairs = [
            (Tone::Blue, color::BLUE),
            (Tone::Green, color::GREEN),
            (Tone::Orange, color::ORANGE),
            (Tone::Red, color::RED),
            (Tone::Purple, color::PURPLE),
            (Tone::Teal, color::TEAL),
            (Tone::Yellow, color::YELLOW),
            (Tone::Gray, color::GRAY),
        ];
        for (named, host) in pairs {
            assert_eq!(tone(named), host, "{named:?}");
        }
    }
}
