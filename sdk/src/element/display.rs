//! Text, icons, images, progress, timers and other things that only show.

use super::{Element, lower_static};
use crate::protocol::{Align, Color, Icon, ImageSource, Node, Span, TextNode, TextStyle, TimerMode, Tone};

pub struct Text {
    node: TextNode,
}

pub fn text(text: impl Into<String>) -> Text {
    Text {
        node: TextNode {
            text: text.into(),
            style: TextStyle::Body,
            tone: Tone::Primary,
            wrap: false,
            align: Align::Start,
        },
    }
}

impl Text {
    pub fn style(mut self, style: TextStyle) -> Self {
        self.node.style = style;
        self
    }

    pub fn tone(mut self, tone: Tone) -> Self {
        self.node.tone = tone;
        self
    }

    pub fn wrap(mut self) -> Self {
        self.node.wrap = true;
        self
    }

    pub fn align(mut self, align: Align) -> Self {
        self.node.align = align;
        self
    }

    pub fn title(self) -> Self {
        self.style(TextStyle::Title)
    }

    pub fn body(self) -> Self {
        self.style(TextStyle::Body)
    }

    pub fn headline(self) -> Self {
        self.style(TextStyle::Headline)
    }

    pub fn callout(self) -> Self {
        self.style(TextStyle::Callout)
    }

    pub fn subheadline(self) -> Self {
        self.style(TextStyle::Subheadline)
    }

    pub fn section(self) -> Self {
        self.style(TextStyle::Section)
    }

    pub fn caption(self) -> Self {
        self.style(TextStyle::Caption)
    }

    pub fn value(self) -> Self {
        self.style(TextStyle::Value)
    }

    pub fn display(self) -> Self {
        self.style(TextStyle::Display)
    }

    pub fn hero(self) -> Self {
        self.style(TextStyle::Hero)
    }

    pub fn secondary(self) -> Self {
        self.tone(Tone::Secondary)
    }

    pub fn tertiary(self) -> Self {
        self.tone(Tone::Tertiary)
    }

    pub fn quaternary(self) -> Self {
        self.tone(Tone::Quaternary)
    }
}

impl<Msg> From<Text> for Element<Msg> {
    fn from(text: Text) -> Self {
        Element::node(Node::Text(text.node))
    }
}

pub struct IconElement {
    icon: Icon,
    size: f32,
    tone: Tone,
}

pub fn icon(icon: Icon) -> IconElement {
    IconElement { icon, size: 15.0, tone: Tone::Primary }
}

impl IconElement {
    pub fn size(mut self, size: f32) -> Self {
        self.size = size;
        self
    }

    pub fn tone(mut self, tone: Tone) -> Self {
        self.tone = tone;
        self
    }
}

impl<Msg> From<IconElement> for Element<Msg> {
    fn from(icon: IconElement) -> Self {
        Element::node(Node::Icon { icon: icon.icon, size: icon.size, tone: icon.tone })
    }
}

impl<Msg> From<Icon> for Element<Msg> {
    fn from(glyph: Icon) -> Self {
        icon(glyph).into()
    }
}

pub struct Swatch {
    color: Color,
    size: f32,
}

pub fn swatch(color: impl Into<Color>) -> Swatch {
    Swatch { color: color.into(), size: 20.0 }
}

impl Swatch {
    pub fn size(mut self, size: f32) -> Self {
        self.size = size;
        self
    }
}

impl<Msg> From<Swatch> for Element<Msg> {
    fn from(swatch: Swatch) -> Self {
        Element::node(Node::Swatch { color: swatch.color, size: swatch.size })
    }
}

pub fn dot<Msg>(color: impl Into<Color>, size: f32) -> Element<Msg> {
    Element::node(Node::Dot { color: color.into(), size })
}

pub fn badge<Msg>(icon: Icon, tone: Tone, size: f32) -> Element<Msg> {
    Element::node(Node::Badge { icon, size, tone })
}

pub struct Progress {
    value: f32,
    tone: Tone,
}

pub fn progress(value: f32) -> Progress {
    Progress { value, tone: Tone::White }
}

