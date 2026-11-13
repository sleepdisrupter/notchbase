//! Reads a dropped IDataObject, best first: files, a page, virtual files from browsers and mail, then a link.

use std::ffi::OsString;
use std::os::windows::ffi::OsStringExt as _;
use std::path::PathBuf;

use ::windows::Win32::Foundation::HGLOBAL;
use ::windows::Win32::System::Com::{
    DVASPECT_CONTENT, FORMATETC, IDataObject, IStream, STGMEDIUM, TYMED, TYMED_HGLOBAL, TYMED_ISTREAM,
};
use ::windows::Win32::System::DataExchange::RegisterClipboardFormatW;
use ::windows::Win32::System::Memory::{GlobalLock, GlobalSize, GlobalUnlock};
use ::windows::Win32::System::Ole::{CF_HDROP, CF_UNICODETEXT, ReleaseStgMedium};
use ::windows::Win32::UI::Shell::{DragQueryFileW, FILEDESCRIPTORW, HDROP};
use ::windows::core::HSTRING;

use super::super::incoming::Dropped;
use super::super::links;

const LARGEST: usize = 512 << 20;
const CHUNK: usize = 64 << 10;
const WHOLE: i32 = -1;
const HAS_ATTRIBUTES: u32 = 0x4;
const HAS_SIZE: u32 = 0x40;
const DIRECTORY: u32 = 0x10;
const DESCRIPTORS: &str = "FileGroupDescriptorW";
const CONTENTS: &str = "FileContents";
const HTML: &str = "HTML Format";
const WIDE_LINK: &str = "UniformResourceLocatorW";
const NARROW_LINK: &str = "UniformResourceLocator";
const FIREFOX_LINK: &str = "text/x-moz-url";
const FIREFOX_TAB: &str = "text/x-moz-text-internal";

#[derive(Clone, Debug, PartialEq, Eq)]
struct Descriptor {
    name: String,
    size: Option<usize>,
    folder: bool,
}

pub(super) fn count(data: &IDataObject) -> usize {
    let files = dropped_files(data).len();
    if files > 0 {
        return files;
    }
    if offered_page(data).is_some() {
        return 1;
    }
    let promised = descriptors(data).iter().filter(|descriptor| !descriptor.folder).count();
    if promised > 0 {
        return promised;
    }
    usize::from(link(data).is_some())
}

pub(super) fn offered_page(data: &IDataObject) -> Option<String> {
    let page = page_link(data).filter(|link| !links::is_inline_or_file(link))?;
    let alone = || descriptors(data).is_empty() && image_link(data).is_none();
    (super::super::prefers_page(&page) || alone()).then_some(page)
}

pub(super) fn read(data: &IDataObject) -> Vec<Dropped> {
    let files = dropped_files(data);
    if !files.is_empty() {
        return files.into_iter().map(Dropped::File).collect();
    }
    if let Some(page) = offered_page(data) {
        return vec![Dropped::Page(page)];
    }
    let contents = virtual_files(data);
    if !contents.is_empty() {
        return contents;
    }
    link(data).map(Dropped::Link).into_iter().collect()
}

fn dropped_files(data: &IDataObject) -> Vec<PathBuf> {
    let Some(global) = Medium::get(data, CF_HDROP.0, WHOLE, TYMED_HGLOBAL).and_then(|medium| medium.global()) else {
        return Vec::new();
    };
    let drop = HDROP(global.0);
    let count = unsafe { DragQueryFileW(drop, u32::MAX, None) };
    (0..count)
        .filter_map(|index| {
            let length = unsafe { DragQueryFileW(drop, index, None) } as usize;
            let mut buffer = vec![0u16; length + 1];
            let written = unsafe { DragQueryFileW(drop, index, Some(&mut buffer)) } as usize;
            (written > 0).then(|| PathBuf::from(OsString::from_wide(&buffer[..written])))
        })
        .collect()
}

fn descriptors(data: &IDataObject) -> Vec<Descriptor> {
    Medium::get(data, format(DESCRIPTORS), WHOLE, TYMED_HGLOBAL)
        .and_then(|medium| medium.bytes())
        .map(|bytes| parse_descriptors(&bytes))
        .unwrap_or_default()
}

