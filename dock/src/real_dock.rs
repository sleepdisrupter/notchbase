//! Hides the system Dock while this one runs, the way uBar does: autohide on with a delay so long it never shows.
//! The user's own values are saved to a file first and put back on quit. A file left by a crash is restored
//! from before anything is saved again, so the hacked values never overwrite the real ones.

use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde::{Deserialize, Serialize};

const DOMAIN: &str = "com.apple.dock";
const AUTOHIDE: &str = "autohide";
const DELAY: &str = "autohide-delay";
/// About eleven days: the Dock never comes out.
pub const HIDDEN_DELAY: f64 = 1_000_000.0;
/// No real setting is this slow, so a saved delay this long was our own and is dropped.
const OURS_FROM: f64 = 1_000.0;
const STATE_FILE: &str = "saved-dock.json";
const TILESIZE: &str = "tilesize";
const MAGNIFICATION: &str = "magnification";
const LARGESIZE: &str = "largesize";
const MINEFFECT: &str = "mineffect";
/// Windows the dock minimizes itself go down out of sight in the screen's corner; the plain scale effect stays down
/// there with them, where the genie would sweep up across the screen.
const OUR_MINEFFECT: &str = "scale";
/// The real Dock's icon size while ours covers it (Mission Control still shows it).
pub const TINY_TILES: f64 = 16.0;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Saved {
    pub autohide: Option<bool>,
    pub autohide_delay: Option<f64>,
    #[serde(default)]
    pub tilesize: Option<f64>,
    #[serde(default)]
    pub magnification: Option<bool>,
    #[serde(default)]
    pub largesize: Option<f64>,
    /// Whether the size keys above were changed and must be put back.
    #[serde(default)]
    pub shrunk: bool,
    #[serde(default)]
    pub mineffect: Option<String>,
    /// Whether the minimize effect was changed and must be put back.
    #[serde(default)]
    pub scaled: bool,
}

/// The `com.apple.dock` defaults and a way to restart the Dock, so the logic can be tested without touching them.
pub trait Defaults {
    fn read(&self, key: &str) -> Option<String>;
    fn write_bool(&self, key: &str, value: bool);
    fn write_float(&self, key: &str, value: f64);
    fn write_string(&self, key: &str, value: &str);
    fn delete(&self, key: &str);
    fn restart_dock(&self);
}

pub struct System;

fn quiet(cmd: &mut Command) -> bool {
    cmd.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).status().is_ok_and(|s| s.success())
}

impl Defaults for System {
    fn read(&self, key: &str) -> Option<String> {
        let out = Command::new("/usr/bin/defaults").args(["read", DOMAIN, key]).stderr(Stdio::null()).output().ok()?;
        out.status.success().then(|| String::from_utf8_lossy(&out.stdout).trim().to_owned())
    }

    fn write_bool(&self, key: &str, value: bool) {
        let value = if value { "true" } else { "false" };
        quiet(Command::new("/usr/bin/defaults").args(["write", DOMAIN, key, "-bool", value]));
    }

    fn write_float(&self, key: &str, value: f64) {
        quiet(Command::new("/usr/bin/defaults").args(["write", DOMAIN, key, "-float", &value.to_string()]));
    }

    fn write_string(&self, key: &str, value: &str) {
        quiet(Command::new("/usr/bin/defaults").args(["write", DOMAIN, key, "-string", value]));
    }

    fn delete(&self, key: &str) {
        quiet(Command::new("/usr/bin/defaults").args(["delete", DOMAIN, key]));
    }

    fn restart_dock(&self) {
        quiet(Command::new("/usr/bin/killall").arg("Dock"));
    }
}

pub fn state_path(dir: &Path) -> PathBuf {
    dir.join(STATE_FILE)
}

fn parse_bool(text: &str) -> Option<bool> {
    match text.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" => Some(true),
        "0" | "false" | "no" => Some(false),
        _ => None,
    }
}

fn parse_size(text: &str) -> Option<f64> {
    text.trim().parse::<f64>().ok().filter(|d| d.is_finite() && *d > 0.0)
}

fn parse_delay(text: &str) -> Option<f64> {
    text.trim().parse::<f64>().ok().filter(|d| d.is_finite() && *d >= 0.0 && *d < OURS_FROM)
}

pub fn read_current(defaults: &impl Defaults) -> Saved {
    Saved {
        autohide: defaults.read(AUTOHIDE).as_deref().and_then(parse_bool),
        autohide_delay: defaults.read(DELAY).as_deref().and_then(parse_delay),
        tilesize: defaults.read(TILESIZE).as_deref().and_then(parse_size),
        magnification: defaults.read(MAGNIFICATION).as_deref().and_then(parse_bool),
        largesize: defaults.read(LARGESIZE).as_deref().and_then(parse_size),
        shrunk: false,
        mineffect: defaults.read(MINEFFECT).filter(|e| !e.is_empty()),
        scaled: false,
    }
}

/// Whether the Dock is hidden our way right now.
pub fn is_hidden(defaults: &impl Defaults) -> bool {
    defaults.read(DELAY).and_then(|d| d.trim().parse::<f64>().ok()).is_some_and(|d| d >= OURS_FROM)
}

