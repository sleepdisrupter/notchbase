//! The tiles along the dock and how they come and go: pinned apps, then running ones, then the folders and the
//! Trash. A tile that leaves stays in the list while it melts away.

use std::path::{Path, PathBuf};
use std::time::Instant;

use notchbase_paint::spring::{Spring, SpringConfig, UNIT_PRECISION};

use crate::config::Config;
use crate::minimized::Minimized;
use crate::os::Running;

// The notch's own springs (`src/ui/motion.rs` and `src/notch/moves/` in the notchbase app): arriving is `BOUNCY`,
// leaving and dots `SMOOTH`, sliding aside the island's dodge `SLIDE`, hover `HOVER` and pressing `PRESS`.
pub const ARRIVE: SpringConfig = SpringConfig::new(0.42, 0.78);
pub const LEAVE: SpringConfig = SpringConfig::new(0.32, 1.0);
pub const DOT: SpringConfig = SpringConfig::new(0.32, 1.0);
pub const SLIDE: SpringConfig = SpringConfig::new(0.4, 0.92);
/// The hovered icon lifting.
pub const HOVER: SpringConfig = SpringConfig::new(0.18, 1.0);
/// Letting go of it: a little softer, still critically damped, so it eases back with no dip below rest.
pub const HOVER_OUT: SpringConfig = SpringConfig::new(0.24, 1.0);
/// Pressing an icon down, and letting it back up.
pub const PRESS: SpringConfig = SpringConfig::new(0.2, 0.8);
/// The notch's launch swell (`src/notch/moves/launch.rs`): a kick of this many points a second into `SWELL`.
pub const GULP_KICK: f32 = 150.0;
/// An app dropped into the dock: it hops up off the cradle and lands with a little give (`fall`, in points above home).
pub const LAND: SpringConfig = SpringConfig::new(0.38, 0.55);
pub const HOP: f32 = 170.0;
pub const SWELL: SpringConfig = SpringConfig::new(0.36, 0.52);
/// A launch that never reports in stops pulsing after this long.
pub const LAUNCH_TIMEOUT: f32 = 30.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    App,
    /// A minimized window, shown between the separator and the folders.
    Window,
    Folder,
    Trash,
}

pub struct Tile {
    pub key: String,
    pub kind: Kind,
    pub path: PathBuf,
    pub name: String,
    pub pinned: bool,
    pub pid: Option<i32>,
    pub presence: Spring,
    pub leaving: bool,
    /// Burst on its way out: the burst draws it, the tile only holds its slot open, then closes it.
    pub popped: bool,
    /// Seconds the slot stays open before closing, while the burst swells.
    pub hold: f32,
    pub dot: Spring,
    pub slide: Spring,
    /// The launch swell, in points, like the notch's gulp when notchbase starts.
    pub gulp: Spring,
    /// How far above its home the icon is while it lands after being dropped in, in points.
    pub fall: Spring,
    /// When the app was asked to launch, until it's running: the dot pulses meanwhile.
    pub launching: Option<Instant>,
    /// 1 while it's the icon under the pointer.
    pub hover: Spring,
    /// 1 while it's pressed.
    pub down: Spring,
    pub icon: Option<egui::TextureHandle>,
    pub icon_full: bool,
    /// The icon was drawn for another size: it's redrawn when there's time, and shown as it is meanwhile.
    pub stale: bool,
    /// A minimized window's app and the window itself.
    pub window: Option<(i32, crate::windows::Win)>,
}

