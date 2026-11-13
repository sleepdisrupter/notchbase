//! The notchbase executable as hooks name it, and telling our commands apart from everyone else's.

use std::path::Path;

const NAME: &str = "notchbase";
const WINDOWS_SUFFIX: &str = ".exe";

pub(super) fn current() -> Result<String, String> {
    let path = std::env::current_exe().map_err(|error| format!("Couldn't find notchbase's own location: {error}"))?;
    path.to_str().map(str::to_owned).ok_or_else(|| format!("{} isn't a path hooks can use", path.display()))
}

/// Whether a command runs notchbase: named notchbase or like this executable, with or without .exe.
pub(super) fn names_us(command: &str) -> bool {
    let command = command.trim().trim_matches(['"', '\'']);
    let file = command.rsplit(['/', '\\']).next().unwrap_or(command);
    let stem = strip_windows_suffix(file);
    stem.eq_ignore_ascii_case(NAME) || own_stem().is_some_and(|own| stem.eq_ignore_ascii_case(&own))
}

/// A shell command line for Codex: the path quoted for cmd on Windows and for sh elsewhere.
pub(super) fn shell_command(exe: &str, tail: &str, windows: bool) -> String {
    if windows { format!("\"{exe}\" {tail}") } else { format!("'{}' {tail}", exe.replace('\'', r"'\''")) }
}

fn own_stem() -> Option<String> {
    let exe = std::env::current_exe().ok()?;
    Path::new(&exe).file_stem().map(|stem| stem.to_string_lossy().into_owned())
}

fn strip_windows_suffix(file: &str) -> &str {
    let cut = file.len().saturating_sub(WINDOWS_SUFFIX.len());
    match file.get(cut..) {
        Some(suffix) if cut > 0 && suffix.eq_ignore_ascii_case(WINDOWS_SUFFIX) => &file[..cut],
        _ => file,
    }
}
