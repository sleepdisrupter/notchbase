//! Shortcut labels for the keycaps: modifiers in a fixed order, then the key, joined the way each system writes them.

use global_hotkey::hotkey::{Code, HotKey, Modifiers};

use super::imp::{KEY_JOINER, MODIFIER_LABELS};

const MODIFIERS: [Modifiers; 4] = [Modifiers::CONTROL, Modifiers::ALT, Modifiers::SHIFT, Modifiers::SUPER];

pub fn hotkey_labels(hotkey: &HotKey) -> Vec<String> {
    MODIFIERS
        .into_iter()
        .zip(MODIFIER_LABELS)
        .filter(|(modifier, _)| hotkey.mods.contains(*modifier))
        .map(|(_, label)| label.to_owned())
        .chain([key_label(hotkey.key)])
        .collect()
}

pub fn hotkey_caption(keys: &[String]) -> String {
    joined(keys, KEY_JOINER)
}

fn joined(keys: &[String], joiner: &str) -> String {
    keys.join(joiner)
}

fn key_label(code: Code) -> String {
    let name = format!("{code:?}");
    name.strip_prefix("Key").or_else(|| name.strip_prefix("Digit")).unwrap_or(&name).to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_drop_their_kind_prefix() {
        assert_eq!(key_label(Code::KeyK), "K");
        assert_eq!(key_label(Code::Digit7), "7");
        assert_eq!(key_label(Code::Space), "Space");
    }

    #[test]
    fn modifiers_come_first_in_order() {
        let hotkey = HotKey::new(Some(Modifiers::SHIFT | Modifiers::CONTROL), Code::KeyK);
        let expected = [MODIFIER_LABELS[0], MODIFIER_LABELS[2], "K"];
        assert_eq!(hotkey_labels(&hotkey), expected);
    }

    #[test]
    fn mac_captions_run_together() {
        let keys = ["⌃", "⇧", "C"].map(String::from);
        assert_eq!(joined(&keys, ""), "⌃⇧C");
    }

    #[test]
    fn other_captions_join_with_plus() {
        let keys = ["Ctrl", "Shift", "C"].map(String::from);
        assert_eq!(joined(&keys, "+"), "Ctrl+Shift+C");
    }

    #[test]
    fn caption_lists_every_key() {
        let hotkey = HotKey::new(Some(Modifiers::CONTROL | Modifiers::SHIFT), Code::KeyC);
        let caption = hotkey_caption(&hotkey_labels(&hotkey));
        let expected = [MODIFIER_LABELS[0], MODIFIER_LABELS[2], "C"].join(KEY_JOINER);
        assert_eq!(caption, expected);
    }
}
