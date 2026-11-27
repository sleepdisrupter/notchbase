//! notchbase dock: a Dock for macOS in the notchbase style. See README.md.

// The dock itself: its drawing, tiles, menus, stacks, flights and settings.
#[cfg(target_os = "macos")]
mod badges;
#[cfg(target_os = "macos")]
mod clock;
#[cfg(target_os = "macos")]
mod config;
#[cfg(target_os = "macos")]
mod dock;
#[cfg(target_os = "macos")]
mod downloads;
#[cfg(target_os = "macos")]
mod flight;
#[cfg(target_os = "macos")]
mod flow;
#[cfg(target_os = "macos")]
mod gl;
#[cfg(target_os = "macos")]
mod layout;
#[cfg(target_os = "macos")]
mod liquid;
#[cfg(target_os = "macos")]
mod minimized;
#[cfg(target_os = "macos")]
mod pop;
#[cfg(target_os = "macos")]
mod pulse;
#[cfg(target_os = "macos")]
mod recorder;
#[cfg(target_os = "macos")]
mod side;
#[cfg(target_os = "macos")]
mod snapshot;
#[cfg(target_os = "macos")]
mod stack;
#[cfg(target_os = "macos")]
mod stress;
#[cfg(target_os = "macos")]
mod tiles;
#[cfg(target_os = "macos")]
mod usage;

// AppKit, Accessibility, ScreenCaptureKit and the system Dock.
#[cfg(target_os = "macos")]
mod app;
#[cfg(target_os = "macos")]
mod app_menu;
#[cfg(target_os = "macos")]
mod avoid;
#[cfg(target_os = "macos")]
mod files;
#[cfg(target_os = "macos")]
mod glass;
#[cfg(target_os = "macos")]
mod lifecycle;
#[cfg(target_os = "macos")]
mod mac;
#[cfg(target_os = "macos")]
mod real_dock;
#[cfg(target_os = "macos")]
mod switcher;
#[cfg(target_os = "macos")]
mod titlebar;
#[cfg(target_os = "macos")]
mod wallpaper;
#[cfg(target_os = "macos")]
mod windows;
/// What the shared dock asks of the system.
#[cfg(target_os = "macos")]
use mac as os;

#[cfg(target_os = "macos")]
const HELP: &str = "notchbase dock: a Dock for macOS in the notchbase style

Usage:
  notchbase-dock                 start the dock and hide the system Dock
  notchbase-dock --quit          ask a running dock to quit (it puts the system Dock back)
  notchbase-dock --restore-dock  put the system Dock back, quitting a running dock first
  notchbase-dock --status        say whether a dock is running
  notchbase-dock --snapshot <out.png> [--scene <name>]
                                 draw the dock offscreen, without showing or hiding anything
  notchbase-dock --app-menu <bundle id>
                                 list the actions a running app adds to its menu (needs Accessibility)
";

#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("notchbase dock runs on macOS.");
    std::process::exit(1);
}

#[cfg(target_os = "macos")]
fn main() {
    std::process::exit(run());
}

