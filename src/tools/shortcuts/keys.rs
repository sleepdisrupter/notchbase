//! Turns an egui key press into a global hotkey. Keys global-hotkey has no code for can't be shortcuts.

use egui::Key;
use global_hotkey::hotkey::{Code, HotKey, Modifiers};

pub const UNSUPPORTED_HINT: &str = "That key can't be a shortcut";

pub fn hotkey_from_egui(key: Key, modifiers: egui::Modifiers) -> Option<HotKey> {
    let code = function_code(key)
        .or_else(|| letter_code(key))
        .or_else(|| digit_code(key))
        .or_else(|| symbol_code(key))
        .or_else(|| navigation_code(key))
        .or_else(|| editing_code(key))?;
    let mods = hotkey_modifiers(modifiers);
    Some(HotKey::new((!mods.is_empty()).then_some(mods), code))
}

/// A modifier counts if the window reported it or the keyboard holds it, since the window can miss one.
pub fn merge_modifiers(reported: egui::Modifiers, held: egui::Modifiers) -> egui::Modifiers {
    egui::Modifiers {
        alt: reported.alt || held.alt,
        ctrl: reported.ctrl || held.ctrl,
        shift: reported.shift || held.shift,
        mac_cmd: reported.mac_cmd || held.mac_cmd,
        command: reported.command || held.command,
    }
}

fn hotkey_modifiers(held: egui::Modifiers) -> Modifiers {
    let mut mods = Modifiers::empty();
    mods.set(Modifiers::CONTROL, held.ctrl);
    mods.set(Modifiers::ALT, held.alt);
    mods.set(Modifiers::SHIFT, held.shift);
    // Command on a Mac, and the Windows key on Windows.
    mods.set(Modifiers::SUPER, held.mac_cmd);
    mods
}

fn function_code(key: Key) -> Option<Code> {
    Some(match key {
        Key::F1 => Code::F1,
        Key::F2 => Code::F2,
        Key::F3 => Code::F3,
        Key::F4 => Code::F4,
        Key::F5 => Code::F5,
        Key::F6 => Code::F6,
        Key::F7 => Code::F7,
        Key::F8 => Code::F8,
        Key::F9 => Code::F9,
        Key::F10 => Code::F10,
        Key::F11 => Code::F11,
        Key::F12 => Code::F12,
        Key::F13 => Code::F13,
        Key::F14 => Code::F14,
        Key::F15 => Code::F15,
        Key::F16 => Code::F16,
        Key::F17 => Code::F17,
        Key::F18 => Code::F18,
        Key::F19 => Code::F19,
        Key::F20 => Code::F20,
        Key::F21 => Code::F21,
        Key::F22 => Code::F22,
        Key::F23 => Code::F23,
        Key::F24 => Code::F24,
        _ => return None,
    })
}

fn letter_code(key: Key) -> Option<Code> {
    Some(match key {
        Key::A => Code::KeyA,
        Key::B => Code::KeyB,
        Key::C => Code::KeyC,
        Key::D => Code::KeyD,
        Key::E => Code::KeyE,
        Key::F => Code::KeyF,
        Key::G => Code::KeyG,
        Key::H => Code::KeyH,
        Key::I => Code::KeyI,
        Key::J => Code::KeyJ,
        Key::K => Code::KeyK,
        Key::L => Code::KeyL,
        Key::M => Code::KeyM,
        Key::N => Code::KeyN,
        Key::O => Code::KeyO,
        Key::P => Code::KeyP,
        Key::Q => Code::KeyQ,
        Key::R => Code::KeyR,
        Key::S => Code::KeyS,
        Key::T => Code::KeyT,
        Key::U => Code::KeyU,
        Key::V => Code::KeyV,
        Key::W => Code::KeyW,
        Key::X => Code::KeyX,
        Key::Y => Code::KeyY,
        Key::Z => Code::KeyZ,
        _ => return None,
    })
}

