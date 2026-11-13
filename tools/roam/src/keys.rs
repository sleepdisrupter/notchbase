//! Keys travel as USB HID usage codes (page 7), which name the physical key, so a key pressed on
//! a Mac lands on the same key of a Windows keyboard and the other computer's own layout decides
//! what it types.

pub const LEFT_CTRL: u16 = 0xE0;
pub const LEFT_SHIFT: u16 = 0xE1;
pub const LEFT_ALT: u16 = 0xE2;
pub const LEFT_GUI: u16 = 0xE3;
pub const RIGHT_CTRL: u16 = 0xE4;
pub const RIGHT_SHIFT: u16 = 0xE5;
pub const RIGHT_ALT: u16 = 0xE6;
pub const RIGHT_GUI: u16 = 0xE7;
pub const ESCAPE: u16 = 0x29;
pub const TAB: u16 = 0x2B;
pub const D: u16 = 0x07;
pub const RIGHT: u16 = 0x4F;
pub const LEFT: u16 = 0x50;
pub const DOWN: u16 = 0x51;
pub const UP: u16 = 0x52;
pub const H: u16 = 0x0B;
pub const L: u16 = 0x0F;

const NONE: u16 = 0xFFFF;

/// (HID usage, macOS virtual key code, Windows set-1 scan code with 0xE000 for extended keys).
const KEYS: &[(u16, u16, u16)] = &[
    (0x04, 0x00, 0x1E),   // A
    (0x05, 0x0B, 0x30),   // B
    (0x06, 0x08, 0x2E),   // C
    (0x07, 0x02, 0x20),   // D
    (0x08, 0x0E, 0x12),   // E
    (0x09, 0x03, 0x21),   // F
    (0x0A, 0x05, 0x22),   // G
    (0x0B, 0x04, 0x23),   // H
    (0x0C, 0x22, 0x17),   // I
    (0x0D, 0x26, 0x24),   // J
    (0x0E, 0x28, 0x25),   // K
    (0x0F, 0x25, 0x26),   // L
    (0x10, 0x2E, 0x32),   // M
    (0x11, 0x2D, 0x31),   // N
    (0x12, 0x1F, 0x18),   // O
    (0x13, 0x23, 0x19),   // P
    (0x14, 0x0C, 0x10),   // Q
    (0x15, 0x0F, 0x13),   // R
    (0x16, 0x01, 0x1F),   // S
    (0x17, 0x11, 0x14),   // T
    (0x18, 0x20, 0x16),   // U
    (0x19, 0x09, 0x2F),   // V
    (0x1A, 0x0D, 0x11),   // W
    (0x1B, 0x07, 0x2D),   // X
    (0x1C, 0x10, 0x15),   // Y
    (0x1D, 0x06, 0x2C),   // Z
    (0x1E, 0x12, 0x02),   // 1
    (0x1F, 0x13, 0x03),   // 2
    (0x20, 0x14, 0x04),   // 3
    (0x21, 0x15, 0x05),   // 4
    (0x22, 0x17, 0x06),   // 5
    (0x23, 0x16, 0x07),   // 6
    (0x24, 0x1A, 0x08),   // 7
    (0x25, 0x1C, 0x09),   // 8
    (0x26, 0x19, 0x0A),   // 9
    (0x27, 0x1D, 0x0B),   // 0
    (0x28, 0x24, 0x1C),   // Return
    (0x29, 0x35, 0x01),   // Escape
    (0x2A, 0x33, 0x0E),   // Backspace
    (0x2B, 0x30, 0x0F),   // Tab
    (0x2C, 0x31, 0x39),   // Space
    (0x2D, 0x1B, 0x0C),   // -
    (0x2E, 0x18, 0x0D),   // =
    (0x2F, 0x21, 0x1A),   // [
    (0x30, 0x1E, 0x1B),   // ]
    (0x31, 0x2A, 0x2B),   // backslash
    (0x33, 0x29, 0x27),   // ;
    (0x34, 0x27, 0x28),   // '
    (0x35, 0x32, 0x29),   // `
    (0x36, 0x2B, 0x33),   // ,
    (0x37, 0x2F, 0x34),   // .
    (0x38, 0x2C, 0x35),   // /
    (0x39, 0x39, 0x3A),   // Caps Lock
    (0x3A, 0x7A, 0x3B),   // F1
    (0x3B, 0x78, 0x3C),   // F2
    (0x3C, 0x63, 0x3D),   // F3
    (0x3D, 0x76, 0x3E),   // F4
    (0x3E, 0x60, 0x3F),   // F5
    (0x3F, 0x61, 0x40),   // F6
    (0x40, 0x62, 0x41),   // F7
    (0x41, 0x64, 0x42),   // F8
    (0x42, 0x65, 0x43),   // F9
    (0x43, 0x6D, 0x44),   // F10
    (0x44, 0x67, 0x57),   // F11
    (0x45, 0x6F, 0x58),   // F12
    (0x46, NONE, 0xE037), // Print Screen
    (0x47, NONE, 0x46),   // Scroll Lock
    (0x48, NONE, 0x45),   // Pause
    (0x49, 0x72, 0xE052), // Insert (Help on a Mac)
    (0x4A, 0x73, 0xE047), // Home
    (0x4B, 0x74, 0xE049), // Page Up
    (0x4C, 0x75, 0xE053), // Delete forward
    (0x4D, 0x77, 0xE04F), // End
    (0x4E, 0x79, 0xE051), // Page Down
    (0x4F, 0x7C, 0xE04D), // Right
    (0x50, 0x7B, 0xE04B), // Left
    (0x51, 0x7D, 0xE050), // Down
    (0x52, 0x7E, 0xE048), // Up
    (0x53, 0x47, 0xE045), // Num Lock (Clear on a Mac)
    (0x54, 0x4B, 0xE035), // Keypad /
    (0x55, 0x43, 0x37),   // Keypad *
    (0x56, 0x4E, 0x4A),   // Keypad -
    (0x57, 0x45, 0x4E),   // Keypad +
    (0x58, 0x4C, 0xE01C), // Keypad Enter
    (0x59, 0x53, 0x4F),   // Keypad 1
    (0x5A, 0x54, 0x50),   // Keypad 2
    (0x5B, 0x55, 0x51),   // Keypad 3
    (0x5C, 0x56, 0x4B),   // Keypad 4
    (0x5D, 0x57, 0x4C),   // Keypad 5
    (0x5E, 0x58, 0x4D),   // Keypad 6
    (0x5F, 0x59, 0x47),   // Keypad 7
    (0x60, 0x5B, 0x48),   // Keypad 8
    (0x61, 0x5C, 0x49),   // Keypad 9
    (0x62, 0x52, 0x52),   // Keypad 0
    (0x63, 0x41, 0x53),   // Keypad .
    (0x64, 0x0A, 0x56),   // The extra key beside left Shift on ISO keyboards
    (0x65, 0x6E, 0xE05D), // Menu
    (0x67, 0x51, 0x59),   // Keypad =
    (0x68, 0x69, 0x64),   // F13
    (0x69, 0x6B, 0x65),   // F14
    (0x6A, 0x71, 0x66),   // F15
    (0x6B, 0x6A, 0x67),   // F16
    (0x6C, 0x40, 0x68),   // F17
    (0x6D, 0x4F, 0x69),   // F18
    (0x6E, 0x50, 0x6A),   // F19
    (0x6F, 0x5A, 0x6B),   // F20
    (0x7F, 0x4A, 0xE020), // Mute
    (0x80, 0x48, 0xE030), // Volume up
    (0x81, 0x49, 0xE02E), // Volume down
    (0x87, 0x5E, 0x73),   // International 1 (JIS underscore)
    (0x89, 0x5D, 0x7D),   // International 3 (JIS yen)
    (0x90, 0x68, 0x72),   // Kana
    (0x91, 0x66, 0x71),   // Eisu
    (0xE0, 0x3B, 0x1D),   // Left Control
    (0xE1, 0x38, 0x2A),   // Left Shift
    (0xE2, 0x3A, 0x38),   // Left Alt / Option
    (0xE3, 0x37, 0xE05B), // Left Windows / Command
    (0xE4, 0x3E, 0xE01D), // Right Control
    (0xE5, 0x3C, 0x36),   // Right Shift
    (0xE6, 0x3D, 0xE038), // Right Alt / Option
    (0xE7, 0x36, 0xE05C), // Right Windows / Command
];

