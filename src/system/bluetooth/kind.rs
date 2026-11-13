//! What sort of device it is: from its name first, then its class of device, then its Low Energy appearance.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeviceKind {
    Headphones,
    Speaker,
    Keyboard,
    Mouse,
    Controller,
    Phone,
    Computer,
    Watch,
    Other,
}

impl DeviceKind {
    pub fn key(self) -> &'static str {
        match self {
            DeviceKind::Headphones => "headphones",
            DeviceKind::Speaker => "speaker",
            DeviceKind::Keyboard => "keyboard",
            DeviceKind::Mouse => "mouse",
            DeviceKind::Controller => "controller",
            DeviceKind::Phone => "phone",
            DeviceKind::Computer => "computer",
            DeviceKind::Watch => "watch",
            DeviceKind::Other => "other",
        }
    }
}

const NAMED: [(DeviceKind, &[&str]); 7] = [
    (DeviceKind::Controller, &["controller", "gamepad", "xbox", "dualsense", "dualshock", "joy-con", "8bitdo"]),
    (DeviceKind::Headphones, &["airpods", "buds", "headphone", "headset", "earphone", "beats", "wh-1000", "wf-1000"]),
    (DeviceKind::Speaker, &["speaker", "soundbar", "homepod", "sonos", "boombox", "soundlink"]),
    (DeviceKind::Keyboard, &["keyboard", "keys"]),
    (DeviceKind::Mouse, &["mouse", "trackpad", "trackball", "mx master", "mx anywhere"]),
    (DeviceKind::Watch, &["watch"]),
    (DeviceKind::Phone, &["iphone", "phone", "galaxy s", "pixel"]),
];

const COMPUTER: u32 = 1;
const PHONE: u32 = 2;
const AUDIO: u32 = 4;
const PERIPHERAL: u32 = 5;
const WEARABLE: u32 = 7;
const TOY: u32 = 8;

const HID: u16 = 15;
const AUDIO_SINK: u16 = 33;
const WEARABLE_AUDIO: u16 = 37;

pub(super) struct Hints<'a> {
    pub(super) name: &'a str,
    pub(super) class: Option<(u32, u32)>,
    pub(super) appearance: Option<u16>,
}

pub(super) fn classify(hints: &Hints) -> DeviceKind {
    by_name(hints.name)
        .or_else(|| hints.class.and_then(|(major, minor)| by_class(major, minor)))
        .or_else(|| hints.appearance.and_then(by_appearance))
        .unwrap_or(DeviceKind::Other)
}

fn by_name(name: &str) -> Option<DeviceKind> {
    let name = name.to_lowercase();
    NAMED.iter().find(|(_, words)| words.iter().any(|word| name.contains(word))).map(|(kind, _)| *kind)
}

fn by_class(major: u32, minor: u32) -> Option<DeviceKind> {
    Some(match (major, minor) {
        (COMPUTER, _) => DeviceKind::Computer,
        (PHONE, _) => DeviceKind::Phone,
        (AUDIO, 5 | 7 | 8 | 10) => DeviceKind::Speaker,
        (AUDIO, _) => DeviceKind::Headphones,
        (PERIPHERAL, minor) if matches!(minor & 0xF, 1 | 2) => DeviceKind::Controller,
        (PERIPHERAL, minor) => match minor >> 4 {
            1 | 3 => DeviceKind::Keyboard,
            2 => DeviceKind::Mouse,
            _ => return None,
        },
        (WEARABLE, 1) => DeviceKind::Watch,
        (TOY, 5) => DeviceKind::Controller,
        _ => return None,
    })
}

fn by_appearance(appearance: u16) -> Option<DeviceKind> {
    Some(match (appearance >> 6, appearance & 0x3F) {
        (1, _) => DeviceKind::Phone,
        (2, _) => DeviceKind::Computer,
        (3, _) => DeviceKind::Watch,
        (HID, 1) => DeviceKind::Keyboard,
        (HID, 2) => DeviceKind::Mouse,
        (HID, 3 | 4) => DeviceKind::Controller,
        (AUDIO_SINK, _) => DeviceKind::Speaker,
        (WEARABLE_AUDIO, _) => DeviceKind::Headphones,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kind(name: &str, class: Option<(u32, u32)>, appearance: Option<u16>) -> DeviceKind {
        classify(&Hints { name, class, appearance })
    }

    #[test]
    fn names_say_the_most() {
        assert_eq!(kind("Sam's AirPods Pro", Some((AUDIO, 1)), None), DeviceKind::Headphones);
        assert_eq!(kind("JBL Flip 6 Speaker", Some((AUDIO, 1)), None), DeviceKind::Speaker);
        assert_eq!(kind("Xbox Wireless Controller", None, None), DeviceKind::Controller);
        assert_eq!(kind("MX Master 3S", None, None), DeviceKind::Mouse);
    }

    #[test]
    fn class_of_device_fills_in() {
        assert_eq!(kind("WH-X", Some((AUDIO, 6)), None), DeviceKind::Headphones);
        assert_eq!(kind("Flip", Some((AUDIO, 5)), None), DeviceKind::Speaker);
        assert_eq!(kind("K380", Some((PERIPHERAL, 0x10)), None), DeviceKind::Keyboard);
        assert_eq!(kind("M720", Some((PERIPHERAL, 0x20)), None), DeviceKind::Mouse);
        assert_eq!(kind("Pad", Some((PERIPHERAL, 0x02)), None), DeviceKind::Controller);
    }

    #[test]
    fn appearance_is_the_last_hint() {
        assert_eq!(kind("LE-1", None, Some((HID << 6) | 2)), DeviceKind::Mouse);
        assert_eq!(kind("LE-2", None, Some(WEARABLE_AUDIO << 6)), DeviceKind::Headphones);
        assert_eq!(kind("Thing", None, None), DeviceKind::Other);
    }
}
