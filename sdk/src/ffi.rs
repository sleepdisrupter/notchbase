//! The plugin boundary. Native plugins export a C entry point and portable ones export `nb_*`
//! functions, and both trade JSON with the host.

use std::any::Any;
use std::ffi::c_void;
use std::panic::{AssertUnwindSafe, catch_unwind};

use crate::driver::{Driver, Waker};
use crate::protocol::{Answer, Call};
use crate::runtime::Runtime;
use crate::tool::Tool;

pub const ABI_VERSION: u32 = 1;
pub const ENTRY_PREFIX: &str = "notchbase_tool_";

/// Bytes handed to the host. It gives them back through `release`, so the plugin's allocator frees them.
#[repr(C)]
pub struct Buffer {
    pub ptr: *mut u8,
    pub len: usize,
    pub cap: usize,
}

impl Buffer {
    fn from_vec(bytes: Vec<u8>) -> Self {
        let mut bytes = std::mem::ManuallyDrop::new(bytes);
        Self { ptr: bytes.as_mut_ptr(), len: bytes.len(), cap: bytes.capacity() }
    }

    pub fn as_slice(&self) -> &[u8] {
        if self.ptr.is_null() { &[] } else { unsafe { std::slice::from_raw_parts(self.ptr, self.len) } }
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct HostWaker {
    pub context: *const c_void,
    pub wake: unsafe extern "C" fn(*const c_void),
}

struct SendWaker(HostWaker);

unsafe impl Send for SendWaker {}
unsafe impl Sync for SendWaker {}

impl SendWaker {
    fn wake(&self) {
        unsafe { (self.0.wake)(self.0.context) };
    }
}

#[repr(C)]
pub struct Plugin {
    pub abi: u32,
    pub create: unsafe extern "C" fn(HostWaker) -> *mut c_void,
    pub call: unsafe extern "C" fn(*mut c_void, *const u8, usize) -> Buffer,
    pub release: unsafe extern "C" fn(Buffer),
    pub destroy: unsafe extern "C" fn(*mut c_void),
}

pub type Entry = unsafe extern "C" fn() -> *const Plugin;

impl Plugin {
    pub const fn of<T: Tool>() -> Self {
        Self { abi: ABI_VERSION, create: create::<T>, call: call::<T>, release, destroy: destroy::<T> }
    }
}

/// The symbol the host looks up in a plugin library. Windows libraries have no `lib` prefix, and
/// symbols can't hold dashes.
pub fn entry_symbol(library_stem: &str) -> String {
    let name = if cfg!(windows) { library_stem } else { library_stem.strip_prefix("lib").unwrap_or(library_stem) };
    format!("{ENTRY_PREFIX}{}", name.replace('-', "_"))
}

unsafe extern "C" fn create<T: Tool>(waker: HostWaker) -> *mut c_void {
    let host = SendWaker(waker);
    let runtime = Runtime::<T>::new(Waker::new(move || host.wake()));
    Box::into_raw(Box::new(runtime)).cast()
}

unsafe extern "C" fn call<T: Tool>(instance: *mut c_void, request: *const u8, len: usize) -> Buffer {
    // A panic must never unwind into the host.
    let answer = catch_unwind(AssertUnwindSafe(|| {
        let runtime = unsafe { &mut *instance.cast::<Runtime<T>>() };
        let bytes = if request.is_null() { &[][..] } else { unsafe { std::slice::from_raw_parts(request, len) } };
        serve(runtime, bytes)
    }))
    .unwrap_or_else(|panic| Answer::Failed { error: panic_message(panic.as_ref()) });
    Buffer::from_vec(serde_json::to_vec(&answer).unwrap_or_default())
}

unsafe extern "C" fn release(buffer: Buffer) {
    if !buffer.ptr.is_null() {
        drop(unsafe { Vec::from_raw_parts(buffer.ptr, buffer.len, buffer.cap) });
    }
}

unsafe extern "C" fn destroy<T: Tool>(instance: *mut c_void) {
    if !instance.is_null() {
        drop(unsafe { Box::from_raw(instance.cast::<Runtime<T>>()) });
    }
}

pub fn serve(driver: &mut dyn Driver, request: &[u8]) -> Answer {
    match serde_json::from_slice::<Call>(request) {
        Ok(Call::Manifest) => Answer::Manifest { manifest: driver.manifest() },
        Ok(Call::Start { start }) => Answer::Effects { effects: driver.start(*start) },
        Ok(Call::Handle { events }) => Answer::Effects { effects: driver.handle(events) },
        Ok(Call::Views) => Answer::Views { views: Box::new(driver.views()) },
        Err(err) => Answer::Failed { error: format!("unreadable call: {err}") },
    }
}

pub fn panic_message(panic: &(dyn Any + Send)) -> String {
    panic
        .downcast_ref::<&str>()
        .map(|text| (*text).to_owned())
        .or_else(|| panic.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "the tool panicked".to_owned())
}

pub mod portable {
    use std::cell::RefCell;

    use super::{Answer, Driver, Runtime, Tool, Waker, serve};

    const PANICKED: &str = "the tool panicked";

    thread_local! {
        static TOOL: RefCell<Option<Box<dyn Driver>>> = const { RefCell::new(None) };
        static PANIC: RefCell<Option<String>> = const { RefCell::new(None) };
    }

    pub fn alloc(len: u32) -> u32 {
        let bytes = vec![0_u8; len as usize].into_boxed_slice();
        Box::into_raw(bytes).cast::<u8>() as usize as u32
    }

    pub unsafe fn free(ptr: u32, len: u32) {
        if ptr != 0 {
            let bytes = std::ptr::slice_from_raw_parts_mut(ptr as usize as *mut u8, len as usize);
            drop(unsafe { Box::from_raw(bytes) });
        }
    }

    pub unsafe fn call<T: Tool>(ptr: u32, len: u32) -> u64 {
        let request = if ptr == 0 {
            &[][..]
        } else {
            unsafe { std::slice::from_raw_parts(ptr as usize as *const u8, len as usize) }
        };
        pack(answer::<T>(request))
    }

    pub fn answer<T: Tool>(request: &[u8]) -> Vec<u8> {
        #[cfg(target_arch = "wasm32")]
        remember_panics();
        let answer = TOOL.with(|tool| {
            let mut tool = tool.borrow_mut();
            let driver = tool.get_or_insert_with(|| Box::new(Runtime::<T>::new(Waker::noop())));
            serve(driver.as_mut(), request)
        });
        serde_json::to_vec(&answer).unwrap_or_else(|err| failed(&err.to_string()))
    }

    #[cfg(target_arch = "wasm32")]
    fn remember_panics() {
        static HOOKED: std::sync::Once = std::sync::Once::new();
        HOOKED.call_once(|| {
            std::panic::set_hook(Box::new(|panic| {
                let at = panic.location().map(|at| (at.file(), at.line()));
                remember(describe(panic.payload_as_str(), at));
            }));
        });
    }

    pub fn describe(said: Option<&str>, at: Option<(&str, u32)>) -> String {
        let said = said.unwrap_or(PANICKED);
        match at {
            Some((file, line)) => format!("{said} ({file}:{line})"),
            None => said.to_owned(),
        }
    }

    pub fn remember(panic: String) {
        PANIC.with(|last| {
            if let Ok(mut last) = last.try_borrow_mut() {
                *last = Some(panic);
            }
        });
    }

    pub fn last_panic() -> Option<String> {
        PANIC.with(|last| last.try_borrow().ok().and_then(|last| last.clone()))
    }

    pub fn panicked() -> u64 {
        last_panic().map_or(0, |panic| pack(panic.into_bytes()))
    }

    /// A wasm export returns one number, so the pointer and the length share a u64.
    pub fn pack(bytes: Vec<u8>) -> u64 {
        let bytes = bytes.into_boxed_slice();
        let len = bytes.len() as u64;
        let ptr = Box::into_raw(bytes).cast::<u8>() as usize as u64;
        ptr << 32 | len
    }

    pub fn unpack(packed: u64) -> (u32, u32) {
        ((packed >> 32) as u32, packed as u32)
    }

    fn failed(error: &str) -> Vec<u8> {
        serde_json::to_vec(&Answer::Failed { error: error.to_owned() }).unwrap_or_default()
    }
}

/// Exports a tool as a plugin: a C entry point in native builds, or the `nb_*` functions in
/// WebAssembly.
#[macro_export]
macro_rules! export {
    ($tool:ty) => {
        #[cfg(not(target_arch = "wasm32"))]
        #[unsafe(export_name = concat!("notchbase_tool_", env!("CARGO_CRATE_NAME")))]
        pub extern "C" fn __notchbase_tool_entry() -> *const $crate::ffi::Plugin {
            static PLUGIN: $crate::ffi::Plugin = $crate::ffi::Plugin::of::<$tool>();
            &PLUGIN
        }

        #[cfg(target_arch = "wasm32")]
        const _: () = {
            #[unsafe(no_mangle)]
            pub extern "C" fn nb_abi() -> u32 {
                $crate::ffi::ABI_VERSION
            }

            #[unsafe(no_mangle)]
            pub extern "C" fn nb_alloc(len: u32) -> u32 {
                $crate::ffi::portable::alloc(len)
            }

            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn nb_free(ptr: u32, len: u32) {
                unsafe { $crate::ffi::portable::free(ptr, len) }
            }

            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn nb_call(ptr: u32, len: u32) -> u64 {
                unsafe { $crate::ffi::portable::call::<$tool>(ptr, len) }
            }

            #[unsafe(no_mangle)]
            pub extern "C" fn nb_panic() -> u64 {
                $crate::ffi::portable::panicked()
            }
        };
    };
}
