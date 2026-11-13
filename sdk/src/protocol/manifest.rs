//! The manifest: a tool's id, name, page, shortcuts and settings.

use serde::{Deserialize, Serialize};

use super::session::PROTOCOL_VERSION;
use super::style::Icon;
use super::values::{Value, Values};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Manifest {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub page: Option<String>,
    #[serde(default)]
    pub page_icon: Option<Icon>,
    #[serde(default = "tabbed")]
    pub tab: bool,
    #[serde(default)]
    pub header_tab: bool,
    #[serde(default)]
    pub background: bool,
    #[serde(default)]
    pub shortcuts: Vec<ShortcutSpec>,
    #[serde(default)]
    pub settings: Vec<SettingSpec>,
    #[serde(default = "first_protocol")]
    pub protocol: u32,
}

fn tabbed() -> bool {
    true
}

fn first_protocol() -> u32 {
    1
}

impl Manifest {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            description: String::new(),
            version: String::new(),
            page: None,
            page_icon: None,
            tab: true,
            header_tab: false,
            background: false,
            shortcuts: Vec::new(),
            settings: Vec::new(),
            protocol: PROTOCOL_VERSION,
        }
    }

    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = description.into();
        self
    }

    pub fn version(mut self, version: impl Into<String>) -> Self {
        self.version = version.into();
        self
    }

    pub fn page(mut self, title: impl Into<String>) -> Self {
        self.page = Some(title.into());
        self
    }

    #[deprecated(note = "tool pages always open from the Tools tab, so this does nothing")]
    pub fn no_tab(mut self) -> Self {
        self.tab = false;
        self
    }

    pub fn header_tab(mut self) -> Self {
        self.header_tab = true;
        self
    }

    pub fn background(mut self) -> Self {
        self.background = true;
        self
    }

    pub fn page_icon(mut self, icon: Icon) -> Self {
        self.page_icon = Some(icon);
        self
    }

    pub fn shortcut(mut self, id: impl Into<String>, title: impl Into<String>, default: Option<&str>) -> Self {
        self.shortcuts.push(ShortcutSpec { id: id.into(), title: title.into(), default: default.map(str::to_owned) });
        self
    }

    pub fn setting(mut self, setting: SettingSpec) -> Self {
        self.settings.push(setting);
        self
    }

    pub fn defaults(&self) -> Values {
        self.settings.iter().map(|setting| (setting.key.clone(), setting.default.clone())).collect()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShortcutSpec {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub default: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ChoiceOption {
    pub value: String,
    pub label: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SettingKind {
    Toggle,
    Choice {
        options: Vec<ChoiceOption>,
    },
    Number {
        min: f64,
        max: f64,
        step: f64,
        unit: String,
    },
    Text {
        placeholder: String,
        #[serde(default)]
        secret: bool,
    },
    Folder {
        placeholder: String,
    },
    Data,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SettingSpec {
    pub key: String,
    pub title: String,
    #[serde(default)]
    pub detail: String,
    pub kind: SettingKind,
    pub default: Value,
    #[serde(default = "shown")]
    pub visible: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shown_when: Option<String>,
}

fn shown() -> bool {
    true
}

impl SettingSpec {
    fn new(key: impl Into<String>, title: impl Into<String>, kind: SettingKind, default: Value) -> Self {
        let (key, title) = (key.into(), title.into());
        Self { key, title, detail: String::new(), kind, default, visible: true, shown_when: None }
    }

    pub fn toggle(key: impl Into<String>, title: impl Into<String>, default: bool) -> Self {
        Self::new(key, title, SettingKind::Toggle, Value::Bool(default))
    }

    pub fn choice<'a>(
        key: impl Into<String>,
        title: impl Into<String>,
        options: impl IntoIterator<Item = (&'a str, &'a str)>,
        default: &str,
    ) -> Self {
        let options = options
            .into_iter()
            .map(|(value, label)| ChoiceOption { value: value.into(), label: label.into() })
            .collect();
        Self::new(key, title, SettingKind::Choice { options }, Value::String(default.into()))
    }

    pub fn number(key: impl Into<String>, title: impl Into<String>, range: [f64; 2], step: f64, default: f64) -> Self {
        let kind = SettingKind::Number { min: range[0], max: range[1], step, unit: String::new() };
        let default = if default.fract() == 0.0 && default.abs() < 9.0e15 {
            Value::from(default as i64)
        } else {
            Value::from(default)
        };
        Self::new(key, title, kind, default)
    }

    pub fn text(key: impl Into<String>, title: impl Into<String>, default: &str) -> Self {
        Self::new(
            key,
            title,
            SettingKind::Text { placeholder: String::new(), secret: false },
            Value::String(default.into()),
        )
    }

    pub fn folder(key: impl Into<String>, title: impl Into<String>) -> Self {
        Self::new(key, title, SettingKind::Folder { placeholder: String::new() }, Value::String(String::new()))
    }

    pub fn data(key: impl Into<String>, default: Value) -> Self {
        let key = key.into();
        Self { visible: false, ..Self::new(key.clone(), key, SettingKind::Data, default) }
    }

    pub fn detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = detail.into();
        self
    }

    pub fn unit(mut self, unit: impl Into<String>) -> Self {
        if let SettingKind::Number { unit: current, .. } = &mut self.kind {
            *current = unit.into();
        }
        self
    }

    pub fn placeholder(mut self, placeholder: impl Into<String>) -> Self {
        if let SettingKind::Text { placeholder: current, .. } | SettingKind::Folder { placeholder: current } =
            &mut self.kind
        {
            *current = placeholder.into();
        }
        self
    }

    pub fn secret(mut self) -> Self {
        if let SettingKind::Text { secret, .. } = &mut self.kind {
            *secret = true;
        }
        self
    }

    pub fn shown_when(mut self, toggle: impl Into<String>) -> Self {
        self.shown_when = Some(toggle.into());
        self
    }

    pub fn shown_when_is(mut self, choice: impl AsRef<str>, value: impl AsRef<str>) -> Self {
        self.shown_when = Some(format!("{}={}", choice.as_ref(), value.as_ref()));
        self
    }

    pub fn shows_with<'a>(&self, current: impl Fn(&str) -> Option<&'a Value>) -> bool {
        let Some(condition) = self.shown_when.as_deref() else { return true };
        match condition.split_once('=') {
            Some((key, wanted)) => current(key).is_some_and(|value| value.as_str() == Some(wanted)),
            None => current(condition).and_then(|value| value.as_bool()) == Some(true),
        }
    }

    pub fn hidden(mut self) -> Self {
        self.visible = false;
        self
    }
}
