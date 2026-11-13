//! The SDK notchbase tools are written against.
//!
//! A tool works like a small Elm app: a model, messages and an `update` function. It describes its
//! tile, page and live activity as data, and asks the host for things by returning commands. Most
//! tools only need `use notchbase_sdk::prelude::*;`. See KIT.md for a walkthrough.

mod command;
pub mod draw;
mod driver;
pub mod element;
pub mod ffi;
pub mod protocol;
mod runtime;
mod settings;
mod subscription;
mod surface;
pub mod testing;
mod time;
mod tool;

#[cfg(test)]
mod tests;

pub use command::Command;
pub use driver::{Driver, Emitter, Waker};
pub use element::Element;
pub use runtime::{Runtime, boxed};
pub use settings::Settings;
pub use subscription::{Subscription, widest};
pub use surface::{Live, Notice, Tile};
pub use time::{clock, now_ms, unix_ms};
pub use tool::{Context, Tool};

pub mod prelude {
    pub use crate::element::{
        Element, Sizing, adaptive, badge, button, canvas, card, checkbox, chip, column, divider, dot, dropdown, icon,
        image, keycaps, keyed, list, month, palette, pan, picker, progress, ring, round_button, row, segmented,
        shortcut, slider, space, spacer, swatch, text, text_input, timer, toggle, week, wheel,
    };
    pub use crate::protocol::{
        Align, Anchor, AudioInput, ButtonStyle, CalendarEvent, CalendarFeed, CalendarInfo, Capture, Color,
        DeviceChange, Download, DownloadChange, EventDraft, Focus, HttpRequest, HttpResponse, Icon, ImageFormat,
        ImageId, ImageSource, Manifest, MediaControl, NotchPhase, NotchState, NotchStyle, NoticeSize, NowPlaying,
        Paint, Platform, Pointer, PointerKind, Power, RecordingOptions, RecordingState, RecordingUpdate, Sensors,
        SettingSpec, Shape, TextStyle, Tone, Value, VolumeLevel, WeekDay, WeekEvent, Weight,
    };
    pub use crate::{Command, Context, Emitter, Live, Notice, Settings, Subscription, Tile, Tool};
    pub use crate::{clock, draw, now_ms, row, unix_ms};
}
