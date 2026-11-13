//! A thin wrapper over the SetupDi device list and the properties of each node.

use std::ptr::{null, null_mut};

use windows_sys::Win32::Devices::DeviceAndDriverInstallation::{
    DIGCF_ALLCLASSES, DIGCF_PRESENT, HDEVINFO, SP_DEVINFO_DATA, SetupDiDestroyDeviceInfoList, SetupDiEnumDeviceInfo,
    SetupDiGetClassDevsW, SetupDiGetDeviceInstanceIdW, SetupDiGetDevicePropertyW,
};
use windows_sys::Win32::Devices::Properties::{
    DEVPROP_TYPE_BYTE, DEVPROP_TYPE_GUID, DEVPROP_TYPE_STRING, DEVPROP_TYPE_UINT32, DEVPROPTYPE,
};
use windows_sys::Win32::Foundation::{DEVPROPKEY, INVALID_HANDLE_VALUE};
use windows_sys::core::GUID;

const VALUE_BYTES: usize = 1024;
const ID_CHARS: usize = 512;

pub(super) struct DeviceSet(HDEVINFO);

impl DeviceSet {
    pub(super) fn present(class: Option<&GUID>) -> Option<Self> {
        let flags = if class.is_some() { DIGCF_PRESENT } else { DIGCF_PRESENT | DIGCF_ALLCLASSES };
        let class = class.map_or(null(), |class| class as *const GUID);
        let set = unsafe { SetupDiGetClassDevsW(class, null(), null_mut(), flags) };
        (set != 0 && set != INVALID_HANDLE_VALUE as HDEVINFO).then_some(Self(set))
    }

    pub(super) fn nodes(&self) -> impl Iterator<Item = Node<'_>> {
        (0..).map_while(move |index| {
            let mut data = SP_DEVINFO_DATA { cbSize: size_of::<SP_DEVINFO_DATA>() as u32, ..Default::default() };
            let found = unsafe { SetupDiEnumDeviceInfo(self.0, index, &mut data) } != 0;
            found.then_some(Node { set: self, data })
        })
    }
}

impl Drop for DeviceSet {
    fn drop(&mut self) {
        unsafe { SetupDiDestroyDeviceInfoList(self.0) };
    }
}

pub(super) struct Node<'a> {
    set: &'a DeviceSet,
    data: SP_DEVINFO_DATA,
}

impl Node<'_> {
    pub(super) fn instance_id(&self) -> Option<String> {
        let mut id = [0u16; ID_CHARS];
        let mut needed = 0;
        let read = unsafe {
            SetupDiGetDeviceInstanceIdW(self.set.0, &self.data, id.as_mut_ptr(), ID_CHARS as u32, &mut needed)
        };
        (read != 0).then(|| wide(&id))
    }

    pub(super) fn string(&self, key: &DEVPROPKEY) -> Option<String> {
        let bytes = self.value(key, DEVPROP_TYPE_STRING)?;
        let units: Vec<u16> = bytes.chunks_exact(2).map(|pair| u16::from_le_bytes([pair[0], pair[1]])).collect();
        Some(wide(&units)).filter(|text| !text.trim().is_empty())
    }

    pub(super) fn guid(&self, key: &DEVPROPKEY) -> Option<u128> {
        let bytes: [u8; 16] = self.value(key, DEVPROP_TYPE_GUID)?.try_into().ok()?;
        Some(u128::from_le_bytes(bytes))
    }

    pub(super) fn number(&self, key: &DEVPROPKEY) -> Option<u32> {
        let bytes: [u8; 4] = self.value(key, DEVPROP_TYPE_UINT32)?.try_into().ok()?;
        Some(u32::from_le_bytes(bytes))
    }

    pub(super) fn byte(&self, key: &DEVPROPKEY) -> Option<u8> {
        self.value(key, DEVPROP_TYPE_BYTE)?.first().copied()
    }

    fn value(&self, key: &DEVPROPKEY, expected: DEVPROPTYPE) -> Option<Vec<u8>> {
        let mut buffer = vec![0u8; VALUE_BYTES];
        let (mut kind, mut size) = (0, 0);
        let read = unsafe {
            SetupDiGetDevicePropertyW(
                self.set.0,
                &self.data,
                key,
                &mut kind,
                buffer.as_mut_ptr(),
                VALUE_BYTES as u32,
                &mut size,
                0,
            )
        };
        if read == 0 || kind != expected {
            return None;
        }
        buffer.truncate(size as usize);
        Some(buffer)
    }
}

fn wide(units: &[u16]) -> String {
    let end = units.iter().position(|unit| *unit == 0).unwrap_or(units.len());
    String::from_utf16_lossy(&units[..end])
}
