use std::ffi::c_void;
use std::sync::atomic::{AtomicUsize, Ordering};

use super::counter::Counter;
use crate::ffi::{Buffer, HostWaker, Plugin, entry_symbol, serve};
use crate::protocol::{Answer, Call, Event};

static WAKES: AtomicUsize = AtomicUsize::new(0);

unsafe extern "C" fn count_wake(_: *const c_void) {
    WAKES.fetch_add(1, Ordering::SeqCst);
}

fn call(plugin: &Plugin, instance: *mut c_void, call: &Call) -> Answer {
    let request = serde_json::to_vec(call).unwrap();
    let buffer: Buffer = unsafe { (plugin.call)(instance, request.as_ptr(), request.len()) };
    let answer = serde_json::from_slice(buffer.as_slice()).unwrap();
    unsafe { (plugin.release)(buffer) };
    answer
}

#[test]
fn plugin_speaks_json_over_c() {
    let plugin = Plugin::of::<Counter>();
    assert_eq!(plugin.abi, crate::ffi::ABI_VERSION);
    let instance = unsafe { (plugin.create)(HostWaker { context: std::ptr::null(), wake: count_wake }) };
    let Answer::Manifest { manifest } = call(&plugin, instance, &Call::Manifest) else { panic!("a manifest") };
    assert_eq!(manifest.id, "counter");
    let Answer::Effects { effects } = call(&plugin, instance, &Call::Start { start: Box::default() }) else {
        panic!("effects")
    };
    assert_eq!(effects.wants.every_ms, vec![1000]);
    let events = vec![Event::Shortcut { id: "add".into() }];
    assert!(matches!(call(&plugin, instance, &Call::Handle { events }), Answer::Effects { .. }));
    let Answer::Views { views } = call(&plugin, instance, &Call::Views) else { panic!("views") };
    assert_eq!(views.tile.unwrap().title, "Counter");
    unsafe { (plugin.destroy)(instance) };
}

#[test]
fn garbage_gets_a_failure() {
    let mut runtime = crate::Runtime::<Counter>::new(crate::Waker::noop());
    assert!(matches!(serve(&mut runtime, b"{nope"), Answer::Failed { .. }));
}

#[test]
fn entry_symbol_follows_library() {
    assert_eq!(entry_symbol("focus_timer"), "notchbase_tool_focus_timer");
    assert_eq!(entry_symbol("my-tool"), "notchbase_tool_my_tool");
    let dylib = if cfg!(windows) { "notchbase_tool_libfocus_timer" } else { "notchbase_tool_focus_timer" };
    assert_eq!(entry_symbol("libfocus_timer"), dylib);
}

#[test]
fn portable_tool_remembers_panic() {
    use crate::ffi::portable::{describe, last_panic, panicked, remember};
    assert_eq!((last_panic(), panicked()), (None, 0), "nothing to report until something went wrong");
    assert_eq!(describe(Some("index out of bounds"), Some(("src/lib.rs", 44))), "index out of bounds (src/lib.rs:44)");
    assert_eq!(describe(None, None), "the tool panicked");
    remember(describe(Some("no such key"), Some(("src/lib.rs", 7))));
    remember(describe(Some("index out of bounds"), Some(("src/lib.rs", 44))));
    assert_eq!(last_panic().as_deref(), Some("index out of bounds (src/lib.rs:44)"), "the last one is what stopped it");
}
