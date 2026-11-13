//! Connected devices on macOS from IOBluetooth's paired list, once Bluetooth access is granted.

use std::collections::HashMap;

use objc2::rc::{Retained, autoreleasepool};
use objc2::runtime::{AnyClass, AnyObject, Sel};
use objc2::{msg_send, sel};
use objc2_foundation::{NSArray, NSString};

use super::Device;
use super::kind::{Hints, classify};
use crate::system::permissions::{Access, Gate};

#[link(name = "IOBluetooth", kind = "framework")]
unsafe extern "C" {}

pub(super) struct Scanner {
    devices: HashMap<String, Retained<AnyObject>>,
    gate: Gate,
}

impl Scanner {
    pub(super) fn new() -> Self {
        Self { devices: HashMap::new(), gate: Gate::new(Access::Bluetooth) }
    }

    pub(super) fn connected(&mut self) -> Option<Vec<Device>> {
        if !self.gate.open() {
            return None;
        }
        autoreleasepool(|_| {
            let class = AnyClass::get(c"IOBluetoothDevice")?;
            let paired: Option<Retained<NSArray<AnyObject>>> = unsafe { msg_send![class, pairedDevices] };
            self.devices.clear();
            let mut devices = Vec::new();
            for device in paired?.iter() {
                let connected: bool = unsafe { msg_send![&*device, isConnected] };
                if !connected {
                    continue;
                }
                let address: Option<Retained<NSString>> = unsafe { msg_send![&*device, addressString] };
                let Some(id) = address.map(|address| address.to_string()) else { continue };
                let name: Option<Retained<NSString>> = unsafe { msg_send![&*device, name] };
                let name = name.map(|name| name.to_string()).filter(|name| !name.trim().is_empty());
                let name = name.unwrap_or_else(|| id.clone());
                let major: u32 = unsafe { msg_send![&*device, deviceClassMajor] };
                let minor: u32 = unsafe { msg_send![&*device, deviceClassMinor] };
                let kind = classify(&Hints { name: &name, class: Some((major, minor)), appearance: None });
                devices.push(Device { id: id.clone(), name, kind, battery: None });
                self.devices.insert(id, device);
            }
            Some(devices)
        })
    }

    pub(super) fn battery(&mut self, id: &str) -> Option<u8> {
        let device = self.devices.get(id)?;
        autoreleasepool(|_| {
            level(device, "batteryPercentSingle", sel!(batteryPercentSingle))
                .or_else(|| level(device, "batteryPercentCombined", sel!(batteryPercentCombined)))
                .or_else(|| {
                    let left = level(device, "batteryPercentLeft", sel!(batteryPercentLeft));
                    let right = level(device, "batteryPercentRight", sel!(batteryPercentRight));
                    left.into_iter().chain(right).min()
                })
        })
    }
}

fn level(device: &AnyObject, key: &str, selector: Sel) -> Option<u8> {
    let responds: bool = unsafe { msg_send![device, respondsToSelector: selector] };
    if !responds {
        return None;
    }
    let value: Option<Retained<AnyObject>> = unsafe { msg_send![device, valueForKey: &*NSString::from_str(key)] };
    let percent: isize = unsafe { msg_send![&*value?, integerValue] };
    u8::try_from(percent).ok().filter(|percent| (1..=100).contains(percent))
}