fn write_atomically(path: &Path, text: &str) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let partial = path.with_extension("json.partial");
    {
        use std::io::Write as _;
        let mut file = std::fs::File::create(&partial)?;
        file.write_all(text.as_bytes())?;
        file.sync_all()?;
    }
    std::fs::rename(&partial, path)
}

/// Saves the user's Dock settings and hides the Dock. With `shrink`, also makes its icons tiny and turns off
/// magnification, so ours covers it in Mission Control (where the system shows it anyway).
pub fn hide(dir: &Path, defaults: &impl Defaults, shrink: bool) -> io::Result<()> {
    // Left over from a crash: those are the user's values, and the current ones are ours.
    restore_without_restart(dir, defaults)?;
    let mut saved = read_current(defaults);
    saved.shrunk = shrink;
    saved.scaled = true;
    let text = serde_json::to_string_pretty(&saved).map_err(io::Error::other)?;
    write_atomically(&state_path(dir), &text)?;
    defaults.write_bool(AUTOHIDE, true);
    defaults.write_float(DELAY, HIDDEN_DELAY);
    if shrink {
        defaults.write_float(TILESIZE, TINY_TILES);
        defaults.write_bool(MAGNIFICATION, false);
    }
    defaults.write_string(MINEFFECT, OUR_MINEFFECT);
    defaults.restart_dock();
    Ok(())
}

/// Puts the saved settings back and restarts the Dock. Returns whether there was anything to restore.
pub fn restore(dir: &Path, defaults: &impl Defaults) -> io::Result<bool> {
    let restored = restore_without_restart(dir, defaults)?;
    if restored {
        defaults.restart_dock();
    }
    Ok(restored)
}

fn restore_without_restart(dir: &Path, defaults: &impl Defaults) -> io::Result<bool> {
    let path = state_path(dir);
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(err) => return Err(err),
    };
    // A file that can't be read still means the Dock was hidden: fall back to the macOS defaults.
    let saved: Saved = serde_json::from_str(&text).unwrap_or_default();
    match saved.autohide {
        Some(on) => defaults.write_bool(AUTOHIDE, on),
        None => defaults.delete(AUTOHIDE),
    }
    match saved.autohide_delay.filter(|d| *d < OURS_FROM) {
        Some(delay) => defaults.write_float(DELAY, delay),
        None => defaults.delete(DELAY),
    }
    if saved.shrunk {
        match saved.tilesize {
            Some(size) => defaults.write_float(TILESIZE, size),
            None => defaults.delete(TILESIZE),
        }
        match saved.magnification {
            Some(on) => defaults.write_bool(MAGNIFICATION, on),
            None => defaults.delete(MAGNIFICATION),
        }
        match saved.largesize {
            Some(size) => defaults.write_float(LARGESIZE, size),
            None => defaults.delete(LARGESIZE),
        }
    }
    if saved.scaled {
        match &saved.mineffect {
            Some(effect) => defaults.write_string(MINEFFECT, effect),
            None => defaults.delete(MINEFFECT),
        }
    }
    std::fs::remove_file(&path)?;
    Ok(true)
}

/// Brings the Dock back even with no saved file, when it is still hidden our way (the file was lost).
pub fn unhide_orphan(defaults: &impl Defaults) -> bool {
    if !is_hidden(defaults) {
        return false;
    }
    defaults.delete(DELAY);
    defaults.write_bool(AUTOHIDE, false);
    defaults.restart_dock();
    true
}

#[cfg(test)]
pub mod tests {
    use std::cell::{Cell, RefCell};
    use std::collections::BTreeMap;

    use super::*;

    #[derive(Default)]
    pub struct Fake {
        pub values: RefCell<BTreeMap<String, String>>,
        pub restarts: Cell<u32>,
    }

    impl Fake {
        pub fn with(pairs: &[(&str, &str)]) -> Self {
            let fake = Self::default();
            for (k, v) in pairs {
                fake.values.borrow_mut().insert((*k).into(), (*v).into());
            }
            fake
        }
        fn get(&self, key: &str) -> Option<String> {
            self.values.borrow().get(key).cloned()
        }
    }

    impl Defaults for Fake {
        fn read(&self, key: &str) -> Option<String> {
            self.get(key)
        }
        fn write_bool(&self, key: &str, value: bool) {
            self.values.borrow_mut().insert(key.into(), if value { "1" } else { "0" }.into());
        }
        fn write_float(&self, key: &str, value: f64) {
            self.values.borrow_mut().insert(key.into(), value.to_string());
        }
        fn write_string(&self, key: &str, value: &str) {
            self.values.borrow_mut().insert(key.into(), value.into());
        }
        fn delete(&self, key: &str) {
            self.values.borrow_mut().remove(key);
        }
        fn restart_dock(&self) {
            self.restarts.set(self.restarts.get() + 1);
        }
    }

