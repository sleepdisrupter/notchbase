//! Shortcuts the way `config.json` writes them, like `Ctrl+Shift+C`, with Option and Cmd on a Mac.

use global_hotkey::hotkey::{HotKey, Modifiers};

const MAC: bool = cfg!(target_os = "macos");

/// Writes a shortcut the way people type it, or in the library's own form if that wouldn't read back the same.
pub fn format(hotkey: &HotKey) -> String {
    let names = [
        (Modifiers::CONTROL, "Ctrl"),
        (Modifiers::ALT, if MAC { "Option" } else { "Alt" }),
        (Modifiers::SHIFT, "Shift"),
        (Modifiers::SUPER, if MAC { "Cmd" } else { "Super" }),
    ];
    let key = hotkey.key.to_string();
    let key = key.strip_prefix("Key").or_else(|| key.strip_prefix("Digit")).unwrap_or(&key);
    let friendly = names
        .iter()
        .filter(|(modifier, _)| hotkey.mods.contains(*modifier))
        .map(|(_, name)| *name)
        .chain([key])
        .collect::<Vec<_>>()
        .join("+");
    if parse(&friendly).as_ref() == Some(hotkey) { friendly } else { hotkey.to_string() }
}

/// Reads a shortcut. Blank means none.
pub fn parse(text: &str) -> Option<HotKey> {
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    text.parse().ok()
}

#[cfg(test)]
mod tests {
    use global_hotkey::hotkey::Code;

    use super::*;

    #[test]
    fn hotkeys_read_as_people_type_them() {
        let hotkey = HotKey::new(Some(Modifiers::CONTROL | Modifiers::SHIFT), Code::KeyC);
        assert_eq!(format(&hotkey), "Ctrl+Shift+C");
        let digit = HotKey::new(Some(Modifiers::CONTROL | Modifiers::ALT), Code::Digit4);
        assert_eq!(parse(&format(&digit)), Some(digit));
        let function = HotKey::new(None, Code::F5);
        assert_eq!(format(&function), "F5");
    }

    #[test]
    fn every_supported_key_round_trips() {
        for code in [Code::Space, Code::Backquote, Code::Numpad7, Code::ArrowUp, Code::F24, Code::PrintScreen] {
            let hotkey = HotKey::new(Some(Modifiers::SUPER | Modifiers::ALT), code);
            assert_eq!(parse(&format(&hotkey)), Some(hotkey), "{code:?}");
        }
    }

    #[test]
    fn blanks_and_nonsense_mean_no_shortcut() {
        assert_eq!(parse("  "), None);
        assert_eq!(parse("Ctrl+Nope+Q+Z"), None);
        assert_eq!(
            parse("control+shift+KeyC"),
            Some(HotKey::new(Some(Modifiers::CONTROL | Modifiers::SHIFT), Code::KeyC))
        );
    }
}
