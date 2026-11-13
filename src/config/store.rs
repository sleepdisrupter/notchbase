//! `config.json` on disk. Edits are picked up while the app runs, and a file with a mistake is never written over.

use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

use super::migrate::{from_legacy, upgrade};
use super::model::Config;

/// How often the file is looked at for edits made outside the app.
const CHECK_EVERY: Duration = Duration::from_secs(1);
const BROKEN: &str = "config.json has a mistake, so changes aren't saved until it's fixed";
const RUNNING: &str = env!("CARGO_PKG_VERSION");

/// When the file last changed and how long it is, to notice an edit without reading the file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Stamp {
    modified: Option<SystemTime>,
    len: u64,
}

/// What `refresh` found. `Changed` carries the config from before, so the app can see what moved.
pub enum Refresh {
    Unchanged,
    Changed(Box<Config>),
    Broken(String),
}

pub struct ConfigStore {
    path: Option<PathBuf>,
    config: Config,
    stamp: Option<Stamp>,
    checked_at: Option<Instant>,
    broken: bool,
}

impl ConfigStore {
    /// Opens the file, or creates it from the defaults and any old `settings.txt`. A problem to show comes with it.
    pub fn open(path: Option<PathBuf>, legacy: Option<PathBuf>) -> (Self, Option<String>) {
        let mut store = Self { path, config: Config::default(), stamp: None, checked_at: None, broken: false };
        let problem = match store.path.as_deref().map(read) {
            Some(Ok(Some(text))) => store.adopt(&text).err(),
            Some(Ok(None)) => {
                store.create(legacy.as_deref()).err().map(|err| format!("Couldn't create config.json: {err}"))
            }
            Some(Err(err)) => Some(format!("Couldn't read config.json: {err}")),
            None => None,
        };
        super::publish(store.config.clone());
        (store, problem)
    }

    pub fn config(&self) -> &Config {
        &self.config
    }

    /// Changes the config and saves it, unless the file has a mistake, which is never written over.
    pub fn update(&mut self, change: impl FnOnce(&mut Config)) -> io::Result<()> {
        let mut next = self.config.clone();
        change(&mut next);
        if next == self.config {
            return Ok(());
        }
        self.config = next;
        super::publish(self.config.clone());
        if self.broken {
            return Err(io::Error::other(BROKEN));
        }
        self.save()
    }

    /// Takes in edits made to the file, looking at most once a second.
    pub fn refresh(&mut self, now: Instant) -> Refresh {
        if self.checked_at.is_some_and(|at| now.duration_since(at) < CHECK_EVERY) {
            return Refresh::Unchanged;
        }
        self.checked_at = Some(now);
        let Some(path) = self.path.clone() else { return Refresh::Unchanged };
        let stamp = stamp_of(&path);
        if stamp == self.stamp {
            return Refresh::Unchanged;
        }
        self.stamp = stamp;
        let text = match read(&path) {
            Ok(Some(text)) => text,
            Ok(None) => return Refresh::Unchanged,
            Err(err) => return Refresh::Broken(format!("Couldn't read config.json: {err}")),
        };
        let previous = self.config.clone();
        match self.adopt(&text) {
            Ok(()) if self.config != previous => {
                super::publish(self.config.clone());
                Refresh::Changed(Box::new(previous))
            }
            Ok(()) => Refresh::Unchanged,
            Err(problem) => Refresh::Broken(problem),
        }
    }

    /// Takes the file's text as the config, saving it straight back when an older format was upgraded.
    fn adopt(&mut self, text: &str) -> Result<(), String> {
        match serde_json::from_str::<Config>(text) {
            Ok(mut config) => {
                let upgraded = upgrade(&mut config);
                self.config = config;
                self.broken = false;
                self.stamp = self.path.as_deref().and_then(stamp_of);
                if upgraded {
                    self.save().map_err(|err| format!("Couldn't update config.json: {err}"))?;
                }
                Ok(())
            }
            Err(err) => {
                self.broken = true;
                Err(format!("config.json line {}: {err}", err.line()))
            }
        }
    }

    fn create(&mut self, legacy: Option<&Path>) -> io::Result<()> {
        if let Some(text) = legacy.map(read).and_then(Result::ok).flatten() {
            self.config = from_legacy(&text);
        }
        // A new copy has no changes to announce.
        self.config.updates.seen = RUNNING.to_owned();
        self.save()
    }

    fn save(&mut self) -> io::Result<()> {
        let Some(path) = &self.path else { return Ok(()) };
        let mut text = serde_json::to_string_pretty(&self.config).map_err(io::Error::other)?;
        text.push('\n');
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(path, text)?;
        self.stamp = stamp_of(path);
        Ok(())
    }
}

fn read(path: &Path) -> io::Result<Option<String>> {
    match std::fs::read_to_string(path) {
        Ok(text) => Ok(Some(text)),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(err) => Err(err),
    }
}

fn stamp_of(path: &Path) -> Option<Stamp> {
    let meta = std::fs::metadata(path).ok()?;
    Some(Stamp { modified: meta.modified().ok(), len: meta.len() })
}

#[cfg(test)]
mod tests;
