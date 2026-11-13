//! GPU use and name from IOKit's IOAccelerator, and the boot volume's free space the way Finder counts it.

use std::ffi::{CStr, c_char, c_void};
use std::ptr;

use super::Disk;

type CFTypeRef = *const c_void;
type CFIndex = isize;
type CFTypeID = usize;
type Boolean = u8;
type IoObject = u32;
type KernReturn = i32;

const K_CF_STRING_ENCODING_UTF8: u32 = 0x0800_0100;
const K_CF_NUMBER_SINT64_TYPE: CFIndex = 4;
const K_CF_NUMBER_DOUBLE_TYPE: CFIndex = 13;
const K_CF_URL_POSIX_PATH_STYLE: CFIndex = 0;
const IO_MAIN_PORT_DEFAULT: u32 = 0;

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    #[link_name = "kCFURLVolumeTotalCapacityKey"]
    static VOLUME_TOTAL_CAPACITY_KEY: CFTypeRef;
    #[link_name = "kCFURLVolumeAvailableCapacityKey"]
    static VOLUME_AVAILABLE_CAPACITY_KEY: CFTypeRef;
    #[link_name = "kCFURLVolumeAvailableCapacityForImportantUsageKey"]
    static VOLUME_AVAILABLE_CAPACITY_FOR_IMPORTANT_USAGE_KEY: CFTypeRef;

    fn CFRelease(cf: CFTypeRef);
    fn CFGetTypeID(cf: CFTypeRef) -> CFTypeID;
    fn CFStringGetTypeID() -> CFTypeID;
    fn CFNumberGetTypeID() -> CFTypeID;
    fn CFDictionaryGetTypeID() -> CFTypeID;
    fn CFDataGetTypeID() -> CFTypeID;
    fn CFStringCreateWithBytes(
        alloc: CFTypeRef,
        bytes: *const u8,
        len: CFIndex,
        encoding: u32,
        is_external: Boolean,
    ) -> CFTypeRef;
    fn CFStringGetLength(s: CFTypeRef) -> CFIndex;
    fn CFStringGetMaximumSizeForEncoding(len: CFIndex, encoding: u32) -> CFIndex;
    fn CFStringGetCString(s: CFTypeRef, buf: *mut c_char, size: CFIndex, encoding: u32) -> Boolean;
    fn CFNumberGetValue(number: CFTypeRef, the_type: CFIndex, out: *mut c_void) -> Boolean;
    fn CFDictionaryGetValue(dict: CFTypeRef, key: *const c_void) -> *const c_void;
    fn CFDataGetLength(data: CFTypeRef) -> CFIndex;
    fn CFDataGetBytePtr(data: CFTypeRef) -> *const u8;
    fn CFURLCreateWithFileSystemPath(alloc: CFTypeRef, path: CFTypeRef, style: CFIndex, is_dir: Boolean) -> CFTypeRef;
    fn CFURLCopyResourcePropertyForKey(
        url: CFTypeRef,
        key: CFTypeRef,
        value: *mut CFTypeRef,
        error: *mut CFTypeRef,
    ) -> Boolean;
}

#[link(name = "IOKit", kind = "framework")]
unsafe extern "C" {
    fn IOServiceMatching(name: *const c_char) -> CFTypeRef;
    fn IOServiceGetMatchingServices(main_port: u32, matching: CFTypeRef, existing: *mut IoObject) -> KernReturn;
    fn IOIteratorNext(iterator: IoObject) -> IoObject;
    fn IORegistryEntryCreateCFProperty(entry: IoObject, key: CFTypeRef, alloc: CFTypeRef, options: u32) -> CFTypeRef;
    fn IOObjectRelease(object: IoObject) -> KernReturn;
}

struct Cf(CFTypeRef);

impl Cf {
    fn owned(ptr: CFTypeRef) -> Option<Self> {
        (!ptr.is_null()).then_some(Self(ptr))
    }

    fn string(s: &str) -> Option<Self> {
        Self::owned(unsafe {
            CFStringCreateWithBytes(ptr::null(), s.as_ptr(), s.len() as CFIndex, K_CF_STRING_ENCODING_UTF8, 0)
        })
    }
}

impl Drop for Cf {
    fn drop(&mut self) {
        unsafe { CFRelease(self.0) };
    }
}

struct Io(IoObject);

impl Drop for Io {
    fn drop(&mut self) {
        unsafe { IOObjectRelease(self.0) };
    }
}

fn is_type(value: CFTypeRef, type_id: CFTypeID) -> bool {
    !value.is_null() && unsafe { CFGetTypeID(value) } == type_id
}

fn number(value: CFTypeRef) -> Option<f64> {
    if !is_type(value, unsafe { CFNumberGetTypeID() }) {
        return None;
    }
    let mut out = 0f64;
    unsafe { CFNumberGetValue(value, K_CF_NUMBER_DOUBLE_TYPE, (&mut out as *mut f64).cast()) };
    out.is_finite().then_some(out)
}

fn integer(value: CFTypeRef) -> Option<i64> {
    if !is_type(value, unsafe { CFNumberGetTypeID() }) {
        return None;
    }
    let mut out = 0i64;
    let exact = unsafe { CFNumberGetValue(value, K_CF_NUMBER_SINT64_TYPE, (&mut out as *mut i64).cast()) };
    (exact != 0).then_some(out)
}

