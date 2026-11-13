//! Adding and removing our hook groups in the config's `hooks` object, leaving everyone else's alone.

use serde_json::{Map, Value};

use super::Hooks;

const ANSWERING: [&str; 2] = ["PermissionRequest", "PreToolUse"];

/// Groups that hold the assistant until we answer: a PermissionRequest or PreToolUse that isn't async.
fn answers(event: &str, group: &Value) -> bool {
    let waits = group["hooks"].as_array().is_some_and(|handlers| handlers.iter().any(|h| h["async"] != true));
    ANSWERING.contains(&event) && waits
}

pub(super) type Ours = fn(&Value) -> bool;

pub(super) struct Spec {
    pub(super) groups: Vec<(&'static str, Value)>,
    pub(super) ours: Ours,
}

impl Spec {
    pub(super) fn reaching(mut self, hooks: Hooks) -> Self {
        if hooks == Hooks::Progress {
            self.groups.retain(|(event, group)| !answers(event, group));
        }
        self
    }
}

/// Takes out any old handlers of ours, then adds the spec's groups.
pub(super) fn apply(root: &mut Map<String, Value>, spec: &Spec) -> Result<(), String> {
    strip(root, spec.ours)?;
    let hooks = root.entry("hooks").or_insert_with(|| Value::Object(Map::new()));
    let events = hooks.as_object_mut().ok_or_else(not_object)?;
    for (event, group) in &spec.groups {
        let groups = events.entry(*event).or_insert_with(|| Value::Array(Vec::new()));
        let groups = groups.as_array_mut().ok_or_else(|| format!("\"hooks.{event}\" isn't a list"))?;
        groups.push(group.clone());
    }
    Ok(())
}

/// Removes our handlers, then any group, event or `hooks` object that this left empty.
pub(super) fn strip(root: &mut Map<String, Value>, ours: Ours) -> Result<(), String> {
    let Some(hooks) = root.get_mut("hooks") else {
        return Ok(());
    };
    let events = hooks.as_object_mut().ok_or_else(not_object)?;
    let mut emptied = Vec::new();
    for (event, groups) in events.iter_mut() {
        if let Some(groups) = groups.as_array_mut()
            && strip_groups(groups, ours)
            && groups.is_empty()
        {
            emptied.push(event.clone());
        }
    }
    if emptied.is_empty() {
        return Ok(());
    }
    for event in &emptied {
        events.shift_remove(event);
    }
    if events.is_empty() {
        root.shift_remove("hooks");
    }
    Ok(())
}

fn strip_groups(groups: &mut Vec<Value>, ours: Ours) -> bool {
    let mut stripped = false;
    groups.retain_mut(|group| {
        let Some(handlers) = group.get_mut("hooks").and_then(Value::as_array_mut) else {
            return true;
        };
        let before = handlers.len();
        handlers.retain(|handler| !ours(handler));
        if handlers.len() == before {
            return true;
        }
        stripped = true;
        !handlers.is_empty()
    });
    stripped
}

fn not_object() -> String {
    "\"hooks\" isn't an object".to_owned()
}