/// Virtual files by index, each cut to the size its descriptor gives.
fn virtual_files(data: &IDataObject) -> Vec<Dropped> {
    let contents = format(CONTENTS);
    let kinds = TYMED(TYMED_ISTREAM.0 | TYMED_HGLOBAL.0);
    descriptors(data)
        .into_iter()
        .enumerate()
        .filter(|(_, descriptor)| !descriptor.folder)
        .filter_map(|(index, descriptor)| {
            let mut bytes = Medium::get(data, contents, index as i32, kinds)?.bytes()?;
            if let Some(size) = descriptor.size {
                bytes.truncate(size);
            }
            Some(Dropped::Data { name: Some(descriptor.name), bytes })
        })
        .collect()
}

fn link(data: &IDataObject) -> Option<String> {
    image_link(data).or_else(|| page_link(data))
}

fn image_link(data: &IDataObject) -> Option<String> {
    text(data, format(HTML), false).and_then(|html| links::image_source(&html))
}

fn page_link(data: &IDataObject) -> Option<String> {
    let wide = || text(data, format(WIDE_LINK), true).and_then(|text| links::first_link(&text));
    let firefox = || text(data, format(FIREFOX_LINK), true).and_then(|text| links::first_link(&text));
    let tab = || text(data, format(FIREFOX_TAB), true).and_then(|text| links::first_link(&text));
    let narrow = || text(data, format(NARROW_LINK), false).and_then(|text| links::first_link(&text));
    let plain =
        || text(data, CF_UNICODETEXT.0, true).filter(|text| links::is_link(text)).map(|text| text.trim().to_owned());
    wide().or_else(firefox).or_else(tab).or_else(narrow).or_else(plain)
}

fn text(data: &IDataObject, format: u16, wide: bool) -> Option<String> {
    let bytes = Medium::get(data, format, WHOLE, TYMED_HGLOBAL)?.bytes()?;
    Some(if wide { utf16(&bytes) } else { narrow(&bytes) })
}

fn format(name: &str) -> u16 {
    unsafe { RegisterClipboardFormatW(&HSTRING::from(name)) as u16 }
}

/// A FILEGROUPDESCRIPTORW: a count, then that many descriptors, stopping early if the bytes run out.
fn parse_descriptors(bytes: &[u8]) -> Vec<Descriptor> {
    let Some(head) = bytes.get(..4).and_then(|head| <[u8; 4]>::try_from(head).ok()) else {
        return Vec::new();
    };
    let size = std::mem::size_of::<FILEDESCRIPTORW>();
    (0..u32::from_le_bytes(head) as usize)
        .map_while(|index| {
            let raw = bytes.get(4 + index * size..4 + (index + 1) * size)?;
            let descriptor = unsafe { std::ptr::read_unaligned(raw.as_ptr().cast::<FILEDESCRIPTORW>()) };
            Some(describe(&descriptor))
        })
        .collect()
}

fn describe(descriptor: &FILEDESCRIPTORW) -> Descriptor {
    let flags = descriptor.dwFlags;
    let units = descriptor.cFileName;
    let end = units.iter().position(|unit| *unit == 0).unwrap_or(units.len());
    let bytes = (u64::from(descriptor.nFileSizeHigh) << 32) | u64::from(descriptor.nFileSizeLow);
    Descriptor {
        name: String::from_utf16_lossy(&units[..end]),
        size: (flags & HAS_SIZE != 0).then_some(bytes as usize),
        folder: flags & HAS_ATTRIBUTES != 0 && descriptor.dwFileAttributes & DIRECTORY != 0,
    }
}

fn utf16(bytes: &[u8]) -> String {
    let units: Vec<u16> = bytes
        .chunks_exact(2)
        .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
        .take_while(|unit| *unit != 0)
        .collect();
    String::from_utf16_lossy(&units)
}

fn narrow(bytes: &[u8]) -> String {
    let end = bytes.iter().position(|byte| *byte == 0).unwrap_or(bytes.len());
    String::from_utf8_lossy(&bytes[..end]).into_owned()
}

struct Medium(STGMEDIUM);

impl Medium {
    fn get(data: &IDataObject, format: u16, index: i32, kinds: TYMED) -> Option<Self> {
        let request = FORMATETC {
            cfFormat: format,
            ptd: std::ptr::null_mut(),
            dwAspect: DVASPECT_CONTENT.0,
            lindex: index,
            tymed: kinds.0 as u32,
        };
        unsafe { data.GetData(&request) }.ok().map(Self)
    }