/// Keys a Mac has no key for, sent to the nearest one it does.
const MAC_STAND_INS: &[(u16, u16)] = &[(0x46, 0x69), (0x47, 0x6B), (0x48, 0x71)];

pub fn from_mac(code: u16) -> Option<u16> {
    KEYS.iter().find(|(_, mac, _)| *mac == code).map(|(hid, _, _)| *hid)
}

pub fn to_mac(hid: u16) -> Option<u16> {
    let direct = KEYS.iter().find(|(h, mac, _)| *h == hid && *mac != NONE).map(|(_, mac, _)| *mac);
    direct.or_else(|| MAC_STAND_INS.iter().find(|(h, _)| *h == hid).map(|(_, mac)| *mac))
}

/// `scan` is the set-1 scan code, `extended` the hook's extended-key flag.
pub fn from_windows(scan: u16, extended: bool) -> Option<u16> {
    let code = if extended { 0xE000 | (scan & 0xFF) } else { scan & 0xFF };
    KEYS.iter().find(|(_, _, win)| *win == code).map(|(hid, _, _)| *hid).or_else(|| {
        // Some keyboards flag keys extended that aren't; take the plain one.
        extended.then(|| KEYS.iter().find(|(_, _, win)| *win == (scan & 0xFF)).map(|(hid, _, _)| *hid)).flatten()
    })
}

