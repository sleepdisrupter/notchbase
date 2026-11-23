//! The tools built into notchbase. Ones the layout doesn't place keep this order.

use notchbase_sdk::{Driver, Waker, boxed};

pub type Construct = fn(Waker) -> Box<dyn Driver>;

pub fn all() -> [Construct; 13] {
    [
        boxed::<color_picker::ColorPicker>,
        boxed::<::screenshot::Screenshot>,
        boxed::<screen_recorder::ScreenRecorder>,
        boxed::<::dictation::Dictation>,
        boxed::<::timer::Timer>,
        boxed::<keep_awake::KeepAwake>,
        boxed::<::roam::Roam>,
        boxed::<alerts::Alerts>,
        boxed::<::calendar::Calendar>,
        boxed::<battery::Battery>,
        boxed::<camera_mic::CameraMic>,
        boxed::<::downloads::Downloads>,
        boxed::<focus_status::FocusStatus>,
    ]
}
