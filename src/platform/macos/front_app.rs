//! Clicking the notch makes notchbase the active app, so the app that was in front is remembered and handed back.

use std::path::PathBuf;

use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObjectProtocol as _};
use objc2::{MainThreadMarker, sel};
use objc2_app_kit::{
    NSApplication, NSApplicationActivationOptions, NSApplicationActivationPolicy, NSRunningApplication, NSWorkspace,
};

pub struct FrontApp(Retained<NSRunningApplication>);

pub fn remember_front_app() -> Option<FrontApp> {
    let workspace = NSWorkspace::sharedWorkspace();
    let me = NSRunningApplication::currentApplication();
    let me: &AnyObject = &me;
    [workspace.frontmostApplication(), workspace.menuBarOwningApplication()]
        .into_iter()
        .flatten()
        .find(|app| !app.isEqual(Some(me)))
        .map(FrontApp)
}

pub fn front_app_name() -> Option<String> {
    let me = NSRunningApplication::currentApplication();
    let me: &AnyObject = &me;
    let front = NSWorkspace::sharedWorkspace().frontmostApplication().filter(|app| !app.isEqual(Some(me)))?;
    front.localizedName().map(|name| name.to_string())
}

pub fn running_apps() -> Vec<PathBuf> {
    let running = NSWorkspace::sharedWorkspace().runningApplications();
    let shown = running.iter().filter(|app| app.activationPolicy() == NSApplicationActivationPolicy::Regular);
    shown.filter_map(|app| app.bundleURL()?.path()).map(|path| PathBuf::from(path.to_string())).collect()
}

pub fn restore_front_app(app: FrontApp) {
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let target = app.0;
    let ns_app = NSApplication::sharedApplication(mtm);
    if target.isTerminated() || !ns_app.isActive() {
        return;
    }
    let options = NSApplicationActivationOptions::empty();
    if ns_app.respondsToSelector(sel!(yieldActivationToApplication:))
        && target.respondsToSelector(sel!(activateFromApplication:options:))
    {
        ns_app.yieldActivationToApplication(&target);
        target.activateFromApplication_options(&NSRunningApplication::currentApplication(), options);
    } else {
        target.activateWithOptions(options);
    }
}
