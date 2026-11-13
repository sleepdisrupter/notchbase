//! Global hotkeys for tool shortcuts: each one the manifest declares, with the user's choice or its default.

use global_hotkey::hotkey::HotKey;

use super::Tools;
use crate::config::{Config, hotkey};
use crate::tools::shortcuts::ShortcutRef;

#[derive(Clone, Debug, PartialEq)]
pub struct Binding {
    pub which: ShortcutRef,
    pub tool: String,
    pub id: String,
    pub title: String,
    pub hotkey: Option<HotKey>,
}

pub fn bindings(tools: &Tools, config: &Config) -> Vec<Binding> {
    let mut bindings = Vec::new();
    for host in tools.hosts() {
        let saved = config.tools.get(host.id()).map(|tool| &tool.shortcuts);
        for (index, spec) in host.manifest.shortcuts.iter().enumerate() {
            let text = match saved.and_then(|shortcuts| shortcuts.get(&spec.id)) {
                Some(chosen) => chosen.clone(),
                None => spec.default.clone(),
            };
            bindings.push(Binding {
                which: ShortcutRef { tool: host.slot, index: index as u16 },
                tool: host.id().to_owned(),
                id: spec.id.clone(),
                title: spec.title.clone(),
                hotkey: text.as_deref().and_then(hotkey::parse),
            });
        }
    }
    bindings
}
