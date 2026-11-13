//! Folders shown as stacks, Downloads above all: what's in them, kept current the moment a file comes or goes, and
//! the pictures of what's in them, made off the main thread so opening a stack never waits on the disk.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, SystemTime};

use crate::config::{StackSort, StackView};

/// "Automatic" fans out up to this many items, as the Dock does, and shows a grid past it.
pub const FAN_MOST: usize = 12;
/// The most items a stack lists; a folder with more shows its newest this many.
pub const MOST_LISTED: usize = 20_000;
/// Files no one means to see, whatever the system says about them: Windows' and macOS's own folder clutter. Any
/// name starting with a dot (`.DS_Store`, `._` AppleDouble files, `.Trashes`) is left out as well.
const JUNK: [&str; 6] = ["desktop.ini", "thumbs.db", "ehthumbs.db", "ehthumbs_vista.db", "icon\r", "$recycle.bin"];
/// The file flag Finder hides things by.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
const UF_HIDDEN: u32 = 0x8000;
/// Downloads still on their way, by the endings browsers give them until they're done.
const PARTIAL: [&str; 5] = ["download", "crdownload", "part", "partial", "opdownload"];
/// Edge sizes of the pictures made for stacks, in pixels.
pub const PICTURE_PX: u32 = 160;
/// With nothing happening, the folder is looked at again this often anyway, in case a change went unheard.
const RESCAN: Duration = Duration::from_secs(10);
/// A burst of changes (a browser renaming its partial file) is read once, this long after it starts.
const SETTLE: Duration = Duration::from_millis(60);

/// One thing in a stack.
#[derive(Clone, Debug, PartialEq)]
pub struct Item {
    pub path: PathBuf,
    pub name: String,
    /// When it came into the folder, as near as the file system says (its creation time).
    pub added: SystemTime,
    pub modified: SystemTime,
    /// Bytes, or 0 for a folder.
    pub size: u64,
    pub folder: bool,
}

/// What a folder holds: its finished items, and how many downloads are still arriving.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Listing {
    pub items: Vec<Item>,
    pub partial: usize,
}

/// How an open stack is laid out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shape {
    /// An arc of names and icons rising from the tile.
    Fan,
    /// A glass panel of icons, scrolling when there are many.
    Grid,
}

/// Whether a file is a download still in progress.
pub fn is_partial(name: &str) -> bool {
    Path::new(name)
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| PARTIAL.iter().any(|p| e.eq_ignore_ascii_case(p)))
}

/// Whether a name is one Finder never shows: dot files, and Windows' and macOS's own folder clutter.
pub fn is_junk(name: &str) -> bool {
    name.starts_with('.') || JUNK.iter().any(|j| name.eq_ignore_ascii_case(j))
}

/// Reads a folder as Finder shows it: hidden things left out (dot files, folder clutter such as `desktop.ini`,
/// anything flagged hidden or named in the folder's `.hidden`), downloads in progress counted rather than listed.
/// None when it can't be read at all (gone, or not allowed).
pub fn list(folder: &Path) -> Option<Listing> {
    let read = std::fs::read_dir(folder).ok()?;
    let named_hidden: HashSet<String> = std::fs::read_to_string(folder.join(".hidden"))
        .map(|text| text.lines().map(|l| l.trim().to_lowercase()).filter(|l| !l.is_empty()).collect())
        .unwrap_or_default();
    let mut listing = Listing::default();
    for entry in read.filter_map(Result::ok) {
        let name = entry.file_name().to_string_lossy().into_owned();
        if is_junk(&name) || named_hidden.contains(&name.to_lowercase()) {
            continue;
        }
        // The entry's own, not a link's target's: its flags, and when it came into this folder.
        let Ok(meta) = entry.metadata() else { continue };
        if flagged_hidden(&meta) {
            continue;
        }
        if is_partial(&name) {
            listing.partial += 1;
            continue;
        }
        let modified = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);
        let added = meta.created().unwrap_or(modified);
        // A link reads as what it points to: a folder, or that file's size.
        let target = if meta.file_type().is_symlink() { std::fs::metadata(entry.path()).ok() } else { None };
        let folder = target.as_ref().unwrap_or(&meta).is_dir();
        let size = if folder { 0 } else { target.as_ref().unwrap_or(&meta).len() };
        listing.items.push(Item { path: entry.path(), name, added, modified, size, folder });
    }
    // The newest first, so a folder past the limit keeps what matters.
    sort(&mut listing.items, StackSort::Added);
    listing.items.truncate(MOST_LISTED);
    Some(listing)
}

