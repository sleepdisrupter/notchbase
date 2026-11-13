//! What kind of thing a file is, from its ending and its flags: a folder, an app, a document, a picture and so on.

use super::model::flag;

#[cfg(test)]
pub const MOST_EXTENSIONS: usize = 12;

const DOT: u8 = b'.';
const BUNDLE: &[u8] = b".app";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    Folder,
    App,
    Document,
    Pdf,
    Image,
    Video,
    Audio,
    Archive,
    Code,
    Text,
    Sheet,
    Slides,
    Font,
    Disk,
    Link,
    Other,
}

const EXTENSIONS: [(Kind, &[&str]); 14] = [
    (Kind::App, &["app", "exe"]),
    (Kind::Document, &["doc", "docx", "pages", "rtf", "odt", "epub"]),
    (Kind::Pdf, &["pdf"]),
    (Kind::Image, &["png", "jpg", "jpeg", "gif", "heic", "webp", "tiff", "tif", "bmp", "svg", "psd", "ico"]),
    (Kind::Video, &["mov", "mp4", "m4v", "mkv", "avi", "webm", "wmv", "mpg", "mpeg"]),
    (Kind::Audio, &["mp3", "m4a", "wav", "flac", "aac", "aiff", "aif", "ogg", "opus", "wma", "mid"]),
    (Kind::Archive, &["zip", "tar", "gz", "tgz", "bz2", "xz", "7z", "rar", "zst"]),
    (Kind::Code, &["rs", "py", "js", "ts", "tsx", "swift", "c", "h", "cpp", "go", "java", "sh"]),
    (Kind::Text, &["txt", "md", "json", "toml", "yaml", "yml", "xml", "log", "ini", "html", "css", "plist"]),
    (Kind::Sheet, &["xlsx", "xls", "numbers", "csv", "ods", "tsv"]),
    (Kind::Slides, &["pptx", "ppt", "key", "odp"]),
    (Kind::Font, &["ttf", "otf", "woff", "woff2", "ttc"]),
    (Kind::Disk, &["dmg", "iso", "img", "sparseimage", "vhd", "vhdx"]),
    (Kind::Link, &["lnk", "url", "webloc"]),
];

const WORDS: [(&str, Kind); 14] = [
    ("folder", Kind::Folder),
    ("folders", Kind::Folder),
    ("app", Kind::App),
    ("apps", Kind::App),
    ("applications", Kind::App),
    ("image", Kind::Image),
    ("images", Kind::Image),
    ("video", Kind::Video),
    ("videos", Kind::Video),
    ("music", Kind::Audio),
    ("zip", Kind::Archive),
    ("code", Kind::Code),
    ("doc", Kind::Document),
    ("docs", Kind::Document),
];

impl Kind {
    pub fn of(folded_name: &[u8], flags: u8) -> Kind {
        if flags & flag::LINK != 0 {
            return Kind::Link;
        }
        if flags & flag::DIR != 0 {
            return if folded_name.ends_with(BUNDLE) { Kind::App } else { Kind::Folder };
        }
        extension(folded_name).and_then(Kind::ending).unwrap_or(Kind::Other)
    }

    pub fn extensions(self) -> &'static [&'static str] {
        EXTENSIONS.iter().find(|(kind, _)| *kind == self).map_or(&[], |(_, endings)| endings)
    }

    pub fn word(folded: &str) -> Option<Kind> {
        Kind::named(folded).or_else(|| Kind::ending(folded.as_bytes()))
    }

    pub fn named(folded: &str) -> Option<Kind> {
        WORDS.iter().find(|(word, _)| *word == folded).map(|(_, kind)| *kind)
    }

    pub fn ending(extension: &[u8]) -> Option<Kind> {
        let known = |(_, endings): &&(Kind, &[&str])| endings.iter().any(|ending| ending.as_bytes() == extension);
        EXTENSIONS.iter().find(known).map(|(kind, _)| *kind)
    }
}

