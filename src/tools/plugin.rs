//! Native plugins: a .dll, .dylib or .so in its own folder under tools, loaded through the C ABI the SDK exports.

use std::ffi::c_void;
use std::path::{Path, PathBuf};

use notchbase_sdk::ffi::{ABI_VERSION, Entry, HostWaker, Plugin, entry_symbol};
use notchbase_sdk::protocol::{Answer, Call, Effects, Event, Manifest, PROTOCOL_VERSION, Start, Views};
use notchbase_sdk::{Driver, Waker};

#[cfg(target_os = "windows")]
const EXTENSION: &str = "dll";
#[cfg(target_os = "macos")]
const EXTENSION: &str = "dylib";
#[cfg(not(any(target_os = "windows", target_os = "macos")))]
const EXTENSION: &str = "so";
const SHADOWS: &str = "notchbase-plugins";

pub struct PluginDriver {
    plugin: &'static Plugin,
    instance: *mut c_void,
    manifest: Manifest,
}

impl PluginDriver {
    pub fn load(path: &Path, waker: Waker) -> Result<Self, String> {
        let stem = path.file_stem().and_then(|stem| stem.to_str()).ok_or("the library has no name")?;
        let library = unsafe { libloading::Library::new(shadow(path, stem)) }.map_err(|err| err.to_string())?;
        // Plugins are never unloaded during a run, so the library lives as long as notchbase.
        let library: &'static libloading::Library = Box::leak(Box::new(library));
        let symbol = entry_symbol(stem);
        let entry = unsafe { library.get::<Entry>(symbol.as_bytes()) }
            .map_err(|_| format!("it doesn't export {symbol}; end the tool with notchbase_sdk::export!"))?;
        let plugin = unsafe { entry().as_ref() }.ok_or("its entry point returned nothing")?;
        if plugin.abi != ABI_VERSION {
            return Err(format!("it was built for tool ABI {} but notchbase speaks {ABI_VERSION}", plugin.abi));
        }
        let context: *const Waker = Box::leak(Box::new(waker));
        let instance = unsafe { (plugin.create)(HostWaker { context: context.cast(), wake }) };
        if instance.is_null() {
            return Err("it couldn't create itself".into());
        }
        let mut driver = Self { plugin, instance, manifest: placeholder(stem) };
        match driver.call(&Call::Manifest) {
            Answer::Manifest { manifest } if manifest.protocol > PROTOCOL_VERSION => {
                return Err(format!("it needs a newer notchbase (tool protocol {})", manifest.protocol));
            }
            Answer::Manifest { manifest } => driver.manifest = manifest,
            Answer::Failed { error } => return Err(error),
            _ => return Err("it answered the manifest call with something else".into()),
        }
        Ok(driver)
    }

    fn call(&mut self, call: &Call) -> Answer {
        let request = match serde_json::to_vec(call) {
            Ok(request) => request,
            Err(err) => return Answer::Failed { error: err.to_string() },
        };
        let buffer = unsafe { (self.plugin.call)(self.instance, request.as_ptr(), request.len()) };
        let answer = serde_json::from_slice(buffer.as_slice())
            .unwrap_or_else(|err| Answer::Failed { error: format!("unreadable answer: {err}") });
        unsafe { (self.plugin.release)(buffer) };
        answer
    }

    fn expect_effects(&mut self, call: &Call) -> Effects {
        match self.call(call) {
            Answer::Effects { effects } => effects,
            Answer::Failed { error } => panic!("{error}"),
            _ => panic!("the plugin answered with something other than effects"),
        }
    }
}

impl Driver for PluginDriver {
    fn manifest(&self) -> Manifest {
        self.manifest.clone()
    }

    fn start(&mut self, start: Start) -> Effects {
        self.expect_effects(&Call::Start { start: Box::new(start) })
    }

    fn handle(&mut self, events: Vec<Event>) -> Effects {
        self.expect_effects(&Call::Handle { events })
    }

    fn views(&mut self) -> Views {
        match self.call(&Call::Views) {
            Answer::Views { views } => *views,
            Answer::Failed { error } => panic!("{error}"),
            _ => panic!("the plugin answered with something other than views"),
        }
    }
}

impl Drop for PluginDriver {
    fn drop(&mut self) {
        unsafe { (self.plugin.destroy)(self.instance) };
    }
}

unsafe extern "C" fn wake(context: *const c_void) {
    if let Some(waker) = unsafe { context.cast::<Waker>().as_ref() } {
        waker.wake();
    }
}

