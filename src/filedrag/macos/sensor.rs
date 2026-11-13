//! Senses a file drag before it reaches the notch by watching the system drag pasteboard while the button is down.

use objc2::rc::{Retained, autoreleasepool};
use objc2_app_kit::{NSEvent, NSPasteboard, NSPasteboardNameDrag, NSPasteboardTypeFileURL};

use super::super::Sensed;

const PRIMARY_BUTTON_MASK: usize = 1;
const POLLS_TO_WAIT_FOR_FILE_ITEMS: u8 = 5;

pub(in crate::filedrag) fn primary_button_down() -> bool {
    NSEvent::pressedMouseButtons() & PRIMARY_BUTTON_MASK != 0
}

pub(in crate::filedrag) struct Sensor {
    change_count_at_press: Option<isize>,
    last_change_count: isize,
    file_checks_left: u8,
    state: Sensed,
}

impl Default for Sensor {
    fn default() -> Self {
        Self { change_count_at_press: None, last_change_count: 0, file_checks_left: 0, state: Sensed::Idle }
    }
}

impl Sensor {
    pub(in crate::filedrag) fn poll(&mut self) -> Sensed {
        if !primary_button_down() {
            *self = Self::default();
            return self.state;
        }
        autoreleasepool(|_| self.watch(&drag_pasteboard()))
    }

    fn watch(&mut self, pasteboard: &NSPasteboard) -> Sensed {
        let change_count = pasteboard.changeCount();
        let Some(at_press) = self.change_count_at_press else {
            self.begin_press(change_count);
            return self.state;
        };
        if change_count != self.last_change_count {
            self.last_change_count = change_count;
            self.file_checks_left = if change_count == at_press { 0 } else { POLLS_TO_WAIT_FOR_FILE_ITEMS };
            self.state = Sensed::Pressed;
        }
        if self.file_checks_left > 0 {
            self.look_for_files(pasteboard);
        }
        self.state
    }

    fn begin_press(&mut self, change_count: isize) {
        self.change_count_at_press = Some(change_count);
        self.last_change_count = change_count;
        self.state = Sensed::Pressed;
    }

    fn look_for_files(&mut self, pasteboard: &NSPasteboard) {
        self.file_checks_left -= 1;
        let files = file_item_count(pasteboard);
        if files > 0 {
            self.file_checks_left = 0;
            self.state = Sensed::Files(files);
        }
    }
}

fn drag_pasteboard() -> Retained<NSPasteboard> {
    NSPasteboard::pasteboardWithName(unsafe { NSPasteboardNameDrag })
}

fn file_item_count(pasteboard: &NSPasteboard) -> usize {
    let Some(items) = pasteboard.pasteboardItems() else {
        return 0;
    };
    let file_url_type = unsafe { NSPasteboardTypeFileURL };
    items.iter().filter(|item| item.types().iter().any(|kind| kind.isEqualToString(file_url_type))).count()
}
