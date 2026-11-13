//! Ctrl+C, Ctrl+Break and closing the console ask the app to quit, and 1.5 seconds later the process ends regardless.

use std::sync::OnceLock;
use std::time::Duration;

use windows_sys::Win32::Foundation::{FALSE, TRUE};
use windows_sys::Win32::System::Console::{CTRL_BREAK_EVENT, CTRL_C_EVENT, CTRL_CLOSE_EVENT, SetConsoleCtrlHandler};
use windows_sys::Win32::System::Threading::{GetCurrentProcess, TerminateProcess};
use windows_sys::core::BOOL;

const SHUTDOWN_GRACE: Duration = Duration::from_millis(1500);
const INTERRUPTED_EXIT_CODE: u32 = 130;

static QUIT: OnceLock<Box<dyn Fn() + Send + Sync>> = OnceLock::new();

pub fn on_console_quit(quit: impl Fn() + Send + Sync + 'static) {
    if QUIT.set(Box::new(quit)).is_ok() {
        unsafe { SetConsoleCtrlHandler(Some(console_handler), TRUE) };
    }
}

pub fn exit_now() -> ! {
    terminate(0);
    std::process::exit(0)
}

unsafe extern "system" fn console_handler(ctrl: u32) -> BOOL {
    if !matches!(ctrl, CTRL_C_EVENT | CTRL_BREAK_EVENT | CTRL_CLOSE_EVENT) {
        return FALSE;
    }
    if let Some(quit) = QUIT.get() {
        quit();
    }
    std::thread::sleep(SHUTDOWN_GRACE);
    terminate(INTERRUPTED_EXIT_CODE);
    TRUE
}

fn terminate(exit_code: u32) {
    unsafe { TerminateProcess(GetCurrentProcess(), exit_code) };
}
