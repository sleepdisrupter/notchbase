//! Which part each root belongs to and how it's listed: each child, large files then folders, or one whole item.

use std::path::PathBuf;

use super::Part;
use crate::platform::{self, KnownDir};

pub(crate) const DEVELOPER: [&str; 22] = [
    ".cargo",
    ".rustup",
    ".npm",
    ".nvm",
    ".bun",
    ".deno",
    ".gradle",
    ".m2",
    ".pyenv",
    ".rbenv",
    ".cache",
    ".local",
    ".android",
    ".cocoapods",
    ".swiftpm",
    ".vscode",
    ".cursor",
    ".docker",
    ".conda",
    "go",
    "miniconda3",
    "Developer",
];
const MACHINES: [&str; 4] = ["Parallels", "Virtual Machines", "Virtual Machines.localized", "UTM"];
const TOOLS: &str = "Homebrew";
const TRASH: &str = "Trash";

/// How a root is listed: each child, its big files and top folders, one whole item, or split (Caches, Library).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Plan {
    Each(Part),
    Files(Part),
    Whole(Part),
    Caches,
    Library,
}

#[derive(Debug)]
pub(super) struct Root {
    pub(super) name: String,
    pub(super) path: PathBuf,
    pub(super) plan: Plan,
}

pub(super) fn roots() -> Vec<Root> {
    let fixed = |name: &str, path: &str, plan| Root { name: name.to_owned(), path: PathBuf::from(path), plan };
    let mut roots = vec![
        fixed("Applications", "/Applications", Plan::Each(Part::Apps)),
        fixed("Library", "/Library", Plan::Each(Part::System)),
        fixed("Shared", "/Users/Shared", Plan::Files(Part::Other)),
        fixed(TOOLS, "/opt", Plan::Whole(Part::Developer)),
    ];
    let home = platform::known_dir(KnownDir::Home);
    for entry in home.and_then(|home| std::fs::read_dir(home).ok()).into_iter().flatten().flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let (name, plan) = in_home(&name);
        roots.push(Root { name, path: entry.path(), plan });
    }
    roots
}

fn in_home(name: &str) -> (String, Plan) {
    let plan = match name {
        "Applications" => Plan::Each(Part::Apps),
        "Documents" => Plan::Files(Part::Documents),
        "Desktop" => Plan::Files(Part::Desktop),
        "Downloads" => Plan::Files(Part::Downloads),
        "Pictures" | "Movies" => Plan::Files(Part::Media),
        "Music" => Plan::Files(Part::Music),
        "Library" => Plan::Library,
        ".Trash" => return (TRASH.to_owned(), Plan::Whole(Part::Other)),
        _ if MACHINES.contains(&name) => Plan::Files(Part::Machines),
        _ if DEVELOPER.contains(&name) => return (pretty(name), Plan::Whole(Part::Developer)),
        _ => Plan::Files(Part::Other),
    };
    (name.to_owned(), plan)
}

pub(super) fn in_library(name: &str) -> Plan {
    match name {
        "Caches" => Plan::Caches,
        "Application Support" | "Containers" | "Group Containers" => Plan::Each(Part::AppData),
        "Developer" => Plan::Each(Part::Developer),
        _ => Plan::Whole(Part::AppData),
    }
}

/// Makes names readable: com.google.Chrome becomes Chrome, and .cargo becomes Cargo.
pub(super) fn pretty(name: &str) -> String {
    const DOMAINS: [&str; 10] = ["com", "org", "net", "io", "app", "dev", "group", "co", "me", "us"];
    let name = name.strip_suffix(".app").unwrap_or(name);
    let words: Vec<&str> = name.split('.').filter(|word| !word.is_empty()).collect();
    let named = match words.as_slice() {
        [first, .., last] if words.len() >= 3 && DOMAINS.contains(&first.to_ascii_lowercase().as_str()) => last,
        [only] if name.starts_with('.') => only,
        _ => name,
    };
    let mut letters = named.chars();
    match letters.next() {
        Some(first) if named.chars().all(|letter| !letter.is_uppercase()) => {
            first.to_uppercase().chain(letters).collect()
        }
        _ => named.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn home_folders_go_to_their_part() {
        assert_eq!(in_home("Documents"), ("Documents".into(), Plan::Files(Part::Documents)));
        assert_eq!(in_home("Movies").1, Plan::Files(Part::Media));
        assert_eq!(in_home(".Trash"), ("Trash".into(), Plan::Whole(Part::Other)));
        assert_eq!(in_home(".cargo"), ("Cargo".into(), Plan::Whole(Part::Developer)));
        assert_eq!(in_home("Parallels").1, Plan::Files(Part::Machines));
        assert_eq!(in_home("Library").1, Plan::Library);
        assert_eq!(in_home("Projects").1, Plan::Files(Part::Other));
        assert_eq!(in_library("Caches"), Plan::Caches);
        assert_eq!(in_library("Containers"), Plan::Each(Part::AppData));
        assert_eq!(in_library("Mail"), Plan::Whole(Part::AppData));
        assert!(roots().iter().any(|root| root.name == "Applications" && root.plan == Plan::Each(Part::Apps)));
    }

    #[test]
    fn names_are_made_readable() {
        assert_eq!(pretty("com.google.Chrome"), "Chrome");
        assert_eq!(pretty("group.com.apple.notes"), "Notes");
        assert_eq!(pretty("Xcode.app"), "Xcode");
        assert_eq!(pretty(".rustup"), "Rustup");
        assert_eq!(pretty("Google"), "Google");
        assert_eq!(pretty("iMovie.app"), "iMovie");
        assert_eq!(pretty("notes.txt"), "Notes.txt");
    }
}