/// A private copy in the temp folder, named by size and time, so the original is never locked and can be replaced.
fn shadow(path: &Path, stem: &str) -> PathBuf {
    let Ok(meta) = std::fs::metadata(path) else { return path.to_owned() };
    let modified =
        meta.modified().ok().and_then(|at| at.duration_since(std::time::UNIX_EPOCH).ok()).unwrap_or_default();
    let root = std::env::temp_dir().join(SHADOWS);
    let prefix = format!("{stem}-");
    let folder = root.join(format!("{prefix}{:x}-{:x}", meta.len(), modified.as_nanos()));
    let copy = folder.join(path.file_name().unwrap_or_default());
    let fresh = std::fs::metadata(&copy).is_ok_and(|copied| copied.len() == meta.len());
    if !fresh && (std::fs::create_dir_all(&folder).is_err() || std::fs::copy(path, &copy).is_err()) {
        return path.to_owned();
    }
    for old in std::fs::read_dir(&root).into_iter().flatten().flatten().map(|entry| entry.path()) {
        let name = old.file_name().unwrap_or_default().to_string_lossy().into_owned();
        if old != folder && name.starts_with(&prefix) {
            let _ = std::fs::remove_dir_all(&old);
        }
    }
    copy
}

/// Whether a library is named after its folder, ignoring case, a `lib` prefix and dashes against underscores.
fn named_after(folder: &Path, library: &Path) -> bool {
    let plain = |path: &Path| {
        let stem = path.file_stem().or_else(|| path.file_name()).map(|name| name.to_string_lossy().to_lowercase());
        stem.unwrap_or_default().trim_start_matches("lib").replace('-', "_")
    };
    plain(folder) == plain(library)
}

fn placeholder(stem: &str) -> Manifest {
    Manifest::new(stem, stem)
}

/// A native library for this platform if the folder has one, else a .wasm. The one named after its folder wins.
pub fn libraries(tools_dir: &Path) -> Vec<PathBuf> {
    let Ok(folders) = std::fs::read_dir(tools_dir) else { return Vec::new() };
    let mut found: Vec<PathBuf> = folders
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .filter_map(|folder| {
            let files = std::fs::read_dir(&folder).ok()?;
            let files: Vec<PathBuf> = files.flatten().map(|entry| entry.path()).collect();
            [EXTENSION, super::wasm::EXTENSION].into_iter().find_map(|wanted| {
                let mut libraries: Vec<&PathBuf> =
                    files.iter().filter(|path| path.extension().is_some_and(|extension| extension == wanted)).collect();
                libraries.sort();
                let named = libraries.iter().position(|library| named_after(&folder, library)).unwrap_or(0);
                (!libraries.is_empty()).then(|| libraries.swap_remove(named).clone())
            })
        })
        .collect();
    found.sort();
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_wins_over_portable() {
        let dir = std::env::temp_dir().join(format!("notchbase-portable-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        for (folder, files) in [("clock", vec!["clock.wasm"]), ("both", vec!["both.wasm", "native"]), ("none", vec![])]
        {
            std::fs::create_dir_all(dir.join(folder)).unwrap();
            for file in files {
                let name = if file == "native" { format!("both.{EXTENSION}") } else { file.to_owned() };
                std::fs::write(dir.join(folder).join(name), b"").unwrap();
            }
        }
        let found = libraries(&dir);
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(found, [dir.join("both").join(format!("both.{EXTENSION}")), dir.join("clock").join("clock.wasm")]);
    }

    #[test]
    fn each_tool_folder_offers_its_library() {
        let dir = std::env::temp_dir().join(format!("notchbase-plugins-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("focus-timer")).unwrap();
        std::fs::write(dir.join("focus-timer").join(format!("a_helper.{EXTENSION}")), b"").unwrap();
        std::fs::write(dir.join("focus-timer").join(format!("focus_timer.{EXTENSION}")), b"").unwrap();
        for (folder, file) in [("timer", "focus_timer"), ("notes", "readme"), ("weather", "weather")] {
            std::fs::create_dir_all(dir.join(folder)).unwrap();
            let extension = if file == "readme" { "md" } else { EXTENSION };
            std::fs::write(dir.join(folder).join(format!("{file}.{extension}")), b"").unwrap();
        }
        std::fs::write(dir.join(format!("stray.{EXTENSION}")), b"").unwrap();
        let found = libraries(&dir);
        let _ = std::fs::remove_dir_all(&dir);
        let names: Vec<_> = found.iter().map(|path| path.file_stem().unwrap().to_string_lossy().into_owned()).collect();
        assert_eq!(names, ["focus_timer", "focus_timer", "weather"]);
    }

    #[test]
    fn non_library_file_is_refused() {
        let file = std::env::temp_dir().join(format!("notchbase-not-a-tool-{}.{EXTENSION}", std::process::id()));
        std::fs::write(&file, b"nope").unwrap();
        let loaded = PluginDriver::load(&file, Waker::noop());
        let _ = std::fs::remove_file(&file);
        assert!(loaded.is_err());
    }
}
