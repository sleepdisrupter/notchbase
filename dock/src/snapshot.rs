//! Headless pictures of the dock, for checking the look without putting anything on screen:
//! `notchbase-dock --snapshot out.png --scene hover`. The window is never shown and the system Dock is left alone.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use egui::{pos2, vec2};
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop};
#[cfg(target_os = "macos")]
use winit::platform::macos::{ActivationPolicy, EventLoopBuilderExtMacOS as _};
use winit::window::{WindowAttributes, WindowId};

use crate::config::Config;
use crate::dock::{Dock, WINDOW_HEIGHT};
use crate::gl::{GlWindow, Offscreen};
use crate::liquid::Liquid;
use crate::pop::{Pop, Reach};
use crate::{app, os};

const WIDTH: f32 = 1280.0;
const STEP: f32 = 1.0 / 120.0;

struct Shot {
    out: PathBuf,
    scene: String,
    dir: PathBuf,
    result: Result<(), String>,
}

pub fn run(out: PathBuf, scene: String, dir: PathBuf) -> Result<(), String> {
    let mut builder = EventLoop::<()>::with_user_event();
    #[cfg(target_os = "macos")]
    builder.with_activation_policy(ActivationPolicy::Prohibited).with_default_menu(false);
    let event_loop = builder.build().map_err(|e| e.to_string())?;
    let mut shot = Shot { out, scene, dir, result: Ok(()) };
    event_loop.run_app(&mut shot).map_err(|e| e.to_string())?;
    shot.result
}

impl ApplicationHandler for Shot {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        self.result = self.take(event_loop);
        event_loop.exit();
    }

    fn window_event(&mut self, _: &ActiveEventLoop, _: WindowId, _: WindowEvent) {}
}

