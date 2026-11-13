//! Lowered views as they travel to the host.

use serde::{Deserialize, Serialize};

use super::canvas::CanvasNode;
use super::style::{Align, ButtonStyle, Color, Icon, NoticeSize, TextStyle, TimerMode, Tone};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct HandlerId(pub u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ImageId(pub u64);

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImageSource {
    Capture(ImageId),
    File(String),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Choice {
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<Icon>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TextNode {
    pub text: String,
    #[serde(default)]
    pub style: TextStyle,
    #[serde(default)]
    pub tone: Tone,
    #[serde(default)]
    pub wrap: bool,
    #[serde(default = "start")]
    pub align: Align,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Span {
    pub start: u64,
    pub end: u64,
}

impl Span {
    pub fn left_at(&self, now_ms: u64) -> f32 {
        let length = self.end.saturating_sub(self.start);
        if length == 0 {
            return 0.0;
        }
        (self.end.saturating_sub(now_ms) as f64 / length as f64).clamp(0.0, 1.0) as f32
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PickerColumn {
    pub options: Vec<String>,
    pub selected: usize,
    #[serde(default)]
    pub unit: String,
    #[serde(default)]
    pub looping: bool,
}

fn start() -> Align {
    Align::Start
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct MonthNode {
    pub year: i32,
    pub month: u8,
    #[serde(default)]
    pub today: Option<u8>,
    #[serde(default)]
    pub selected: Option<u8>,
    #[serde(default)]
    pub marks: Vec<DayMark>,
    #[serde(default)]
    pub monday_first: bool,
    #[serde(default)]
    pub on_select: Option<HandlerId>,
    #[serde(default)]
    pub on_scroll: Option<HandlerId>,
}

impl MonthNode {
    pub fn pack(year: i32, month: u8, day: u8) -> usize {
        year.max(0) as usize * 10_000 + usize::from(month) * 100 + usize::from(day)
    }

    pub fn unpack(value: usize) -> (i32, u8, u8) {
        ((value / 10_000) as i32, (value / 100 % 100) as u8, (value % 100) as u8)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct WeekNode {
    pub days: Vec<WeekDay>,
    #[serde(default)]
    pub on_day: Option<HandlerId>,
    #[serde(default)]
    pub on_event: Option<HandlerId>,
    #[serde(default)]
    pub on_scroll: Option<HandlerId>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct WeekDay {
    pub year: i32,
    pub month: u8,
    pub day: u8,
    pub weekday: String,
    #[serde(default)]
    pub today: bool,
    #[serde(default)]
    pub selected: bool,
    #[serde(default)]
    pub events: Vec<WeekEvent>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct WeekEvent {
    pub id: String,
    #[serde(default)]
    pub time: String,
    pub title: String,
    #[serde(default)]
    pub color: Option<Color>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct DayMark {
    pub day: u8,
    pub colors: Vec<Color>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Node {
    Row {
        children: Vec<Node>,
        #[serde(default)]
        spacing: Option<f32>,
        #[serde(default)]
        align: Align,
    },
    Column {
        children: Vec<Node>,
        #[serde(default)]
        spacing: Option<f32>,
        #[serde(default = "start")]
        align: Align,
    },
    Frame {
        child: Box<Node>,
        #[serde(default)]
        width: Option<f32>,
        #[serde(default)]
        height: Option<f32>,
        #[serde(default)]
        align: Align,
    },
    Card {
        child: Box<Node>,
        #[serde(default)]
        padding: Option<f32>,
    },
    List {
        children: Vec<Node>,
    },
    Adaptive {
        notch: Box<Node>,
        island: Box<Node>,
    },
    Keyed {
        key: String,
        child: Box<Node>,
    },
    Pan {
        child: Box<Node>,
        #[serde(default)]
        offset: f32,
        #[serde(default)]
        on_pan: Option<HandlerId>,
        #[serde(default)]
        on_tap: Option<HandlerId>,
    },
    Spacer {
        #[serde(default)]
        size: Option<f32>,
    },
    Divider,
    Text(TextNode),
    Icon {
        icon: Icon,
        size: f32,
        #[serde(default)]
        tone: Tone,
    },
    Button {
        label: String,
        #[serde(default)]
        icon: Option<Icon>,
        #[serde(default)]
        style: ButtonStyle,
        #[serde(default)]
        on_press: Option<HandlerId>,
    },
    Toggle {
        on: bool,
        #[serde(default)]
        on_toggle: Option<HandlerId>,
    },
    Checkbox {
        label: String,
        checked: bool,
        #[serde(default)]
        on_toggle: Option<HandlerId>,
    },
    Chip {
        label: String,
        #[serde(default)]
        icon: Option<Icon>,
        #[serde(default)]
        icon_off: Option<Icon>,
        on: bool,
        #[serde(default)]
        on_toggle: Option<HandlerId>,
    },
    Dropdown {
        options: Vec<Choice>,
        #[serde(default)]
        selected: Option<usize>,
        #[serde(default)]
        placeholder: String,
        #[serde(default)]
        on_select: Option<HandlerId>,
        #[serde(default)]
        compact: bool,
    },
    Segmented {
        options: Vec<String>,
        selected: usize,
        #[serde(default)]
        on_select: Option<HandlerId>,
    },
    Slider {
        value: f32,
        min: f32,
        max: f32,
        #[serde(default)]
        step: Option<f32>,
        #[serde(default)]
        icon: Option<Icon>,
        #[serde(default)]
        on_change: Option<HandlerId>,
    },
    TextInput {
        value: String,
        #[serde(default)]
        placeholder: String,
        #[serde(default)]
        on_change: Option<HandlerId>,
        #[serde(default)]
        on_submit: Option<HandlerId>,
    },
    Shortcut {
        id: String,
    },
    Keycaps {
        keys: Vec<String>,
    },
    Progress {
        value: f32,
        #[serde(default)]
        tone: Tone,
    },
    Ring {
        value: f32,
        size: f32,
        #[serde(default)]
        tone: Tone,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        span: Option<Span>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        stroke: Option<f32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        center: Option<Box<Node>>,
    },
    Picker {
        columns: Vec<PickerColumn>,
        #[serde(default)]
        on_change: Option<HandlerId>,
    },
    RoundButton {
        label: String,
        #[serde(default)]
        tone: Tone,
        size: f32,
        #[serde(default)]
        on_press: Option<HandlerId>,
    },
    Swatch {
        color: Color,
        size: f32,
    },
    Dot {
        color: Color,
        size: f32,
    },
    Badge {
        icon: Icon,
        size: f32,
        #[serde(default)]
        tone: Tone,
    },
    Image {
        source: ImageSource,
        width: f32,
        height: f32,
        #[serde(default)]
        radius: f32,
    },
    Timer {
        at: u64,
        #[serde(default)]
        mode: TimerMode,
        #[serde(default)]
        style: TextStyle,
        #[serde(default)]
        tone: Tone,
    },
    Month(MonthNode),
    Week(WeekNode),
    Canvas(CanvasNode),
    Palette {
        colors: Vec<Color>,
        #[serde(default)]
        selected: Option<usize>,
        #[serde(default)]
        on_select: Option<HandlerId>,
    },
}

impl Node {
    pub fn children(&self) -> Vec<&Node> {
        match self {
            Node::Row { children, .. } | Node::Column { children, .. } | Node::List { children } => {
                children.iter().collect()
            }
            Node::Frame { child, .. }
            | Node::Card { child, .. }
            | Node::Keyed { child, .. }
            | Node::Pan { child, .. } => vec![child],
            Node::Adaptive { notch, island } => vec![notch, island],
            Node::Ring { center: Some(center), .. } => vec![center],
            _ => Vec::new(),
        }
    }

    pub fn any(&self, test: &impl Fn(&Node) -> bool) -> bool {
        test(self) || self.children().into_iter().any(|child| child.any(test))
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TileNode {
    pub title: String,
    pub icon: Node,
    #[serde(default)]
    pub well: Option<Tone>,
    #[serde(default)]
    pub tint: Option<Tone>,
    #[serde(default)]
    pub wide: bool,
    #[serde(default = "enabled")]
    pub enabled: bool,
    #[serde(default)]
    pub on_press: Option<HandlerId>,
    #[serde(default)]
    pub shortcut: Option<String>,
    #[serde(default)]
    pub accessory: Option<Node>,
    #[serde(default)]
    pub footer: Option<Node>,
}

fn enabled() -> bool {
    true
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LiveNode {
    pub leading: Node,
    pub trailing: Node,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub controls: Option<Node>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct NoticeNode {
    pub leading: Node,
    pub trailing: Node,
    #[serde(default)]
    pub below: Option<Node>,
    #[serde(default)]
    pub banner: bool,
    pub duration_ms: u64,
    #[serde(default)]
    pub pop: bool,
    #[serde(default)]
    pub shake: bool,
    #[serde(default)]
    pub size: NoticeSize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub controls: Option<Node>,
}

impl NoticeNode {
    pub fn sticky(&self) -> bool {
        self.duration_ms == 0
    }
}
