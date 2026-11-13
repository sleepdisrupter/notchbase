//! An app's name and icon from its running application, falling back to the process name or the end of the bundle id.

use std::ffi::c_void;

use objc2::rc::Retained;
use objc2::{ClassType, msg_send};
use objc2_app_kit::NSRunningApplication;
use objc2_foundation::NSString;

use super::app_icon::app_icon;
use crate::audio::keys::pid_in_key;
use crate::audio::labels::{AppLabel, capitalized};

const PROC_NAME_CAPACITY: usize = 256;

unsafe extern "C" {
    fn proc_name(pid: i32, buffer: *mut c_void, size: u32) -> i32;
}

pub fn load_label(key: &str, pid: i32) -> AppLabel {
    let app = app_for(key, pid);
    let name = app
        .as_ref()
        .and_then(|app| app.localizedName())
        .map(|name| name.to_string())
        .filter(|name| !name.trim().is_empty())
        .or_else(|| process_name(pid))
        .unwrap_or_else(|| key.rsplit('.').next().unwrap_or(key).to_owned());
    let icon = app.as_deref().and_then(app_icon);
    AppLabel { name, icon }
}

pub fn running_app(pid: i32) -> Option<Retained<NSRunningApplication>> {
    unsafe { msg_send![NSRunningApplication::class(), runningApplicationWithProcessIdentifier: pid] }
}

fn app_for(key: &str, pid: i32) -> Option<Retained<NSRunningApplication>> {
    if pid_in_key(key).is_none() {
        let apps = NSRunningApplication::runningApplicationsWithBundleIdentifier(&NSString::from_str(key));
        if let Some(app) = apps.firstObject() {
            return Some(app);
        }
    }
    running_app(pid)
}

fn process_name(pid: i32) -> Option<String> {
    let mut buf = [0u8; PROC_NAME_CAPACITY];
    let len = unsafe { proc_name(pid, buf.as_mut_ptr().cast(), buf.len() as u32) };
    let name = String::from_utf8_lossy(&buf[..len.clamp(0, buf.len() as i32) as usize]);
    capitalized(name.trim())
}
