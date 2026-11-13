//! An outbox that wakes the app each time something is posted.

use std::sync::Arc;

use super::{Outbox, Wake};

pub struct Mailbox<T> {
    outbox: Arc<Outbox<T>>,
    wake: Wake,
}

impl<T> Clone for Mailbox<T> {
    fn clone(&self) -> Self {
        Self { outbox: Arc::clone(&self.outbox), wake: Arc::clone(&self.wake) }
    }
}

impl<T> Mailbox<T> {
    pub fn new(wake: Wake) -> Self {
        Self { outbox: Arc::new(Outbox::default()), wake }
    }

    pub fn post(&self, item: T) {
        self.outbox.push(item);
        (self.wake)();
    }

    pub fn take(&self) -> Vec<T> {
        self.outbox.take_all()
    }
}