#[cfg(target_os = "macos")]
fn run() -> i32 {
    use std::time::Duration;

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
    if let Some(script) = value("--try-switch") {
        // Types keys for the switcher, through the dock's own event tap: "cmd tab wait wait up".
        let Some(pid) = lifecycle::running_instance(&lifecycle::data_dir()) else { return 1 };
        let _ = std::fs::write(lifecycle::data_dir().join("try-switch"), script);
        unsafe { libc::kill(pid, libc::SIGUSR2) };
        return 0;
    }
    if let Some(what) = value("--try-click") {
        // Clicks the newest minimized window's tile ("tile") or its app's icon ("app") with a real mouse click,
        // through the dock's own event handling.
        let Some(pid) = lifecycle::running_instance(&lifecycle::data_dir()) else { return 1 };
        let _ = std::fs::write(lifecycle::data_dir().join("try-click"), what);
        unsafe { libc::kill(pid, libc::SIGUSR2) };
        return 0;
    }
    if has("--try-minimize") || has("--try-restore") {
        let signal = if has("--try-minimize") { libc::SIGUSR1 } else { libc::SIGUSR2 };
        let Some(pid) = lifecycle::running_instance(&lifecycle::data_dir()) else { return 1 };
        // `--try-minimize system` minimizes the macOS way, as an app's own button would; `--try-restore app` clicks
        // the app's icon rather than the window's tile.
        if has("system") || has("app") {
            let _ = std::fs::write(lifecycle::data_dir().join("try-other-way"), "");
        }
        unsafe { libc::kill(pid, signal) };
        return 0;
    }
    if has("--can-capture") {
        // Whether this copy may take window pictures (Screen Recording), without asking.
        let yes = windows::can_capture(false);
        println!("{yes}");
        return if yes { 0 } else { 1 };
    }
    if has("--dock-items") {
        // The system Dock's minimized-window items and where they sit, to see where minimizing aims.
        let Some(pid) = mac::running_pid("com.apple.dock") else { return 1 };
        for (role, title, frame) in titlebar::dock_items(pid) {
            println!("{role}\t{title}\t{frame:?}");
        }
        return 0;
    }
    if let Some(seconds) = value("--sample-dock") {
        // Prints the system Dock's frame whenever it changes, every few milliseconds, for the given seconds.
        let Some(pid) = mac::running_pid("com.apple.dock") else { return 1 };
        let start = std::time::Instant::now();
        let until = seconds.parse::<f64>().unwrap_or(3.0);
        let mut last = None;
        while start.elapsed().as_secs_f64() < until {
            let frame = titlebar::dock_list_frame(pid);
            if frame != last {
                let wall = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
                println!("{} {:?}", wall.as_millis() % 100_000, frame);
                last = frame;
            }
            std::thread::sleep(std::time::Duration::from_millis(4));
        }
        return 0;
    }
    if let Some(bundle) = value("--move-probe") {
        // Moves the app's first on-screen window to each spot given after it ("x,y") and says where it ended up.
        let Some(pid) = mac::running_pid(&bundle) else { return 1 };
        let Some(win) = windows::of_app(pid).into_iter().find(|w| w.on_screen) else { return 1 };
        let start = win.frame.min;
        let spots: Vec<egui::Pos2> = args
            .iter()
            .filter_map(|a| a.split_once(',').and_then(|(x, y)| Some(egui::pos2(x.parse().ok()?, y.parse().ok()?))))
            .collect();
        for to in spots {
            windows::move_to(pid, win.id, to);
            std::thread::sleep(Duration::from_millis(150));
            println!("asked {to:?}, got {:?}", windows::frame_of(pid, win.id));
        }
        windows::move_to(pid, win.id, start);
        return 0;
    }
    if let Some(bundle) = value("--title-bars") {
        // Probes each window of a running app a little below its top edge, along its width.
        let Some(pid) = mac::running_pid(&bundle) else {
            eprintln!("notchbase-dock: {bundle} isn't running");
            return 1;
        };
        for win in windows::of_app(pid).iter().filter(|w| w.on_screen) {
            println!("{} ({:.0}x{:.0})", win.title, win.frame.width(), win.frame.height());
            for f in [0.2, 0.35, 0.5, 0.65, 0.8] {
                let (x, y) = (win.frame.min.x + win.frame.width() * f, win.frame.min.y + 12.0);
                println!("  {:>3.0}% across: {}", f * 100.0, titlebar::probe(x.into(), y.into()));
            }
        }
        return 0;
    }
    if let Some(bundle) = value("--app-menu") {
        let Some(pid) = mac::running_pid(&bundle) else {
            eprintln!("notchbase-dock: {bundle} isn't running");
            return 1;
        };
        for item in app_menu::read(pid) {
            println!(
                "{} > {}{}",
                item.menu,
                item.title,
                if item.activates { "" } else { " (stays in the background)" }
            );
        }
        return 0;
    }
    if let Some(out) = value("--snapshot") {
        let scene = value("--scene").unwrap_or_else(|| "rest".into());
        return match snapshot::run(out.into(), scene, dir) {
            Ok(()) => 0,
            Err(err) => {
                eprintln!("notchbase-dock: {err}");
                1
            }
        };
    }
    if has("--quit") || has("--restore-dock") {
        if let Some(pid) = lifecycle::running_instance(&dir) {
            if !lifecycle::ask_to_quit(pid, Duration::from_secs(5)) {
                eprintln!("notchbase-dock: the dock (pid {pid}) did not quit");
                return 1;
            }
            println!("quit the dock; the system Dock is back");
        } else if has("--quit") {
            println!("no dock is running");
        }
        // macOS's own Cmd+Tab comes back too, even after a crash.
        switcher::system_switcher(true);
        // Whatever is still saved (a crash, or a dock that was killed) goes back too.
        match real_dock::restore(&dir, &real_dock::System) {
            Ok(true) => println!("restored the system Dock settings"),
            Ok(false) if real_dock::unhide_orphan(&real_dock::System) => println!("showed the system Dock again"),
            Ok(false) => {}
            Err(err) => {
                eprintln!("notchbase-dock: could not restore the Dock: {err}");
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
    // A dock that was killed (or made way for this one) may have left macOS's Cmd+Tab off; it's turned off again
    // only while this one catches it.
    switcher::system_switcher(true);
    let result = app::run(dir.clone());
    // `run` restores on the way out; this covers an event loop that failed to start. Not when making way for a
    // newer copy, which takes the hidden Dock over.
    if !lifecycle::take_handover(&dir) {
        let _ = real_dock::restore(&dir, &real_dock::System);
        switcher::system_switcher(true);
    }
    lifecycle::release(&dir);
    match result {
        Ok(()) => 0,
        Err(err) => {
            eprintln!("notchbase-dock: {err}");
            1
        }
    }
}
