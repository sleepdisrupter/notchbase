//! The app's side of Find: the search service, its icons and the view the Find page draws.

mod actions;
mod focus;
mod pick;
mod summon;
#[cfg(test)]
mod tests;

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use crate::config::Config;
use crate::find::context::Context;
use crate::find::icons::Icons;
use crate::find::view::{Access, Answer, FindRow, FindState, FindView, Kind, Phase, Status};
use crate::find::{Find, Key, Setup, key_of};
use crate::platform::{self, KnownDir};
use crate::poll::Wake;

const ICONS_FOLDER: &str = "icons";

pub(super) struct Finding {
    pub(super) service: Option<Find>,
    icons: Icons,
    seen: u64,
    drawn: u64,
    status: Status,
    answer: Option<Answer>,
    answered: u64,
    app: Option<Arc<str>>,
    running: Arc<[Key]>,
    reported: HashSet<PathBuf>,
    gone: Vec<u64>,
    pub(super) asked_this_run: bool,
    pub(super) lifting: Option<Instant>,
    pub(super) kept: u8,
    pub(super) focus_checked: Option<Instant>,
    presses: Option<platform::PressesElsewhere>,
}

impl Finding {
    pub(super) fn start(wake: Wake, config: &Config, enabled: bool) -> Self {
        if !enabled {
            return Self::resting(None);
        }
        let dir = Find::folder();
        let home = platform::known_dir(KnownDir::Home);
        let (config, ram_bytes) = (config.find.clone(), ram_bytes());
        let icons = Icons::start(wake.clone(), true, dir.as_ref().map(|dir| dir.join(ICONS_FOLDER)));
        let service = Find::start(wake, Setup { dir, home, config, ram_bytes, writes: true });
        Self { icons, ..Self::resting(Some(service)) }
    }

    fn resting(service: Option<Find>) -> Self {
        Self {
            service,
            icons: Icons::start(Arc::new(|| {}), false, None),
            seen: 0,
            drawn: 0,
            status: Status::default(),
            answer: None,
            answered: 0,
            app: None,
            running: Arc::default(),
            reported: HashSet::new(),
            gone: Vec::new(),
            asked_this_run: false,
            lifting: None,
            kept: 0,
            focus_checked: None,
            presses: None,
        }
    }

    pub(super) fn synced(&mut self) -> bool {
        let Some(service) = &self.service else { return false };
        let (version, drawn) = (service.version(), self.icons.version());
        if (version, drawn) == (self.seen, self.drawn) {
            return false;
        }
        if version != self.seen {
            self.status = service.status();
            if let Some(mut answer) = service.take() {
                answer.rows.retain(|row| !self.gone.contains(&row.key));
                self.icons.want(&answer.rows, self.status.access == Access::Limited);
                self.answer = Some(answer);
                self.answered += 1;
            }
        }
        (self.seen, self.drawn) = (version, drawn);
        true
    }

    pub(super) fn view(&self, config: &Config) -> FindView {
        let on = config.find.enabled && self.service.is_some();
        let Status { phase, entries, access, truncated, .. } = self.status;
        let state = match (on, phase) {
            (false, _) => FindState::Off,
            (true, Phase::Off | Phase::Loading) => FindState::Starting,
            (true, Phase::Scanning | Phase::Watching) => FindState::Ready,
        };
        let indexing = (on && matches!(phase, Phase::Loading | Phase::Scanning)).then_some(entries);
        let nudge = on && self.limited() && !config.permissions.find_access_hidden;
        let mut view = FindView { state, entries, access, nudge, truncated, indexing, ..FindView::default() };
        if let Some(answer) = &self.answer {
            let rows: Vec<FindRow> = answer.rows.iter().map(|row| self.with_icon(row)).collect();
            (view.query, view.rows, view.reading, view.more) =
                (answer.query.clone(), rows.into(), answer.reading.clone(), answer.more);
        }
        view.answered = self.answered;
        view.gone = self.gone.as_slice().into();
        let pins: Vec<FindRow> = config.find.pins.iter().map(pinned_row).collect();
        self.icons.want(&pins, false);
        view.pins = pins.iter().map(|pin| self.with_icon(pin)).collect();
        view
    }

    fn with_icon(&self, row: &FindRow) -> FindRow {
        FindRow { icon: self.icons.of(row), ..row.clone() }
    }

    pub(super) fn shown(&mut self, app: Option<String>) {
        self.app = app.map(Arc::from);
        self.kept = 0;
        self.focus_checked = Some(Instant::now());
        self.presses = (!cfg!(test)).then(platform::watch_presses_elsewhere).flatten();
        self.running = platform::running_apps().iter().map(|path| key_of(path)).collect();
        if let Some(service) = &self.service {
            service.shown(self.context());
            service.recheck_access();
        }
    }

    pub(super) fn left(&mut self) {
        self.presses = None;
        self.answer = None;
        self.gone.clear();
        if let Some(service) = &self.service {
            service.left();
        }
    }

    pub(super) fn pressed_elsewhere(&self) -> bool {
        self.presses.as_ref().is_some_and(platform::PressesElsewhere::take_press)
    }

    pub(super) fn context(&self) -> Context {
        Context::current(self.app.clone()).among(self.running.clone())
    }

    pub(super) fn limited(&self) -> bool {
        self.service.is_some() && self.status.phase != Phase::Off && self.status.access == Access::Limited
    }

    pub(super) fn reachable(&self, path: &Path) -> bool {
        self.service.as_ref().is_some_and(|service| service.reachable(path))
    }

    pub(super) fn missing(&mut self, path: &Path) -> bool {
        if let Some(answer) = &mut self.answer {
            let gone: Vec<u64> = answer.rows.iter().filter(|row| *row.path == *path).map(|row| row.key).collect();
            answer.rows.retain(|row| *row.path != *path);
            self.answered += u64::from(!gone.is_empty());
            self.gone.extend(gone);
        }
        if let Some(service) = &self.service {
            service.gone(path);
        }
        self.reported.insert(path.to_path_buf())
    }
}

fn ram_bytes() -> u64 {
    let mut system = sysinfo::System::new();
    system.refresh_memory_specifics(sysinfo::MemoryRefreshKind::nothing().with_ram());
    system.total_memory()
}

fn pinned_row(pin: &crate::config::FindPin) -> FindRow {
    let path = Path::new(&pin.path);
    let place = path.parent().map_or_else(String::new, |above| above.to_string_lossy().into_owned());
    FindRow {
        key: key_of(path).0,
        path: Arc::from(path),
        name: pin.name.as_str().into(),
        place: place.into(),
        marks: Vec::new(),
        kind: if pin.app {
            Kind::App
        } else if pin.folder {
            Kind::Folder
        } else {
            Kind::Other
        },
        app: pin.app,
        folder: pin.folder,
        cloud: false,
        modified: 0,
        why: None,
        predicted: false,
        icon: None,
    }
}
