//! Where a tool shows up outside its page: the tile, the live activity and notices.

use std::time::Duration;

use crate::element::{Element, Handlers, lower_static};
use crate::protocol::{Icon, LiveNode, Node, NoticeNode, NoticeSize, TileNode, Tone};

/// The tool's tile on the Tools page.
pub struct Tile<Msg> {
    title: String,
    icon: Element<Msg>,
    well: Option<Tone>,
    tint: Option<Tone>,
    wide: bool,
    enabled: bool,
    on_press: Option<Msg>,
    shortcut: Option<String>,
    accessory: Option<Element<Msg>>,
    footer: Option<Element<Msg>>,
}

impl<Msg> Tile<Msg> {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            icon: glyph(Icon::Star, Tone::Blue),
            well: None,
            tint: None,
            wide: false,
            enabled: true,
            on_press: None,
            shortcut: None,
            accessory: None,
            footer: None,
        }
    }

    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = title.into();
        self
    }

    pub fn icon(mut self, icon: Icon) -> Self {
        self.icon = glyph(icon, Tone::Blue);
        self
    }

    pub fn symbol(mut self, symbol: impl Into<Element<Msg>>) -> Self {
        self.icon = symbol.into();
        self
    }

    pub fn well(mut self, tone: Tone) -> Self {
        self.well = Some(tone);
        self
    }

    pub fn tint(mut self, tone: Tone) -> Self {
        self.tint = Some(tone);
        self
    }

    pub fn wide(mut self) -> Self {
        self.wide = true;
        self
    }

    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    pub fn on_press(mut self, msg: Msg) -> Self {
        self.on_press = Some(msg);
        self
    }

    pub fn shortcut(mut self, id: impl Into<String>) -> Self {
        self.shortcut = Some(id.into());
        self
    }

    pub fn accessory(mut self, accessory: impl Into<Element<Msg>>) -> Self {
        self.accessory = Some(accessory.into());
        self
    }

    pub fn footer(mut self, footer: impl Into<Element<Msg>>) -> Self {
        self.footer = Some(footer.into());
        self
    }

    pub(crate) fn lower(self, handlers: &mut Handlers<Msg>) -> TileNode {
        TileNode {
            title: self.title,
            icon: self.icon.lower(handlers),
            well: self.well,
            tint: self.tint,
            wide: self.wide,
            enabled: self.enabled,
            on_press: self.on_press.map(|msg| handlers.add(crate::element::Handler::Press(msg))),
            shortcut: self.shortcut,
            accessory: self.accessory.map(|element| element.lower(handlers)),
            footer: self.footer.map(|element| element.lower(handlers)),
        }
    }
}

fn glyph<Msg>(icon: Icon, tone: Tone) -> Element<Msg> {
    Element::node(Node::Icon { icon, size: 15.0, tone })
}

/// A live activity in the closed notch, with content on both sides and optional controls.
pub struct Live<Msg> {
    leading: Element<Msg>,
    trailing: Element<Msg>,
    controls: Option<Element<Msg>>,
}

impl<Msg> Live<Msg> {
    pub fn new(leading: impl Into<Element<Msg>>, trailing: impl Into<Element<Msg>>) -> Self {
        Self { leading: leading.into(), trailing: trailing.into(), controls: None }
    }

    pub fn controls(mut self, controls: impl Into<Element<Msg>>) -> Self {
        self.controls = Some(controls.into());
        self
    }

    pub(crate) fn lower(self, handlers: &mut Handlers<Msg>) -> LiveNode {
        LiveNode {
            leading: self.leading.lower(handlers),
            trailing: self.trailing.lower(handlers),
            controls: self.controls.map(|controls| controls.lower(handlers)),
        }
    }
}

/// A short-lived message in the notch. It lasts 2.4 seconds unless told otherwise.
pub struct Notice {
    node: NoticeNode,
}

impl Notice {
    pub fn new(leading: impl Into<Element<()>>, trailing: impl Into<Element<()>>) -> Self {
        Self {
            node: NoticeNode {
                leading: lower_static(leading),
                trailing: lower_static(trailing),
                below: None,
                banner: false,
                duration_ms: 2400,
                pop: false,
                shake: false,
                size: NoticeSize::Medium,
                controls: None,
            },
        }
    }

    pub fn below(mut self, below: impl Into<Element<()>>) -> Self {
        self.node.below = Some(lower_static(below));
        self
    }

    pub fn banner(mut self) -> Self {
        self.node.banner = true;
        self
    }

    pub fn duration(mut self, duration: Duration) -> Self {
        self.node.duration_ms = duration.as_millis().clamp(1, u128::from(u64::MAX)) as u64;
        self
    }

    pub fn pop(mut self) -> Self {
        self.node.pop = true;
        self
    }

    pub fn shake(mut self) -> Self {
        self.node.shake = true;
        self.node.pop = true;
        self
    }

    /// Keeps the notice up until it's dismissed.
    pub fn sticky(mut self) -> Self {
        self.node.duration_ms = 0;
        self
    }

    pub fn size(mut self, size: NoticeSize) -> Self {
        self.node.size = size;
        self
    }

    pub fn into_node(self) -> NoticeNode {
        self.node
    }
}
