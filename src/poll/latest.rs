//! The newest reading from a thread, handed out once. A reading that equals the last one isn't news.

use std::sync::Mutex;

use super::lock;

pub struct Latest<T> {
    fresh: Mutex<Option<T>>,
    last: Mutex<Option<T>>,
}

impl<T> Default for Latest<T> {
    fn default() -> Self {
        Self { fresh: Mutex::new(None), last: Mutex::new(None) }
    }
}

impl<T> Latest<T> {
    pub fn take(&self) -> Option<T> {
        lock(&self.fresh).take()
    }
}

impl<T: Clone + PartialEq> Latest<T> {
    /// Keeps the reading unless it equals the last one, and says whether it was news.
    pub fn publish(&self, reading: T) -> bool {
        let mut last = lock(&self.last);
        if last.as_ref() == Some(&reading) {
            return false;
        }
        *lock(&self.fresh) = Some(reading.clone());
        *last = Some(reading);
        true
    }
}
