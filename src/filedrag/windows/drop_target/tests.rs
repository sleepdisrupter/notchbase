use std::mem::ManuallyDrop;
use std::os::windows::ffi::OsStrExt as _;
use std::path::{Path, PathBuf};
use std::sync::MutexGuard;

use ::windows::Win32::Foundation::HGLOBAL;
use ::windows::Win32::System::Com::{
    DVASPECT_CONTENT, FORMATETC, STGMEDIUM, STGMEDIUM_0, TYMED_HGLOBAL, TYMED_ISTREAM,
};
use ::windows::Win32::System::DataExchange::RegisterClipboardFormatW;
use ::windows::Win32::System::Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalUnlock};
use ::windows::Win32::System::Ole::{CF_HDROP, CF_UNICODETEXT, DROPEFFECT_MOVE, OleUninitialize};
use ::windows::Win32::UI::Shell::{DROPFILES, FILEDESCRIPTORW, SHCreateDataObject, SHCreateMemStream};
use ::windows::core::{BOOL, HSTRING};

use super::*;
use crate::filedrag::incoming::Dropped;

const JPEG: &[u8] = &[0xFF, 0xD8, 0xFF, 0xE0, 0, 0x10, b'J', b'F', b'I', b'F', 0, 0, 0, 0];

struct Ole {
    _turn: MutexGuard<'static, ()>,
}

impl Ole {
    fn start() -> Self {
        let turn = crate::filedrag::tests::shell_turn();
        unsafe { OleInitialize(None) }.unwrap();
        Self { _turn: turn }
    }
}

impl Drop for Ole {
    fn drop(&mut self) {
        unsafe { OleUninitialize() };
    }
}

fn target(inbox: &Inbox) -> IDropTarget {
    let window = WindowId::dummy();
    DropTarget { window, hwnd: HWND::default(), inbox: inbox.clone(), helper: None, accepting: AtomicBool::new(false) }
        .into()
}

fn empty_data() -> IDataObject {
    unsafe { SHCreateDataObject(None, None, None::<&IDataObject>) }.unwrap()
}

fn format(name: &str) -> u16 {
    unsafe { RegisterClipboardFormatW(&HSTRING::from(name)) as u16 }
}

fn global(bytes: &[u8]) -> HGLOBAL {
    let handle = unsafe { GlobalAlloc(GMEM_MOVEABLE, bytes.len().max(1)) }.unwrap();
    let pointer = unsafe { GlobalLock(handle) };
    unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), pointer.cast::<u8>(), bytes.len()) };
    let _ = unsafe { GlobalUnlock(handle) };
    handle
}

fn in_memory(bytes: &[u8]) -> STGMEDIUM {
    STGMEDIUM {
        tymed: TYMED_HGLOBAL.0 as u32,
        u: STGMEDIUM_0 { hGlobal: global(bytes) },
        pUnkForRelease: ManuallyDrop::new(None),
    }
}

fn streamed(bytes: &[u8]) -> STGMEDIUM {
    let stream = unsafe { SHCreateMemStream(Some(bytes)) };
    STGMEDIUM {
        tymed: TYMED_ISTREAM.0 as u32,
        u: STGMEDIUM_0 { pstm: ManuallyDrop::new(stream) },
        pUnkForRelease: ManuallyDrop::new(None),
    }
}

fn offer(data: &IDataObject, format: u16, index: i32, medium: STGMEDIUM) {
    let request = FORMATETC {
        cfFormat: format,
        ptd: std::ptr::null_mut(),
        dwAspect: DVASPECT_CONTENT.0,
        lindex: index,
        tymed: medium.tymed,
    };
    unsafe { data.SetData(&request, &medium, true) }.unwrap();
}

fn wide(text: &str) -> Vec<u8> {
    text.encode_utf16().chain([0]).flat_map(u16::to_le_bytes).collect()
}

fn file_group(name: &str, size: usize) -> Vec<u8> {
    let mut descriptor = FILEDESCRIPTORW { dwFlags: 0x40, nFileSizeLow: size as u32, ..FILEDESCRIPTORW::default() };
    let mut units = [0u16; 260];
    for (slot, unit) in units.iter_mut().zip(name.encode_utf16()) {
        *slot = unit;
    }
    descriptor.cFileName = units;
    let raw = unsafe {
        std::slice::from_raw_parts(std::ptr::from_ref(&descriptor).cast::<u8>(), std::mem::size_of::<FILEDESCRIPTORW>())
    };
    let mut bytes = 1u32.to_le_bytes().to_vec();
    bytes.extend_from_slice(raw);
    bytes
}

fn file_drop(paths: &[&Path]) -> Vec<u8> {
    let header = DROPFILES {
        pFiles: std::mem::size_of::<DROPFILES>() as u32,
        fNC: BOOL::from(false),
        fWide: BOOL::from(true),
        ..DROPFILES::default()
    };
    let mut bytes = unsafe {
        std::slice::from_raw_parts(std::ptr::from_ref(&header).cast::<u8>(), std::mem::size_of::<DROPFILES>())
    }
    .to_vec();
    for path in paths {
        bytes.extend(path.as_os_str().encode_wide().chain([0]).flat_map(u16::to_le_bytes));
    }
    bytes.extend([0, 0]);
    bytes
}

