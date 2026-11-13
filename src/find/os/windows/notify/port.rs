//! The completion port loop that drains the change notifications of every drive.

use std::os::windows::ffi::OsStrExt as _;
use std::path::PathBuf;
use std::ptr;
use std::sync::Arc;
use std::sync::mpsc::Sender;
use std::thread::{self, JoinHandle};

use windows::Win32::Foundation::{CloseHandle, ERROR_NOTIFY_ENUM_DIR, HANDLE, INVALID_HANDLE_VALUE};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OVERLAPPED, FILE_LIST_DIRECTORY, FILE_NOTIFY_CHANGE,
    FILE_NOTIFY_CHANGE_CREATION, FILE_NOTIFY_CHANGE_DIR_NAME, FILE_NOTIFY_CHANGE_FILE_NAME,
    FILE_NOTIFY_CHANGE_LAST_WRITE, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
    ReadDirectoryChangesExW, ReadDirectoryNotifyExtendedInformation,
};
use windows::Win32::System::IO::{
    CancelIoEx, CreateIoCompletionPort, GetQueuedCompletionStatus, OVERLAPPED, PostQueuedCompletionStatus,
};
use windows::core::PCWSTR;

use crate::find::WATCH_THREAD;
use crate::find::os::Root;
use crate::find::shared::Order;
use crate::find::watch::notify::parse;
use crate::find::watch::{Burst, Change, Event, Watcher};

use super::super::bus::detachable;

const NOTIFY_BUFFER: usize = 256 * 1024;
const WORD: usize = 4;
const STOP: usize = usize::MAX;
const ONE_THREAD: u32 = 1;
const FOREVER: u32 = u32::MAX;
const DRAIN_PATIENCE: u32 = 1_000;
const FILTER: FILE_NOTIFY_CHANGE = FILE_NOTIFY_CHANGE(
    FILE_NOTIFY_CHANGE_FILE_NAME.0
        | FILE_NOTIFY_CHANGE_DIR_NAME.0
        | FILE_NOTIFY_CHANGE_LAST_WRITE.0
        | FILE_NOTIFY_CHANGE_CREATION.0,
);

struct Port(HANDLE);

unsafe impl Send for Port {}
unsafe impl Sync for Port {}

impl Port {
    fn ask_to_stop(&self) {
        let _ = unsafe { PostQueuedCompletionStatus(self.0, 0, STOP, None) };
    }
}

impl Drop for Port {
    fn drop(&mut self) {
        let _ = unsafe { CloseHandle(self.0) };
    }
}

struct Armed {
    port: Arc<Port>,
    drives: Vec<Option<Drive>>,
}

impl Drop for Armed {
    fn drop(&mut self) {
        stop(&self.port, &mut self.drives);
    }
}

struct Drive {
    root: PathBuf,
    handle: HANDLE,
    overlapped: Box<OVERLAPPED>,
    buffer: Vec<u32>,
    pending: bool,
}

unsafe impl Send for Drive {}

impl Drive {
    fn open(root: &Root, port: HANDLE, key: usize) -> Option<Self> {
        let wide: Vec<u16> = root.path.as_os_str().encode_wide().chain([0]).collect();
        let share = FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE;
        let flags = FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OVERLAPPED;
        let opened = unsafe {
            CreateFileW(PCWSTR(wide.as_ptr()), FILE_LIST_DIRECTORY.0, share, None, OPEN_EXISTING, flags, None)
        };
        let handle = opened.ok()?;
        let buffer = vec![0; NOTIFY_BUFFER / WORD];
        let mut drive = Self { root: root.path.clone(), handle, overlapped: Box::default(), buffer, pending: false };
        let joined = unsafe { CreateIoCompletionPort(handle, Some(port), key, ONE_THREAD) };
        (joined.is_ok() && drive.arm()).then_some(drive)
    }

    fn arm(&mut self) -> bool {
        *self.overlapped = OVERLAPPED::default();
        let length = (self.buffer.len() * WORD) as u32;
        let armed = unsafe {
            ReadDirectoryChangesExW(
                self.handle,
                self.buffer.as_mut_ptr().cast(),
                length,
                true,
                FILTER,
                None,
                Some(ptr::from_mut(&mut *self.overlapped)),
                None,
                ReadDirectoryNotifyExtendedInformation,
            )
        };
        self.pending = armed.is_ok();
        self.pending
    }

