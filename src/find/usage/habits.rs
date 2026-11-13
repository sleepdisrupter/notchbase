//! Habits: the part of the day a file tends to be opened in, and the two apps it is usually opened with. App names
//! are kept once in a short shared list.

use crate::find::context::Daypart;
use crate::find::weights::APPS;

use super::decay::share;
use super::record::{FileRecord, SLOTS};

const SEPARATOR: char = '\0';

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Apps {
    names: Vec<Box<str>>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Habit {
    pub count: f32,
    pub share: f32,
}

impl Apps {
    pub fn from_names(names: Vec<Box<str>>) -> Self {
        Self { names: names.into_iter().take(APPS).collect() }
    }

    pub fn names(&self) -> &[Box<str>] {
        &self.names
    }

    pub fn find(&self, name: &str) -> Option<u8> {
        self.names.iter().position(|known| &**known == name).and_then(|slot| u8::try_from(slot).ok())
    }

    pub fn intern(&mut self, name: &str, in_use: impl Fn(u8) -> bool) -> Option<u8> {
        if name.is_empty() || name.contains(SEPARATOR) {
            return None;
        }
        if let Some(known) = self.find(name) {
            return Some(known);
        }
        if self.names.len() < APPS {
            self.names.push(name.into());
            return u8::try_from(self.names.len() - 1).ok();
        }
        let free = (0..self.names.len()).filter_map(|slot| u8::try_from(slot).ok()).find(|slot| !in_use(*slot))?;
        self.names[usize::from(free)] = name.into();
        Some(free)
    }
}

impl FileRecord {
    pub fn count_daypart(&mut self, part: Daypart) {
        let Some(count) = self.dayparts.get(usize::from(part.0)).copied() else { return };
        if count == u8::MAX {
            self.dayparts.iter_mut().for_each(|count| *count /= 2);
        }
        self.dayparts[usize::from(part.0)] += 1;
    }

    pub fn around(&self, part: Daypart) -> Option<Habit> {
        let count = f32::from(*self.dayparts.get(usize::from(part.0))?);
        let total: f32 = self.dayparts.iter().map(|count| f32::from(*count)).sum();
        (count > 0.0).then(|| Habit { count: total, share: share(count, total) })
    }

    pub fn count_app(&mut self, app: u8) {
        if let Some(slot) = self.slot_of(app) {
            if self.app_weight[slot] == u8::MAX {
                self.app_weight.iter_mut().for_each(|weight| *weight /= 2);
            }
            self.app_weight[slot] += 1;
            return;
        }
        let weaker = (0..SLOTS).min_by_key(|slot| self.app_weight[*slot]).unwrap_or(0);
        self.app_weight[weaker] = self.app_weight[weaker].saturating_sub(1);
        if self.app_weight[weaker] == 0 {
            self.apps[weaker] = app;
            self.app_weight[weaker] = 1;
        }
    }

    pub fn with(&self, app: u8) -> Option<Habit> {
        let count = f32::from(self.app_weight[self.slot_of(app)?]);
        let total: f32 = self.app_weight.iter().map(|weight| f32::from(*weight)).sum();
        Some(Habit { count, share: share(count, total) })
    }

    pub fn uses_app(&self, app: u8) -> bool {
        self.slot_of(app).is_some()
    }

    fn slot_of(&self, app: u8) -> Option<usize> {
        (0..SLOTS).find(|slot| self.apps[*slot] == app && self.app_weight[*slot] > 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::find::key::START;

    const XCODE: u8 = 0;
    const PREVIEW: u8 = 1;
    const MAIL: u8 = 2;

    fn record() -> FileRecord {
        FileRecord::new(START)
    }

    #[test]
    fn opens_count_by_daypart() {
        let mut record = record();
        assert!(record.around(Daypart(2)).is_none());
        (0..3).for_each(|_| record.count_daypart(Daypart(2)));
        record.count_daypart(Daypart(6));
        assert_eq!(record.around(Daypart(2)), Some(Habit { count: 4.0, share: 0.75 }));
        assert_eq!(record.around(Daypart(6)).map(|habit| habit.share), Some(0.25));
        assert!(record.around(Daypart(0)).is_none());
        record.count_daypart(Daypart(Daypart::COUNT as u8));
        assert_eq!(record.dayparts.iter().map(|count| u32::from(*count)).sum::<u32>(), 4);
    }

    #[test]
    fn full_counter_halves_evenly() {
        let mut record = record();
        record.dayparts[1] = u8::MAX;
        record.dayparts[3] = 100;
        record.count_daypart(Daypart(1));
        assert_eq!((record.dayparts[1], record.dayparts[3]), (128, 50));
    }

    #[test]
    fn file_remembers_two_apps() {
        let mut record = record();
        (0..3).for_each(|_| record.count_app(XCODE));
        record.count_app(PREVIEW);
        assert_eq!(record.with(XCODE), Some(Habit { count: 3.0, share: 0.75 }));
        assert_eq!(record.with(PREVIEW).map(|habit| habit.share), Some(0.25));
        assert!(record.with(MAIL).is_none() && !record.uses_app(MAIL));
    }

    #[test]
    fn third_app_wears_weaker_slot() {
        let mut record = record();
        (0..3).for_each(|_| record.count_app(XCODE));
        (0..2).for_each(|_| record.count_app(PREVIEW));
        record.count_app(MAIL);
        assert!(record.uses_app(PREVIEW) && !record.uses_app(MAIL));
        record.count_app(MAIL);
        assert!(!record.uses_app(PREVIEW));
        assert_eq!(record.with(MAIL).map(|habit| habit.count), Some(1.0));
        assert_eq!(record.with(XCODE).map(|habit| habit.count), Some(3.0));
    }

    #[test]
    fn full_app_list_reuses_free_slot() {
        let mut apps = Apps::default();
        assert_eq!((apps.intern("Xcode", |_| true), apps.intern("Preview", |_| true)), (Some(0), Some(1)));
        assert_eq!(apps.intern("Xcode", |_| true), Some(0));
        assert_eq!(apps.intern("", |_| true), None);
        assert_eq!(apps.intern("bad\0name", |_| true), None);
        (2..APPS).for_each(|serial| assert!(apps.intern(&format!("app {serial}"), |_| true).is_some()));
        assert_eq!(apps.intern("One Too Many", |_| true), None);
        assert_eq!(apps.intern("One Too Many", |slot| slot != 1), Some(1));
        assert_eq!((&*apps.names()[1], apps.find("Preview"), apps.names().len()), ("One Too Many", None, APPS));
    }
}
