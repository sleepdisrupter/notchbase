//! Which glyph a device gets, from how it's connected: built in, Bluetooth, HDMI, DisplayPort or AirPlay.

use super::fourcc;
use crate::audio::{DeviceForm, DeviceKind};

pub const BUILT_IN: u32 = fourcc(b"bltn");
const BLUETOOTH: u32 = fourcc(b"blue");
const BLUETOOTH_LE: u32 = fourcc(b"blea");
const HDMI: u32 = fourcc(b"hdmi");
const DISPLAY_PORT: u32 = fourcc(b"dprt");
const AIRPLAY: u32 = fourcc(b"airp");

const DATA_SOURCE_HEADPHONES: u32 = fourcc(b"hdpn");

pub fn device_form(transport: u32, data_source: Option<u32>, kind: DeviceKind) -> DeviceForm {
    match (transport, kind) {
        (HDMI | DISPLAY_PORT, DeviceKind::Output) => DeviceForm::Display,
        (BLUETOOTH | BLUETOOTH_LE, _) => DeviceForm::Headphones,
        (BUILT_IN, DeviceKind::Output) if data_source == Some(DATA_SOURCE_HEADPHONES) => DeviceForm::Headphones,
        (BUILT_IN | AIRPLAY, DeviceKind::Output) => DeviceForm::Speakers,
        (_, DeviceKind::Input) => DeviceForm::Microphone,
        (_, DeviceKind::Output) => DeviceForm::Other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_glyphs() {
        use DeviceKind::{Input, Output};
        assert_eq!(device_form(BUILT_IN, Some(fourcc(b"ispk")), Output), DeviceForm::Speakers);
        assert_eq!(device_form(BUILT_IN, Some(DATA_SOURCE_HEADPHONES), Output), DeviceForm::Headphones);
        assert_eq!(device_form(BUILT_IN, None, Input), DeviceForm::Microphone);
        assert_eq!(device_form(BLUETOOTH, None, Output), DeviceForm::Headphones);
        assert_eq!(device_form(HDMI, None, Output), DeviceForm::Display);
        assert_eq!(device_form(fourcc(b"usb "), None, Output), DeviceForm::Other);
        assert_eq!(device_form(fourcc(b"usb "), None, Input), DeviceForm::Microphone);
    }
}
