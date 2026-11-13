//! Items pushed from any thread, kept in order until they're all taken at once.

use std::sync::Mutex;

use super::lock;

pub struct Outbox<T> {
    items: Mutex<Vec<T>>,
}

impl<T> Default for Outbox<T> {
    fn default() -> Self {
        Self { items: Mutex::new(Vec::new()) }
    }
}

impl<T> Outbox<T> {
    pub fn push(&self, item: T) {
        lock(&self.items).push(item);
    }

    pub fn extend(&self, items: impl IntoIterator<Item = T>) {
        lock(&self.items).extend(items);
    }

    pub fn take_all(&self) -> Vec<T> {
        std::mem::take(&mut *lock(&self.items))
    }

    pub fn is_empty(&self) -> bool {
        lock(&self.items).is_empty()
    }
}
