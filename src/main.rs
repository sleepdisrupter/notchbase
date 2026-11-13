//! notchbase, a fluid notch for macOS and Windows. One executable is the app, its installer, the assistant's hook
//! and a few commands, and `launch.rs` tells which one this run is.

// Release builds open no console window on Windows.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod annotate;
mod app;
mod assistant;
mod audio;
mod bridge;
mod capture;
mod config;
mod filedrag;
mod find;
mod installer;
mod launch;
mod media;
mod notch;
mod pages;
mod platform;
mod poll;
mod record;
mod shelf;
mod snap;
mod stall;
mod stats;
mod system;
mod tools;
mod ui;
mod update;
mod webvideo;
mod whats_new;

use launch::Launch;

fn main() {
    match Launch::current() {
        Launch::Hook => std::process::exit(assistant::run_hook_from_args().unwrap_or(0)),
        Launch::Installer => std::process::exit(installer::run_from_args().unwrap_or(1)),
        Launch::Probe => std::process::exit(system::permissions::run_probe()),
        Launch::Stray => {}
        Launch::App => start(),
    }
}

/// Runs the app, unless another copy already runs on this data folder. That one is asked to show itself.
fn start() {
    let arrival = installer::arrive();
    let Some(instance) = app::claim_instance(arrival) else {
        app::step_aside();
        return;
    };
    installer::note_version();
    keep_panics();
    app::run(arrival, instance);
}

/// The app has no console on Windows, so a crash would leave no trace: each panic is also added to crash.log in the
/// data folder.
fn keep_panics() {
    use std::io::Write as _;
    const MOST: u64 = 256 * 1024;
    let Some(file) = platform::data_dir().map(|dir| dir.join("crash.log")) else { return };
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let thread = std::thread::current();
        let since = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
        let text = format!(
            "--- notchbase {} panicked on thread '{}' (unix time {since})\n{info}\n{}\n",
            env!("CARGO_PKG_VERSION"),
            thread.name().unwrap_or("unnamed"),
            std::backtrace::Backtrace::force_capture()
        );
        if std::fs::metadata(&file).is_ok_and(|meta| meta.len() > MOST) {
            let _ = std::fs::remove_file(&file);
        }
        if let Ok(mut log) = std::fs::OpenOptions::new().create(true).append(true).open(&file) {
            let _ = log.write_all(text.as_bytes());
        }
        previous(info);
    }));
}
