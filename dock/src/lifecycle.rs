//! One dock at a time, and a clean way out: the pid file, `--quit`, signals and the panic hook all end with the
//! system Dock back as it was.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use crate::real_dock;

const PID_FILE: &str = "dock.pid";
/// Under the pid in the pid file: this dock quits for its newer self without showing the macOS Dock in between.
const CAN_HAND_OVER: &str = "handover";
/// Left by notchbase just before it asks this dock to quit for a newer copy of itself (after an update).
const HANDOVER_FILE: &str = "handover";
/// A handover note older than this is a leftover, not a request.
const HANDOVER_FRESH: Duration = Duration::from_secs(20);
static HANDING_OVER: AtomicBool = AtomicBool::new(false);
static QUIT: AtomicBool = AtomicBool::new(false);
static DATA_DIR: OnceLock<PathBuf> = OnceLock::new();

/// `~/Library/Application Support/notchbase dock`, or `NOTCHBASE_DOCK_DIR` when set.
pub fn data_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("NOTCHBASE_DOCK_DIR") {
        return PathBuf::from(dir);
    }
    let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/tmp"));
    home.join("Library/Application Support/notchbase dock")
}

pub fn pid_path(dir: &Path) -> PathBuf {
    dir.join(PID_FILE)
}

fn alive(pid: i32) -> bool {
    pid > 0 && unsafe { libc::kill(pid, 0) } == 0
}

/// The pid of a dock already running from this data folder.
pub fn running_instance(dir: &Path) -> Option<i32> {
    let pid = read_pid(&std::fs::read_to_string(pid_path(dir)).ok()?)?;
    (pid != std::process::id() as i32 && alive(pid) && is_dock(pid)).then_some(pid)
}

fn is_dock(pid: i32) -> bool {
    let mut path = [0u8; libc::PROC_PIDPATHINFO_MAXSIZE as usize];
    let len = unsafe { libc::proc_pidpath(pid, path.as_mut_ptr().cast(), path.len() as u32) };
    len > 0 && String::from_utf8_lossy(&path[..len as usize]).ends_with("notchbase-dock")
}

/// The pid on the pid file's first line (older docks wrote only that).
fn read_pid(text: &str) -> Option<i32> {
    text.lines().next()?.trim().parse().ok()
}

pub fn claim(dir: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    std::fs::write(pid_path(dir), format!("{}\n{CAN_HAND_OVER}\n", std::process::id()))
}

pub fn release(dir: &Path) {
    let ours = std::fs::read_to_string(pid_path(dir)).is_ok_and(|p| read_pid(&p) == Some(std::process::id() as i32));
    if ours {
        let _ = std::fs::remove_file(pid_path(dir));
    }
}

/// Sends the running dock SIGTERM and waits for it to put the Dock back and exit.
pub fn ask_to_quit(pid: i32, wait: Duration) -> bool {
    unsafe { libc::kill(pid, libc::SIGTERM) };
    let start = Instant::now();
    while start.elapsed() < wait {
        if !alive(pid) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(40));
    }
    !alive(pid)
}

extern "C" fn on_signal(_: libc::c_int) {
    QUIT.store(true, Ordering::SeqCst);
}

pub fn quit_requested() -> bool {
    QUIT.load(Ordering::SeqCst)
}

/// On the way out: whether notchbase asked this dock to make way for a newer copy of itself. Then the macOS Dock
/// stays hidden and its saved settings stay saved, and the new dock takes them over as from a crash, so the real
/// Dock never shows in between. Reads (and removes) the note once; later calls give the same answer.
pub fn take_handover(dir: &Path) -> bool {
    let note = dir.join(HANDOVER_FILE);
    let fresh = std::fs::metadata(&note)
        .and_then(|m| m.modified())
        .is_ok_and(|at| at.elapsed().is_ok_and(|age| age < HANDOVER_FRESH));
    if std::fs::remove_file(&note).is_ok() && fresh {
        HANDING_OVER.store(true, Ordering::SeqCst);
    }
    HANDING_OVER.load(Ordering::SeqCst)
}

/// For trying minimizing by hand: SIGUSR1 minimizes the front window the dock's way, SIGUSR2 restores the newest
/// minimized window from its tile (`--try-minimize`, `--try-restore`).
static TRY_MINIMIZE: AtomicBool = AtomicBool::new(false);
static TRY_RESTORE: AtomicBool = AtomicBool::new(false);

extern "C" fn on_try(signal: libc::c_int) {
    if signal == libc::SIGUSR1 {
        TRY_MINIMIZE.store(true, Ordering::SeqCst);
    } else {
        TRY_RESTORE.store(true, Ordering::SeqCst);
    }
}

pub fn take_try_minimize() -> bool {
    TRY_MINIMIZE.swap(false, Ordering::SeqCst)
}

pub fn take_try_restore() -> bool {
    TRY_RESTORE.swap(false, Ordering::SeqCst)
}

/// SIGINT, SIGTERM and SIGHUP ask the event loop to quit, which restores the Dock on the way out. A panic restores
/// it on the spot.
pub fn install_handlers(dir: &Path) {
    let _ = DATA_DIR.set(dir.to_path_buf());
    let handler = on_signal as extern "C" fn(libc::c_int) as libc::sighandler_t;
    for signal in [libc::SIGINT, libc::SIGTERM, libc::SIGHUP] {
        unsafe { libc::signal(signal, handler) };
    }
    let try_handler = on_try as extern "C" fn(libc::c_int) as libc::sighandler_t;
    for signal in [libc::SIGUSR1, libc::SIGUSR2] {
        unsafe { libc::signal(signal, try_handler) };
    }
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        // A panic while picking a GL config is caught, and the dock carries on.
        if crate::gl::picking_config() {
            return;
        }
        if let Some(dir) = DATA_DIR.get() {
            let _ = real_dock::restore(dir, &real_dock::System);
            release(dir);
        }
        previous(info);
    }));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_pid_file_says_it_can_hand_over_and_older_ones_still_read() {
        let dir = real_dock::tests::scratch("pid-file");
        claim(&dir).unwrap();
        let text = std::fs::read_to_string(pid_path(&dir)).unwrap();
        assert_eq!(read_pid(&text), Some(std::process::id() as i32));
        assert_eq!(text.lines().nth(1), Some(CAN_HAND_OVER));
        assert_eq!(read_pid("4242\n"), Some(4242), "an older dock's file");
        assert_eq!(read_pid("nonsense"), None);
        release(&dir);
        assert!(!pid_path(&dir).exists(), "its own pid file goes on the way out");
    }

    #[test]
    fn only_a_fresh_note_is_a_handover_and_it_is_read_once() {
        let dir = real_dock::tests::scratch("handover");
        let note = dir.join(HANDOVER_FILE);
        std::fs::write(&note, "").unwrap();
        let old = std::time::SystemTime::now() - Duration::from_secs(120);
        std::fs::File::options().write(true).open(&note).unwrap().set_modified(old).unwrap();
        HANDING_OVER.store(false, Ordering::SeqCst);
        assert!(!take_handover(&dir), "a leftover note is not a request");
        assert!(!note.exists(), "and it's cleared away");
        std::fs::write(&note, "").unwrap();
        assert!(take_handover(&dir));
        assert!(!note.exists());
        assert!(take_handover(&dir), "the answer holds for the rest of the way out");
        HANDING_OVER.store(false, Ordering::SeqCst);
    }
}
