//! What drop sites tell the app. Arrivals queue up in the `Inbox` with their window, and posting one wakes the app.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

use winit::window::WindowId;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Dropped {
    File(PathBuf),
    Data {
        name: Option<String>,
        bytes: Vec<u8>,
    },
    Link(String),
    /// A link dragged as a page, not as a picture or file on it.
    Page(String),
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Saved {
    pub paths: Vec<PathBuf>,
    pub problems: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Arrival {
    Hover(usize),
    /// A page is being dragged in, so the notch can offer to download its video.
    Offer(String),
    Leave,
    Drop(Vec<Dropped>),
    Saved(Saved),
}

type Queue = Arc<Mutex<Vec<(WindowId, Arrival)>>>;

#[derive(Clone)]
pub struct Inbox {
    queue: Queue,
    wake: Arc<dyn Fn() + Send + Sync>,
    /// The Downloads folder, so the platform code never has to look it up.
    folder: Option<PathBuf>,
}

impl Inbox {
    pub fn new(folder: Option<PathBuf>, wake: impl Fn() + Send + Sync + 'static) -> Self {
        Self { queue: Queue::default(), wake: Arc::new(wake), folder }
    }

    pub fn folder(&self) -> Option<&Path> {
        self.folder.as_deref()
    }

    pub fn post(&self, window: WindowId, arrival: Arrival) {
        self.queue.lock().unwrap_or_else(PoisonError::into_inner).push((window, arrival));
        (self.wake)();
    }

    pub fn take(&self) -> Vec<(WindowId, Arrival)> {
        std::mem::take(&mut *self.queue.lock().unwrap_or_else(PoisonError::into_inner))
    }
}