fn digit_code(key: Key) -> Option<Code> {
    Some(match key {
        Key::Num0 => Code::Digit0,
        Key::Num1 => Code::Digit1,
        Key::Num2 => Code::Digit2,
        Key::Num3 => Code::Digit3,
        Key::Num4 => Code::Digit4,
        Key::Num5 => Code::Digit5,
        Key::Num6 => Code::Digit6,
        Key::Num7 => Code::Digit7,
        Key::Num8 => Code::Digit8,
        Key::Num9 => Code::Digit9,
        _ => return None,
    })
}

fn symbol_code(key: Key) -> Option<Code> {
    Some(match key {
        Key::Space => Code::Space,
        Key::Minus => Code::Minus,
        Key::Equals => Code::Equal,
        Key::Comma => Code::Comma,
        Key::Period => Code::Period,
        Key::Slash => Code::Slash,
        Key::Semicolon => Code::Semicolon,
        Key::OpenBracket => Code::BracketLeft,
        Key::CloseBracket => Code::BracketRight,
        Key::Backslash => Code::Backslash,
        Key::Backtick => Code::Backquote,
        Key::Quote => Code::Quote,
        _ => return None,
    })
}

fn navigation_code(key: Key) -> Option<Code> {
    Some(match key {
        Key::ArrowUp => Code::ArrowUp,
        Key::ArrowDown => Code::ArrowDown,
        Key::ArrowLeft => Code::ArrowLeft,
        Key::ArrowRight => Code::ArrowRight,
        Key::Home => Code::Home,
        Key::End => Code::End,
        Key::PageUp => Code::PageUp,
        Key::PageDown => Code::PageDown,
        Key::Insert => Code::Insert,
        Key::Tab => Code::Tab,
        Key::Enter => Code::Enter,
        _ => return None,
    })
}

fn editing_code(key: Key) -> Option<Code> {
    Some(match key {
        Key::Backspace => Code::Backspace,
        Key::Delete => Code::Delete,
        Key::Plus => Code::NumpadAdd,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn any_key_with_or_without_modifiers() {
        let plain = egui::Modifiers::NONE;
        assert_eq!(hotkey_from_egui(Key::C, plain), Some(HotKey::new(None, Code::KeyC)));
        let ctrl_alt = egui::Modifiers { ctrl: true, alt: true, ..Default::default() };
        let chord = Some(HotKey::new(Some(Modifiers::CONTROL | Modifiers::ALT), Code::KeyC));
        assert_eq!(hotkey_from_egui(Key::C, ctrl_alt), chord);
        assert_eq!(hotkey_from_egui(Key::PageDown, plain), Some(HotKey::new(None, Code::PageDown)));
        assert_eq!(hotkey_from_egui(Key::F15, plain), Some(HotKey::new(None, Code::F15)));
        assert_eq!(hotkey_from_egui(Key::F24, plain), Some(HotKey::new(None, Code::F24)));
        assert_eq!(hotkey_from_egui(Key::Plus, plain), Some(HotKey::new(None, Code::NumpadAdd)));
    }

    #[test]
    fn every_held_modifier_is_kept() {
        let held = egui::Modifiers { ctrl: true, alt: true, shift: true, mac_cmd: true, command: true };
        let all = Modifiers::CONTROL | Modifiers::ALT | Modifiers::SHIFT | Modifiers::SUPER;
        assert_eq!(hotkey_from_egui(Key::Num7, held), Some(HotKey::new(Some(all), Code::Digit7)));
    }

    #[test]
    fn missed_modifiers_come_from_keyboard() {
        let reported = egui::Modifiers { shift: true, ..Default::default() };
        let held = egui::Modifiers { ctrl: true, alt: true, ..Default::default() };
        let merged = merge_modifiers(reported, held);
        assert!(merged.ctrl && merged.alt && merged.shift && !merged.mac_cmd);
    }

    #[test]
    fn keys_without_a_code_are_rejected() {
        assert!(hotkey_from_egui(Key::Escape, egui::Modifiers::NONE).is_none());
        assert!(hotkey_from_egui(Key::F35, egui::Modifiers::NONE).is_none());
    }
}
