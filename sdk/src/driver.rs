//! The host's side of a tool: the `Driver` trait it calls, and the `Waker` and `Emitter` that
//! background work uses to reach it.

use std::sync::Arc;

use crate::protocol::{Effects, Event, Manifest, Start, Views};

/// What the host calls on a tool. `Runtime` implements it for any `Tool`, and plugins speak it
/// over FFI.
pub trait Driver {
    fn manifest(&self) -> Manifest;
    fn start(&mut self, start: Start) -> Effects;
    fn handle(&mut self, events: Vec<Event>) -> Effects;
    fn views(&mut self) -> Views;
}

/// Tells the host a background task has messages waiting, so it comes and collects them.
#[derive(Clone)]
pub struct Waker(Arc<dyn Fn() + Send + Sync>);

impl Waker {
    pub fn new(wake: impl Fn() + Send + Sync + 'static) -> Self {
        Self(Arc::new(wake))
    }

    pub fn noop() -> Self {
        Self::new(|| {})
    }

    pub fn wake(&self) {
        (self.0)();
    }
}

/// Sends messages back from a task started with `Command::spawn`.
pub struct Emitter<Msg> {
    sink: Arc<dyn Fn(Msg) + Send + Sync>,
}

impl<Msg> Clone for Emitter<Msg> {
    fn clone(&self) -> Self {
        Self { sink: self.sink.clone() }
    }
}

impl<Msg: 'static> Emitter<Msg> {
    pub(crate) fn new(sink: impl Fn(Msg) + Send + Sync + 'static) -> Self {
        Self { sink: Arc::new(sink) }
    }

    pub fn send(&self, msg: Msg) {
        (self.sink)(msg);
    }

    pub fn map<Inner: 'static>(self, map: impl Fn(Inner) -> Msg + Send + Sync + 'static) -> Emitter<Inner> {
        Emitter::new(move |inner| self.send(map(inner)))
    }
}