impl Shot {
    fn take(&self, event_loop: &ActiveEventLoop) -> Result<(), String> {
        let attrs = WindowAttributes::default()
            .with_visible(false)
            .with_decorations(false)
            .with_transparent(true)
            .with_inner_size(LogicalSize::new(64.0, 64.0));
        let mut gl = GlWindow::new(event_loop, attrs, false)?;
        let ppp = 2.0;
        gl.fixed_scale = Some(ppp);
        let config = Config::load_or_seed(&self.dir, app::seed_from_system_dock);

        if let Some(p) = self.scene.strip_prefix("pop:") {
            let seconds: f32 = p.parse().map_err(|_| "pop:<seconds>")?;
            return self.pop(&mut gl, seconds, ppp);
        }

        let size = vec2(WIDTH, WINDOW_HEIGHT);
        let mut config = config;
        config.show_names |= self.scene.starts_with("tooltip");
        // Saves (a drop pins an app) go to a scratch folder, never the real config.
        let scratch = std::env::temp_dir().join("notchbase-dock-snapshot");
        let mut dock = Dock::new(config, scratch, size);
        let mut running = os::running_apps();
        // The browser and player the ambient scenes show, as if running.
        let ambient = self.scene.starts_with("download") || self.scene.starts_with("pulse");
        if self.scene == "showcase" || ambient {
            // Only pinned apps show as running, so whatever else is open on this Mac stays out of the picture.
            running.retain(|app| app.path.as_ref().is_some_and(|path| dock.config.pinned.contains(path)));
        }
        if ambient {
            for (pid, path, name) in
                [(90_001, "/Applications/Zen.app", "Zen"), (90_002, "/Applications/Spotify.app", "Spotify")]
            {
                let path = PathBuf::from(path);
                running.retain(|r| r.path.as_ref() != Some(&path));
                running.push(os::Running { pid, path: Some(path), name: name.into(), active: false, hidden: false });
            }
        }
        dock.refresh(&running);
        dock.trash_full = os::trash_is_full();
        let px = [(size.x * ppp) as u32, (size.y * ppp) as u32];
        let target = Offscreen::new(&gl.gl, px);
        let liquid = Liquid::new(&gl.gl, gl.shader_version);
        let render = |gl: &mut GlWindow, dock: &mut Dock| {
            let blobs = std::cell::RefCell::new(Vec::new());
            let band = std::cell::Cell::new(dock.glass_band());
            gl.frame(
                |g, s, _| liquid.draw(g, s, ppp, &blobs.borrow(), 1.0, Some(band.get()), None),
                |ui| {
                    *blobs.borrow_mut() = dock.compose(ui.painter());
                    band.set(dock.glass_band());
                },
                Some(target.fbo),
                Some(px),
            );
        };
        // A first frame lays everything out and loads the icons.
        render(&mut gl, &mut dock);
        let steps = |gl: &mut GlWindow, dock: &mut Dock, seconds: f32| {
            for _ in 0..(seconds / STEP) as usize {
                dock.step(STEP);
                render(gl, dock);
            }
        };
        let icon_y = size.y - 4.0 - 7.0 - dock.config.tile_size * 0.5;
        let nth_x = |dock: &Dock, n: usize| {
            let count = dock.tiles.len() as f32;
            let slot = dock.config.tile_size + 8.0;
            size.x * 0.5 - count * slot * 0.5 + slot * (n as f32 + 0.5)
        };
        match self.scene.as_str() {
            "rest" => steps(&mut gl, &mut dock, 0.5),
            "showcase" => {
                dock.set_pointer(Some(pos2(nth_x(&dock, 3), icon_y)));
                steps(&mut gl, &mut dock, 1.2);
            }
            "hover" => {
                dock.set_pointer(Some(pos2(nth_x(&dock, 2), icon_y)));
                steps(&mut gl, &mut dock, 1.2);
            }
            "tooltip-ooze" => {
                dock.set_pointer(Some(pos2(nth_x(&dock, 2), icon_y)));
                steps(&mut gl, &mut dock, 0.11);
            }
            "menu" | "menu-dark" | "menu-light" => {
                // The first app with a menu that has dividers, in either appearance; "menu" is the second app's own.
                dock.dark = self.scene != "menu-light";
                let nth = if self.scene == "menu" { 1 } else { 0 };
                dock.set_pointer(Some(pos2(nth_x(&dock, nth), icon_y)));
                steps(&mut gl, &mut dock, 0.3);
                if nth == 0 {
                    dock.show_sample_menu();
                } else {
                    dock.press(true);
                    dock.release();
                }
                steps(&mut gl, &mut dock, 0.9);
            }
            "menu-claude" | "menu-chatgpt" | "menu-claude-light" | "menu-chatgpt-light" => {
                // The app's own menu with made-up usage, so nothing real is read.
                use crate::usage::{self, Provider};
                let provider = if self.scene.contains("claude") { Provider::Claude } else { Provider::ChatGpt };
                dock.dark = !self.scene.ends_with("-light");
                dock.usage.pin(provider, usage::sample(provider));
                let n = dock
                    .tiles
                    .iter()
                    .position(|t| Provider::of(&t.path) == Some(provider))
                    .ok_or("that app isn't in the dock")?;
                dock.set_pointer(Some(pos2(nth_x(&dock, n), icon_y)));
                steps(&mut gl, &mut dock, 0.3);
                dock.press(true);
                dock.release();
                steps(&mut gl, &mut dock, 1.2);
            }
            "menu-ooze" => {
                dock.set_pointer(Some(pos2(nth_x(&dock, 1), icon_y)));
                steps(&mut gl, &mut dock, 0.3);
                dock.press(true);
                dock.release();
                steps(&mut gl, &mut dock, 0.12);
            }
            "stack" => {
                let n = dock.tiles.iter().position(|t| t.kind == crate::tiles::Kind::Folder).unwrap_or(0);
                dock.set_pointer(Some(pos2(nth_x(&dock, n) + 18.0, icon_y)));
                steps(&mut gl, &mut dock, 0.3);
                dock.press(false);
                dock.release();
                // Don't open whatever the stack holds: only show it.
                steps(&mut gl, &mut dock, 0.9);
            }
            scene if scene.starts_with("stack-") => {
                // A made-up Downloads in a scratch folder (pictures linked from the system's own), so nothing of this
                // Mac's own files shows. `stack-<what>[:<seconds>][-light]`.
                let light = scene.ends_with("-light");
                dock.dark = !light;
                let scene = scene.trim_end_matches("-light");
                let (what, at) = match scene.split_once(':') {
                    Some((w, t)) => (w, t.parse::<f32>().map_err(|_| "stack-<what>:<seconds>")?),
                    None => (scene, 1.0),
                };
                // `stack-fan#3` shows three files.
                let (what, count) = match what.split_once('#') {
                    Some((w, n)) => (w, Some(n.parse::<usize>().map_err(|_| "stack-<what>#<count>")?)),
                    None => (what, None),
                };
                let many = matches!(
                    what,
                    "stack-grid" | "stack-grid-open" | "stack-grid-close" | "stack-card" | "stack-scroll"
                );
                let sample = sample_downloads(count.unwrap_or(if many { 34 } else { 5 }))?;
                let i = dock
                    .tiles
                    .iter()
                    .position(|t| t.kind == crate::tiles::Kind::Folder)
                    .ok_or("no folder in the dock")?;
                let folder = dock.tiles[i].path.clone();
                // As on a Mac where the event tap runs: the search can show.
                dock.stack_keys = true;
                let mut listing = crate::stack::list(&sample).unwrap_or_default();
                if what == "stack-empty" {
                    listing.items.clear();
                }
                if what == "stack-progress" {
                    listing.partial = 1;
                }
                let dropping = what == "stack-drop";
                let newest = listing.items.first().cloned();
                if dropping {
                    let mut before = listing.clone();
                    before.items.remove(0);
                    dock.set_listing(folder.clone(), before);
                } else {
                    dock.set_listing(folder.clone(), listing.clone());
                }
                if self.scene.contains("grid") || what == "stack-card" || what == "stack-scroll" {
                    dock.config.stack_view = crate::config::StackView::Grid;
                }
                let feed = |gl: &mut GlWindow, dock: &mut Dock| {
                    for _ in 0..3 {
                        let wants = dock.take_picture_wants();
                        let made: Vec<_> = wants
                            .into_iter()
                            .filter_map(|path| {
                                crate::files::thumbnail(&path, crate::stack::PICTURE_PX)
                                    .map(|(image, thumb)| crate::stack::Picture { path, image, thumb })
                            })
                            .collect();
                        dock.give_pictures(made);
                        render(gl, dock);
                    }
                };
                feed(&mut gl, &mut dock);
                steps(&mut gl, &mut dock, 0.4);
                let tile = dock.icon_rect(i).center();
                match what {
                    "stack-pile" | "stack-progress" | "stack-empty-tile" => steps(&mut gl, &mut dock, at),
                    "stack-drop" => {
                        let _ = newest;
                        dock.set_listing(folder.clone(), listing.clone());
                        feed(&mut gl, &mut dock);
                        steps(&mut gl, &mut dock, at);
                    }
                    _ => {
                        dock.set_pointer(Some(tile));
                        steps(&mut gl, &mut dock, 0.3);
                        dock.press(false);
                        dock.release();
                        let open_for = if matches!(what, "stack-fan-open" | "stack-grid-open") { at } else { 0.9 };
                        let closing = what.ends_with("-close");
                        steps(&mut gl, &mut dock, 0.05);
                        feed(&mut gl, &mut dock);
                        steps(&mut gl, &mut dock, open_for);
                        if closing {
                            // `stack-grid-close:0.08`: that long into closing.
                            dock.close_popup();
                            steps(&mut gl, &mut dock, at);
                        } else if what == "stack-card" || what == "stack-scroll" {
                            let second = dock.stack_cell(1).ok_or("no cell")?.center();
                            dock.set_pointer(Some(second));
                            steps(&mut gl, &mut dock, 0.2);
                            if what == "stack-card" {
                                dock.press(true);
                                dock.release();
                                steps(&mut gl, &mut dock, at.min(0.6));
                            } else {
                                dock.scroll(-at * 100.0, false);
                                steps(&mut gl, &mut dock, 0.5);
                                feed(&mut gl, &mut dock);
                                steps(&mut gl, &mut dock, 0.2);
                            }
                        } else if what == "stack-fan" || what == "stack-grid" {
                            // Rest on the second item, to see its highlight.
                            if let Some(cell) = dock.stack_cell(1) {
                                dock.set_pointer(Some(cell.center()));
                                steps(&mut gl, &mut dock, 0.4);
                            }
                        }
                    }
                }
            }
            "previews" | "previews-ooze" => {
                // Made-up windows, without capturing anything: the panel with its titles and the app's icon.
                dock.list_windows = |pid| {
                    let titles = ["Inbox", "Notes for Friday", "Design review \u{2014} notchbase dock"];
                    let sizes = [vec2(1200.0, 800.0), vec2(700.0, 860.0), vec2(1500.0, 900.0)];
                    titles
                        .iter()
                        .zip(sizes)
                        .enumerate()
                        .map(|(n, (title, size))| crate::windows::Win {
                            id: pid as u32 * 10 + n as u32,
                            title: (*title).into(),
                            frame: egui::Rect::from_min_size(pos2(0.0, 0.0), size),
                            on_screen: n != 1,
                        })
                        .collect()
                };
                let n =
                    dock.tiles.iter().position(|t| t.pid.is_some() && t.kind == crate::tiles::Kind::App).unwrap_or(0);
                dock.set_pointer(Some(pos2(nth_x(&dock, n), icon_y)));
                steps(&mut gl, &mut dock, 0.36);
                let more = if self.scene == "previews" { 0.9 } else { 0.1 };
                steps(&mut gl, &mut dock, more);
            }
            "burst-swell" | "burst-pop" => {
                steps(&mut gl, &mut dock, 0.3);
                let at = dock.icon_rect(3);
                dock.burst(3, at.center(), at.width());
                steps(&mut gl, &mut dock, if self.scene == "burst-swell" { 0.07 } else { 0.17 });
            }
            "cradle" | "landing" => {
                // An app dragged over the gap between the second and third pinned apps, and in the landing, let go.
                let x = (nth_x(&dock, 1) + nth_x(&dock, 2)) * 0.5;
                dock.set_pointer(Some(pos2(x, icon_y)));
                dock.hover_file(PathBuf::from("/System/Applications/Calculator.app"));
                steps(&mut gl, &mut dock, 0.6);
                if self.scene == "landing" {
                    dock.drop_file(PathBuf::from("/System/Applications/Calculator.app"));
                    steps(&mut gl, &mut dock, 0.09);
                }
            }
            // `download-<percent>`, `download-glide`, `download-done`; `pulse-<percent>`.
            scene if ambient => {
                use crate::downloads::{Family, Fetching};
                let scene = scene.trim_end_matches("-light");
                let what = scene.split_once('-').map_or("", |(_, w)| w);
                let recent = [90_001];
                steps(&mut gl, &mut dock, 0.6);
                if let Some(level) = scene.strip_prefix("pulse-") {
                    let level: f32 = level.parse::<f32>().map_err(|_| "pulse-<percent>")? / 100.0;
                    dock.set_beat(Some((90_002, level)));
                    steps(&mut gl, &mut dock, STEP);
                } else {
                    let fraction = match what {
                        "glide" => None,
                        "done" => Some(0.82),
                        n => Some(n.parse::<f32>().map_err(|_| "download-<percent>")? / 100.0),
                    };
                    dock.set_downloads(&[Fetching { family: Family::Firefox, fraction }], &recent);
                    steps(&mut gl, &mut dock, if what == "glide" { 0.9 } else { 1.2 });
                    if what == "done" {
                        dock.set_downloads(&[], &recent);
                        steps(&mut gl, &mut dock, 0.2);
                    }
                }
            }
            "swell" => {
                if let Some(t) = dock.tiles.iter_mut().find(|t| t.pid.is_none() && t.kind == crate::tiles::Kind::App) {
                    t.gulp.velocity = crate::tiles::GULP_KICK;
                    t.launching = Some(Instant::now() - Duration::from_millis(500));
                }
                dock.step(STEP);
                render(&mut gl, &mut dock);
            }
            "arrive" => {
                if let Some(t) = dock.tiles.iter_mut().find(|t| t.pid.is_some() && !t.pinned) {
                    t.presence.snap_to(0.45);
                    t.presence.animate_to(1.0, crate::tiles::ARRIVE);
                }
                dock.step(STEP);
                render(&mut gl, &mut dock);
            }
            "leave" => {
                if let Some(t) = dock.tiles.iter_mut().find(|t| t.kind == crate::tiles::Kind::App) {
                    t.leaving = true;
                    t.presence.snap_to(0.55);
                    t.presence.animate_to(0.0, crate::tiles::LEAVE);
                }
                dock.step(STEP);
                render(&mut gl, &mut dock);
            }
            other => {
                return Err(format!(
                    "unknown scene {other}: rest, showcase, hover, tooltip-ooze, menu, menu-claude, menu-chatgpt, menu-ooze, stack, previews, previews-ooze, swell, arrive, leave, download-<percent|glide|done>, pulse-<percent>, pop:<seconds>"
                ));
            }
        }
        let pixels = target.read(&gl.gl);
        target.delete(&gl.gl);
        let backdrop = match self.scene.as_str() {
            "showcase" => Backdrop::Wallpaper,
            s if ambient && s.ends_with("-light") => Backdrop::Light,
            _ if ambient => Backdrop::Wallpaper,
            _ => Backdrop::Sweep,
        };
        write_png(&self.out, px, &pixels, backdrop)
    }

