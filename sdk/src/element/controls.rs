//! Buttons, toggles, sliders, pickers and the other controls.

use super::{Element, Kind, Map, PickMap};
use crate::protocol::{ButtonStyle, Choice, Color, Icon, PickerColumn, Tone};

pub struct Button<Msg> {
    label: String,
    icon: Option<Icon>,
    style: ButtonStyle,
    on_press: Option<Msg>,
    enabled: bool,
}

pub fn button<Msg>(label: impl Into<String>) -> Button<Msg> {
    Button { label: label.into(), icon: None, style: ButtonStyle::Glass, on_press: None, enabled: true }
}

impl<Msg> Button<Msg> {
    pub fn icon(mut self, icon: Icon) -> Self {
        self.icon = Some(icon);
        self
    }

    pub fn style(mut self, style: ButtonStyle) -> Self {
        self.style = style;
        self
    }

    pub fn filled(self) -> Self {
        self.style(ButtonStyle::Filled)
    }

    pub fn plain(self) -> Self {
        self.style(ButtonStyle::Plain)
    }

    pub fn destructive(self) -> Self {
        self.style(ButtonStyle::Destructive)
    }

    pub fn tinted(self, color: impl Into<Color>) -> Self {
        self.style(ButtonStyle::Tinted(color.into()))
    }

    pub fn on_press(mut self, msg: Msg) -> Self {
        self.on_press = Some(msg);
        self
    }

    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }
}

impl<Msg> From<Button<Msg>> for Element<Msg> {
    fn from(button: Button<Msg>) -> Self {
        let Button { label, icon, style, on_press, enabled } = button;
        Element::new(Kind::Button { label, icon, style, on_press: on_press.filter(|_| enabled) })
    }
}

pub struct Toggle<Msg> {
    on: bool,
    on_toggle: Option<Map<bool, Msg>>,
}

pub fn toggle<Msg>(on: bool) -> Toggle<Msg> {
    Toggle { on, on_toggle: None }
}

impl<Msg> Toggle<Msg> {
    pub fn on_toggle(mut self, map: impl Fn(bool) -> Msg + 'static) -> Self {
        self.on_toggle = Some(Box::new(map));
        self
    }
}

impl<Msg> From<Toggle<Msg>> for Element<Msg> {
    fn from(toggle: Toggle<Msg>) -> Self {
        Element::new(Kind::Toggle { on: toggle.on, on_toggle: toggle.on_toggle })
    }
}

pub struct Checkbox<Msg> {
    label: String,
    checked: bool,
    on_toggle: Option<Map<bool, Msg>>,
}

pub fn checkbox<Msg>(label: impl Into<String>, checked: bool) -> Checkbox<Msg> {
    Checkbox { label: label.into(), checked, on_toggle: None }
}

impl<Msg> Checkbox<Msg> {
    pub fn on_toggle(mut self, map: impl Fn(bool) -> Msg + 'static) -> Self {
        self.on_toggle = Some(Box::new(map));
        self
    }
}

impl<Msg> From<Checkbox<Msg>> for Element<Msg> {
    fn from(checkbox: Checkbox<Msg>) -> Self {
        let Checkbox { label, checked, on_toggle } = checkbox;
        Element::new(Kind::Checkbox { label, checked, on_toggle })
    }
}

pub struct Chip<Msg> {
    label: String,
    icon: Option<Icon>,
    icon_off: Option<Icon>,
    on: bool,
    on_toggle: Option<Map<bool, Msg>>,
}

pub fn chip<Msg>(label: impl Into<String>, on: bool) -> Chip<Msg> {
    Chip { label: label.into(), icon: None, icon_off: None, on, on_toggle: None }
}

impl<Msg> Chip<Msg> {
    pub fn icon(mut self, icon: Icon) -> Self {
        self.icon = Some(icon);
        self
    }

    pub fn icons(mut self, on: Icon, off: Icon) -> Self {
        self.icon = Some(on);
        self.icon_off = Some(off);
        self
    }