    fn bytes(&self, length: usize) -> &[u8] {
        let all = unsafe { std::slice::from_raw_parts(self.buffer.as_ptr().cast::<u8>(), self.buffer.len() * WORD) };
        &all[..length.min(all.len())]
    }
}

impl Drop for Drive {
    fn drop(&mut self) {
        let _ = unsafe { CloseHandle(self.handle) };
    }
}

pub(super) fn watch(roots: &[Root], sink: Sender<Order>) -> Option<Watcher> {
    let port = Arc::new(Port(unsafe { CreateIoCompletionPort(INVALID_HANDLE_VALUE, None, 0, ONE_THREAD) }.ok()?));
    let mut drives = Vec::with_capacity(roots.len());
    for (key, root) in roots.iter().enumerate() {
        let drive = (!detachable(&root.path)).then(|| Drive::open(root, port.0, key)).flatten();
        if drive.is_none() {
            let _ = sink.send(Order::Unwatched(root.path.clone()));
        }
        drives.push(drive);
    }
    if drives.iter().all(Option::is_none) {
        return None;
    }
    let armed = Armed { port: port.clone(), drives };
    let named = thread::Builder::new().name(WATCH_THREAD.to_owned());
    let thread: JoinHandle<()> = named.spawn(move || drain(armed, &sink)).ok()?;
    Some(Watcher::new(move || {
        port.ask_to_stop();
        let _ = thread.join();
    }))
}

fn drain(mut armed: Armed, sink: &Sender<Order>) {
    let mut counter = 0;
    loop {
        let Some((key, bytes, result)) = next(&armed.port, FOREVER) else { continue };
        if key == STOP {
            break;
        }
        let Some(drive) = armed.drives.get_mut(key).and_then(Option::as_mut) else { continue };
        drive.pending = false;
        let events = match result {
            Err(code) if code == ERROR_NOTIFY_ENUM_DIR.to_hresult() => overflow(&drive.root, &mut counter),
            Err(_) => Vec::new(),
            Ok(()) if bytes == 0 => overflow(&drive.root, &mut counter),
            Ok(()) => parse(drive.bytes(bytes as usize), &drive.root, &mut counter),
        };
        let failed = result.is_err() && events.is_empty();
        if !events.is_empty() && sink.send(Order::Changes(Burst { events, caught_up: false })).is_err() {
            break;
        }
        if failed || !drive.arm() {
            let _ = sink.send(Order::Unwatched(drive.root.clone()));
            armed.drives[key] = None;
        }
    }
}

fn next(port: &Port, wait: u32) -> Option<(usize, u32, Result<(), windows::core::HRESULT>)> {
    let (mut bytes, mut key, mut overlapped) = (0u32, 0usize, ptr::null_mut::<OVERLAPPED>());
    let waited = unsafe { GetQueuedCompletionStatus(port.0, &mut bytes, &mut key, &mut overlapped, wait) };
    if waited.is_err() && overlapped.is_null() {
        return (wait == FOREVER).then_some((STOP, 0, Ok(())));
    }
    Some((key, bytes, waited.map_err(|failed| failed.code())))
}

fn overflow(root: &std::path::Path, counter: &mut u64) -> Vec<Event> {
    *counter += 1;
    vec![Event { change: Change::Overflow(root.to_path_buf()), id: *counter }]
}

fn stop(port: &Port, drives: &mut [Option<Drive>]) {
    for drive in drives.iter_mut().flatten().filter(|drive| drive.pending) {
        let _ = unsafe { CancelIoEx(drive.handle, Some(ptr::from_ref(&*drive.overlapped))) };
    }
    while drives.iter().flatten().any(|drive| drive.pending) {
        let Some((key, _, _)) = next(port, DRAIN_PATIENCE) else { break };
        if let Some(drive) = drives.get_mut(key).and_then(Option::as_mut) {
            drive.pending = false;
        }
    }
    let unfinished = drives.iter_mut().filter(|drive| drive.as_ref().is_some_and(|drive| drive.pending));
    unfinished.for_each(|drive| std::mem::forget(drive.take()));
    drives.iter_mut().for_each(|drive| *drive = None);
}
