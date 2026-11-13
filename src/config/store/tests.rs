use std::path::PathBuf;
use std::time::Instant;

use super::super::model::{Config, VERSION};
use super::{CHECK_EVERY, ConfigStore, RUNNING, Refresh};

struct Scratch(PathBuf);

impl Scratch {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("notchbase-config-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }

    fn file(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn first_run() -> Config {
    let mut config = Config::default();
    config.updates.seen = RUNNING.to_owned();
    config
}

#[test]
fn first_run_writes_the_defaults() {
    let scratch = Scratch::new("fresh");
    let path = scratch.file("config.json");
    let (store, problem) = ConfigStore::open(Some(path.clone()), None);
    assert_eq!(problem, None);
    assert_eq!(store.config(), &first_run());
    let text = std::fs::read_to_string(&path).unwrap();
    let written: Config = serde_json::from_str(&text).unwrap();
    assert_eq!(written, first_run());
    let raw: serde_json::Value = serde_json::from_str(&text).unwrap();
    let find = serde_json::json!({
        "enabled": true, "excluded": [], "volumes": true, "max_entries": 0, "learn": true, "recents": true,
        "system_key": true, "hidden": false, "pins": []
    });
    assert_eq!(raw["find"], find);
    assert_eq!(raw["permissions"]["find_disk_asked"], false);
    assert_eq!(raw["permissions"]["find_access_hidden"], false);
    assert_eq!(raw["permissions"]["walkthrough"], "pending");
    assert_eq!(raw["permissions"]["asked"], serde_json::json!([]));
}

#[test]
fn first_run_carries_old_settings() {
    let scratch = Scratch::new("legacy");
    let legacy = scratch.file("settings.txt");
    std::fs::write(&legacy, "volume_indicator=false\nall_displays=true\n").unwrap();
    let (store, _) = ConfigStore::open(Some(scratch.file("config.json")), Some(legacy));
    assert!(!store.config().volume.indicator && store.config().displays.all);
    assert_eq!(store.config().updates.seen, RUNNING);
}

#[test]
fn first_run_has_nothing_new() {
    let scratch = Scratch::new("seen");
    let path = scratch.file("config.json");
    let (store, _) = ConfigStore::open(Some(path.clone()), None);
    assert_eq!(store.config().updates.seen, RUNNING);
    assert!(!crate::whats_new::unseen(&store.config().updates.seen));
    let (again, problem) = ConfigStore::open(Some(path), None);
    assert_eq!((again.config().updates.seen.as_str(), problem), (RUNNING, None));
}

#[test]
fn missing_seen_survives_other_saves() {
    let scratch = Scratch::new("unseen");
    let path = scratch.file("config.json");
    std::fs::write(&path, r#"{ "updates": { "automatic": false } }"#).unwrap();
    let (mut store, problem) = ConfigStore::open(Some(path.clone()), None);
    assert_eq!((store.config().updates.seen.as_str(), problem), ("", None));
    store.update(|config| config.displays.all = true).unwrap();
    let (reopened, _) = ConfigStore::open(Some(path), None);
    assert!(reopened.config().displays.all && !reopened.config().updates.automatic);
    assert_eq!(reopened.config().updates.seen, "");
    assert!(crate::whats_new::unseen(&reopened.config().updates.seen));
}

#[test]
fn partial_files_fill_in_and_save() {
    let scratch = Scratch::new("partial");
    let path = scratch.file("config.json");
    std::fs::write(&path, r#"{ "notch": { "hover_delay_ms": 90 } }"#).unwrap();
    let (mut store, problem) = ConfigStore::open(Some(path.clone()), None);
    assert_eq!(problem, None);
    assert_eq!(store.config().notch.hover_delay_ms, 90);
    assert_eq!(store.config().notch.close_delay_ms, 220);
    store.update(|config| config.displays.all = true).unwrap();
    let saved: Config = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert!(saved.displays.all);
    assert_eq!(saved.notch.hover_delay_ms, 90);
}

#[test]
fn edits_apply_mistakes_never_overwrite() {
    let scratch = Scratch::new("reload");
    let path = scratch.file("config.json");
    let (mut store, _) = ConfigStore::open(Some(path.clone()), None);
    let t0 = Instant::now();
    std::fs::write(&path, r#"{ "volume": { "indicator": false }, "padding": "is fine" }"#).unwrap();
    let Refresh::Changed(previous) = store.refresh(t0) else { panic!("a change") };
    assert!(previous.volume.indicator && !store.config().volume.indicator);
    std::fs::write(&path, "{ not json").unwrap();
    assert!(matches!(store.refresh(t0 + CHECK_EVERY), Refresh::Broken(_)));
    assert!(store.update(|config| config.displays.all = true).is_err());
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "{ not json");
    assert!(matches!(store.refresh(t0 + CHECK_EVERY * 2), Refresh::Unchanged));
}

#[test]
fn older_file_is_upgraded_and_saved() {
    let scratch = Scratch::new("upgrade");
    let path = scratch.file("config.json");
    std::fs::write(&path, r#"{ "version": 1, "notch": { "hover_delay_ms": 300 } }"#).unwrap();
    let (store, problem) = ConfigStore::open(Some(path.clone()), None);
    assert_eq!(problem, None);
    assert_eq!(store.config().notch.hover_delay_ms, 120);
    let saved: Config = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!((saved.version, saved.notch.hover_delay_ms), (VERSION, 120));
}
