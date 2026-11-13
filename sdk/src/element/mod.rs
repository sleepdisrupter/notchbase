//! Elements: the views a tool builds.
//!
//! An `Element` holds closures for its events. Before it goes to the host it is lowered into a plain
//! `Node`, and each closure is swapped for a handler id. A press comes back as that id and is
//! matched to its message.

mod calendar;
mod canvas;
mod controls;
mod display;
mod handler;
mod layout;
mod palette;

pub use calendar::{Month, Week, month, week};
pub use canvas::{Canvas, canvas};
pub use controls::{
    Button, Checkbox, Chip, Dropdown, Picker, RoundButton, Segmented, Slider, TextInput, Toggle, button, checkbox,
    chip, dropdown, picker, round_button, segmented, slider, text_input, toggle, wheel,
};
pub use display::{
    IconElement, Image, Progress, Ring, Swatch, Text, Timer, badge, divider, dot, icon, image, keycaps, progress, ring,
    shortcut, space, spacer, swatch, text, timer,
};
pub(crate) use handler::{Handler, Handlers};
pub use layout::{Column, Pan, Row, Sizing, adaptive, card, column, keyed, list, pan, row};
pub use palette::{Palette, palette};

use std::rc::Rc;

use crate::protocol::{
    Align, ButtonStyle, CanvasNode, Choice, Icon, MonthNode, Node, PickerColumn, Pointer, Tone, WeekNode,
};

pub struct Element<Msg> {
    kind: Kind<Msg>,
}

type Map<T, Msg> = Box<dyn Fn(T) -> Msg>;
type PickMap<Msg> = Box<dyn Fn(usize, usize) -> Msg>;

enum Kind<Msg> {
    Node(Node),
    Row {
        children: Vec<Element<Msg>>,
        spacing: Option<f32>,
        align: Align,
    },
    Column {
        children: Vec<Element<Msg>>,
        spacing: Option<f32>,
        align: Align,
    },
    Frame {
        child: Box<Element<Msg>>,
        width: Option<f32>,
        height: Option<f32>,
        align: Align,
    },
    Card {
        child: Box<Element<Msg>>,
        padding: Option<f32>,
    },
    List {
        children: Vec<Element<Msg>>,
    },
    Adaptive {
        notch: Box<Element<Msg>>,
        island: Box<Element<Msg>>,
    },
    Keyed {
        key: String,
        child: Box<Element<Msg>>,
    },
    Pan {
        child: Box<Element<Msg>>,
        offset: f32,
        on_pan: Option<Map<f32, Msg>>,
        on_tap: Option<Map<f32, Msg>>,
    },
    Button {
        label: String,
        icon: Option<Icon>,
        style: ButtonStyle,
        on_press: Option<Msg>,
    },
    Toggle {
        on: bool,
        on_toggle: Option<Map<bool, Msg>>,
    },
    Checkbox {
        label: String,
        checked: bool,
        on_toggle: Option<Map<bool, Msg>>,
    },
    Chip {
        label: String,
        icon: Option<Icon>,
        icon_off: Option<Icon>,
        on: bool,
        on_toggle: Option<Map<bool, Msg>>,
    },
    Dropdown {
        options: Vec<Choice>,
        selected: Option<usize>,
        placeholder: String,
        on_select: Option<Map<usize, Msg>>,
        compact: bool,
    },
    Segmented {
        options: Vec<String>,
        selected: usize,
        on_select: Option<Map<usize, Msg>>,
    },
    Slider {
        value: f32,
        min: f32,
        max: f32,
        step: Option<f32>,
        icon: Option<Icon>,
        on_change: Option<Map<f32, Msg>>,
    },
    TextInput {
        value: String,
        placeholder: String,
        on_change: Option<Map<String, Msg>>,
        on_submit: Option<Msg>,
    },
    RoundButton {
        label: String,
        tone: Tone,
        size: f32,
        on_press: Option<Msg>,
    },
    Picker {
        columns: Vec<PickerColumn>,
        on_change: Option<PickMap<Msg>>,
    },
    Month {
        node: MonthNode,
        on_select: Option<Map<usize, Msg>>,
        on_scroll: Option<Map<f32, Msg>>,
    },
    Week {
        node: WeekNode,
        on_day: Option<Map<usize, Msg>>,
        on_event: Option<Map<String, Msg>>,
        on_scroll: Option<Map<f32, Msg>>,
    },
    Palette {
        colors: Vec<crate::protocol::Color>,
        selected: Option<usize>,
        on_select: Option<Map<usize, Msg>>,
    },
    Canvas {
        node: CanvasNode,
        on_pointer: Option<Map<Pointer, Msg>>,
        on_scroll: Option<Map<Pointer, Msg>>,
    },
}

impl<Msg> Element<Msg> {
    fn new(kind: Kind<Msg>) -> Self {
        Self { kind }
    }

