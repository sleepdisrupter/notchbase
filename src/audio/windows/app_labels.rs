//! An app's name from its executable's version description, and its icon from the file.

use std::ffi::c_void;

use ::windows::Win32::Storage::FileSystem::{GetFileVersionInfoSizeW, GetFileVersionInfoW, VerQueryValueW};
use ::windows::core::{HSTRING, PCWSTR, w};

use super::app_names::friendly_app_name;
use super::icons::load_icon;
use crate::audio::keys::pid_in_key;
use crate::audio::labels::AppLabel;

const FALLBACK_LANGUAGES: [&str; 3] = ["040904b0", "040904e4", "04090000"];

pub fn load_label(key: &str) -> AppLabel {
    if let Some(pid) = pid_in_key(key) {
        return AppLabel { name: format!("Process {pid}"), icon: None };
    }
    AppLabel { name: friendly_app_name(file_description(key).as_deref(), key), icon: load_icon(key, 0) }
}

fn file_description(path: &str) -> Option<String> {
    let block = version_block(path)?;
    translations(&block)
        .into_iter()
        .chain(FALLBACK_LANGUAGES.map(String::from))
        .find_map(|language| description_in(&block, &language))
}

fn version_block(path: &str) -> Option<Vec<u32>> {
    let wide = HSTRING::from(path);
    let size = unsafe { GetFileVersionInfoSizeW(&wide, None) };
    if size == 0 {
        return None;
    }
    let mut block = vec![0u32; (size as usize).div_ceil(size_of::<u32>())];
    unsafe { GetFileVersionInfoW(&wide, None, size, block.as_mut_ptr().cast()) }.ok()?;
    Some(block)
}

fn translations(block: &[u32]) -> Vec<String> {
    let Some(pairs) = query(block, w!("\\VarFileInfo\\Translation"), Length::Bytes) else { return Vec::new() };
    pairs.chunks_exact(2).map(|pair| format!("{:04x}{:04x}", pair[0], pair[1])).collect()
}

fn description_in(block: &[u32], language: &str) -> Option<String> {
    let sub_block = HSTRING::from(format!("\\StringFileInfo\\{language}\\FileDescription"));
    let text = String::from_utf16_lossy(query(block, PCWSTR(sub_block.as_ptr()), Length::Chars)?);
    let text = text.split('\0').next().unwrap_or_default().trim();
    (!text.is_empty()).then(|| text.to_owned())
}

#[derive(Clone, Copy)]
enum Length {
    Bytes,
    Chars,
}

impl Length {
    fn in_words(self, len: u32) -> usize {
        match self {
            Self::Bytes => len as usize / size_of::<u16>(),
            Self::Chars => len as usize,
        }
    }
}

fn query(block: &[u32], sub_block: PCWSTR, length: Length) -> Option<&[u16]> {
    let mut value: *mut c_void = std::ptr::null_mut();
    let mut len = 0u32;
    let found = unsafe { VerQueryValueW(block.as_ptr().cast(), sub_block, &mut value, &mut len) }.as_bool();
    if !found || value.is_null() {
        return None;
    }
    Some(unsafe { std::slice::from_raw_parts(value.cast::<u16>(), length.in_words(len)) })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::windows::icons::ICON_PX;

    #[test]
    fn explorer_has_a_description_and_icon() {
        let explorer = std::env::var("SystemRoot").map(|r| format!("{r}\\explorer.exe")).unwrap_or_default();
        if std::path::Path::new(&explorer).exists() {
            assert!(file_description(&explorer).is_some());
            let icon = load_icon(&explorer, 0).expect("explorer has an icon");
            assert_eq!(icon.size, [ICON_PX as usize, ICON_PX as usize]);
            assert!(icon.pixels.iter().any(|p| p.a() > 0));
        }
    }

    #[test]
    fn pid_keys_get_a_generic_name() {
        let label = load_label("pid:42");
        assert_eq!(label.name, "Process 42");
        assert!(label.icon.is_none());
    }
}
