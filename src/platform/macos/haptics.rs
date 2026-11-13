//! Trackpad feedback through NSHapticFeedbackManager.

use objc2::msg_send;
use objc2::rc::Retained;
use objc2::runtime::{AnyClass, AnyObject};

use crate::platform::haptics::Feel;

const GENERIC: isize = 0;
const ALIGNMENT: isize = 1;
const LEVEL_CHANGE: isize = 2;
const NOW: isize = 1;

pub fn perform_haptic(feel: Feel) {
    let Some(manager) = AnyClass::get(c"NSHapticFeedbackManager") else { return };
    let performer: Option<Retained<AnyObject>> = unsafe { msg_send![manager, defaultPerformer] };
    let Some(performer) = performer else { return };
    let pattern = match feel {
        Feel::Snap => ALIGNMENT,
        Feel::Flip => LEVEL_CHANGE,
        Feel::Tick => GENERIC,
    };
    let _: () = unsafe { msg_send![&*performer, performFeedbackPattern: pattern, performanceTime: NOW] };
}
