//! Turns a version description into a short app name: no taglines, vendor prefixes or architecture suffixes.

use crate::audio::labels::capitalized;

const NOISE: [&str; 11] =
    ["powertoys", "microsoft", "windows", "ui", "app", "exe", "host", "client", "desktop", "win32", "x64"];
const TAGLINE_SEPARATORS: [&str; 4] = [" - ", " – ", " — ", " | "];
const VENDOR_PREFIXES: [&str; 2] = ["Microsoft ", "PowerToys "];
const ARCHITECTURE_SUFFIXES: [&str; 5] = [" (x64)", " (x86)", " (64-bit)", " (32-bit)", " x64"];
const MAX_NAME_CHARS: usize = 28;

pub fn friendly_app_name(description: Option<&str>, path: &str) -> String {
    description.and_then(shorten_description).unwrap_or_else(|| name_from_path(path))
}

fn name_from_path(path: &str) -> String {
    let file = path.rsplit(['\\', '/']).next().unwrap_or(path);
    let stem = match file.rfind('.') {
        Some(dot) if dot > 0 => &file[..dot],
        _ => file,
    };
    capitalized(stem.trim()).unwrap_or_else(|| "Unknown".to_owned())
}

fn shorten_description(description: &str) -> Option<String> {
    let description = description.trim();
    if description.is_empty() {
        return None;
    }
    let mut name = description.to_owned();
    name.retain(|c| c != '®' && c != '™');
    let name = drop_exe_extension(drop_taglines(name));
    let name = drop_affixes(last_meaningful_segment(name)?);
    let name = name.trim();
    if name.is_empty() || name.chars().count() > MAX_NAME_CHARS {
        return None;
    }
    capitalized(name)
}

fn drop_taglines(mut name: String) -> String {
    for separator in TAGLINE_SEPARATORS {
        if let Some((head, _)) = name.split_once(separator)
            && !head.trim().is_empty()
        {
            name = head.trim().to_owned();
        }
    }
    name
}

fn drop_exe_extension(mut name: String) -> String {
    if name.len() > 4 && name.to_ascii_lowercase().ends_with(".exe") {
        name.truncate(name.len() - 4);
    }
    name
}

fn last_meaningful_segment(name: String) -> Option<String> {
    if name.contains(' ') || !name.contains('.') {
        return Some(name);
    }
    name.split('.')
        .rfind(|part| !part.is_empty() && !NOISE.contains(&part.to_ascii_lowercase().as_str()))
        .map(str::to_owned)
}

fn drop_affixes(mut name: String) -> String {
    for prefix in VENDOR_PREFIXES {
        if name.len() > prefix.len() && name.starts_with(prefix) {
            name = name[prefix.len()..].trim().to_owned();
        }
    }
    for suffix in ARCHITECTURE_SUFFIXES {
        if name.len() > suffix.len() && name.ends_with(suffix) {
            name.truncate(name.len() - suffix.len());
        }
    }
    name
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn name_fallback_uses_capitalized_stem() {
        assert_eq!(name_from_path(r"C:\Apps\spotify.exe"), "Spotify");
        assert_eq!(name_from_path(r"C:\Program Files\VideoLAN\VLC\vlc.exe"), "Vlc");
        assert_eq!(name_from_path("/usr/bin/mpv"), "Mpv");
        assert_eq!(name_from_path(r"C:\x\my.app.exe"), "My.app");
        assert_eq!(name_from_path(r"C:\x\.hidden"), ".hidden");
        assert_eq!(name_from_path(""), "Unknown");
        assert_eq!(name_from_path(r"C:\x\éclair.exe"), "Éclair");
    }

    #[test]
    fn friendly_names_are_short() {
        let path = r"C:\Apps\thing.exe";
        assert_eq!(friendly_app_name(Some("PowerToys.Peek.UI"), path), "Peek");
        assert_eq!(friendly_app_name(Some("Microsoft Teams"), path), "Teams");
        assert_eq!(friendly_app_name(Some("Google Chrome"), path), "Google Chrome");
        assert_eq!(friendly_app_name(Some("Discord - https://discord.com"), path), "Discord");
        assert_eq!(friendly_app_name(Some("zoom.exe"), path), "Zoom");
        assert_eq!(friendly_app_name(Some("Spotify (x64)"), path), "Spotify");
        assert_eq!(friendly_app_name(Some("Firefox®"), path), "Firefox");
        assert_eq!(friendly_app_name(Some("Microsoft.UI.App"), path), "Thing");
        assert_eq!(friendly_app_name(Some("Host Process for Windows Services Extended"), path), "Thing");
        assert_eq!(friendly_app_name(Some("  "), path), "Thing");
        assert_eq!(friendly_app_name(None, r"C:\Apps\vlc.exe"), "Vlc");
        assert_eq!(friendly_app_name(Some("Café"), path), "Café", "non-ASCII never splits a char");
    }
}