    pub fn on_toggle(mut self, map: impl Fn(bool) -> Msg + 'static) -> Self {
        self.on_toggle = Some(Box::new(map));
        self
    }
}

impl<Msg> From<Chip<Msg>> for Element<Msg> {
    fn from(chip: Chip<Msg>) -> Self {
        let Chip { label, icon, icon_off, on, on_toggle } = chip;
        Element::new(Kind::Chip { label, icon, icon_off, on, on_toggle })
    }
}

impl From<&str> for Choice {
    fn from(label: &str) -> Self {
        Choice { label: label.to_owned(), icon: None }
    }
}

impl From<String> for Choice {
    fn from(label: String) -> Self {
        Choice { label, icon: None }
    }
}

impl<T: Into<String>> From<(Icon, T)> for Choice {
    fn from((icon, label): (Icon, T)) -> Self {
        Choice { label: label.into(), icon: Some(icon) }
    }
}

pub struct Dropdown<Msg> {
    options: Vec<Choice>,
    selected: Option<usize>,
    placeholder: String,
    on_select: Option<Map<usize, Msg>>,
    compact: bool,
}

pub fn dropdown<Msg>(options: impl IntoIterator<Item = impl Into<Choice>>, selected: Option<usize>) -> Dropdown<Msg> {
    Dropdown {
        options: options.into_iter().map(Into::into).collect(),
        selected,
        placeholder: String::new(),
        on_select: None,
        compact: false,
    }
}

impl<Msg> Dropdown<Msg> {
    pub fn placeholder(mut self, placeholder: impl Into<String>) -> Self {
        self.placeholder = placeholder.into();
        self
    }

    pub fn on_select(mut self, map: impl Fn(usize) -> Msg + 'static) -> Self {
        self.on_select = Some(Box::new(map));
        self
    }

    pub fn compact(mut self) -> Self {
        self.compact = true;
        self
    }
}

impl<Msg> From<Dropdown<Msg>> for Element<Msg> {
    fn from(dropdown: Dropdown<Msg>) -> Self {
        let Dropdown { options, selected, placeholder, on_select, compact } = dropdown;
        Element::new(Kind::Dropdown { options, selected, placeholder, on_select, compact })
    }
}

pub struct Segmented<Msg> {
    options: Vec<String>,
    selected: usize,
    on_select: Option<Map<usize, Msg>>,
}

pub fn segmented<Msg>(options: impl IntoIterator<Item = impl Into<String>>, selected: usize) -> Segmented<Msg> {
    Segmented { options: options.into_iter().map(Into::into).collect(), selected, on_select: None }
}

impl<Msg> Segmented<Msg> {
    pub fn on_select(mut self, map: impl Fn(usize) -> Msg + 'static) -> Self {
        self.on_select = Some(Box::new(map));
        self
    }
}

impl<Msg> From<Segmented<Msg>> for Element<Msg> {
    fn from(segmented: Segmented<Msg>) -> Self {
        let Segmented { options, selected, on_select } = segmented;
        Element::new(Kind::Segmented { options, selected, on_select })
    }
}

pub struct Slider<Msg> {
    value: f32,
    range: [f32; 2],
    step: Option<f32>,
    icon: Option<Icon>,
    on_change: Option<Map<f32, Msg>>,
}

pub fn slider<Msg>(value: f32) -> Slider<Msg> {
    Slider { value, range: [0.0, 1.0], step: None, icon: None, on_change: None }
}

impl<Msg> Slider<Msg> {
    pub fn range(mut self, min: f32, max: f32) -> Self {
        self.range = [min, max];
        self
    }

    pub fn step(mut self, step: f32) -> Self {
        self.step = Some(step);
        self
    }

    pub fn icon(mut self, icon: Icon) -> Self {
        self.icon = Some(icon);
        self
    }