    pub fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("notchbase-dock-test-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn the_minimize_effect_is_scale_while_hidden_and_put_back_after() {
        let dir = scratch("mineffect");
        let fake = Fake::with(&[(MINEFFECT, "genie")]);
        hide(&dir, &fake, false).unwrap();
        assert_eq!(fake.get(MINEFFECT).as_deref(), Some("scale"));
        restore(&dir, &fake).unwrap();
        assert_eq!(fake.get(MINEFFECT).as_deref(), Some("genie"));
        let unset = Fake::with(&[]);
        hide(&dir, &unset, false).unwrap();
        restore(&dir, &unset).unwrap();
        assert_eq!(unset.get(MINEFFECT), None, "an unset effect is unset again");
    }

    #[test]
    fn hide_then_restore_round_trips_unset_values() {
        let dir = scratch("unset");
        let fake = Fake::default();
        hide(&dir, &fake, false).unwrap();
        assert_eq!(fake.get(AUTOHIDE).as_deref(), Some("1"));
        assert_eq!(fake.get(DELAY).as_deref(), Some("1000000"));
        assert!(is_hidden(&fake));
        assert!(restore(&dir, &fake).unwrap());
        assert_eq!(fake.get(AUTOHIDE), None, "it was unset, so it is deleted");
        assert_eq!(fake.get(DELAY), None);
        assert_eq!(fake.restarts.get(), 2);
        assert!(!state_path(&dir).exists());
        assert!(!restore(&dir, &fake).unwrap(), "nothing left to restore");
    }

    #[test]
    fn restore_puts_back_the_users_own_values() {
        let dir = scratch("own");
        let fake = Fake::with(&[(AUTOHIDE, "1"), (DELAY, "0.2")]);
        hide(&dir, &fake, false).unwrap();
        restore(&dir, &fake).unwrap();
        assert_eq!(fake.get(AUTOHIDE).as_deref(), Some("1"));
        assert_eq!(fake.get(DELAY).as_deref(), Some("0.2"));
    }

    #[test]
    fn a_crash_file_is_restored_before_hiding_again() {
        let dir = scratch("crash");
        let fake = Fake::with(&[(AUTOHIDE, "0")]);
        hide(&dir, &fake, false).unwrap();
        // Killed without a chance to restore; then started again.
        hide(&dir, &fake, false).unwrap();
        let saved: Saved = serde_json::from_str(&std::fs::read_to_string(state_path(&dir)).unwrap()).unwrap();
        assert_eq!(
            saved,
            Saved { autohide: Some(false), scaled: true, ..Saved::default() },
            "the hacked values were not saved"
        );
        restore(&dir, &fake).unwrap();
        assert_eq!(fake.get(AUTOHIDE).as_deref(), Some("0"));
        assert_eq!(fake.get(DELAY), None);
    }

    #[test]
    fn our_own_delay_is_never_saved_as_the_users() {
        let dir = scratch("ours");
        let fake = Fake::with(&[(AUTOHIDE, "1"), (DELAY, "1000000")]);
        hide(&dir, &fake, false).unwrap();
        restore(&dir, &fake).unwrap();
        assert_eq!(fake.get(DELAY), None);
    }

    #[test]
    fn a_broken_file_still_brings_the_dock_back() {
        let dir = scratch("broken");
        std::fs::write(state_path(&dir), "{ nope").unwrap();
        let fake = Fake::with(&[(AUTOHIDE, "1"), (DELAY, "1000000")]);
        assert!(restore(&dir, &fake).unwrap());
        assert_eq!(fake.get(AUTOHIDE), None);
        assert_eq!(fake.get(DELAY), None);
    }

    #[test]
    fn an_orphaned_hack_is_undone() {
        let fake = Fake::with(&[(AUTOHIDE, "1"), (DELAY, "1000000")]);
        assert!(unhide_orphan(&fake));
        assert_eq!(fake.get(AUTOHIDE).as_deref(), Some("0"));
        assert!(!unhide_orphan(&fake), "the second time there is nothing to undo");
    }

    #[test]
    fn shrinking_puts_the_sizes_back_exactly() {
        let dir = scratch("shrink");
        let fake = Fake::with(&[(TILESIZE, "48"), (MAGNIFICATION, "1")]);
        hide(&dir, &fake, true).unwrap();
        assert_eq!(fake.get(TILESIZE).as_deref(), Some("16"));
        assert_eq!(fake.get(MAGNIFICATION).as_deref(), Some("0"));
        // A crash, then a start without shrinking: the sizes still come back.
        hide(&dir, &fake, false).unwrap();
        assert_eq!(fake.get(TILESIZE).as_deref(), Some("48"));
        assert_eq!(fake.get(MAGNIFICATION).as_deref(), Some("1"));
        assert_eq!(fake.get(LARGESIZE), None, "it was unset, so it stays unset");
        restore(&dir, &fake).unwrap();
        assert_eq!(fake.get(TILESIZE).as_deref(), Some("48"));
    }

    #[test]
    fn values_parse_like_defaults_prints_them() {
        assert_eq!(parse_bool("1\n"), Some(true));
        assert_eq!(parse_bool("0"), Some(false));
        assert_eq!(parse_bool("maybe"), None);
        assert_eq!(parse_delay("0.5"), Some(0.5));
        assert_eq!(parse_delay("1000000"), None);
    }
}