/// Whether the file system marks a file hidden: Finder's hidden flag, or Windows' hidden and system attributes.
#[cfg(target_os = "macos")]
fn flagged_hidden(meta: &std::fs::Metadata) -> bool {
    use std::os::macos::fs::MetadataExt as _;
    meta.st_flags() & UF_HIDDEN != 0
}

#[cfg(windows)]
fn flagged_hidden(meta: &std::fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt as _;
    const HIDDEN: u32 = 0x2;
    const SYSTEM: u32 = 0x4;
    meta.file_attributes() & (HIDDEN | SYSTEM) != 0
}

/// Puts a stack's items in the order it shows them, the top of the pile first.
pub fn sort(items: &mut [Item], by: StackSort) {
    match by {
        StackSort::Added => items.sort_by(|a, b| b.added.cmp(&a.added).then_with(|| a.name.cmp(&b.name))),
        StackSort::Modified => items.sort_by(|a, b| b.modified.cmp(&a.modified).then_with(|| a.name.cmp(&b.name))),
        StackSort::Name => items.sort_by_cached_key(|i| (i.name.to_lowercase(), i.name.clone())),
    }
}

/// The layout for `count` items: "automatic" fans out a few and shows a grid for more, or for none.
pub fn shape(view: StackView, count: usize) -> Shape {
    match view {
        // An empty folder fans out too: a quiet "nothing here" under "Open in Finder".
        StackView::Fan => Shape::Fan,
        StackView::Grid => Shape::Grid,
        StackView::Automatic if count <= FAN_MOST => Shape::Fan,
        StackView::Automatic => Shape::Grid,
    }
}

/// Paths in `after` that weren't in `before`: files that just arrived.
pub fn arrivals(before: &Listing, after: &Listing) -> Vec<PathBuf> {
    let had: HashSet<&Path> = before.items.iter().map(|i| i.path.as_path()).collect();
    after.items.iter().filter(|i| !had.contains(i.path.as_path())).map(|i| i.path.clone()).collect()
}

/// How far right the fan's `k`th row (from the bottom) leans, in points: the gentle arc of the Dock's fan.
pub fn fan_bend(k: usize) -> f32 {
    let k = k as f32;
    0.45 * k * k
}

/// Columns in a grid of `count` items: square-ish, three to seven across.
pub fn grid_columns(count: usize) -> usize {
    ((count as f32).sqrt().ceil() as usize).clamp(3, 7)
}

/// A key pressed while a stack is open. The dock never takes the keyboard from the app in front, so these come
/// from its event tap (`titlebar.rs`), on the tap's thread, and only while a stack is open.
#[cfg(target_os = "macos")]
fn picture_of(path: &Path) -> Option<(egui::ColorImage, bool)> {
    objc2::rc::autoreleasepool(|_| crate::files::thumbnail(path, PICTURE_PX))
}

