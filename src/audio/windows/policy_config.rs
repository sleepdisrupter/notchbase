//! Sets the default device through `IPolicyConfig`, an undocumented but long stable interface declared here by hand.

use std::ffi::c_void;

use ::windows::Win32::Media::Audio::{ERole, eCommunications, eConsole, eMultimedia};
use ::windows::Win32::System::Com::{CLSCTX_ALL, CoCreateInstance};
use ::windows::core::{GUID, HRESULT, HSTRING, IUnknown, IUnknown_Vtbl, Interface, PCWSTR};

const CLSID_POLICY_CONFIG_CLIENT: GUID = GUID::from_u128(0x870af99c_171d_4f9e_af0d_e63df40c2bc9);
const IID_POLICY_CONFIG: GUID = GUID::from_u128(0xf8679f50_850a_41cf_9c72_430f290290c8);

#[repr(C)]
struct PolicyConfigVtbl {
    base: IUnknown_Vtbl,
    _preceding_methods: [usize; 10],
    set_default_endpoint: unsafe extern "system" fn(this: *mut c_void, device_id: PCWSTR, role: ERole) -> HRESULT,
}

struct PolicyConfig(IUnknown);

impl PolicyConfig {
    fn create() -> Option<Self> {
        let unknown: IUnknown = unsafe { CoCreateInstance(&CLSID_POLICY_CONFIG_CLIENT, None, CLSCTX_ALL) }.ok()?;
        let mut raw: *mut c_void = std::ptr::null_mut();
        if unsafe { unknown.query(&IID_POLICY_CONFIG, &mut raw) }.is_err() || raw.is_null() {
            return None;
        }
        Some(Self(unsafe { IUnknown::from_raw(raw) }))
    }

    fn set_default_endpoint(&self, device_id: &HSTRING, role: ERole) -> bool {
        let this = self.0.as_raw();
        let vtable = unsafe { &**this.cast::<*const PolicyConfigVtbl>() };
        unsafe { (vtable.set_default_endpoint)(this, PCWSTR(device_id.as_ptr()), role) }.is_ok()
    }
}

pub fn set_default_endpoint(id: &str) -> bool {
    let Some(policy) = PolicyConfig::create() else { return false };
    let device_id = HSTRING::from(id);
    let mut ok = true;
    for role in [eConsole, eMultimedia, eCommunications] {
        ok &= policy.set_default_endpoint(&device_id, role);
    }
    ok
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_default_endpoint_is_slot_13() {
        let offset = std::mem::offset_of!(PolicyConfigVtbl, set_default_endpoint);
        assert_eq!(offset, 13 * size_of::<usize>());
    }
}