fn drag(target: &IDropTarget, data: &IDataObject, allowed: DROPEFFECT) -> (DROPEFFECT, DROPEFFECT) {
    let mut entered = allowed;
    unsafe { target.DragEnter(data, MODIFIERKEYS_FLAGS(0), POINTL::default(), &mut entered) }.unwrap();
    let mut dropped = allowed;
    unsafe { target.Drop(data, MODIFIERKEYS_FLAGS(0), POINTL::default(), &mut dropped) }.unwrap();
    (entered, dropped)
}

fn arrivals(inbox: &Inbox) -> Vec<Arrival> {
    inbox.take().into_iter().map(|(_, arrival)| arrival).collect()
}

#[test]
fn browser_image_arrives_as_bytes() {
    let _ole = Ole::start();
    let inbox = Inbox::new(None, || {});
    let data = empty_data();
    offer(&data, format("FileGroupDescriptorW"), -1, in_memory(&file_group("images.jpeg", JPEG.len())));
    offer(&data, format("FileContents"), 0, streamed(JPEG));
    offer(&data, format("UniformResourceLocatorW"), -1, in_memory(&wide("https://www.google.com/imgres?imgurl=x")));
    let effects = drag(&target(&inbox), &data, DROPEFFECT(DROPEFFECT_COPY.0 | DROPEFFECT_LINK.0 | DROPEFFECT_MOVE.0));
    assert_eq!(effects, (DROPEFFECT_COPY, DROPEFFECT_COPY));
    let data = Dropped::Data { name: Some("images.jpeg".into()), bytes: JPEG.to_vec() };
    assert_eq!(arrivals(&inbox), [Arrival::Hover(1), Arrival::Drop(vec![data])]);
}

#[test]
fn files_from_explorer_arrive_as_paths() {
    let _ole = Ole::start();
    let inbox = Inbox::new(None, || {});
    let data = empty_data();
    let files = [PathBuf::from(r"C:\Users\me\a.png"), PathBuf::from(r"D:\b c\d.pdf")];
    offer(&data, CF_HDROP.0, -1, in_memory(&file_drop(&[&files[0], &files[1]])));
    drag(&target(&inbox), &data, DROPEFFECT_COPY);
    let dropped = files.iter().cloned().map(Dropped::File).collect();
    assert_eq!(arrivals(&inbox), [Arrival::Hover(2), Arrival::Drop(dropped)]);
}

#[test]
fn linked_image_arrives_as_source() {
    let _ole = Ole::start();
    let inbox = Inbox::new(None, || {});
    let data = empty_data();
    let html =
        "Version:0.9\r\nStartHTML:0\r\n<html><body><a href=\"/x\"><img src=\"https://cdn.example.com/cat.webp\"></a>\0";
    offer(&data, format("HTML Format"), -1, in_memory(html.as_bytes()));
    offer(&data, format("UniformResourceLocatorW"), -1, in_memory(&wide("https://example.com/page")));
    let (entered, _) = drag(&target(&inbox), &data, DROPEFFECT_LINK);
    assert_eq!(entered, DROPEFFECT_LINK);
    let link = Dropped::Link("https://cdn.example.com/cat.webp".into());
    assert_eq!(arrivals(&inbox), [Arrival::Hover(1), Arrival::Drop(vec![link])]);
}

#[test]
fn text_and_move_only_drags_refused() {
    let _ole = Ole::start();
    let inbox = Inbox::new(None, || {});
    let data = empty_data();
    offer(&data, CF_UNICODETEXT.0, -1, in_memory(&wide("just some words")));
    assert_eq!(drag(&target(&inbox), &data, DROPEFFECT_COPY), (DROPEFFECT_NONE, DROPEFFECT_NONE));
    assert_eq!(arrivals(&inbox), [Arrival::Leave]);
    let files = empty_data();
    offer(&files, CF_HDROP.0, -1, in_memory(&file_drop(&[Path::new(r"C:\a.png")])));
    assert_eq!(drag(&target(&inbox), &files, DROPEFFECT_MOVE), (DROPEFFECT_NONE, DROPEFFECT_NONE));
    assert_eq!(arrivals(&inbox), [Arrival::Hover(1), Arrival::Leave]);
}

#[test]
fn leaving_takes_hover_back_once() {
    let _ole = Ole::start();
    let inbox = Inbox::new(None, || {});
    let data = empty_data();
    offer(&data, CF_HDROP.0, -1, in_memory(&file_drop(&[Path::new(r"C:\a.png")])));
    let target = target(&inbox);
    let mut effect = DROPEFFECT_COPY;
    unsafe { target.DragEnter(&data, MODIFIERKEYS_FLAGS(0), POINTL::default(), &mut effect) }.unwrap();
    unsafe { target.DragLeave() }.unwrap();
    unsafe { target.DragLeave() }.unwrap();
    assert_eq!(arrivals(&inbox), [Arrival::Hover(1), Arrival::Leave]);
}