fn string(value: CFTypeRef) -> Option<String> {
    let (string_id, data_id) = unsafe { (CFStringGetTypeID(), CFDataGetTypeID()) };
    if is_type(value, string_id) {
        let len = unsafe { CFStringGetLength(value) };
        let cap = unsafe { CFStringGetMaximumSizeForEncoding(len, K_CF_STRING_ENCODING_UTF8) } + 1;
        let mut buf = vec![0 as c_char; cap.max(1) as usize];
        let ok = unsafe { CFStringGetCString(value, buf.as_mut_ptr(), cap, K_CF_STRING_ENCODING_UTF8) };
        if ok == 0 {
            return None;
        }
        return Some(unsafe { CStr::from_ptr(buf.as_ptr()) }.to_string_lossy().into_owned());
    }
    if is_type(value, data_id) {
        let bytes = unsafe {
            let len = CFDataGetLength(value).max(0) as usize;
            let ptr = CFDataGetBytePtr(value);
            if ptr.is_null() { &[][..] } else { std::slice::from_raw_parts(ptr, len) }
        };
        let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
        return Some(String::from_utf8_lossy(&bytes[..end]).into_owned());
    }
    None
}

fn registry_property(entry: IoObject, key: &str) -> Option<Cf> {
    let key = Cf::string(key)?;
    Cf::owned(unsafe { IORegistryEntryCreateCFProperty(entry, key.0, ptr::null(), 0) })
}

fn accelerators() -> Vec<Io> {
    let matching = unsafe { IOServiceMatching(c"IOAccelerator".as_ptr()) };
    if matching.is_null() {
        return Vec::new();
    }
    let mut iterator: IoObject = 0;
    if unsafe { IOServiceGetMatchingServices(IO_MAIN_PORT_DEFAULT, matching, &mut iterator) } != 0 || iterator == 0 {
        return Vec::new();
    }
    let iterator = Io(iterator);
    let mut out = Vec::new();
    loop {
        let entry = unsafe { IOIteratorNext(iterator.0) };
        if entry == 0 {
            break;
        }
        out.push(Io(entry));
    }
    out
}

fn device_utilization(entry: IoObject) -> Option<f64> {
    let stats = registry_property(entry, "PerformanceStatistics")?;
    if !is_type(stats.0, unsafe { CFDictionaryGetTypeID() }) {
        return None;
    }
    let key = Cf::string("Device Utilization %")?;
    let value = unsafe { CFDictionaryGetValue(stats.0, key.0) };
    number(value)
}

pub(super) struct Gpu {
    entry: Io,
}

impl Gpu {
    pub(super) fn open() -> Option<Self> {
        accelerators().into_iter().find(|entry| device_utilization(entry.0).is_some()).map(|entry| Self { entry })
    }

    pub(super) fn sample(&mut self) -> Option<f32> {
        device_utilization(self.entry.0).map(|pct| (pct / 100.0).clamp(0.0, 1.0) as f32)
    }
}

pub(super) fn gpu_name() -> Option<String> {
    let entries = accelerators();
    if entries.is_empty() {
        return None;
    }
    Some(gpu_label(entries.iter().find_map(|entry| model(entry.0))))
}

fn model(entry: IoObject) -> Option<String> {
    let model = registry_property(entry, "model")?;
    string(model.0).map(|s| s.trim().to_owned()).filter(|s| !s.is_empty())
}

fn gpu_label(model: Option<String>) -> String {
    match model {
        Some(model) if model.to_ascii_lowercase().ends_with("gpu") => model,
        Some(model) => format!("{model} GPU"),
        None => "GPU".to_owned(),
    }
}

pub(super) fn system_disk() -> Option<Disk> {
    let path = Cf::string("/")?;
    let root = Cf::owned(unsafe { CFURLCreateWithFileSystemPath(ptr::null(), path.0, K_CF_URL_POSIX_PATH_STYLE, 1) })?;
    let (total_key, important_key, available_key) = unsafe {
        (VOLUME_TOTAL_CAPACITY_KEY, VOLUME_AVAILABLE_CAPACITY_FOR_IMPORTANT_USAGE_KEY, VOLUME_AVAILABLE_CAPACITY_KEY)
    };
    let capacity = |key: CFTypeRef| volume_integer(&root, key);
    let total = capacity(total_key).filter(|&bytes| bytes > 0)?;
    // The "important usage" figure matches what Finder shows as available.
    let free = capacity(important_key).filter(|&bytes| bytes > 0).or_else(|| capacity(available_key))?;
    Some(Disk { free: free.clamp(0, total) as u64, total: total as u64 })
}

fn volume_integer(url: &Cf, key: CFTypeRef) -> Option<i64> {
    let mut value: CFTypeRef = ptr::null();
    let copied = unsafe { CFURLCopyResourcePropertyForKey(url.0, key, &mut value, ptr::null_mut()) };
    let value = if copied == 0 { None } else { Cf::owned(value) }?;
    integer(value.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gpu_labels_end_in_gpu_once() {
        assert_eq!(gpu_label(Some("Apple M4".into())), "Apple M4 GPU");
        assert_eq!(gpu_label(Some("Apple M4 GPU".into())), "Apple M4 GPU");
        assert_eq!(gpu_label(None), "GPU");
    }

    #[test]
    fn probes_answer() {
        let disk = system_disk().expect("the boot volume has a capacity");
        assert!(disk.free <= disk.total);
        assert!(gpu_name().is_none_or(|name| name.ends_with("GPU")));
        if let Some(mut gpu) = Gpu::open() {
            assert!(gpu.sample().is_none_or(|busy| (0.0..=1.0).contains(&busy)));
        }
    }
}