    fn pop(&self, gl: &mut GlWindow, seconds: f32, ppp: f32) -> Result<(), String> {
        let size = vec2(1280.0, 800.0);
        let px = [(size.x * ppp) as u32, (size.y * ppp) as u32];
        let target = Offscreen::new(&gl.gl, px);
        let liquid = Liquid::new(&gl.gl, gl.shader_version);
        let window = egui::Rect::from_min_size(pos2(260.0, 90.0), vec2(760.0, 520.0));
        let mut pop =
            Pop::new(Reach::Launch, egui::Rect::from_center_size(pos2(560.0, 770.0), vec2(48.0, 48.0)), window, true);
        for _ in 0..(seconds / STEP) as usize {
            pop.step(STEP);
        }
        let (blobs, alpha) = (pop.blobs(), pop.alpha());
        gl.frame(|g, s, _| liquid.draw(g, s, ppp, &blobs, alpha, None, None), |_| {}, Some(target.fbo), Some(px));
        let pixels = target.read(&gl.gl);
        target.delete(&gl.gl);
        write_png(&self.out, px, &pixels, Backdrop::Sweep)
    }
}

#[derive(Clone, Copy)]
enum Backdrop {
    /// Dark on the left to bright on the right, so both edges of the glass show.
    Sweep,
    /// A soft dusk wallpaper, top to bottom, for pictures people see.
    Wallpaper,
    /// A pale daylight wallpaper.
    Light,
}