impl Tile {
    pub fn new(key: String, kind: Kind, path: PathBuf, name: String, arriving: bool) -> Self {
        let start = if arriving { 0.0 } else { 1.0 };
        let mut presence = Spring::new(start, ARRIVE, UNIT_PRECISION);
        presence.animate_to(1.0, ARRIVE);
        Self {
            key,
            kind,
            path,
            name,
            pinned: false,
            pid: None,
            presence,
            leaving: false,
            popped: false,
            hold: 0.0,
            dot: Spring::new(0.0, DOT, UNIT_PRECISION),
            slide: Spring::new(0.0, SLIDE, 0.05),
            gulp: Spring::new(0.0, SWELL, 0.01),
            fall: Spring::new(0.0, LAND, 0.02),
            launching: None,
            hover: Spring::new(0.0, HOVER, UNIT_PRECISION),
            down: Spring::new(0.0, PRESS, UNIT_PRECISION),
            icon: None,
            icon_full: false,
            stale: false,
            window: None,
        }
    }

    pub fn section(&self) -> u8 {
        match self.kind {
            Kind::App => 0,
            Kind::Window | Kind::Folder | Kind::Trash => 1,
        }
    }

    pub fn running(&self) -> bool {
        self.pid.is_some()
    }

    pub fn gone(&self) -> bool {
        self.leaving && self.presence.value <= 0.01 && self.presence.target == 0.0
    }

    pub fn step(&mut self, dt: f32) {
        if self.hold > 0.0 {
            self.hold -= dt;
            if self.hold <= 0.0 {
                self.presence.animate_to(0.0, LEAVE);
            }
        }
        self.presence.step(dt);
        self.dot.step(dt);
        self.slide.step(dt);
        self.hover.step(dt);
        self.down.step(dt);
        self.gulp.step(dt);
        self.fall.step(dt);
        if self.launching.is_some_and(|l| self.pid.is_some() || l.elapsed().as_secs_f32() > LAUNCH_TIMEOUT) {
            self.launching = None;
        }
    }

    pub fn settled(&self) -> bool {
        self.hold <= 0.0
            && self.presence.is_settled()
            && self.dot.is_settled()
            && self.slide.is_settled()
            && self.hover.is_settled()
            && self.down.is_settled()
            && self.gulp.is_settled()
            && self.fall.is_settled()
            && self.launching.is_none()
    }
}

pub const TRASH_KEY: &str = "::trash";
/// The file manager, which always leads the dock: Finder, or File Explorer on Windows.
#[cfg(not(windows))]
pub const FINDER: &str = "/System/Library/CoreServices/Finder.app";
#[cfg(windows)]
pub const FINDER: &str = r"C:\Windows\explorer.exe";

/// A minimized window's tile.
pub fn window_key(id: u32) -> String {
    format!("::window {id}")
}

pub fn key_of(path: &Path) -> String {
    path.to_string_lossy().trim_end_matches('/').to_owned()
}

/// What the dock should show, in order: (key, kind, path, pinned, pid).
pub fn wanted(
    config: &Config,
    running: &[Running],
    minimized: &[Minimized],
    trash: &Path,
) -> Vec<(String, Kind, PathBuf, bool, Option<i32>)> {
    let pid_of = |path: &Path| running.iter().find(|r| r.path.as_deref() == Some(path)).map(|r| r.pid);
    let mut out = Vec::new();
    // Finder always leads, as in the system Dock.
    let finder = PathBuf::from(FINDER);
    out.push((key_of(&finder), Kind::App, finder.clone(), true, pid_of(&finder)));
    for path in config.pinned.iter().filter(|p| p.as_path() != finder) {
        if path.exists() {
            out.push((key_of(path), Kind::App, path.clone(), true, pid_of(path)));
        }
    }
    for app in running {
        let Some(path) = &app.path else { continue };
        if !out.iter().any(|(_, _, p, _, _)| p == path) {
            out.push((key_of(path), Kind::App, path.clone(), false, Some(app.pid)));
        }
    }
    // Minimized windows, of apps the dock shows, oldest first. No dot: the window's app has its own.
    for m in minimized {
        if let Some(path) = running.iter().find(|r| r.pid == m.pid).and_then(|r| r.path.clone()) {
            out.push((window_key(m.win.id), Kind::Window, path, false, None));
        }
    }
    for path in &config.folders() {
        if path.exists() {
            out.push((key_of(path), Kind::Folder, path.clone(), true, None));
        }
    }
    out.push((TRASH_KEY.to_owned(), Kind::Trash, trash.to_path_buf(), true, None));
    out
}