#[cfg(not(target_os = "macos"))]
fn picture_of(path: &Path) -> Option<(egui::ColorImage, bool)> {
    crate::files::thumbnail(path, PICTURE_PX)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Key {
    /// Letters typed: the grid's filter, or the fan's type-to-select.
    Text(String),
    Back,
    Up,
    Down,
    Left,
    Right,
    /// Return: open what's picked.
    Open,
    /// Space: Quick Look.
    Look,
    /// Cmd+Delete.
    Trash,
    /// Cmd+A.
    All,
    Esc,
    /// Any other Cmd shortcut (Cmd+Tab, say): it goes on to the app, and the stack closes.
    Away,
}

static CATCH_KEYS: AtomicBool = AtomicBool::new(false);
static KEYS: Mutex<Vec<Key>> = Mutex::new(Vec::new());
type Wake = Box<dyn Fn() + Send + Sync>;
static KEYS_WAKE: OnceLock<Wake> = OnceLock::new();

const COMMAND: u64 = 0x10_0000;
const CONTROL: u64 = 0x4_0000;
const OPTION: u64 = 0x8_0000;

/// Catches keys for an open stack, or stops.
pub fn catch_keys(on: bool) {
    CATCH_KEYS.store(on, Ordering::Relaxed);
}

pub fn catching_keys() -> bool {
    CATCH_KEYS.load(Ordering::Relaxed)
}

pub fn on_keys_wake(wake: impl Fn() + Send + Sync + 'static) {
    let _ = KEYS_WAKE.set(Box::new(wake));
}

/// A key already turned into what it means (Windows reads its own keyboard hook), for an open stack.
#[cfg(windows)]
pub fn push_key(key: Key) {
    if let Ok(mut keys) = KEYS.lock() {
        keys.push(key);
    }
    if let Some(wake) = KEYS_WAKE.get() {
        wake();
    }
}

pub fn take_keys() -> Vec<Key> {
    KEYS.lock().map(|mut k| std::mem::take(&mut *k)).unwrap_or_default()
}

/// A key went down (on the tap's thread) with `text` typed by it. True when the open stack takes it, to keep it
/// from the app in front.
pub fn on_key(keycode: i64, flags: u64, text: &str) -> bool {
    if !catching_keys() {
        return false;
    }
    let Some((key, swallow)) = key_of(keycode, flags, text) else { return false };
    if let Ok(mut keys) = KEYS.lock() {
        keys.push(key);
    }
    if let Some(wake) = KEYS_WAKE.get() {
        wake();
    }
    swallow
}

/// What a key does to an open stack, and whether it's kept from the app in front.
fn key_of(keycode: i64, flags: u64, text: &str) -> Option<(Key, bool)> {
    if flags & (CONTROL | OPTION) != 0 {
        return None;
    }
    if flags & COMMAND != 0 {
        return Some(match keycode {
            51 | 117 => (Key::Trash, true),
            0 => (Key::All, true),
            _ => (Key::Away, false),
        });
    }
    let key = match keycode {
        53 => Key::Esc,
        36 | 76 => Key::Open,
        49 => Key::Look,
        51 | 117 => Key::Back,
        123 => Key::Left,
        124 => Key::Right,
        125 => Key::Down,
        126 => Key::Up,
        48 => return None,
        // Function keys "type" characters of Apple's private range: nothing to search for.
        _ if !text.is_empty() && text.chars().all(|c| !c.is_control() && !('\u{f700}'..='\u{f8ff}').contains(&c)) => {
            Key::Text(text.to_owned())
        }
        _ => return None,
    };
    Some((key, true))
}

/// Watches one folder on a thread of its own, reading it again the moment the folder changes (a kernel event on
/// the folder itself, so nothing polls while nothing happens). The newest reading waits in `latest`.
pub struct Watch {
    stop: Arc<AtomicBool>,
    pub latest: Arc<Mutex<Option<Listing>>>,
}

impl Watch {
    pub fn start(folder: PathBuf, wake: impl Fn() + Send + 'static) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let latest = Arc::new(Mutex::new(None));
        let (quit, slot) = (Arc::clone(&stop), Arc::clone(&latest));
        let _ = std::thread::Builder::new().name("stack watch".into()).spawn(move || {
            let mut last: Option<Listing> = None;
            let mut changes = Changes::open(&folder);
            while !quit.load(Ordering::Relaxed) {
                let now = list(&folder).unwrap_or_default();
                if last.as_ref() != Some(&now) {
                    last = Some(now.clone());
                    if let Ok(mut s) = slot.lock() {
                        *s = Some(now);
                    }
                    wake();
                }
                // Wait for the folder to change, waking every second to see whether the watch is still wanted.
                let mut waited = Duration::ZERO;
                loop {
                    if quit.load(Ordering::Relaxed) {
                        return;
                    }
                    let step = Duration::from_secs(1);
                    match changes.as_mut().map(|c| c.wait(step)) {
                        Some(Changed::Yes) => break,
                        Some(Changed::Replaced) | None => {
                            // The folder itself moved or was never there: look again and listen anew.
                            std::thread::sleep(step);
                            changes = Changes::open(&folder);
                            break;
                        }
                        Some(Changed::No) => {}
                    }
                    waited += step;
                    if waited >= RESCAN {
                        break;
                    }
                }
                std::thread::sleep(SETTLE);
            }
        });
        Self { stop, latest }
    }

    pub fn take(&self) -> Option<Listing> {
        self.latest.lock().ok()?.take()
    }
}

impl Drop for Watch {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

enum Changed {
    Yes,
    No,
    /// The folder was deleted, renamed or unmounted.
    Replaced,
}

/// A kernel queue listening to one folder's own changes: a file added, removed or renamed inside it.
#[cfg(target_os = "macos")]
struct Changes {
    queue: i32,
    folder: i32,
}

#[cfg(target_os = "macos")]
impl Changes {
    fn open(folder: &Path) -> Option<Self> {
        use std::os::unix::ffi::OsStrExt as _;
        let path = std::ffi::CString::new(folder.as_os_str().as_bytes()).ok()?;
        let fd = unsafe { libc::open(path.as_ptr(), libc::O_EVTONLY) };
        if fd < 0 {
            return None;
        }
        let queue = unsafe { libc::kqueue() };
        if queue < 0 {
            unsafe { libc::close(fd) };
            return None;
        }
        let watch = libc::kevent {
            ident: fd as usize,
            filter: libc::EVFILT_VNODE,
            flags: libc::EV_ADD | libc::EV_CLEAR,
            fflags: libc::NOTE_WRITE | libc::NOTE_DELETE | libc::NOTE_RENAME | libc::NOTE_REVOKE | libc::NOTE_ATTRIB,
            data: 0,
            udata: std::ptr::null_mut(),
        };
        let added = unsafe { libc::kevent(queue, &watch, 1, std::ptr::null_mut(), 0, std::ptr::null()) };
        if added < 0 {
            unsafe {
                libc::close(queue);
                libc::close(fd);
            }
            return None;
        }
        Some(Self { queue, folder: fd })
    }

    fn wait(&mut self, at_most: Duration) -> Changed {
        let timeout =
            libc::timespec { tv_sec: at_most.as_secs() as libc::time_t, tv_nsec: at_most.subsec_nanos() as _ };
        let mut event = libc::kevent { ident: 0, filter: 0, flags: 0, fflags: 0, data: 0, udata: std::ptr::null_mut() };
        let n = unsafe { libc::kevent(self.queue, std::ptr::null(), 0, &mut event, 1, &timeout) };
        match n {
            1 if event.fflags & (libc::NOTE_DELETE | libc::NOTE_RENAME | libc::NOTE_REVOKE) != 0 => Changed::Replaced,
            1 => Changed::Yes,
            0 => Changed::No,
            // Interrupted: treat it as a change, which only means one extra look.
            _ => Changed::Yes,
        }
    }
}

#[cfg(target_os = "macos")]
impl Drop for Changes {
    fn drop(&mut self) {
        unsafe {
            libc::close(self.queue);
            libc::close(self.folder);
        }
    }
}

/// A change notification on one folder: a file added, removed or renamed inside it.
#[cfg(windows)]
struct Changes {
    handle: windows_sys::Win32::Foundation::HANDLE,
    folder: PathBuf,
}

// The notification handle is only waited on from the watch's own thread.
#[cfg(windows)]
unsafe impl Send for Changes {}

#[cfg(windows)]
impl Changes {
    fn open(folder: &Path) -> Option<Self> {
        use std::os::windows::ffi::OsStrExt as _;
        use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
        use windows_sys::Win32::Storage::FileSystem::{
            FILE_NOTIFY_CHANGE_ATTRIBUTES, FILE_NOTIFY_CHANGE_DIR_NAME, FILE_NOTIFY_CHANGE_FILE_NAME,
            FILE_NOTIFY_CHANGE_SIZE, FindFirstChangeNotificationW,
        };
        let wide: Vec<u16> = folder.as_os_str().encode_wide().chain([0]).collect();
        let what = FILE_NOTIFY_CHANGE_FILE_NAME
            | FILE_NOTIFY_CHANGE_DIR_NAME
            | FILE_NOTIFY_CHANGE_ATTRIBUTES
            | FILE_NOTIFY_CHANGE_SIZE;
        let handle = unsafe { FindFirstChangeNotificationW(wide.as_ptr(), 0, what) };
        (handle != INVALID_HANDLE_VALUE && !handle.is_null()).then(|| Self { handle, folder: folder.to_path_buf() })
    }

    fn wait(&mut self, at_most: Duration) -> Changed {
        use windows_sys::Win32::Foundation::{WAIT_OBJECT_0, WAIT_TIMEOUT};
        use windows_sys::Win32::Storage::FileSystem::FindNextChangeNotification;
        use windows_sys::Win32::System::Threading::WaitForSingleObject;
        let ms = at_most.as_millis().min(u128::from(u32::MAX - 1)) as u32;
        match unsafe { WaitForSingleObject(self.handle, ms) } {
            WAIT_OBJECT_0 => {
                if unsafe { FindNextChangeNotification(self.handle) } == 0 || !self.folder.is_dir() {
                    Changed::Replaced
                } else {
                    Changed::Yes
                }
            }
            WAIT_TIMEOUT => Changed::No,
            _ => Changed::Replaced,
        }
    }
}

#[cfg(windows)]
impl Drop for Changes {
    fn drop(&mut self) {
        unsafe { windows_sys::Win32::Storage::FileSystem::FindCloseChangeNotification(self.handle) };
    }
}

/// A picture of a file for a stack: a Quick Look thumbnail where there is one (photos, PDFs, videos), else its
/// Finder icon.
pub struct Picture {
    pub path: PathBuf,
    pub image: egui::ColorImage,
    /// A thumbnail of the contents, framed like a print, rather than an icon.
    pub thumb: bool,
}

/// Makes pictures on a thread of its own, the most recently asked for first, so whatever is on screen now comes
/// in before what was scrolled past.
pub struct Pictures {
    ask: Sender<Ask>,
    pub done: Arc<Mutex<Vec<Picture>>>,
}

enum Ask {
    One(PathBuf),
    /// Only these are wanted any more: the rest still waiting are dropped.
    Keep(HashSet<PathBuf>),
}

impl Pictures {
    pub fn start(wake: impl Fn() + Send + 'static) -> Self {
        let (ask, asked) = channel::<Ask>();
        let done: Arc<Mutex<Vec<Picture>>> = Arc::default();
        let out = Arc::clone(&done);
        let _ =
            std::thread::Builder::new().name("stack pictures".into()).spawn(move || make_pictures(&asked, &out, &wake));
        Self { ask, done }
    }

    pub fn ask(&self, path: PathBuf) {
        let _ = self.ask.send(Ask::One(path));
    }

    /// Drops whatever's still waiting that isn't in `keep` (it was scrolled past).
    pub fn keep(&self, keep: HashSet<PathBuf>) {
        let _ = self.ask.send(Ask::Keep(keep));
    }

    pub fn take(&self) -> Vec<Picture> {
        self.done.lock().map(|mut d| std::mem::take(&mut *d)).unwrap_or_default()
    }
}

fn make_pictures(asked: &Receiver<Ask>, out: &Mutex<Vec<Picture>>, wake: &dyn Fn()) {
    let mut queue: Vec<PathBuf> = Vec::new();
    let take = |ask: Ask, queue: &mut Vec<PathBuf>| match ask {
        Ask::One(path) => queue.push(path),
        Ask::Keep(keep) => queue.retain(|p| keep.contains(p)),
    };
    loop {
        if queue.is_empty() {
            match asked.recv() {
                Ok(ask) => take(ask, &mut queue),
                Err(_) => return,
            }
        }
        for ask in asked.try_iter() {
            take(ask, &mut queue);
        }
        let Some(path) = queue.pop() else { continue };
        queue.retain(|p| p != &path);
        if let Some((image, thumb)) = picture_of(&path)
            && let Ok(mut done) = out.lock()
        {
            done.push(Picture { path, image, thumb });
            drop(done);
            wake();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_reach_an_open_stack_and_other_shortcuts_pass_through() {
        assert_eq!(key_of(0, 0, "a"), Some((Key::Text("a".into()), true)));
        assert_eq!(key_of(36, 0, "\r"), Some((Key::Open, true)));
        assert_eq!(key_of(49, 0, " "), Some((Key::Look, true)));
        assert_eq!(key_of(53, 0, "\u{1b}"), Some((Key::Esc, true)));
        assert_eq!(key_of(125, 0, ""), Some((Key::Down, true)));
        assert_eq!(key_of(51, 0, "\u{8}"), Some((Key::Back, true)));
        assert_eq!(key_of(51, COMMAND, ""), Some((Key::Trash, true)));
        assert_eq!(key_of(0, COMMAND, "a"), Some((Key::All, true)));
        assert_eq!(key_of(48, COMMAND, ""), Some((Key::Away, false)), "Cmd+Tab goes on, and the stack closes");
        assert_eq!(key_of(48, 0, "\t"), None, "a plain Tab is left alone");
        assert_eq!(key_of(0, CONTROL, "a"), None);
        assert_eq!(key_of(0, OPTION, "å"), None);
        assert_eq!(key_of(122, 0, "\u{f704}"), None, "F1 and the like type nothing");
    }

    fn item(name: &str, added: u64, modified: u64) -> Item {
        let at = |s| SystemTime::UNIX_EPOCH + Duration::from_secs(s);
        Item {
            path: PathBuf::from(format!("/d/{name}")),
            name: name.into(),
            added: at(added),
            modified: at(modified),
            size: 0,
            folder: false,
        }
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn junk_and_hidden_things_are_left_out_so_a_folder_of_them_is_empty() {
        use std::os::unix::ffi::OsStrExt as _;
        let dir = std::env::temp_dir().join(format!("notchbase-dock-junk-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("$RECYCLE.BIN")).unwrap();
        std::fs::create_dir_all(dir.join(".Trashes")).unwrap();
        for name in [
            "desktop.ini",
            "Thumbs.DB",
            "ehthumbs.db",
            "._photo.jpg",
            ".localized",
            "Icon\r",
            "flagged.txt",
            "Notes.md",
        ] {
            std::fs::write(dir.join(name), b"x").unwrap();
        }
        let flagged = std::ffi::CString::new(dir.join("flagged.txt").as_os_str().as_bytes()).unwrap();
        assert_eq!(unsafe { libc::chflags(flagged.as_ptr(), UF_HIDDEN) }, 0);
        std::fs::write(dir.join(".hidden"), "notes.md\n").unwrap();
        let listing = list(&dir).unwrap();
        assert!(listing.items.is_empty(), "nothing anyone means to see: {:?}", listing.items);
        std::fs::write(dir.join("Report.pdf"), b"12345").unwrap();
        let listing = list(&dir).unwrap();
        assert_eq!(listing.items.iter().map(|i| i.name.as_str()).collect::<Vec<_>>(), ["Report.pdf"]);
        assert_eq!((listing.items[0].size, listing.items[0].folder), (5, false));
        for name in ["DESKTOP.INI", "$Recycle.Bin", ".fseventsd", ".Spotlight-V100", "._x"] {
            assert!(is_junk(name), "{name}");
        }
        assert!(!is_junk("desktop.ini.zip") && !is_junk("Thumbs.db.png"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn downloads_still_arriving_are_counted_not_listed() {
        for name in ["movie.mp4.crdownload", "a.zip.download", "b.iso.part", "C.PDF.PART"] {
            assert!(is_partial(name), "{name}");
        }
        for name in ["download.zip", "notes.txt", "partial", "Report.pdf"] {
            assert!(!is_partial(name), "{name}");
        }
        let dir = std::env::temp_dir().join(format!("notchbase-dock-stack-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for name in [".DS_Store", ".hidden", "photo.jpg", "setup.dmg", "big.iso.crdownload"] {
            std::fs::write(dir.join(name), b"x").unwrap();
        }
        let listing = list(&dir).unwrap();
        let mut names: Vec<_> = listing.items.iter().map(|i| i.name.as_str()).collect();
        names.sort_unstable();
        assert_eq!(names, ["photo.jpg", "setup.dmg"], "hidden files stay out");
        assert_eq!(listing.partial, 1);
        let _ = std::fs::remove_dir_all(&dir);
        assert!(list(&dir).is_none(), "a folder that's gone reads as nothing");
    }

    #[test]
    fn sorting_puts_the_top_of_the_pile_first() {
        let mut items = vec![item("b.txt", 10, 50), item("A.txt", 30, 20), item("c.txt", 20, 40)];
        sort(&mut items, StackSort::Added);
        assert_eq!(items.iter().map(|i| i.name.as_str()).collect::<Vec<_>>(), ["A.txt", "c.txt", "b.txt"]);
        sort(&mut items, StackSort::Modified);
        assert_eq!(items.iter().map(|i| i.name.as_str()).collect::<Vec<_>>(), ["b.txt", "c.txt", "A.txt"]);
        sort(&mut items, StackSort::Name);
        assert_eq!(items.iter().map(|i| i.name.as_str()).collect::<Vec<_>>(), ["A.txt", "b.txt", "c.txt"]);
    }

    #[test]
    fn automatic_fans_a_few_or_none_and_grids_many() {
        assert_eq!(shape(StackView::Automatic, 1), Shape::Fan);
        assert_eq!(shape(StackView::Automatic, FAN_MOST), Shape::Fan);
        assert_eq!(shape(StackView::Automatic, FAN_MOST + 1), Shape::Grid);
        assert_eq!(shape(StackView::Grid, 2), Shape::Grid);
        assert_eq!(shape(StackView::Fan, 40), Shape::Fan);
        assert_eq!(shape(StackView::Fan, 0), Shape::Fan, "an empty folder fans out its \"nothing here\"");
        assert_eq!(shape(StackView::Automatic, 0), Shape::Fan);
    }

    #[test]
    fn arrivals_are_the_new_paths_only() {
        let before = Listing { items: vec![item("a", 1, 1), item("b", 2, 2)], partial: 1 };
        let after = Listing { items: vec![item("c", 3, 3), item("a", 1, 1)], partial: 0 };
        assert_eq!(arrivals(&before, &after), [PathBuf::from("/d/c")]);
        assert!(arrivals(&after, &after).is_empty());
    }

    #[test]
    fn the_fan_arcs_and_grids_stay_squarish() {
        assert_eq!(fan_bend(0), 0.0);
        assert!(fan_bend(3) > fan_bend(2) && fan_bend(2) - fan_bend(1) > fan_bend(1) - fan_bend(0), "it curves");
        assert_eq!(grid_columns(0), 3);
        assert_eq!(grid_columns(9), 3);
        assert_eq!(grid_columns(20), 5);
        assert_eq!(grid_columns(400), 7);
    }

    #[test]
    fn a_watch_hears_a_file_arrive_at_once() {
        let dir = std::env::temp_dir().join(format!("notchbase-dock-watch-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let (tx, rx) = channel();
        let watch = Watch::start(dir.clone(), move || {
            let _ = tx.send(());
        });
        rx.recv_timeout(Duration::from_secs(5)).expect("the first reading");
        assert_eq!(watch.take().map(|l| l.items.len()), Some(0));
        std::fs::write(dir.join("new.zip"), b"x").unwrap();
        rx.recv_timeout(Duration::from_secs(2)).expect("well before the fallback rescan");
        assert_eq!(watch.take().map(|l| l.items.len()), Some(1));
        drop(watch);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
