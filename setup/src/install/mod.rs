//! Installing and removing. The installer downloads notchbase and runs it with the install flags,
//! so the app installs itself. Removing runs the installed copy with the uninstall flag.

mod child;
mod lookup;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(not(any(windows, target_os = "macos")))]
mod other;
#[cfg(windows)]
mod windows;

#[cfg(target_os = "macos")]
use macos as imp;
#[cfg(not(any(windows, target_os = "macos")))]
use other as imp;
#[cfg(windows)]
use windows as imp;

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use notchbase_release::install::{Choices, ERROR_PREFIX, INSTALL, UNINSTALL};
use notchbase_release::{Release, Version};

pub use imp::{OS, SHORTCUTS};
pub use lookup::{Latest, Lookup};

// How far along the progress bar each step is.
const DOWNLOAD_FROM: f32 = 0.02;
const DOWNLOAD_TO: f32 = 0.84;
const INSTALLING: f32 = 0.88;
const STARTING: f32 = 0.97;
// Windows can hold the download open for a moment after it runs, so cleaning up retries.
const TIDY_TRIES: u32 = 20;
const TIDY_PAUSE: Duration = Duration::from_millis(150);
const REMOVING: f32 = 0.35;
const NOT_INSTALLED: &str = "notchbase isn\u{2019}t installed";
const USER_AGENT: &str = notchbase_net::user_agent!();

