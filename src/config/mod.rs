//! Settings, kept in `config.json`. `ConfigStore` reads and writes the file and notices edits made to it, and
//! `current` gives the latest config anywhere in the app.

mod annotate;
pub mod hotkey;
mod migrate;
mod model;
mod store;
mod updates;

use std::sync::Arc;

pub use annotate::AnnotateConfig;
pub use migrate::LEGACY_FILE;
#[cfg(test)]
pub use model::ToolConfig;
pub use model::{
    CORE_TABS, Config, DEFAULT_TABS, DockConfig, FindConfig, FindPin, Permissions, SnapConfig, Walkthrough,
};
pub use store::{ConfigStore, Refresh};

/// The config last published, readable from any thread.
pub fn current() -> Arc<Config> {
    shared::get()
}

/// Sets what `current` gives, for the whole process.
pub(crate) fn publish(config: Config) {
    shared::set(config);
}

#[cfg(not(test))]
mod shared {
    use std::sync::{Arc, LazyLock, PoisonError, RwLock};

    use super::Config;

    static CURRENT: LazyLock<RwLock<Arc<Config>>> = LazyLock::new(RwLock::default);

    pub(super) fn get() -> Arc<Config> {
        CURRENT.read().unwrap_or_else(PoisonError::into_inner).clone()
    }

    pub(super) fn set(config: Config) {
        *CURRENT.write().unwrap_or_else(PoisonError::into_inner) = Arc::new(config);
    }
}

// In tests each thread keeps its own config, so tests running side by side don't see each other's.
#[cfg(test)]
mod shared {
    use std::cell::RefCell;
    use std::sync::Arc;

    use super::Config;

    thread_local! {
        static CURRENT: RefCell<Arc<Config>> = RefCell::default();
    }

    pub(super) fn get() -> Arc<Config> {
        CURRENT.with(|current| current.borrow().clone())
    }

    pub(super) fn set(config: Config) {
        CURRENT.with(|current| *current.borrow_mut() = Arc::new(config));
    }
}
