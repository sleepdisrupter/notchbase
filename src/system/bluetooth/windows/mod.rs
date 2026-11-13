//! Connected devices on Windows, from the Bluetooth device nodes that are present, with battery where it's reported.

mod devices;

use std::collections::{HashMap, HashSet};

use windows_sys::Win32::Devices::DeviceAndDriverInstallation::GUID_DEVCLASS_BLUETOOTH;
use windows_sys::Win32::Devices::Properties::{DEVPKEY_Device_ContainerId, DEVPKEY_Device_FriendlyName};
use windows_sys::Win32::Foundation::DEVPROPKEY;
use windows_sys::core::GUID;

use super::Device;
use super::kind::{Hints, classify};
use devices::{DeviceSet, Node};

const DEVICE_NODES: [&str; 2] = ["BTHENUM\\DEV_", "BTHLE\\DEV_"];
const CLASS_OF_DEVICE: DEVPROPKEY =
    DEVPROPKEY { fmtid: GUID::from_u128(0x2bd67d8b_8beb_48d5_87e0_6cda3428040a), pid: 10 };
const BATTERY: DEVPROPKEY = DEVPROPKEY { fmtid: GUID::from_u128(0x104ea319_6ee2_4701_bd47_8ddbf425bbe5), pid: 2 };

pub(super) struct Scanner {
    containers: HashMap<String, u128>,
}

impl Scanner {
    pub(super) fn new() -> Self {
        Self { containers: HashMap::new() }
    }

    pub(super) fn connected(&mut self) -> Option<Vec<Device>> {
        let set = DeviceSet::present(Some(&GUID_DEVCLASS_BLUETOOTH))?;
        self.containers.clear();
        let mut seen = HashSet::new();
        let mut devices = Vec::new();
        for node in set.nodes() {
            let Some(instance) = node.instance_id().filter(|id| is_device(id)) else { continue };
            let Some(name) = node.string(&DEVPKEY_Device_FriendlyName) else { continue };
            let container = node.guid(&DEVPKEY_Device_ContainerId);
            let id = container.map_or(instance, |container| format!("{container:032x}"));
            if !seen.insert(id.clone()) {
                continue;
            }
            if let Some(container) = container {
                self.containers.insert(id.clone(), container);
            }
            devices.push(Device { kind: classify(&hints(&node, &name)), id, name, battery: None });
        }
        Some(devices)
    }

    pub(super) fn battery(&mut self, id: &str) -> Option<u8> {
        let container = *self.containers.get(id)?;
        let set = DeviceSet::present(None)?;
        set.nodes()
            .filter(|node| node.guid(&DEVPKEY_Device_ContainerId) == Some(container))
            .find_map(|node| node.byte(&BATTERY))
            .filter(|level| (1..=100).contains(level))
    }
}

fn is_device(instance: &str) -> bool {
    let instance = instance.to_ascii_uppercase();
    DEVICE_NODES.iter().any(|prefix| instance.starts_with(prefix))
}

fn hints<'a>(node: &Node, name: &'a str) -> Hints<'a> {
    let class = node.number(&CLASS_OF_DEVICE).map(|class| ((class >> 8) & 0x1F, (class >> 2) & 0x3F));
    Hints { name, class, appearance: None }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_device_nodes_count() {
        assert!(is_device("BTHENUM\\DEV_F26AD255D201\\1&79F5D87&1&BLUETOOTHDEVICE_F26AD255D201"));
        assert!(is_device("BTHLE\\Dev_c1a2b3c4d5e6\\7&1&0"));
        assert!(!is_device("BTHENUM\\{0000111e-0000-1000-8000-00805f9b34fb}_LOCALMFG&0002\\7&2"));
        assert!(!is_device("USB\\VID_8087&PID_0026\\5&1"));
    }

    #[test]
    #[ignore = "reads this computer's Bluetooth devices"]
    fn lists_connected_devices() {
        let mut scanner = Scanner::new();
        let started = std::time::Instant::now();
        let devices = scanner.connected().expect("the device list opens");
        println!("{} connected in {:?}", devices.len(), started.elapsed());
        for device in &devices {
            println!("{device:?} battery={:?}", scanner.battery(&device.id));
        }
    }
}
