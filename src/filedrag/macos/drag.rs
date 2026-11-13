//! Starts a drag of shelf files from the notch's view, one file URL item per file with its Finder icon.

use std::path::{Path, PathBuf};

use objc2::rc::Retained;
use objc2::runtime::{AnyObject, ProtocolObject};
use objc2::{AllocAnyThread as _, MainThreadMarker};
use objc2_app_kit::{
    NSApplication, NSDraggingItem, NSEvent, NSEventModifierFlags, NSEventType, NSView, NSWindow, NSWorkspace,
};
use objc2_core_foundation::CGFloat;
use objc2_foundation::{NSArray, NSPoint, NSProcessInfo, NSRect, NSSize};
use raw_window_handle::{HasWindowHandle as _, RawWindowHandle};
use winit::window::Window;

use super::drag_source::{DragSource, session_active};
use super::paths::{file_url, ns_string_path};
use super::sensor::primary_button_down;

const DRAG_ICON_POINTS: CGFloat = 64.0;

pub(in crate::filedrag) fn dragging_out() -> bool {
    session_active() && primary_button_down()
}

pub(in crate::filedrag) fn start_drag(window: &Window, paths: &[PathBuf]) -> Result<(), String> {
    let mtm = MainThreadMarker::new().ok_or("Drags must start on the main thread")?;
    let view = content_view(window)?;
    let ns_window = view.window().ok_or("The view is not in a window")?;
    let event = triggering_event(mtm, &ns_window).ok_or("No mouse event to drag from")?;
    let items = dragging_items(paths, icon_frame_under_cursor(view, &event))?;
    let source = DragSource::for_new_session(mtm);
    let session = view.beginDraggingSessionWithItems_event_source(
        &NSArray::from_retained_slice(&items),
        &event,
        ProtocolObject::from_ref(&*source),
    );
    session.setAnimatesToStartingPositionsOnCancelOrFail(true);
    Ok(())
}

pub(super) fn content_view(window: &Window) -> Result<&NSView, String> {
    let handle = window.window_handle().map_err(|e| e.to_string())?;
    let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
        return Err("Not an AppKit window".into());
    };
    Ok(unsafe { handle.ns_view.cast::<NSView>().as_ref() })
}

/// The mouse event the drag starts from: the current one if it's a left press or drag here, else a made-up one.
fn triggering_event(mtm: MainThreadMarker, window: &NSWindow) -> Option<Retained<NSEvent>> {
    current_mouse_event_in(mtm, window).or_else(|| synthesized_drag_event(window))
}

fn current_mouse_event_in(mtm: MainThreadMarker, window: &NSWindow) -> Option<Retained<NSEvent>> {
    NSApplication::sharedApplication(mtm)
        .currentEvent()
        .filter(|event| is_left_press_or_drag(event) && event.windowNumber() == window.windowNumber())
}

fn is_left_press_or_drag(event: &NSEvent) -> bool {
    matches!(event.r#type(), NSEventType::LeftMouseDown | NSEventType::LeftMouseDragged)
}

fn synthesized_drag_event(window: &NSWindow) -> Option<Retained<NSEvent>> {
    NSEvent::mouseEventWithType_location_modifierFlags_timestamp_windowNumber_context_eventNumber_clickCount_pressure(
        NSEventType::LeftMouseDragged,
        window.convertPointFromScreen(NSEvent::mouseLocation()),
        NSEventModifierFlags::empty(),
        NSProcessInfo::processInfo().systemUptime(),
        window.windowNumber(),
        None,
        0,
        1,
        1.0,
    )
}

fn icon_frame_under_cursor(view: &NSView, event: &NSEvent) -> NSRect {
    let cursor = view.convertPoint_fromView(event.locationInWindow(), None);
    let half = DRAG_ICON_POINTS / 2.0;
    NSRect::new(NSPoint::new(cursor.x - half, cursor.y - half), NSSize::new(DRAG_ICON_POINTS, DRAG_ICON_POINTS))
}

fn dragging_items(paths: &[PathBuf], frame: NSRect) -> Result<Vec<Retained<NSDraggingItem>>, String> {
    let workspace = NSWorkspace::sharedWorkspace();
    paths.iter().map(|path| dragging_item(&workspace, path, frame)).collect()
}

fn dragging_item(workspace: &NSWorkspace, path: &Path, frame: NSRect) -> Result<Retained<NSDraggingItem>, String> {
    let path_string =
        ns_string_path(path).ok_or_else(|| format!("Couldn't drag {}: its path isn't valid UTF-8", path.display()))?;
    let url = file_url(&path_string);
    let item = NSDraggingItem::initWithPasteboardWriter(NSDraggingItem::alloc(), ProtocolObject::from_ref(&*url));
    let icon = workspace.iconForFile(&path_string);
    icon.setSize(frame.size);
    let contents: &AnyObject = &icon;
    unsafe { item.setDraggingFrame_contents(frame, Some(contents)) };
    Ok(item)
}
