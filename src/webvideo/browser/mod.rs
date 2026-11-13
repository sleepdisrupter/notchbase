//! The browser in front: the addresses its window shows, and the cookie store yt-dlp reads to sign in as it.

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
use macos as backend;

#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "windows")]
use windows as backend;

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
mod other;
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
use other as backend;

use std::path::Path;

/// Browsers by a word in their bundle id or program name, with the cookie store yt-dlp knows them by and, for a
/// Firefox fork, the folder its profiles are in.
const FAMILIES: [(&str, &str, Option<&str>); 13] = [
    ("safari", "safari", None),
    ("chrome", "chrome", None),
    ("chromium", "chromium", None),
    ("brave", "brave", None),
    ("edgemac", "edge", None),
    ("msedge", "edge", None),
    ("opera", "opera", None),
    ("vivaldi", "vivaldi", None),
    ("firefox", "firefox", None),
    ("zen", "firefox", Some("zen")),
    ("librewolf", "firefox", Some("librewolf")),
    ("floorp", "firefox", Some("Floorp")),
    ("waterfox", "firefox", Some("Waterfox")),
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Browser {
    pub pid: u32,
    /// The window's handle on Windows. On a Mac it's 0 and the app's front window is read.
    pub window: u64,
    /// What to give yt-dlp's `--cookies-from-browser`, when the browser's cookie store is known.
    pub cookies: Option<String>,
}

/// The app in front, if it's registered to open web links, so even a browser nobody has heard of counts.
pub fn frontmost() -> Option<Browser> {
    backend::frontmost()
}

/// Addresses the browser's window shows, likeliest first, stopping as soon as one is `wanted`.
pub fn addresses(browser: &Browser, wanted: &dyn Fn(&str) -> bool) -> Vec<String> {
    backend::addresses(browser, wanted)
}

/// The cookie store for a bundle id or program name. A Firefox fork's needs its profile folder to exist.
fn cookies_for(identity: &str, profiles: Option<&Path>) -> Option<String> {
    let identity = identity.to_ascii_lowercase();
    let mut words = identity.split(|c: char| !c.is_ascii_alphanumeric());
    let (_, store, fork) = words.find_map(|word| FAMILIES.iter().find(|(start, _, _)| word.starts_with(start)))?;
    match fork {
        None => Some((*store).to_owned()),
        Some(folder) => {
            let folder = profiles?.join(folder);
            folder.is_dir().then(|| format!("{store}:{}", folder.display()))
        }
    }
}

/// The program's file name in a registry command line, quoted or not.
#[cfg(any(windows, test))]
fn program_name(command: &str) -> Option<String> {
    let command = command.trim();
    let program = match command.strip_prefix('"') {
        Some(quoted) => quoted.split('"').next()?,
        None => {
            let lower = command.to_ascii_lowercase();
            let end = lower.find(".exe").map_or(command.len(), |at| at + ".exe".len());
            &command[..end]
        }
    };
    let name = program.rsplit(['\\', '/']).next()?.trim().to_ascii_lowercase();
    (!name.is_empty()).then_some(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn browser_known_by_program_name() {
        let chrome = r#""C:\Program Files\Google\Chrome\Application\chrome.exe""#;
        assert_eq!(program_name(chrome).as_deref(), Some("chrome.exe"));
        let firefox = r#""C:\Program Files\Mozilla Firefox\firefox.exe" -osint -url "%1""#;
        assert_eq!(program_name(firefox).as_deref(), Some("firefox.exe"));
        let bare = r"C:\Program Files (x86)\Microsoft\Edge\Application\MSEdge.EXE --single-argument %1";
        assert_eq!(program_name(bare).as_deref(), Some("msedge.exe"));
        assert_eq!(program_name("zen.exe").as_deref(), Some("zen.exe"));
        assert_eq!(program_name("  "), None);
    }

    #[test]
    fn known_browsers_map_to_cookie_stores() {
        for (identity, store) in [
            ("com.apple.Safari", "safari"),
            ("com.google.Chrome", "chrome"),
            ("com.google.Chrome.canary", "chrome"),
            ("org.chromium.Chromium", "chromium"),
            ("com.brave.Browser", "brave"),
            ("com.microsoft.edgemac", "edge"),
            ("msedge", "edge"),
            ("com.operasoftware.Opera", "opera"),
            ("com.vivaldi.Vivaldi", "vivaldi"),
            ("org.mozilla.firefox", "firefox"),
            ("org.mozilla.firefoxdeveloperedition", "firefox"),
            ("firefox", "firefox"),
        ] {
            assert_eq!(cookies_for(identity, None).as_deref(), Some(store), "{identity}");
        }
    }

    #[test]
    fn firefox_fork_points_at_its_profiles() {
        let root = std::env::temp_dir().join(format!("notchbase-browser-{}", std::process::id()));
        std::fs::create_dir_all(root.join("zen")).unwrap();
        let zen = cookies_for("app.zen-browser.zen", Some(&root));
        assert_eq!(zen, Some(format!("firefox:{}", root.join("zen").display())));
        assert_eq!(cookies_for("io.gitlab.librewolf-community", Some(&root)), None, "no profiles, nothing to borrow");
        assert_eq!(cookies_for("app.zen-browser.zen", None), None);
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn unknown_browser_has_no_cookie_store() {
        assert_eq!(cookies_for("company.thebrowser.Browser", None), None);
        assert_eq!(cookies_for("com.citizen.app", None), None, "a word that merely contains zen is not Zen");
        assert_eq!(cookies_for("", None), None);
    }
}