    fn global(&self) -> Option<HGLOBAL> {
        (self.0.tymed == TYMED_HGLOBAL.0 as u32).then_some(unsafe { self.0.u.hGlobal })
    }

    fn bytes(&self) -> Option<Vec<u8>> {
        if let Some(global) = self.global() {
            return global_bytes(global);
        }
        if self.0.tymed != TYMED_ISTREAM.0 as u32 {
            return None;
        }
        let stream = unsafe { &self.0.u.pstm };
        Option::as_ref(stream).and_then(read_stream)
    }
}

impl Drop for Medium {
    fn drop(&mut self) {
        unsafe { ReleaseStgMedium(&mut self.0) };
    }
}

fn global_bytes(global: HGLOBAL) -> Option<Vec<u8>> {
    let size = unsafe { GlobalSize(global) };
    if size > LARGEST {
        return None;
    }
    let pointer = unsafe { GlobalLock(global) };
    if pointer.is_null() {
        return None;
    }
    let bytes = unsafe { std::slice::from_raw_parts(pointer.cast::<u8>(), size) }.to_vec();
    let _ = unsafe { GlobalUnlock(global) };
    Some(bytes)
}

fn read_stream(stream: &IStream) -> Option<Vec<u8>> {
    let mut bytes = Vec::new();
    let mut chunk = vec![0u8; CHUNK];
    loop {
        let mut read = 0u32;
        if unsafe { stream.Read(chunk.as_mut_ptr().cast(), CHUNK as u32, Some(&mut read)) }.is_err() {
            return None;
        }
        if read == 0 {
            return Some(bytes);
        }
        bytes.extend_from_slice(&chunk[..read as usize]);
        if bytes.len() > LARGEST {
            return None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn descriptor(name: &str, flags: u32, attributes: u32, size: u64) -> FILEDESCRIPTORW {
        let mut descriptor = FILEDESCRIPTORW::default();
        let mut units = [0u16; 260];
        for (slot, unit) in units.iter_mut().zip(name.encode_utf16()) {
            *slot = unit;
        }
        descriptor.cFileName = units;
        descriptor.dwFlags = flags;
        descriptor.dwFileAttributes = attributes;
        descriptor.nFileSizeHigh = (size >> 32) as u32;
        descriptor.nFileSizeLow = size as u32;
        descriptor
    }

    fn group(descriptors: &[FILEDESCRIPTORW]) -> Vec<u8> {
        let mut bytes = (descriptors.len() as u32).to_le_bytes().to_vec();
        for descriptor in descriptors {
            let raw = unsafe {
                std::slice::from_raw_parts(
                    std::ptr::from_ref(descriptor).cast::<u8>(),
                    std::mem::size_of::<FILEDESCRIPTORW>(),
                )
            };
            bytes.extend_from_slice(raw);
        }
        bytes
    }

    #[test]
    fn file_group_lists_names_and_sizes() {
        let bytes = group(&[
            descriptor("images.jpeg", HAS_SIZE, 0, 5_000_000_000),
            descriptor("Album", HAS_ATTRIBUTES, DIRECTORY, 0),
            descriptor("notes.txt", 0, 0, 12),
        ]);
        let parsed = parse_descriptors(&bytes);
        assert_eq!(
            parsed,
            [
                Descriptor { name: "images.jpeg".into(), size: Some(5_000_000_000), folder: false },
                Descriptor { name: "Album".into(), size: None, folder: true },
                Descriptor { name: "notes.txt".into(), size: None, folder: false },
            ]
        );
    }

    #[test]
    fn short_file_group_stops_early() {
        let mut bytes = group(&[descriptor("a.png", 0, 0, 0)]);
        bytes[..4].copy_from_slice(&5u32.to_le_bytes());
        assert_eq!(parse_descriptors(&bytes).len(), 1);
        assert!(parse_descriptors(&[1, 0]).is_empty());
    }

    #[test]
    fn text_formats_stop_at_the_terminator() {
        let wide: Vec<u8> = "https://a.com/b.png\0junk".encode_utf16().flat_map(u16::to_le_bytes).collect();
        assert_eq!(utf16(&wide), "https://a.com/b.png");
        assert_eq!(narrow(b"https://a.com\0\0"), "https://a.com");
    }
}
