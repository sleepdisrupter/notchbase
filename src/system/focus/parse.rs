//! Reads the active mode out of the Do Not Disturb assertions and finds its name in the mode configurations.

use notchbase_sdk::protocol::Focus;
use serde_json::Value;

const DO_NOT_DISTURB: &str = "Do Not Disturb";
const DEFAULT_MODE: &str = "com.apple.donotdisturb.mode.default";
const ANY_FOCUS: &str = "Focus";

pub(super) fn focus(assertions: &str, modes: &str) -> Focus {
    let Ok(assertions) = serde_json::from_str::<Value>(assertions) else { return Focus::default() };
    let entries = assertions["data"].as_array().into_iter().flatten();
    let records = entries.flat_map(|entry| entry["storeAssertionRecords"].as_array().into_iter().flatten());
    let mut active = records.map(|record| &record["assertionDetails"]["assertionDetailsModeIdentifier"]);
    let Some(mode) = active.next() else { return Focus::default() };
    Focus { active: true, name: Some(name(mode.as_str().unwrap_or_default(), modes)) }
}

fn name(mode: &str, modes: &str) -> String {
    let named = serde_json::from_str::<Value>(modes).ok().and_then(|modes| {
        let entries = modes["data"].as_array()?.clone();
        entries.iter().find_map(|entry| entry["modeConfigurations"][mode]["mode"]["name"].as_str().map(str::to_owned))
    });
    named.filter(|name| !name.trim().is_empty()).unwrap_or_else(|| match mode {
        DEFAULT_MODE => DO_NOT_DISTURB.to_owned(),
        _ => ANY_FOCUS.to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const WORK: &str = r#"{"data":[{"storeAssertionRecords":[{"assertionDetails":
        {"assertionDetailsModeIdentifier":"com.apple.focus.work"},"assertionStartDateTimestamp":749000000}]}]}"#;
    const NOTHING: &str = r#"{"data":[{"storeAssertionRecords":[]}]}"#;
    const MODES: &str = r#"{"data":[{"modeConfigurations":{"com.apple.focus.work":{"mode":{"name":"Work"}}}}]}"#;

    #[test]
    fn focus_named_from_its_config() {
        assert_eq!(focus(WORK, MODES), Focus { active: true, name: Some("Work".into()) });
    }

    #[test]
    fn no_records_means_no_focus() {
        assert_eq!(focus(NOTHING, MODES), Focus::default());
        assert_eq!(focus("not json", MODES), Focus::default());
        assert_eq!(focus("{}", ""), Focus::default());
    }

    #[test]
    fn unnamed_focus_gets_plain_name() {
        let dnd = WORK.replace("com.apple.focus.work", DEFAULT_MODE);
        assert_eq!(focus(&dnd, "").name.as_deref(), Some(DO_NOT_DISTURB));
        assert_eq!(focus(WORK, "").name.as_deref(), Some(ANY_FOCUS));
    }
}
