//! The command line on Windows. The dock is a windowed program, so it borrows the console it was started from to
//! print its answers.

use std::time::Duration;

use windows_sys::Win32::System::Console::{ATTACH_PARENT_PROCESS, AttachConsole};

use super::{app, lifecycle, taskbar};

const HELP: &str = "notchbase dock: a Dock for Windows in the notchbase style

  notchbase-dock                 start the dock (hides the taskbar while it runs)
  notchbase-dock --quit          quit a running dock and bring the taskbar back
  notchbase-dock --restore-taskbar
                                 bring the taskbar back, even after a crash
  notchbase-dock --status        say whether a dock is running
  notchbase-dock --snapshot <out.png> [--scene <name>]
                                 draw the dock offscreen, without showing or hiding anything
";

pub fn run() -> i32 {
    unsafe { AttachConsole(ATTACH_PARENT_PROCESS) };
    let args: Vec<String> = std::env::args().skip(1).collect();
    let dir = lifecycle::data_dir();
    let has = |flag: &str| args.iter().any(|a| a == flag);
    let value = |flag: &str| args.iter().position(|a| a == flag).and_then(|i| args.get(i + 1)).cloned();

    if has("--help") || has("-h") {
        print!("{HELP}");
        return 0;
    }
    if has("--version") {
        println!("notchbase-dock {}", env!("CARGO_PKG_VERSION"));
        return 0;
    }
    if has("--status") {
        return match lifecycle::running_instance(&dir) {
            Some(pid) => {
                println!("running (pid {pid})");
                0
            }
            None => {
                println!("not running");
                1
            }
        };
    }
    if let Some(out) = value("--snapshot") {
        let scene = value("--scene").unwrap_or_else(|| "rest".into());
        return match crate::snapshot::run(out.into(), scene, dir) {
            Ok(()) => 0,
            Err(err) => {
                eprintln!("notchbase-dock: {err}");
                1
            }
        };
    }
    if has("--quit") || has("--restore-dock") || has("--restore-taskbar") {
        if let Some(pid) = lifecycle::running_instance(&dir) {
            if !lifecycle::ask_to_quit(pid, Duration::from_secs(5)) {
                eprintln!("notchbase-dock: the dock (pid {pid}) did not quit");
                return 1;
            }
            println!("quit the dock; the taskbar is back");
        } else if has("--quit") {
            println!("no dock is running");
        }
        match taskbar::restore(&dir) {
            Ok(true) => println!("restored the taskbar settings"),
            Ok(false) => taskbar::show_taskbars(),
            Err(err) => {
                eprintln!("notchbase-dock: could not restore the taskbar: {err}");
                return 1;
            }
        }
        return 0;
    }

    if let Some(pid) = lifecycle::running_instance(&dir) {
        println!("notchbase dock is already running (pid {pid})");
        return 0;
    }
    if let Err(err) = lifecycle::claim(&dir) {
        eprintln!("notchbase-dock: can't write {}: {err}", dir.display());
        return 1;
    }
    lifecycle::install_handlers(&dir);
    let result = app::run(dir.clone());
    // `run` puts the taskbar back on the way out; this covers an event loop that failed to start.
    let _ = taskbar::restore(&dir);
    lifecycle::release(&dir);
    match result {
        Ok(()) => 0,
        Err(err) => {
            eprintln!("notchbase-dock: {err}");
            1
        }
    }
}