/// Writes the picture over `backdrop`.
fn write_png(path: &Path, [w, h]: [u32; 2], premultiplied: &[u8], backdrop: Backdrop) -> Result<(), String> {
    let mut rgb = Vec::with_capacity((w * h * 3) as usize);
    for y in 0..h {
        for x in 0..w {
            let i = ((y * w + x) * 4) as usize;
            let back = match backdrop {
                Backdrop::Sweep => {
                    let t = x as f32 / w as f32;
                    [lerp(38.0, 222.0, t), lerp(52.0, 210.0, t), lerp(88.0, 196.0, t)]
                }
                Backdrop::Wallpaper => {
                    let t = y as f32 / h as f32;
                    [lerp(92.0, 196.0, t), lerp(112.0, 150.0, t), lerp(168.0, 132.0, t)]
                }
                Backdrop::Light => {
                    let t = y as f32 / h as f32;
                    [lerp(214.0, 236.0, t), lerp(226.0, 230.0, t), lerp(240.0, 222.0, t)]
                }
            };
            let a = premultiplied[i + 3] as f32 / 255.0;
            for c in 0..3 {
                rgb.push((premultiplied[i + c] as f32 + back[c] * (1.0 - a)).round().clamp(0.0, 255.0) as u8);
            }
        }
    }
    let file = std::fs::File::create(path).map_err(|e| e.to_string())?;
    let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), w, h);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().map_err(|e| e.to_string())?;
    writer.write_image_data(&rgb).map_err(|e| e.to_string())
}

