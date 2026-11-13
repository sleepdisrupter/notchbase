//! Device names and glyphs from the property store, cached per device.

use std::collections::HashMap;

use ::windows::Win32::Devices::FunctionDiscovery::PKEY_Device_FriendlyName;
use ::windows::Win32::Foundation::PROPERTYKEY;
use ::windows::Win32::Media::Audio::{IMMDevice, IMMEndpoint, PKEY_AudioEndpoint_FormFactor, eCapture};
use ::windows::Win32::System::Com::STGM_READ;
use ::windows::Win32::System::Com::StructuredStorage::{PropVariantToStringAlloc, PropVariantToUInt32};
use ::windows::Win32::UI::Shell::PropertiesSystem::IPropertyStore;
use ::windows::core::Interface;

use super::com::take_string;
use crate::audio::{DeviceForm, DeviceKind};

#[derive(Clone)]
pub struct DeviceLabel {
    pub name: String,
    pub form: DeviceForm,
}

#[derive(Default)]
pub struct DeviceLabels(HashMap<String, DeviceLabel>);

impl DeviceLabels {
    pub fn get(&mut self, device: &IMMDevice, id: &str) -> Option<DeviceLabel> {
        if let Some(known) = self.0.get(id) {
            return Some(known.clone());
        }
        let label = read_label(device)?;
        self.0.insert(id.to_owned(), label.clone());
        Some(label)
    }
}

pub fn device_id(device: &IMMDevice) -> Option<String> {
    let id = unsafe { device.GetId() }.ok()?;
    unsafe { take_string(id) }
}

fn read_label(device: &IMMDevice) -> Option<DeviceLabel> {
    let store = unsafe { device.OpenPropertyStore(STGM_READ) }.ok()?;
    let name = read_text(&store, &PKEY_Device_FriendlyName)?;
    let kind = data_flow_kind(device);
    let form = form_from_factor(read_u32(&store, &PKEY_AudioEndpoint_FormFactor), kind);
    Some(DeviceLabel { name, form })
}

fn data_flow_kind(device: &IMMDevice) -> DeviceKind {
    let flow = device.cast::<IMMEndpoint>().ok().and_then(|endpoint| unsafe { endpoint.GetDataFlow() }.ok());
    if flow == Some(eCapture) { DeviceKind::Input } else { DeviceKind::Output }
}

fn read_text(store: &IPropertyStore, key: &PROPERTYKEY) -> Option<String> {
    let value = unsafe { store.GetValue(key) }.ok()?;
    let raw = unsafe { PropVariantToStringAlloc(&value) }.ok()?;
    let text = unsafe { take_string(raw) }?;
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_owned())
}

fn read_u32(store: &IPropertyStore, key: &PROPERTYKEY) -> Option<u32> {
    let value = unsafe { store.GetValue(key) }.ok()?;
    unsafe { PropVariantToUInt32(&value) }.ok()
}

fn form_from_factor(factor: Option<u32>, kind: DeviceKind) -> DeviceForm {
    const SPEAKERS: u32 = 1;
    const HEADPHONES: u32 = 3;
    const MICROPHONE: u32 = 4;
    const HEADSET: u32 = 5;
    const HANDSET: u32 = 6;
    const DISPLAY: u32 = 9;
    match (factor, kind) {
        (Some(HEADPHONES | HEADSET | HANDSET), _) => DeviceForm::Headphones,
        (Some(DISPLAY), DeviceKind::Output) => DeviceForm::Display,
        (Some(SPEAKERS), DeviceKind::Output) => DeviceForm::Speakers,
        (Some(MICROPHONE), _) => DeviceForm::Microphone,
        (_, DeviceKind::Input) => DeviceForm::Microphone,
        (_, DeviceKind::Output) => DeviceForm::Other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn form_factors_map_to_glyphs() {
        use DeviceKind::{Input, Output};
        assert_eq!(form_from_factor(Some(1), Output), DeviceForm::Speakers);
        assert_eq!(form_from_factor(Some(3), Output), DeviceForm::Headphones);
        assert_eq!(form_from_factor(Some(5), Input), DeviceForm::Headphones);
        assert_eq!(form_from_factor(Some(9), Output), DeviceForm::Display);
        assert_eq!(form_from_factor(Some(4), Input), DeviceForm::Microphone);
        assert_eq!(form_from_factor(Some(2), Output), DeviceForm::Other);
        assert_eq!(form_from_factor(None, Output), DeviceForm::Other);
        assert_eq!(form_from_factor(Some(10), Input), DeviceForm::Microphone);
    }
}
