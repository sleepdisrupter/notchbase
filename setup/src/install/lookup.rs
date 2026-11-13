//! Looks up the newest release in the background.

use std::sync::{Arc, Mutex, PoisonError};

use notchbase_release::{Release, Server, Version};

use super::{USER_AGENT, Wake};

#[derive(Clone, Debug, PartialEq)]
pub enum Latest {
    Checking,
    Found(Version, Release),
    Failed(String),
}

pub struct Lookup {
    latest: Arc<Mutex<Latest>>,
}

impl Lookup {
    pub fn start(wake: Wake) -> Self {
        let latest = Arc::new(Mutex::new(Latest::Checking));
        let shared = Arc::clone(&latest);
        std::thread::spawn(move || {
            let found = match newest() {
                Ok((version, release)) => Latest::Found(version, release),
                Err(problem) => Latest::Failed(problem),
            };
            *shared.lock().unwrap_or_else(PoisonError::into_inner) = found;
            wake();
        });
        Self { latest }
    }

    pub fn latest(&self) -> Latest {
        self.latest.lock().unwrap_or_else(PoisonError::into_inner).clone()
    }
}

fn newest() -> Result<(Version, Release), String> {
    let server = Server::built_in(USER_AGENT);
    let platform = notchbase_release::platform();
    let releases = server.releases(&platform)?;
    notchbase_release::newest(releases, Version::default())
        .ok_or_else(|| format!("There's no notchbase for {platform} yet"))
}
