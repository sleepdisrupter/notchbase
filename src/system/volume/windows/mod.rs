//! Volume keys and output on Windows. Windows steps in 2% increments, so the fine step is the same as the coarse one.

mod keys;
mod output;

pub use keys::KeyTap;
pub use output::Output;

use super::steps::Steps;

pub const STEPS: Steps = Steps { coarse: 50.0, fine: 50.0 };

#[cfg(test)]
mod tests {
    use std::sync::mpsc;

    use super::*;

    #[test]
    #[ignore = "reads the real output device and hooks the keyboard briefly"]
    fn output_and_keys_can_be_used() {
        let mut output = Output::open();
        let reading = output.read().expect("a default output");
        assert!((0.0..=1.0).contains(&reading.level.volume));
        assert!(!reading.device.is_empty());
        let (keys, _received) = mpsc::channel();
        let tap = KeyTap::install(keys, std::sync::Arc::default()).expect("hook installs");
        drop(tap);
    }
}