/// The scan code and whether it is an extended key.
pub fn to_windows(hid: u16) -> Option<(u16, bool)> {
    KEYS.iter().find(|(h, _, win)| *h == hid && *win != NONE).map(|(_, _, win)| (win & 0xFF, win & 0xE000 != 0))
}

pub fn is_modifier(hid: u16) -> bool {
    (LEFT_CTRL..=RIGHT_GUI).contains(&hid)
}

/// Swaps Command and Control, so the shortcut a hand already knows does the same thing on the
/// other kind of computer (Command-C on a Mac keyboard is Ctrl+C on Windows, and back).
/// How the modifier keys of one kind of keyboard act on the other kind of computer.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Modifiers {
    /// Ctrl and Command trade places, so Ctrl C on a PC keyboard copies on a Mac and ⌘C on a
    /// Mac keyboard copies on Windows.
    Shortcuts,
    /// Keys keep their place on the keyboard: the key beside the space bar is ⌘ on a Mac and
    /// Alt on Windows, the one beyond it ⌥ on a Mac and the Windows key on Windows.
    Position,
    /// Keys stay as they are, as with Deskflow or a PC keyboard plugged into a Mac: the Windows
    /// key is ⌘, Alt is ⌥, Ctrl is Ctrl.
    #[default]
    Same,
}

impl Modifiers {
    pub fn apply(self, hid: u16) -> u16 {
        match self {
            Modifiers::Shortcuts => swap_command_control(hid),
            Modifiers::Position => swap_command_option(hid),
            Modifiers::Same => hid,
        }
    }
}

/// Swaps Command (the Windows key) and Option (Alt): a Windows keyboard's Alt sits where a
/// Mac's Command does, and the other way round.
pub fn swap_command_option(hid: u16) -> u16 {
    match hid {
        LEFT_ALT => LEFT_GUI,
        LEFT_GUI => LEFT_ALT,
        RIGHT_ALT => RIGHT_GUI,
        RIGHT_GUI => RIGHT_ALT,
        other => other,
    }
}

pub fn swap_command_control(hid: u16) -> u16 {
    match hid {
        LEFT_CTRL => LEFT_GUI,
        LEFT_GUI => LEFT_CTRL,
        RIGHT_CTRL => RIGHT_GUI,
        RIGHT_GUI => RIGHT_CTRL,
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_key_round_trips_on_both_systems() {
        for (hid, mac, win) in KEYS {
            if *mac != NONE {
                assert_eq!(from_mac(*mac), Some(*hid), "mac {mac:#x}");
                assert_eq!(to_mac(*hid), Some(*mac));
            }
            let (scan, extended) = to_windows(*hid).unwrap();
            assert_eq!(from_windows(scan, extended), Some(*hid), "win {win:#x}");
        }
    }

    #[test]
    fn codes_are_unique() {
        for (i, a) in KEYS.iter().enumerate() {
            for b in &KEYS[i + 1..] {
                assert_ne!(a.0, b.0);
                assert!(a.1 == NONE || a.1 != b.1, "mac {:#x}", a.1);
                assert_ne!(a.2, b.2, "win {:#x}", a.2);
            }
        }
    }

    #[test]
    fn the_same_physical_key() {
        // Command on a Mac is the Windows key; Option is Alt.
        assert_eq!(to_windows(from_mac(0x37).unwrap()), Some((0x5B, true)));
        assert_eq!(to_windows(from_mac(0x3A).unwrap()), Some((0x38, false)));
        assert_eq!(to_mac(from_windows(0x48, true).unwrap()), Some(0x7E)); // up arrow
        assert_eq!(to_mac(0x46), Some(0x69)); // Print Screen is F13
        assert_eq!(swap_command_control(LEFT_GUI), LEFT_CTRL);
        // By position, a PC's Alt is a Mac's Command.
        assert_eq!(Modifiers::Position.apply(LEFT_ALT), LEFT_GUI);
        assert_eq!(Modifiers::Same.apply(LEFT_ALT), LEFT_ALT);
    }
}