pub fn extension(folded_name: &[u8]) -> Option<&[u8]> {
    let dot = memchr::memrchr(DOT, folded_name).filter(|dot| *dot > 0)?;
    Some(&folded_name[dot + 1..]).filter(|ending| !ending.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classified_by_ending() {
        let of = |name: &str| Kind::of(name.as_bytes(), 0);
        assert_eq!((of("report.pdf"), of("photo.heic"), of("main.rs")), (Kind::Pdf, Kind::Image, Kind::Code));
        assert_eq!((of("clip.mov"), of("song.flac"), of("old.tar.gz")), (Kind::Video, Kind::Audio, Kind::Archive));
        assert_eq!((of("budget.xlsx"), of("talk.key"), of("notes.md")), (Kind::Sheet, Kind::Slides, Kind::Text));
        assert_eq!((of("inter.otf"), of("setup.dmg"), of("thesis.docx")), (Kind::Font, Kind::Disk, Kind::Document));
        assert_eq!((of("setup.exe"), of("site.webloc")), (Kind::App, Kind::Link));
    }

    #[test]
    fn unknown_ending_is_a_file() {
        for name in ["makefile", ".zshrc", "archive.", "notes.xyz", "pdf", ""] {
            assert_eq!(Kind::of(name.as_bytes(), 0), Kind::Other, "{name}");
        }
        assert_eq!(extension(b".zshrc"), None, "a leading dot hides a file, it does not name its type");
        assert_eq!(extension(b"old.tar.gz"), Some(&b"gz"[..]));
    }

    #[test]
    fn flags_tell_folders_and_links() {
        assert_eq!(Kind::of(b"documents", flag::DIR), Kind::Folder);
        assert_eq!(Kind::of(b"photos.png", flag::DIR), Kind::Folder, "a folder is a folder whatever it is called");
        assert_eq!(Kind::of(b"calendar.app", flag::DIR), Kind::App);
        assert_eq!(Kind::of(b"current.pdf", flag::LINK), Kind::Link);
        assert_eq!(Kind::of(b"latest", flag::LINK | flag::DIR), Kind::Link);
    }

    #[test]
    fn endings_belong_to_one_kind() {
        let mut all: Vec<&str> = EXTENSIONS.iter().flat_map(|(_, endings)| endings.iter().copied()).collect();
        let listed = all.len();
        all.sort_unstable();
        all.dedup();
        assert_eq!(all.len(), listed, "an ending is listed twice");
        assert!(EXTENSIONS.iter().all(|(_, endings)| (1..=MOST_EXTENSIONS).contains(&endings.len())));
        assert!(all.iter().all(|ending| *ending == ending.to_ascii_lowercase() && !ending.contains('.')));
        assert!(Kind::Folder.extensions().is_empty() && Kind::Other.extensions().is_empty());
        assert_eq!(Kind::Pdf.extensions(), ["pdf"]);
    }

    #[test]
    fn words_and_endings_name_kinds() {
        let named =
            [("folder", Kind::Folder), ("folders", Kind::Folder), ("music", Kind::Audio), ("zip", Kind::Archive)];
        for (word, kind) in named {
            assert_eq!((Kind::named(word), Kind::word(word)), (Some(kind), Some(kind)), "{word}");
        }
        assert_eq!((Kind::named("apps"), Kind::named("applications")), (Some(Kind::App), Some(Kind::App)));
        assert_eq!((Kind::word("doc"), Kind::word("docx")), (Some(Kind::Document), Some(Kind::Document)));
        assert_eq!((Kind::named("pdf"), Kind::word("pdf")), (None, Some(Kind::Pdf)));
        assert_eq!((Kind::word("rs"), Kind::word("heic")), (Some(Kind::Code), Some(Kind::Image)));
        assert_eq!((Kind::word("readme"), Kind::word(""), Kind::word("Folder")), (None, None, None));
    }
}