    pub fn node(node: Node) -> Self {
        Self::new(Kind::Node(node))
    }

    pub(crate) fn lower(self, handlers: &mut Handlers<Msg>) -> Node {
        let mut lower_all = |children: Vec<Element<Msg>>| -> Vec<Node> {
            children.into_iter().map(|child| child.lower(handlers)).collect()
        };
        match self.kind {
            Kind::Node(node) => node,
            Kind::Row { children, spacing, align } => Node::Row { children: lower_all(children), spacing, align },
            Kind::Column { children, spacing, align } => Node::Column { children: lower_all(children), spacing, align },
            Kind::List { children } => Node::List { children: lower_all(children) },
            Kind::Frame { child, width, height, align } => {
                Node::Frame { child: Box::new(child.lower(handlers)), width, height, align }
            }
            Kind::Card { child, padding } => Node::Card { child: Box::new(child.lower(handlers)), padding },
            Kind::Adaptive { notch, island } => {
                Node::Adaptive { notch: Box::new(notch.lower(handlers)), island: Box::new(island.lower(handlers)) }
            }
            Kind::Keyed { key, child } => Node::Keyed { key, child: Box::new(child.lower(handlers)) },
            Kind::Pan { child, offset, on_pan, on_tap } => Node::Pan {
                child: Box::new(child.lower(handlers)),
                offset,
                on_pan: on_pan.map(|map| handlers.add(Handler::Number(map))),
                on_tap: on_tap.map(|map| handlers.add(Handler::Number(map))),
            },
            Kind::Button { label, icon, style, on_press } => {
                Node::Button { label, icon, style, on_press: on_press.map(|msg| handlers.add(Handler::Press(msg))) }
            }
            Kind::Toggle { on, on_toggle } => {
                Node::Toggle { on, on_toggle: on_toggle.map(|map| handlers.add(Handler::Bool(map))) }
            }
            Kind::Checkbox { label, checked, on_toggle } => {
                Node::Checkbox { label, checked, on_toggle: on_toggle.map(|map| handlers.add(Handler::Bool(map))) }
            }
            Kind::Chip { label, icon, icon_off, on, on_toggle } => Node::Chip {
                label,
                icon,
                icon_off,
                on,
                on_toggle: on_toggle.map(|map| handlers.add(Handler::Bool(map))),
            },
            Kind::Dropdown { options, selected, placeholder, on_select, compact } => Node::Dropdown {
                options,
                selected,
                placeholder,
                on_select: on_select.map(|map| handlers.add(Handler::Index(map))),
                compact,
            },
            Kind::Segmented { options, selected, on_select } => {
                Node::Segmented { options, selected, on_select: on_select.map(|map| handlers.add(Handler::Index(map))) }
            }
            Kind::Slider { value, min, max, step, icon, on_change } => Node::Slider {
                value,
                min,
                max,
                step,
                icon,
                on_change: on_change.map(|map| handlers.add(Handler::Number(map))),
            },
            Kind::TextInput { value, placeholder, on_change, on_submit } => Node::TextInput {
                value,
                placeholder,
                on_change: on_change.map(|map| handlers.add(Handler::Text(map))),
                on_submit: on_submit.map(|msg| handlers.add(Handler::Press(msg))),
            },
            Kind::RoundButton { label, tone, size, on_press } => {
                Node::RoundButton { label, tone, size, on_press: on_press.map(|msg| handlers.add(Handler::Press(msg))) }
            }
            Kind::Picker { columns, on_change } => {
                Node::Picker { columns, on_change: on_change.map(|map| handlers.add(Handler::Picked(map))) }
            }
            Kind::Month { node, on_select, on_scroll } => Node::Month(MonthNode {
                on_select: on_select.map(|map| handlers.add(Handler::Index(map))),
                on_scroll: on_scroll.map(|map| handlers.add(Handler::Number(map))),
                ..node
            }),
            Kind::Week { node, on_day, on_event, on_scroll } => Node::Week(WeekNode {
                on_day: on_day.map(|map| handlers.add(Handler::Index(map))),
                on_event: on_event.map(|map| handlers.add(Handler::Text(map))),
                on_scroll: on_scroll.map(|map| handlers.add(Handler::Number(map))),
                ..node
            }),
            Kind::Palette { colors, selected, on_select } => {
                Node::Palette { colors, selected, on_select: on_select.map(|map| handlers.add(Handler::Index(map))) }
            }
            Kind::Canvas { node, on_pointer, on_scroll } => Node::Canvas(CanvasNode {
                on_pointer: on_pointer.map(|map| handlers.add(Handler::Pointer(map))),
                on_scroll: on_scroll.map(|map| handlers.add(Handler::Pointer(map))),
                ..node
            }),
        }
    }
}

