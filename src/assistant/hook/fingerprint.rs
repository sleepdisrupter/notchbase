//! A 64-bit FNV-1a key for a tool call, from its name and the sorted JSON of its main field, like the command or path.
//! A permission request and its PostToolUse get the same key, so the board can tell it was answered elsewhere.

use serde_json::Value;

/// The field that tells calls apart, first match wins. Without one the whole input counts.
const FOCUS: [&str; 8] = ["questions", "command", "file_path", "notebook_path", "url", "query", "pattern", "plan"];
const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0100_0000_01b3;

pub(super) fn key(tool: &str, input: &Value) -> u64 {
    let focus = FOCUS.iter().find_map(|field| input.get(field)).unwrap_or(input);
    let mut text = String::from(tool);
    text.push('\0');
    canonical(focus, &mut text);
    text.bytes().fold(FNV_OFFSET, |hash, byte| (hash ^ u64::from(byte)).wrapping_mul(FNV_PRIME))
}

/// JSON with sorted keys, so the same input always gives the same text.
fn canonical(value: &Value, out: &mut String) {
    match value {
        Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            out.push('{');
            for (index, key) in keys.into_iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                out.push_str(&Value::from(key.as_str()).to_string());
                out.push(':');
                if let Some(inner) = map.get(key.as_str()) {
                    canonical(inner, out);
                }
            }
            out.push('}');
        }
        Value::Array(items) => {
            out.push('[');
            for (index, item) in items.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                canonical(item, out);
            }
            out.push(']');
        }
        other => out.push_str(&other.to_string()),
    }
}
