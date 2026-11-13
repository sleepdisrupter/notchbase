//! Tools as the panel sees them: their slots, tiles, pages and settings.

use notchbase_sdk::protocol::{LiveNode, Node, SettingSpec, TileNode, Value};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ToolSlot(pub u16);

#[derive(Clone, Debug, PartialEq)]
pub struct ToolSetting {
    pub spec: SettingSpec,
    pub value: Value,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ToolInfo {
    pub id: String,
    pub name: String,
    pub version: String,
    pub location: Option<String>,
    pub enabled: bool,
    pub problem: Option<String>,
    pub background: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Locations {
    pub tools: Option<String>,
    pub config: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ToolView {
    pub slot: ToolSlot,
    pub id: String,
    pub name: String,
    pub tile: Option<TileNode>,
    pub page: Option<Node>,
    pub live: Option<LiveNode>,
    pub settings: Vec<ToolSetting>,
    pub failure: Option<String>,
}
