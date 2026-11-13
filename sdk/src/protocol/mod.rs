//! The JSON protocol between notchbase and its tools. Everything here crosses the plugin boundary,
//! so changes must stay readable by older hosts and tools.

mod canvas;
mod event;
mod manifest;
mod node;
mod request;
mod session;
mod style;
mod values;

pub use canvas::{Anchor, CanvasNode, Paint, Pointer, PointerKind, Shape, Stroke, Weight};
pub use event::{
    AudioInput, CalendarEvent, CalendarFeed, CalendarInfo, Capture, Device, DeviceChange, Download, DownloadChange,
    Event, Focus, NotchPhase, NotchState, NotchStyle, NowPlaying, Power, RecordingState, RecordingUpdate, Reply,
    Sensors, Surface, UiValue, VolumeLevel,
};
pub use manifest::{ChoiceOption, Manifest, SettingKind, SettingSpec, ShortcutSpec};
pub use node::{
    Choice, DayMark, HandlerId, ImageId, ImageSource, LiveNode, MonthNode, Node, NoticeNode, PickerColumn, Span,
    TextNode, TileNode, WeekDay, WeekEvent, WeekNode,
};
pub use request::{EventDraft, HttpRequest, HttpResponse, ImageFormat, MediaControl, RecordingOptions, Request};
pub use session::{Answer, Call, Dirs, Effects, PROTOCOL_VERSION, Platform, Start, Views, Wants};
pub use style::{Align, ButtonStyle, Color, Icon, NoticeSize, TextStyle, TimerMode, Tone};
pub use values::{Map, Value, Values};
