//! There's no console to handle on a Mac, so quitting simply exits.

pub fn on_console_quit(_quit: impl Fn() + Send + Sync + 'static) {}

pub fn exit_now() -> ! {
    std::process::exit(0)
}