impl<Msg: 'static> Element<Msg> {
    pub fn map<Outer: 'static>(self, map: impl Fn(Msg) -> Outer + 'static) -> Element<Outer> {
        let map: Rc<dyn Fn(Msg) -> Outer> = Rc::new(map);
        self.map_with(&map)
    }

    fn map_with<Outer: 'static>(self, map: &Rc<dyn Fn(Msg) -> Outer>) -> Element<Outer> {
        let all = |children: Vec<Element<Msg>>| children.into_iter().map(|child| child.map_with(map)).collect();
        let one = |child: Box<Element<Msg>>| Box::new(child.map_with(map));
        let through = |inner: Option<Map<bool, Msg>>| inner.map(|inner| compose(inner, map));
        Element::new(match self.kind {
            Kind::Node(node) => Kind::Node(node),
            Kind::Row { children, spacing, align } => Kind::Row { children: all(children), spacing, align },
            Kind::Column { children, spacing, align } => Kind::Column { children: all(children), spacing, align },
            Kind::Frame { child, width, height, align } => Kind::Frame { child: one(child), width, height, align },
            Kind::Card { child, padding } => Kind::Card { child: one(child), padding },
            Kind::List { children } => Kind::List { children: all(children) },
            Kind::Adaptive { notch, island } => Kind::Adaptive { notch: one(notch), island: one(island) },
            Kind::Keyed { key, child } => Kind::Keyed { key, child: one(child) },
            Kind::Pan { child, offset, on_pan, on_tap } => Kind::Pan {
                child: one(child),
                offset,
                on_pan: on_pan.map(|inner| compose(inner, map)),
                on_tap: on_tap.map(|inner| compose(inner, map)),
            },
            Kind::Button { label, icon, style, on_press } => {
                Kind::Button { label, icon, style, on_press: on_press.map(|msg| map(msg)) }
            }
            Kind::Toggle { on, on_toggle } => Kind::Toggle { on, on_toggle: through(on_toggle) },
            Kind::Checkbox { label, checked, on_toggle } => {
                Kind::Checkbox { label, checked, on_toggle: through(on_toggle) }
            }
            Kind::Chip { label, icon, icon_off, on, on_toggle } => {
                Kind::Chip { label, icon, icon_off, on, on_toggle: through(on_toggle) }
            }
            Kind::Dropdown { options, selected, placeholder, on_select, compact } => Kind::Dropdown {
                options,
                selected,
                placeholder,
                on_select: on_select.map(|inner| compose(inner, map)),
                compact,
            },
            Kind::Segmented { options, selected, on_select } => {
                Kind::Segmented { options, selected, on_select: on_select.map(|inner| compose(inner, map)) }
            }
            Kind::Slider { value, min, max, step, icon, on_change } => {
                Kind::Slider { value, min, max, step, icon, on_change: on_change.map(|inner| compose(inner, map)) }
            }
            Kind::TextInput { value, placeholder, on_change, on_submit } => Kind::TextInput {
                value,
                placeholder,
                on_change: on_change.map(|inner| compose(inner, map)),
                on_submit: on_submit.map(|msg| map(msg)),
            },
            Kind::RoundButton { label, tone, size, on_press } => {
                Kind::RoundButton { label, tone, size, on_press: on_press.map(|msg| map(msg)) }
            }
            Kind::Picker { columns, on_change } => Kind::Picker {
                columns,
                on_change: on_change.map(|inner| {
                    let map = map.clone();
                    Box::new(move |column, index| map(inner(column, index))) as PickMap<Outer>
                }),
            },
            Kind::Month { node, on_select, on_scroll } => Kind::Month {
                node,
                on_select: on_select.map(|inner| compose(inner, map)),
                on_scroll: on_scroll.map(|inner| compose(inner, map)),
            },
            Kind::Week { node, on_day, on_event, on_scroll } => Kind::Week {
                node,
                on_day: on_day.map(|inner| compose(inner, map)),
                on_event: on_event.map(|inner| compose(inner, map)),
                on_scroll: on_scroll.map(|inner| compose(inner, map)),
            },
            Kind::Palette { colors, selected, on_select } => {
                Kind::Palette { colors, selected, on_select: on_select.map(|inner| compose(inner, map)) }
            }
            Kind::Canvas { node, on_pointer, on_scroll } => Kind::Canvas {
                node,
                on_pointer: on_pointer.map(|inner| compose(inner, map)),
                on_scroll: on_scroll.map(|inner| compose(inner, map)),
            },
        })
    }
}

fn compose<T: 'static, Msg: 'static, Outer: 'static>(
    inner: Map<T, Msg>,
    map: &Rc<dyn Fn(Msg) -> Outer>,
) -> Map<T, Outer> {
    let map = map.clone();
    Box::new(move |value| map(inner(value)))
}

impl<Msg> From<Node> for Element<Msg> {
    fn from(node: Node) -> Self {
        Element::node(node)
    }
}

/// Lowers an element that sends no messages, as notices do.
pub fn lower_static(element: impl Into<Element<()>>) -> Node {
    element.into().lower(&mut Handlers::default())
}