/// Brings `tiles` in line with `wanted`, keeping each tile's springs and textures. Tiles no longer wanted start
/// leaving where they are; new ones arrive. `arriving` is false on the first fill so nothing grows in at start.
pub fn merge(
    tiles: &mut Vec<Tile>,
    wanted: Vec<(String, Kind, PathBuf, bool, Option<i32>)>,
    name_of: impl Fn(&Path, Kind) -> String,
    arriving: bool,
) {
    let mut old: Vec<Tile> = std::mem::take(tiles);
    let old_keys: Vec<String> = old.iter().map(|t| t.key.clone()).collect();
    let mut out: Vec<Tile> = Vec::with_capacity(wanted.len() + 2);
    for (key, kind, path, pinned, pid) in wanted {
        let tile = match old.iter().position(|t| t.key == key) {
            Some(i) => {
                let mut t = old.remove(i);
                if t.leaving {
                    t.leaving = false;
                    t.popped = false;
                    t.hold = 0.0;
                    t.presence.animate_to(1.0, ARRIVE);
                }
                t
            }
            None => Tile::new(key, kind, path.clone(), name_of(&path, kind), arriving),
        };
        out.push(tile);
        let t = out.last_mut().expect("just pushed");
        t.pinned = pinned;
        if t.pid.is_none() && pid.is_some() {
            t.dot.animate_to(1.0, DOT);
            t.launching = None;
        }
        if pid.is_none() {
            t.dot.animate_to(0.0, DOT);
        }
        t.pid = pid;
        if !arriving {
            t.dot.snap_to(t.dot.target);
        }
    }
    // What's left is leaving: put each back after the tile it followed before.
    for mut t in old {
        if !t.leaving {
            t.leaving = true;
            // A window's tile: the window itself flies out of it, so the tile just makes way, with no droplet.
            t.popped = t.kind == Kind::Window;
            t.presence.animate_to(0.0, LEAVE);
            t.dot.animate_to(0.0, DOT);
        }
        let before = old_keys.iter().position(|k| *k == t.key).unwrap_or(0);
        let anchor = old_keys[..before].iter().rev().find_map(|k| out.iter().position(|o| &o.key == k));
        let at = anchor.map_or(0, |i| i + 1);
        out.insert(at.min(out.len()), t);
    }
    out.retain(|t| !t.gone());
    *tiles = out;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(pinned: &[&str]) -> Config {
        Config { pinned: pinned.iter().map(PathBuf::from).collect(), show_downloads: false, ..Config::default() }
    }

    #[test]
    fn downloads_shows_first_of_the_folders_once_and_only_while_it_is_on() {
        let Some(downloads) = crate::config::downloads_folder().filter(|d| d.is_dir()) else { return };
        let config = Config { others: vec!["/tmp".into(), downloads.clone()], show_downloads: true, ..cfg(&[]) };
        let folders: Vec<PathBuf> = wanted(&config, &[], &[], Path::new("/t"))
            .into_iter()
            .filter(|w| w.1 == Kind::Folder)
            .map(|w| w.2)
            .collect();
        assert_eq!(folders, [downloads.clone(), PathBuf::from("/tmp")], "first, and only once");
        let off = Config { show_downloads: false, others: vec!["/tmp".into()], ..config };
        assert!(wanted(&off, &[], &[], Path::new("/t")).iter().all(|w| w.2 != downloads), "turned off, it's gone");
        let older: Config = serde_json::from_str("{}").unwrap();
        assert!(older.show_downloads, "a dock.json from before the setting shows it");
    }

    fn app(pid: i32, path: &str) -> Running {
        Running { pid, path: Some(path.into()), name: path.into(), active: false, hidden: false }
    }

    fn keys(tiles: &[Tile]) -> Vec<&str> {
        tiles.iter().map(|t| t.key.as_str()).collect()
    }

    #[test]
    fn pinned_then_running_then_trash() {
        let pinned = ["/System/Applications/Calendar.app", "/System/Applications/Notes.app"];
        let running = [app(10, "/System/Applications/Notes.app"), app(11, "/System/Applications/Chess.app")];
        let w = wanted(&cfg(&pinned), &running, &[], Path::new("/tmp/.Trash"));
        let order: Vec<&str> = w.iter().map(|x| x.0.as_str()).collect();
        assert_eq!(
            order,
            [
                FINDER,
                "/System/Applications/Calendar.app",
                "/System/Applications/Notes.app",
                "/System/Applications/Chess.app",
                TRASH_KEY
            ]
        );
        assert_eq!(w[2].4, Some(10), "a pinned app that runs keeps its pid");
        assert!(!w[3].3, "a running app nobody pinned is not pinned");
    }

    #[test]
    fn minimized_windows_sit_between_the_apps_and_the_folders() {
        let running = [app(10, "/System/Applications/Notes.app")];
        let win = |id| crate::windows::Win { id, title: String::new(), frame: egui::Rect::ZERO, on_screen: false };
        let minimized = [
            Minimized { pid: 10, win: win(7) },
            // An app the dock doesn't show (no path) has no tile for its windows.
            Minimized { pid: 99, win: win(8) },
        ];
        let config = Config { others: vec!["/tmp".into()], ..cfg(&[]) };
        let w = wanted(&config, &running, &minimized, Path::new("/tmp/.Trash"));
        let order: Vec<&str> = w.iter().map(|x| x.0.as_str()).collect();
        assert_eq!(order, [FINDER, "/System/Applications/Notes.app", "::window 7", "/tmp", TRASH_KEY]);
        assert_eq!(w[2].1, Kind::Window);
        assert_eq!(w[2].4, None, "a window's tile has no dot of its own");
    }

    #[test]
    fn a_quitting_app_melts_away_where_it_was() {
        let mut tiles = Vec::new();
        let first = vec![
            ("a".into(), Kind::App, "a".into(), true, None),
            ("b".into(), Kind::App, "b".into(), false, Some(1)),
            ("c".into(), Kind::App, "c".into(), false, Some(2)),
        ];
        merge(&mut tiles, first, |_, _| String::new(), false);
        assert!(tiles.iter().all(|t| t.presence.value == 1.0), "nothing grows in at start");
        let second =
            vec![("a".into(), Kind::App, "a".into(), true, None), ("c".into(), Kind::App, "c".into(), false, Some(2))];
        merge(&mut tiles, second, |_, _| String::new(), true);
        assert_eq!(keys(&tiles), ["a", "b", "c"], "b stays in place while it leaves");
        assert!(tiles[1].leaving);
        for _ in 0..240 {
            tiles.iter_mut().for_each(|t| t.step(1.0 / 120.0));
        }
        let third =
            vec![("a".into(), Kind::App, "a".into(), true, None), ("c".into(), Kind::App, "c".into(), false, Some(2))];
        merge(&mut tiles, third, |_, _| String::new(), true);
        assert_eq!(keys(&tiles), ["a", "c"]);
    }

    #[test]
    fn a_new_app_grows_in_and_its_dot_comes_with_it() {
        let mut tiles = Vec::new();
        merge(&mut tiles, vec![("a".into(), Kind::App, "a".into(), true, None)], |_, _| String::new(), false);
        merge(&mut tiles, vec![("a".into(), Kind::App, "a".into(), true, Some(5))], |_, _| String::new(), true);
        assert_eq!(tiles[0].dot.target, 1.0);
        merge(
            &mut tiles,
            vec![
                ("a".into(), Kind::App, "a".into(), true, Some(5)),
                ("n".into(), Kind::App, "n".into(), false, Some(6)),
            ],
            |_, _| String::new(),
            true,
        );
        assert_eq!(tiles[1].presence.value, 0.0);
        assert_eq!(tiles[1].presence.target, 1.0);
    }
}
