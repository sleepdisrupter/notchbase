//! The screenshot and recording area pickers. Both are the live picker drawn over every display, and on macOS a
//! wrapper takes the screenshot itself with `screencapture`.

#[cfg(target_os = "macos")]
mod macos;

mod live;

use crate::capture::Screenshot;
use crate::record::Region;

pub use live::Picker as AreaPicker;
#[cfg(not(target_os = "macos"))]
pub use live::Picker;
#[cfg(target_os = "macos")]
pub use macos::Picker;

/// What a picker ended with: a screenshot, an area to record, or nothing.
pub enum PickerOutcome {
    Region(Screenshot),
    Area(Region),
    Cancelled,
}
