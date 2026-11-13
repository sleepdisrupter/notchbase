//! Volume keys and output on macOS.

mod keys;
mod output;
mod trust;

pub use keys::KeyTap;
pub use output::Output;
pub use trust::may_watch_keys;

use super::steps::Steps;

pub const STEPS: Steps = Steps { coarse: 16.0, fine: 64.0 };