impl Progress {
    pub fn tone(mut self, tone: Tone) -> Self {
        self.tone = tone;
        self
    }
}

impl<Msg> From<Progress> for Element<Msg> {
    fn from(progress: Progress) -> Self {
        Element::node(Node::Progress { value: progress.value, tone: progress.tone })
    }
}

pub struct Ring {
    value: f32,
    size: f32,
    tone: Tone,
    span: Option<Span>,
    stroke: Option<f32>,
    center: Option<Node>,
}

pub fn ring(value: f32) -> Ring {
    Ring { value, size: 13.0, tone: Tone::Green, span: None, stroke: None, center: None }
}

impl Ring {
    pub fn counting_down(mut self, start: u64, end: u64) -> Self {
        self.span = Some(Span { start, end });
        self
    }

    pub fn stroke(mut self, width: f32) -> Self {
        self.stroke = Some(width);
        self
    }

    pub fn center(mut self, center: impl Into<Element<()>>) -> Self {
        self.center = Some(lower_static(center));
        self
    }

    pub fn size(mut self, size: f32) -> Self {
        self.size = size;
        self
    }

    pub fn tone(mut self, tone: Tone) -> Self {
        self.tone = tone;
        self
    }
}

impl<Msg> From<Ring> for Element<Msg> {
    fn from(ring: Ring) -> Self {
        let Ring { value, size, tone, span, stroke, center } = ring;
        Element::node(Node::Ring { value, size, tone, span, stroke, center: center.map(Box::new) })
    }
}

pub struct Image {
    source: ImageSource,
    size: [f32; 2],
    radius: f32,
}

pub fn image(source: ImageSource) -> Image {
    Image { source, size: [30.0, 20.0], radius: 5.0 }
}

impl Image {
    pub fn size(mut self, width: f32, height: f32) -> Self {
        self.size = [width, height];
        self
    }

    pub fn radius(mut self, radius: f32) -> Self {
        self.radius = radius;
        self
    }
}

impl<Msg> From<Image> for Element<Msg> {
    fn from(image: Image) -> Self {
        let [width, height] = image.size;
        Element::node(Node::Image { source: image.source, width, height, radius: image.radius })
    }
}

pub struct Timer {
    at: u64,
    mode: TimerMode,
    style: TextStyle,
    tone: Tone,
}

/// A live clock the host keeps ticking, counting up from `at` (Unix milliseconds) unless set otherwise.
pub fn timer(at: u64) -> Timer {
    Timer { at, mode: TimerMode::Elapsed, style: TextStyle::Value, tone: Tone::Primary }
}

impl Timer {
    pub fn remaining(mut self) -> Self {
        self.mode = TimerMode::Remaining;
        self
    }

    pub fn clock(mut self) -> Self {
        self.mode = TimerMode::Clock;
        self
    }

    pub fn style(mut self, style: TextStyle) -> Self {
        self.style = style;
        self
    }

    pub fn tone(mut self, tone: Tone) -> Self {
        self.tone = tone;
        self
    }
}

impl<Msg> From<Timer> for Element<Msg> {
    fn from(timer: Timer) -> Self {
        Element::node(Node::Timer { at: timer.at, mode: timer.mode, style: timer.style, tone: timer.tone })
    }
}

pub fn keycaps<Msg>(keys: impl IntoIterator<Item = impl Into<String>>) -> Element<Msg> {
    Element::node(Node::Keycaps { keys: keys.into_iter().map(Into::into).collect() })
}

/// Keycaps showing whatever keys the user gave the shortcut with this id.
pub fn shortcut<Msg>(id: impl Into<String>) -> Element<Msg> {
    Element::node(Node::Shortcut { id: id.into() })
}

/// Takes up the room left over. `space` takes a fixed amount.
pub fn spacer<Msg>() -> Element<Msg> {
    Element::node(Node::Spacer { size: None })
}

pub fn space<Msg>(size: f32) -> Element<Msg> {
    Element::node(Node::Spacer { size: Some(size) })
}

pub fn divider<Msg>() -> Element<Msg> {
    Element::node(Node::Divider)
}
