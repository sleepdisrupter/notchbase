//! Starts the system calendar when a tool wants it, and passes its events and changes through.

use crate::app::App;
use crate::system::calendar::{Change, SystemCalendar};

const NO_CALENDAR: &str = "Calendar isn't running";

impl App {
    pub(in crate::app) fn sync_calendar(&mut self) {
        let wanted = self.tools.wants_calendar();
        if wanted != self.calendar.is_some() {
            self.calendar = wanted.then(SystemCalendar::start);
        }
        let Some(calendar) = &self.calendar else { return };
        calendar.reach(self.tools.calendar_range());
        let (feed, problems) = (calendar.take(), calendar.take_problems());
        if let Some(feed) = feed {
            self.tools.feed_calendar(feed);
        }
        for problem in problems {
            self.notify(&problem);
        }
    }

    pub(in crate::app) fn change_calendar(&mut self, change: Change) {
        match &self.calendar {
            Some(calendar) => calendar.change(change),
            None => self.notify(NO_CALENDAR),
        }
    }
}
