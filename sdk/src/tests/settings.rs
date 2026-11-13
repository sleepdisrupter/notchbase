use serde_json::json;

use crate::protocol::{SettingKind, SettingSpec, Value, Values};

fn values(pairs: Value) -> Values {
    pairs.as_object().cloned().unwrap_or_default()
}

#[test]
fn setting_follows_toggle_or_choice() {
    let current = values(json!({ "enabled": true, "source": "ical", "off": false }));
    let lookup = |key: &str| current.get(key);
    assert!(SettingSpec::text("a", "A", "").shows_with(lookup));
    assert!(SettingSpec::text("a", "A", "").shown_when("enabled").shows_with(lookup));
    assert!(!SettingSpec::text("a", "A", "").shown_when("off").shows_with(lookup));
    assert!(SettingSpec::text("a", "A", "").shown_when_is("source", "ical").shows_with(lookup));
    assert!(!SettingSpec::text("a", "A", "").shown_when_is("source", "apple").shows_with(lookup));
    assert!(!SettingSpec::text("a", "A", "").shown_when("missing").shows_with(lookup));
}

#[test]
fn secret_text_keeps_placeholder() {
    let spec = SettingSpec::text("link", "Link", "").placeholder("Paste it").secret();
    assert_eq!(spec.kind, SettingKind::Text { placeholder: "Paste it".into(), secret: true });
    let plain: SettingKind = serde_json::from_value(json!({ "type": "text", "placeholder": "" })).unwrap();
    assert_eq!(plain, SettingKind::Text { placeholder: String::new(), secret: false });
}