pub type Wake = Arc<dyn Fn() + Send + Sync>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Options {
    pub login: bool,
    pub updates: bool,
    pub assistant: bool,
    pub desktop: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Setting {
    Login,
    Updates,
    Assistant,
    Desktop,
}

impl Default for Options {
    fn default() -> Self {
        Self { login: true, updates: true, assistant: true, desktop: false }
    }
}

impl Options {
    pub fn get(mut self, setting: Setting) -> bool {
        *self.flag(setting)
    }

    pub fn toggle(&mut self, setting: Setting) {
        let flag = self.flag(setting);
        *flag = !*flag;
    }

    fn flag(&mut self, setting: Setting) -> &mut bool {
        match setting {
            Setting::Login => &mut self.login,
            Setting::Updates => &mut self.updates,
            Setting::Assistant => &mut self.assistant,
            Setting::Desktop => &mut self.desktop,
        }
    }

    fn choices(self) -> Choices {
        Choices {
            login: self.login,
            updates_off: !self.updates,
            assistant_off: !self.assistant,
            desktop: self.desktop && SHORTCUTS,
        }
    }

    fn flags(self) -> Vec<&'static str> {
        self.choices().to_args()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Stage {
    Removing,
    Downloading,
    Installing,
    Starting,
    Done,
    Failed(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Progress {
    pub stage: Stage,
    pub fraction: f32,
}

pub struct Job {
    progress: Arc<Mutex<Progress>>,
}

impl Job {
    pub fn install(options: Options, release: Release, wake: Wake) -> Self {
        Self::spawn(Stage::Downloading, wake, move |report| run(options, &release, report))
    }

    pub fn uninstall(wake: Wake) -> Self {
        Self::spawn(Stage::Removing, wake, |report| {
            report(Stage::Removing, REMOVING);
            remove()
        })
    }

    fn spawn(
        first: Stage,
        wake: Wake,
        work: impl FnOnce(&dyn Fn(Stage, f32)) -> Result<(), String> + Send + 'static,
    ) -> Self {
        let progress = Arc::new(Mutex::new(Progress { stage: first, fraction: 0.0 }));
        let shared = Arc::clone(&progress);
        let report = move |stage: Stage, fraction: f32| {
            *shared.lock().unwrap_or_else(PoisonError::into_inner) = Progress { stage, fraction };
            wake();
        };
        std::thread::spawn(move || match work(&report) {
            Ok(()) => report(Stage::Done, 1.0),
            Err(problem) => report(Stage::Failed(problem), 0.0),
        });
        Self { progress }
    }

    pub fn progress(&self) -> Progress {
        self.progress.lock().unwrap_or_else(PoisonError::into_inner).clone()
    }
}

pub fn destination() -> Option<PathBuf> {
    imp::destination()
}

pub fn installed() -> Option<Installed> {
    destination().filter(|path| path.exists())?;
    Some(Installed { version: imp::installed_version().and_then(|text| Version::parse(&text)) })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Installed {
    pub version: Option<Version>,
}

impl Installed {
    pub fn current_with(self, latest: Version) -> bool {
        self.version.is_some_and(|version| version >= latest)
    }
}

fn remove() -> Result<(), String> {
    let app = imp::installed_binary().filter(|app| app.exists()).ok_or(NOT_INSTALLED)?;
    let mut command = imp::command(&app);
    command.arg(UNINSTALL);
    child::finish(command)
}

fn run(options: Options, release: &Release, report: &dyn Fn(Stage, f32)) -> Result<(), String> {
    let dir = std::env::temp_dir().join(format!("notchbase-setup-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).map_err(|err| format!("Couldn't make a folder to download into: {err}"))?;
    let result = download_and_install(&dir, options, release, report);
    tidy(&dir);
    result
}

fn download_and_install(
    dir: &Path,
    options: Options,
    release: &Release,
    report: &dyn Fn(Stage, f32),
) -> Result<(), String> {
    report(Stage::Downloading, DOWNLOAD_FROM);
    let file = dir.join(imp::DOWNLOAD);
    notchbase_release::download(release, &file, USER_AGENT, |done| {
        report(Stage::Downloading, DOWNLOAD_FROM + done * (DOWNLOAD_TO - DOWNLOAD_FROM));
    })?;
    let app = imp::unpack(&file, dir)?;
    report(Stage::Installing, INSTALLING);
    let mut command = imp::command(&app);
    command.arg(INSTALL).args(options.flags());
    child::finish(command)?;
    report(Stage::Starting, STARTING);
    Ok(())
}

fn problem(stderr: &str, status: &str) -> String {
    stderr
        .lines()
        .rev()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(|line| line.strip_prefix(ERROR_PREFIX).unwrap_or(line).to_owned())
        .unwrap_or_else(|| format!("The install stopped ({status})"))
}

fn tidy(dir: &Path) {
    for _ in 0..TIDY_TRIES {
        if std::fs::remove_dir_all(dir).is_ok() || !dir.exists() {
            return;
        }
        std::thread::sleep(TIDY_PAUSE);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_changed_choices_become_flags() {
        assert!(Options::default().flags().is_empty());
        let off = Options { login: false, updates: false, assistant: false, desktop: true };
        let expected: Vec<&str> = ["--no-login", "--no-updates", "--no-assistant"]
            .into_iter()
            .chain(SHORTCUTS.then_some("--desktop"))
            .collect();
        assert_eq!(off.flags(), expected);
    }

    #[test]
    fn each_setting_flips_its_option() {
        let settings = [Setting::Login, Setting::Updates, Setting::Assistant, Setting::Desktop];
        let defaults = Options::default();
        assert_eq!(settings.map(|setting| defaults.get(setting)), [true, true, true, false]);
        for setting in settings {
            let mut options = defaults;
            options.toggle(setting);
            assert_eq!(options.get(setting), !defaults.get(setting), "{setting:?}");
            let others = settings.iter().filter(|other| **other != setting);
            assert!(others.clone().all(|other| options.get(*other) == defaults.get(*other)), "{setting:?}");
            options.toggle(setting);
            assert_eq!(options, defaults, "{setting:?}");
        }
    }

    #[test]
    fn options_become_choices() {
        let off = Options { login: false, updates: false, assistant: true, desktop: true };
        let choices = off.choices();
        assert_eq!((choices.login, choices.updates_off, choices.assistant_off), (false, true, false));
        assert_eq!(choices.desktop, SHORTCUTS, "a desktop shortcut only where there are shortcuts");
        assert_eq!(Options::default().choices(), Choices::default());
    }

    #[test]
    fn last_error_line_explains_failure() {
        assert_eq!(problem("warming up\nnotchbase: Access is denied.\n\n", "exit code: 1"), "Access is denied.");
        assert_eq!(problem("", "exit code: 1"), "The install stopped (exit code: 1)");
        assert_eq!(problem("  \n\n", "signal: 9"), "The install stopped (signal: 9)");
        assert_eq!(problem("Something else broke\n", "exit code: 2"), "Something else broke");
    }
}
