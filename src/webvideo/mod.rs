//! Video downloads from a dragged tab or link. Each one runs yt-dlp on its own `notchbase-video` thread, and the
//! finished file moves into Downloads under a name nothing else has.

pub(crate) mod browser;
mod codecs;
mod cover;
mod engine;
mod job;
mod progress;
mod site;

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use egui::ColorImage;
use notchbase_sdk::now_ms;

use crate::poll::Wake;

use job::{Failure, Reason, Request, Update};

pub use job::Kind;
pub use site::{Page, Site, recognize, recognize_any};

const THREAD: &str = "notchbase-video";
const STAGING: &str = "staging";
// An ended download stays a moment, so the notch can show how it ended.
const SHOW_SAVED: Duration = Duration::from_millis(2600);
const SHOW_FAILED: Duration = Duration::from_millis(3400);
/// Progress reaches the UI at most this often.
const TELL_EVERY: Duration = Duration::from_millis(80);
const RETRY_AFTER: Duration = Duration::from_secs(2);
const NO_FOLDER: &str = "Couldn't find your Downloads folder";
const NOT_MOVED: &str = "Couldn't save the video";
const OTHER_NAME: &str = "Video";

/// Only one download sets up the engine at a time. The others wait, then find it ready.
static SETTING_UP: Mutex<()> = Mutex::new(());

/// Setting up, looking up the page, downloading, joining picture and sound, then saved or failed.
#[derive(Clone, Debug, PartialEq)]
pub enum Stage {
    Preparing,
    Finding,
    Downloading,
    Joining,
    Saved(PathBuf),
    Failed(String),
}