/// A scratch Downloads with `count` things in it, newest first: photos (linked from the system's own desktop
/// pictures, so they get real thumbnails), a disk image, an archive, documents and a folder.
fn sample_downloads(count: usize) -> Result<PathBuf, String> {
    let dir = std::env::temp_dir().join(format!("notchbase-dock-sample-downloads-{count}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let photos: Vec<PathBuf> = std::fs::read_dir("/System/Library/Desktop Pictures/.thumbnails")
        .map(|r| {
            r.filter_map(Result::ok).map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "heic")).collect()
        })
        .unwrap_or_default();
    let names = [
        "notchbase-0.23.9.dmg",
        "Coastline.heic",
        "Quarterly Report \u{2014} Final.pdf",
        "Project Files",
        "Archive.zip",
        "Invoice 2026-11.pdf",
        "Mountains.heic",
        "notes.txt",
        "Presentation.key",
        "song.mp3",
        "Waters Edge.heic",
        "setup.pkg",
        "Screenshot 2026-11-03 at 14.02.11.png",
        "Figma.app",
        "data-export.csv",
        "desktop.ini",
        "IMG_4031.heic",
        "Lecture 07 \u{2014} Signals and Systems.mp4",
        "Thumbs.db",
        "font-pack.zip",
    ];
    // Made oldest first, so their creation times ("date added") put the first name on top.
    for n in (0..count).rev() {
        let base = names[n % names.len()];
        let name = if n < names.len() { base.to_owned() } else { format!("{} {}", n / names.len() + 1, base) };
        let path = dir.join(&name);
        if name.ends_with(".heic") && !photos.is_empty() {
            #[cfg(unix)]
            let _ = std::os::unix::fs::symlink(&photos[n % photos.len()], &path);
            #[cfg(windows)]
            let _ = std::fs::copy(&photos[n % photos.len()], &path);
        } else if name.ends_with("Project Files") || name.ends_with(".app") {
            let _ = std::fs::create_dir_all(&path);
        } else {
            std::fs::write(&path, b"sample").map_err(|e| e.to_string())?;
        }
        // Newest first: each one a minute older than the one before.
        let when = std::time::SystemTime::now() - Duration::from_secs(60 * n as u64);
        if let Ok(f) = std::fs::File::options().write(true).open(&path) {
            let _ = f.set_times(std::fs::FileTimes::new().set_modified(when));
        }
    }
    Ok(dir)
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}