    pub fn on_change(mut self, map: impl Fn(f32) -> Msg + 'static) -> Self {
        self.on_change = Some(Box::new(map));
        self
    }
}

impl<Msg> From<Slider<Msg>> for Element<Msg> {
    fn from(slider: Slider<Msg>) -> Self {
        let Slider { value, range: [min, max], step, icon, on_change } = slider;
        Element::new(Kind::Slider { value, min, max, step, icon, on_change })
    }
}

pub struct TextInput<Msg> {
    value: String,
    placeholder: String,
    on_change: Option<Map<String, Msg>>,
    on_submit: Option<Msg>,
}

pub fn text_input<Msg>(value: impl Into<String>) -> TextInput<Msg> {
    TextInput { value: value.into(), placeholder: String::new(), on_change: None, on_submit: None }
}

impl<Msg> TextInput<Msg> {
    pub fn placeholder(mut self, placeholder: impl Into<String>) -> Self {
        self.placeholder = placeholder.into();
        self
    }

    pub fn on_change(mut self, map: impl Fn(String) -> Msg + 'static) -> Self {
        self.on_change = Some(Box::new(map));
        self
    }

    pub fn on_submit(mut self, msg: Msg) -> Self {
        self.on_submit = Some(msg);
        self
    }
}

impl<Msg> From<TextInput<Msg>> for Element<Msg> {
    fn from(input: TextInput<Msg>) -> Self {
        let TextInput { value, placeholder, on_change, on_submit } = input;
        Element::new(Kind::TextInput { value, placeholder, on_change, on_submit })
    }
}

pub struct RoundButton<Msg> {
    label: String,
    tone: Tone,
    size: f32,
    on_press: Option<Msg>,
    enabled: bool,
}

pub fn round_button<Msg>(label: impl Into<String>) -> RoundButton<Msg> {
    RoundButton { label: label.into(), tone: Tone::Gray, size: 64.0, on_press: None, enabled: true }
}

impl<Msg> RoundButton<Msg> {
    pub fn tone(mut self, tone: Tone) -> Self {
        self.tone = tone;
        self
    }

    pub fn size(mut self, size: f32) -> Self {
        self.size = size;
        self
    }

    pub fn on_press(mut self, msg: Msg) -> Self {
        self.on_press = Some(msg);
        self
    }

    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }
}

impl<Msg> From<RoundButton<Msg>> for Element<Msg> {
    fn from(button: RoundButton<Msg>) -> Self {
        let RoundButton { label, tone, size, on_press, enabled } = button;
        Element::new(Kind::RoundButton { label, tone, size, on_press: on_press.filter(|_| enabled) })
    }
}

/// One spinning column for a `picker`.
pub fn wheel(options: impl IntoIterator<Item = impl Into<String>>, selected: usize) -> PickerColumn {
    PickerColumn {
        options: options.into_iter().map(Into::into).collect(),
        selected,
        unit: String::new(),
        looping: false,
    }
}

impl PickerColumn {
    pub fn unit(mut self, unit: impl Into<String>) -> Self {
        self.unit = unit.into();
        self
    }

    pub fn looping(mut self) -> Self {
        self.looping = true;
        self
    }
}

pub struct Picker<Msg> {
    columns: Vec<PickerColumn>,
    on_change: Option<PickMap<Msg>>,
}

pub fn picker<Msg>(columns: impl IntoIterator<Item = PickerColumn>) -> Picker<Msg> {
    Picker { columns: columns.into_iter().collect(), on_change: None }
}

impl<Msg> Picker<Msg> {
    pub fn on_change(mut self, map: impl Fn(usize, usize) -> Msg + 'static) -> Self {
        self.on_change = Some(Box::new(map));
        self
    }
}

impl<Msg> From<Picker<Msg>> for Element<Msg> {
    fn from(picker: Picker<Msg>) -> Self {
        Element::new(Kind::Picker { columns: picker.columns, on_change: picker.on_change })
    }
}