impl Stage {
    pub fn ended(&self) -> bool {
        matches!(self, Stage::Saved(_) | Stage::Failed(_))
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Download {
    pub id: u64,
    pub page: Page,
    pub kind: Kind,
    pub title: Option<String>,
    pub stage: Stage,
    pub share: f32,
    pub received: u64,
    pub speed: Option<f64>,
    pub eta: Option<f64>,
    pub since_ms: u64,
    pub cover: Option<Arc<ColorImage>>,
}

/// What to download and where to put it, from the settings and the tab it came from.
#[derive(Clone, Debug, PartialEq)]
pub struct Options {
    pub folder: Option<PathBuf>,
    pub kind: Kind,
    pub tallest: u32,
    pub audio: String,
    /// The browser to sign in as, if a site asks for that.
    pub cookies: Option<String>,
}

struct Entry {
    download: Download,
    stop: Arc<AtomicBool>,
    ended: Option<Instant>,
    announced: bool,
}

#[derive(Default)]
struct State {
    entries: Vec<Entry>,
    version: u64,
    next: u64,
}

struct Shared {
    state: Mutex<State>,
    wake: Wake,
}

impl Shared {
    fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn change(&self, id: u64, change: impl FnOnce(&mut Download)) {
        let mut state = self.state();
        let Some(entry) = state.entries.iter_mut().find(|entry| entry.download.id == id) else { return };
        change(&mut entry.download);
        if entry.download.stage.ended() {
            entry.ended.get_or_insert_with(Instant::now);
        }
        state.version += 1;
        drop(state);
        (self.wake)();
    }
}

pub struct Videos {
    shared: Arc<Shared>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Ending {
    Saved { title: String, path: PathBuf },
    Failed(String),
}

impl Videos {
    pub fn new(wake: Wake) -> Self {
        Self { shared: Arc::new(Shared { state: Mutex::default(), wake }) }
    }

    /// Starts a download and returns its id. If that page and kind are already downloading, it's that one's id.
    pub fn start(&self, page: Page, options: Options) -> u64 {
        let mut state = self.shared.state();
        let kind = options.kind;
        let running = |entry: &&Entry| {
            entry.download.page == page && entry.download.kind == kind && !entry.download.stage.ended()
        };
        if let Some(entry) = state.entries.iter().find(running) {
            return entry.download.id;
        }
        state.next += 1;
        let id = state.next;
        let stop = Arc::new(AtomicBool::new(false));
        let download = Download {
            id,
            page: page.clone(),
            kind,
            title: None,
            stage: Stage::Finding,
            share: 0.0,
            received: 0,
            speed: None,
            eta: None,
            since_ms: now_ms(),
            cover: None,
        };
        state.entries.push(Entry { download, stop: stop.clone(), ended: None, announced: false });
        state.version += 1;
        drop(state);
        let shared = self.shared.clone();
        let work = move || {
            let outcome = fetch(&shared, id, &page, &options, &stop);
            let saved = outcome.is_ok();
            shared.change(id, |download| match outcome {
                Ok(path) => (download.stage, download.share) = (Stage::Saved(path), 1.0),
                Err(failure) => download.stage = Stage::Failed(failure.message(page.site)),
            });
            // Sites change under yt-dlp, so after a save check for a newer one if it's been a while.
            if saved && let Some(home) = engine::folder().filter(|home| engine::stale(home)) {
                let _ = engine::refresh(&home);
            }
        };
        if std::thread::Builder::new().name(THREAD.into()).spawn(work).is_err() {
            self.shared.change(id, |download| download.stage = Stage::Failed(NOT_MOVED.into()));
        }
        id
    }

    pub fn cancel(&self, id: u64) {
        let state = self.shared.state();
        if let Some(entry) = state.entries.iter().find(|entry| entry.download.id == id) {
            entry.stop.store(true, Ordering::Relaxed);
        }
    }

    /// Moves whenever a download changes, so the app knows to read `downloads` again.
    pub fn version(&self) -> u64 {
        self.shared.state().version
    }

    pub fn downloads(&self, now: Instant) -> Vec<Download> {
        let mut state = self.shared.state();
        state.entries.retain(|entry| !entry.ended.is_some_and(|at| now >= at + linger(&entry.download.stage)));
        state.entries.iter().map(|entry| entry.download.clone()).collect()
    }

    pub fn next_expiry(&self) -> Option<Instant> {
        let state = self.shared.state();
        state.entries.iter().filter_map(|entry| Some(entry.ended? + linger(&entry.download.stage))).min()
    }

    /// The downloads that ended since the last call, each handed over once.
    pub fn take_endings(&self) -> Vec<Ending> {
        let mut state = self.shared.state();
        let fresh = state.entries.iter_mut().filter(|entry| entry.download.stage.ended() && !entry.announced);
        fresh
            .map(|entry| {
                entry.announced = true;
                match &entry.download.stage {
                    Stage::Saved(path) => Ending::Saved { title: title_of(&entry.download, path), path: path.clone() },
                    Stage::Failed(problem) => Ending::Failed(problem.clone()),
                    _ => unreachable!("only ended downloads are announced"),
                }
            })
            .collect()
    }
}

impl Drop for Videos {
    fn drop(&mut self) {
        for entry in &self.shared.state().entries {
            entry.stop.store(true, Ordering::Relaxed);
        }
    }
}

fn linger(stage: &Stage) -> Duration {
    if matches!(stage, Stage::Failed(_)) { SHOW_FAILED } else { SHOW_SAVED }
}

fn title_of(download: &Download, path: &Path) -> String {
    let named = download.title.clone().filter(|title| !title.trim().is_empty());
    named.or_else(|| path.file_stem().map(|stem| stem.to_string_lossy().into_owned())).unwrap_or(OTHER_NAME.into())
}

fn fetch(shared: &Arc<Shared>, id: u64, page: &Page, options: &Options, stop: &AtomicBool) -> Result<PathBuf, Failure> {
    let unknown = |detail: &str| Failure { reason: Reason::Unknown, detail: detail.to_owned() };
    let home = engine::folder().ok_or_else(|| unknown(NO_FOLDER))?;
    let folder = options.folder.clone().ok_or_else(|| unknown(NO_FOLDER))?;
    let engine = prepare(shared, id, &home, stop)?;
    let staging = home.join(STAGING).join(format!("{}-{id}", std::process::id()));
    let request = Request {
        page: page.clone(),
        staging: staging.clone(),
        kind: options.kind,
        tallest: options.tallest,
        audio: options.audio.clone(),
        cookies: None,
    };
    let fetched = attempts(shared, id, &home, &engine, &request, options, stop);
    let playable = fetched.and_then(|file| match options.kind {
        Kind::Video => {
            shared.change(id, |download| (download.stage, download.share) = (Stage::Joining, 1.0));
            codecs::make_playable(&engine, &file, stop).map_err(|detail| unknown(&detail))
        }
        Kind::Audio => Ok(file),
    });
    let saved = playable.and_then(|file| keep(&file, &folder).map_err(|detail| unknown(&detail)));
    let _ = std::fs::remove_dir_all(&staging);
    saved
}

fn prepare(shared: &Shared, id: u64, home: &Path, stop: &AtomicBool) -> Result<engine::Engine, Failure> {
    if let Some(engine) = engine::ready(home) {
        return Ok(engine);
    }
    shared.change(id, |download| download.stage = Stage::Preparing);
    let _turn = SETTING_UP.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some(engine) = engine::ready(home) {
        shared.change(id, |download| download.stage = Stage::Finding);
        return Ok(engine);
    }
    let told = Mutex::new(Instant::now());
    let report = |share: f32| {
        let mut told = told.lock().unwrap_or_else(PoisonError::into_inner);
        if told.elapsed() >= TELL_EVERY || share >= 1.0 {
            *told = Instant::now();
            shared.change(id, |download| download.share = share);
        }
    };
    let engine = engine::install(home, &report, stop).map_err(|detail| {
        let reason = if stop.load(Ordering::Relaxed) { Reason::Cancelled } else { Reason::Offline };
        Failure { reason, detail }
    })?;
    shared.change(id, |download| (download.stage, download.share) = (Stage::Finding, 0.0));
    Ok(engine)
}

/// Tries again where that could help: signed in as the browser when the site asks, once after a pause when the
/// connection wobbles, and once more with a newer yt-dlp if there is one.
fn attempts(
    shared: &Arc<Shared>,
    id: u64,
    home: &Path,
    engine: &engine::Engine,
    request: &Request,
    options: &Options,
    stop: &AtomicBool,
) -> Result<PathBuf, Failure> {
    let mut request = request.clone();
    let (mut signed_in, mut waited, mut renewed, mut covered) = (false, false, false, false);
    loop {
        let failure = match once(shared, id, engine, &request, stop, &mut covered) {
            Ok(file) => return Ok(file),
            Err(failure) => failure,
        };
        eprintln!("notchbase: video download failed ({:?}): {}", failure.reason, failure.detail);
        let again = match failure.reason {
            Reason::Cancelled | Reason::NoVideo | Reason::Live | Reason::Gone | Reason::Protected => false,
            Reason::SignIn if !signed_in && request.page.site.wants_sign_in() && options.cookies.is_some() => {
                signed_in = true;
                request.cookies.clone_from(&options.cookies);
                true
            }
            Reason::Offline | Reason::Stalled if !waited => {
                waited = true;
                std::thread::sleep(RETRY_AFTER);
                true
            }
            _ if !renewed => {
                renewed = true;
                engine::refresh(home).unwrap_or(false)
            }
            _ => false,
        };
        if !again || stop.load(Ordering::Relaxed) {
            return Err(failure);
        }
        let _ = std::fs::remove_dir_all(&request.staging);
        shared.change(id, |download| (download.stage, download.share) = (Stage::Finding, 0.0));
    }
}

fn once(
    shared: &Arc<Shared>,
    id: u64,
    engine: &engine::Engine,
    request: &Request,
    stop: &AtomicBool,
    covered: &mut bool,
) -> Result<PathBuf, Failure> {
    let mut told = Instant::now() - TELL_EVERY;
    job::run(engine, request, stop, &mut |update| match update {
        Update::Found(meta) => {
            if let Some(address) = meta.thumbnail.filter(|address| !*covered && address.starts_with("https://")) {
                *covered = true;
                cover::fetch_for(shared.clone(), id, address);
            }
            shared.change(id, |download| {
                download.title = Some(meta.title).filter(|title| !title.is_empty());
                download.stage = Stage::Downloading;
            });
        }
        Update::Moving { share, received, speed, eta } => {
            if told.elapsed() < TELL_EVERY && share < 1.0 {
                return;
            }
            told = Instant::now();
            shared.change(id, |download| {
                download.stage = Stage::Downloading;
                download.share = download.share.max(share);
                (download.received, download.speed, download.eta) = (received, speed, eta);
            });
        }
        Update::Joining => shared.change(id, |download| (download.stage, download.share) = (Stage::Joining, 1.0)),
    })
}

/// Moves the file into the folder under a name nothing there has, copying when a rename can't move it.
fn keep(file: &Path, folder: &Path) -> Result<PathBuf, String> {
    std::fs::create_dir_all(folder).map_err(|_| NO_FOLDER)?;
    let name = file.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
    let (stem, extension) = match name.rsplit_once('.') {
        Some((stem, extension)) if !stem.is_empty() => (stem.to_owned(), format!(".{extension}")),
        _ => (if name.is_empty() { OTHER_NAME.to_owned() } else { name.clone() }, String::new()),
    };
    let candidates =
        std::iter::once(format!("{stem}{extension}")).chain((1..).map(|n| format!("{stem} ({n}){extension}")));
    for candidate in candidates.take(10_000) {
        let target = folder.join(candidate);
        if target.exists() {
            continue;
        }
        if std::fs::rename(file, &target).is_ok() {
            return Ok(target);
        }
        std::fs::copy(file, &target).map_err(|_| NOT_MOVED)?;
        let _ = std::fs::remove_file(file);
        return Ok(target);
    }
    Err(NOT_MOVED.into())
}

/// Keeps a console window from flashing up for each program run on Windows.
fn quiet(command: &mut Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt as _;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    #[cfg(not(windows))]
    let _ = command;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder(name: &str) -> PathBuf {
        let folder = std::env::temp_dir().join(format!("notchbase-videos-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(&folder).unwrap();
        folder
    }

    fn videos() -> Videos {
        Videos::new(Arc::new(|| {}))
    }

    fn entry(videos: &Videos, stage: Stage, title: Option<&str>) -> u64 {
        let mut state = videos.shared.state();
        state.next += 1;
        let id = state.next;
        let page = recognize("https://youtu.be/jNQXAC9IVRw").unwrap();
        let download = Download {
            id,
            page,
            kind: Kind::Video,
            title: title.map(str::to_owned),
            stage,
            share: 0.0,
            received: 0,
            speed: None,
            eta: None,
            since_ms: 0,
            cover: None,
        };
        state.entries.push(Entry { download, stop: Arc::default(), ended: None, announced: false });
        id
    }

    #[test]
    fn finished_video_gets_unique_name() {
        let (staging, downloads) = (folder("staging"), folder("downloads"));
        std::fs::write(downloads.join("Me at the zoo.mp4"), b"old").unwrap();
        let file = staging.join("Me at the zoo.mp4");
        std::fs::write(&file, b"new").unwrap();
        let kept = keep(&file, &downloads).unwrap();
        assert_eq!(kept, downloads.join("Me at the zoo (1).mp4"));
        assert_eq!(std::fs::read(&kept).unwrap(), b"new");
        assert_eq!(std::fs::read(downloads.join("Me at the zoo.mp4")).unwrap(), b"old");
        assert!(!file.exists(), "nothing stays behind in staging");
        for folder in [staging, downloads] {
            std::fs::remove_dir_all(folder).unwrap();
        }
    }

    #[test]
    fn version_moves_and_endings_come_once() {
        let videos = videos();
        let id = entry(&videos, Stage::Downloading, Some("Me at the zoo"));
        let before = videos.version();
        videos.shared.change(id, |download| download.share = 0.5);
        assert!(videos.version() > before);
        assert!(videos.take_endings().is_empty());
        videos.shared.change(id, |download| download.stage = Stage::Saved("/d/zoo.mp4".into()));
        let saved = Ending::Saved { title: "Me at the zoo".into(), path: "/d/zoo.mp4".into() };
        assert_eq!(videos.take_endings(), [saved]);
        assert!(videos.take_endings().is_empty());
    }

    #[test]
    fn ended_download_lingers_then_leaves() {
        let videos = videos();
        let id = entry(&videos, Stage::Downloading, None);
        assert_eq!(videos.next_expiry(), None);
        videos.shared.change(id, |download| download.stage = Stage::Failed("Can't reach YouTube".into()));
        let leaves = videos.next_expiry().expect("an expiry");
        assert_eq!(videos.downloads(leaves - Duration::from_millis(50)).len(), 1);
        assert!(videos.downloads(leaves).is_empty());
        assert_eq!(videos.next_expiry(), None);
    }

    #[test]
    fn untitled_video_named_after_file() {
        let videos = videos();
        let id = entry(&videos, Stage::Downloading, Some("  "));
        videos.shared.change(id, |download| download.stage = Stage::Saved("/d/Clip by someone.mp4".into()));
        let named = Ending::Saved { title: "Clip by someone".into(), path: "/d/Clip by someone.mp4".into() };
        assert_eq!(videos.take_endings(), [named]);
    }

    #[test]
    fn cancel_raises_the_threads_stop_flag() {
        let videos = videos();
        let id = entry(&videos, Stage::Downloading, None);
        videos.cancel(id);
        assert!(videos.shared.state().entries[0].stop.load(Ordering::Relaxed));
        videos.cancel(id + 1);
    }

    #[test]
    #[ignore = "installs the engine if needed (set NOTCHBASE_DATA_DIR) and downloads a 19 second video from YouTube"]
    fn real_video_downloads_to_folder() {
        let downloads = folder("real");
        let address = std::env::var("NOTCHBASE_VIDEO_URL");
        let page = recognize_any(address.as_deref().unwrap_or("https://www.youtube.com/watch?v=jNQXAC9IVRw")).unwrap();
        let kind = match std::env::var("NOTCHBASE_VIDEO_KIND").as_deref() {
            Ok("audio") => Kind::Audio,
            _ => Kind::Video,
        };
        let cookies = std::env::var("NOTCHBASE_VIDEO_COOKIES").ok();
        let options = Options { folder: Some(downloads.clone()), kind, tallest: 4320, audio: "m4a".into(), cookies };
        let videos = videos();
        let id = videos.start(page.clone(), options.clone());
        assert_eq!(videos.start(page, options), id, "the same page is not fetched twice at once");
        let mut last = String::new();
        let ending = loop {
            std::thread::sleep(Duration::from_millis(200));
            if let Some(ending) = videos.take_endings().pop() {
                break ending;
            }
            let download = &videos.downloads(Instant::now())[0];
            let now =
                format!("{:?} {:.0}% {:?}", download.stage, (download.share * 10.0).floor() * 10.0, download.title);
            if now != last {
                eprintln!("{now}");
                last = now;
            }
        };
        let Ending::Saved { title, path } = ending else { panic!("{ending:?}") };
        let size = std::fs::metadata(&path).unwrap().len();
        eprintln!("saved {title:?} as {:?}, {size} bytes", path.file_name().unwrap());
        assert!(path.starts_with(&downloads) && size > 50_000);
        let wanted = if kind == Kind::Audio { "m4a" } else { "mp4" };
        assert!(path.extension().is_some_and(|extension| extension == wanted), "{path:?}");
        if address.is_err() {
            assert_eq!(title, "Me at the zoo");
        }
        if std::env::var_os("NOTCHBASE_VIDEO_KEEP").is_none() {
            std::fs::remove_dir_all(&downloads).unwrap();
        }
    }
}
