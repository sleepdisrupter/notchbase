//! Turns drops that aren't files yet into files in Downloads, named like a browser download. Links are fetched
//! with a 30 second timeout and a 512 MB cap.

use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::time::Duration;

use super::incoming::{Dropped, Saved};
use super::links;

const LARGEST: u64 = 512 << 20;
const TIMEOUT: Duration = Duration::from_secs(30);
const LONGEST_NAME: usize = 120;
const USER_AGENT: &str = concat!("Mozilla/5.0 (compatible; notchbase/", env!("CARGO_PKG_VERSION"), ")");
const IMAGE_STEM: &str = "Image";
const OTHER_STEM: &str = "Download";
const IMAGE_EXTENSIONS: [&str; 12] =
    ["png", "jpg", "jpeg", "gif", "webp", "bmp", "ico", "tiff", "tif", "svg", "avif", "heic"];
const WEB_PAGES: [&str; 2] = ["text/html", "application/xhtml+xml"];
const PDF: [&str; 1] = ["pdf"];
const NOTHING: &str = "There was nothing to save";
const WEB_PAGE: &str = "That link is a web page, not a file";
const UNKNOWN_LINK: &str = "Couldn't download that link";

struct Payload {
    name: Option<String>,
    kind: Option<&'static str>,
    bytes: Vec<u8>,
}

pub fn save(items: Vec<Dropped>, folder: &Path) -> Saved {
    let mut saved = Saved::default();
    for item in items {
        match keep(item, folder) {
            Ok(path) => saved.paths.push(path),
            Err(problem) => saved.problems.push(problem),
        }
    }
    saved
}

/// Copies dropped files out of the temporary folder at once, since the sender deletes them soon after.
pub fn rescue(items: Vec<Dropped>, folder: Option<&Path>) -> Vec<Dropped> {
    items
        .into_iter()
        .map(|item| match (item, folder) {
            (Dropped::File(path), Some(folder)) if is_temporary(&path) => {
                copy_in(&path, folder).map_or(Dropped::File(path), Dropped::File)
            }
            (item, _) => item,
        })
        .collect()
}

pub fn is_temporary(path: &Path) -> bool {
    let resolve = |path: &Path| path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    path.is_file() && resolve(path).starts_with(resolve(&std::env::temp_dir()))
}

fn keep(item: Dropped, folder: &Path) -> Result<PathBuf, String> {
    match item {
        Dropped::File(path) if is_temporary(&path) => copy_in(&path, folder),
        Dropped::File(path) => Ok(path),
        Dropped::Data { name, bytes } => write(folder, Payload { name, kind: None, bytes }),
        Dropped::Link(url) | Dropped::Page(url) => match links::file_path(&url) {
            Some(path) => keep(Dropped::File(path), folder),
            None => write(folder, fetch(&url)?),
        },
    }
}

fn copy_in(path: &Path, folder: &Path) -> Result<PathBuf, String> {
    let name = path.file_name().map(|name| clean_name(&name.to_string_lossy())).unwrap_or_default();
    let target = place(folder, if name.is_empty() { OTHER_STEM } else { &name })?;
    std::fs::copy(path, &target).map_err(|err| format!("Couldn't save {name}: {err}"))?;
    Ok(target)
}

fn write(folder: &Path, payload: Payload) -> Result<PathBuf, String> {
    if payload.bytes.is_empty() {
        return Err(NOTHING.into());
    }
    let (name, bytes) = tidy(payload);
    let target = place(folder, &name)?;
    std::fs::write(&target, bytes).map_err(|err| format!("Couldn't save {name}: {err}"))?;
    Ok(target)
}

fn place(folder: &Path, name: &str) -> Result<PathBuf, String> {
    std::fs::create_dir_all(folder).map_err(|err| format!("Couldn't open {}: {err}", folder.display()))?;
    Ok(unique(folder, name))
}

/// Names the bytes, and turns an unnamed TIFF into a PNG.
fn tidy(payload: Payload) -> (String, Vec<u8>) {
    let sniffed = sniff(&payload.bytes);
    let tiff = sniffed.is_some_and(|extensions| extensions.contains(&"tiff"));
    if payload.name.is_none()
        && tiff
        && let Some(png) = to_png(&payload.bytes)
    {
        return (file_name(None, Some(&["png"]), None), png);
    }
    (file_name(payload.name.as_deref(), sniffed, payload.kind), payload.bytes)
}

/// The extension the bytes call for when the sender's is wrong or missing, else one from the MIME type.
fn file_name(name: Option<&str>, sniffed: Option<&'static [&'static str]>, kind: Option<&str>) -> String {
    let preferred = sniffed.and_then(|extensions| extensions.first().copied()).or(kind);
    let name = name.map(clean_name).filter(|name| !name.is_empty()).unwrap_or_else(|| {
        let image = preferred.is_some_and(|extension| IMAGE_EXTENSIONS.contains(&extension));
        (if image { IMAGE_STEM } else { OTHER_STEM }).to_owned()
    });
    let path = Path::new(&name);
    let stem = path.file_stem().map_or_else(|| name.clone(), |stem| stem.to_string_lossy().into_owned());
    match (path.extension().map(|extension| extension.to_string_lossy().to_ascii_lowercase()), sniffed, preferred) {
        (Some(extension), Some(known), _) if !known.contains(&extension.as_str()) => format!("{stem}.{}", known[0]),
        (None, _, Some(extension)) => format!("{name}.{extension}"),
        _ => name,
    }
}

fn sniff(bytes: &[u8]) -> Option<&'static [&'static str]> {
    if bytes.starts_with(b"%PDF") {
        return Some(&PDF);
    }
    image::guess_format(bytes).ok().map(|format| format.extensions_str()).filter(|extensions| !extensions.is_empty())
}

fn to_png(bytes: &[u8]) -> Option<Vec<u8>> {
    let image = image::load_from_memory_with_format(bytes, image::ImageFormat::Tiff).ok()?;
    let mut png = Vec::new();
    image.write_to(&mut Cursor::new(&mut png), image::ImageFormat::Png).ok()?;
    Some(png)
}

/// Makes a name safe on any file system: no folders, no characters Windows refuses, at most 120 characters.
fn clean_name(name: &str) -> String {
    let last = name.rsplit(['/', '\\']).next().unwrap_or_default();
    let safe: String = last
        .chars()
        .map(|c| if c.is_control() || matches!(c, '<' | '>' | ':' | '"' | '|' | '?' | '*') { '-' } else { c })
        .collect();
    let safe = safe.trim().trim_end_matches('.').trim_start_matches('.').trim().to_owned();
    if safe.chars().count() <= LONGEST_NAME {
        return safe;
    }
    let path = Path::new(&safe);
    let extension = path.extension().map(|extension| extension.to_string_lossy().into_owned()).unwrap_or_default();
    let stem: String = path.file_stem().map(|stem| stem.to_string_lossy().into_owned()).unwrap_or_default();
    let room = LONGEST_NAME.saturating_sub(extension.chars().count() + 1).max(1);
    let stem: String = stem.chars().take(room).collect();
    if extension.is_empty() { stem } else { format!("{}.{extension}", stem.trim_end()) }
}

/// `cat.png`, then `cat (1).png` and so on, like a browser download.
fn unique(folder: &Path, name: &str) -> PathBuf {
    let first = folder.join(name);
    if !first.exists() {
        return first;
    }
    let path = Path::new(name);
    let stem = path.file_stem().map_or_else(|| name.to_owned(), |stem| stem.to_string_lossy().into_owned());
    let extension = path.extension().map(|extension| format!(".{}", extension.to_string_lossy())).unwrap_or_default();
    (1..).map(|n| folder.join(format!("{stem} ({n}){extension}"))).find(|path| !path.exists()).unwrap_or(first)
}

fn fetch(url: &str) -> Result<Payload, String> {
    let url = links::unwrap(url);
    if let Some(inline) = links::inline(&url) {
        let kind = links::extension_for_mime(&inline.mime);
        return Ok(Payload { name: None, kind, bytes: inline.bytes });
    }
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return Err(UNKNOWN_LINK.into());
    }
    let mut response = agent().get(&url).call().map_err(|err| describe(&err))?;
    let mime = response.headers().get("content-type").and_then(|value| value.to_str().ok()).unwrap_or_default();
    let mime = mime.split(';').next().unwrap_or_default().trim().to_ascii_lowercase();
    if WEB_PAGES.contains(&mime.as_str()) {
        return Err(WEB_PAGE.into());
    }
    let bytes = response.body_mut().with_config().limit(LARGEST).read_to_vec().map_err(|err| describe(&err))?;
    Ok(Payload { name: links::name_of(&url), kind: links::extension_for_mime(&mime), bytes })
}

fn agent() -> ureq::Agent {
    notchbase_net::config(TIMEOUT).user_agent(USER_AGENT).build().into()
}

fn describe(err: &ureq::Error) -> String {
    match err {
        ureq::Error::Timeout(_) => "The download timed out".into(),
        ureq::Error::HostNotFound | ureq::Error::ConnectionFailed => "Couldn't reach the site to download it".into(),
        ureq::Error::BodyExceedsLimit(_) => "That file is too big to download".into(),
        ureq::Error::StatusCode(status) => format!("The download failed (HTTP {status})"),
        _ => UNKNOWN_LINK.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PNG: &[u8] = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR";
    const JPEG: &[u8] = &[0xFF, 0xD8, 0xFF, 0xE0, 0, 0x10, b'J', b'F', b'I', b'F'];

    fn folder(name: &str) -> PathBuf {
        let folder = std::env::temp_dir().join(format!("notchbase-save-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        folder
    }

    #[test]
    fn names_get_the_right_extension() {
        assert_eq!(file_name(None, sniff(PNG), None), "Image.png");
        assert_eq!(file_name(Some("images"), sniff(JPEG), None), "images.jpg");
        assert_eq!(file_name(Some("photo.php"), sniff(PNG), None), "photo.png");
        assert_eq!(file_name(Some("Photo.JPEG"), sniff(JPEG), None), "Photo.JPEG");
        assert_eq!(file_name(Some("report.docx"), sniff(b"PK\x03\x04"), None), "report.docx");
        assert_eq!(file_name(Some("drawing"), None, Some("svg")), "drawing.svg");
        assert_eq!(file_name(None, None, None), "Download");
        assert_eq!(file_name(None, sniff(b"%PDF-1.7"), None), "Download.pdf");
    }

    #[test]
    fn names_are_safe_on_any_system() {
        assert_eq!(clean_name(r"C:\temp\a:b?.jpg"), "a-b-.jpg");
        assert_eq!(clean_name("../../.hidden."), "hidden");
        assert_eq!(clean_name("  spaced  "), "spaced");
        let long = clean_name(&format!("{}.png", "x".repeat(300)));
        assert_eq!(long.chars().count(), LONGEST_NAME);
        assert!(long.ends_with(".png"));
    }

    #[test]
    fn taken_name_gets_a_number() {
        let folder = folder("unique");
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join("cat.jpg"), b"").unwrap();
        std::fs::write(folder.join("cat (1).jpg"), b"").unwrap();
        assert_eq!(unique(&folder, "cat.jpg"), folder.join("cat (2).jpg"));
        assert_eq!(unique(&folder, "dog.jpg"), folder.join("dog.jpg"));
        std::fs::write(folder.join("notes"), b"").unwrap();
        assert_eq!(unique(&folder, "notes"), folder.join("notes (1)"));
        std::fs::remove_dir_all(&folder).unwrap();
    }

    #[test]
    fn bytes_and_data_links_are_saved() {
        let folder = folder("bytes");
        let items = vec![
            Dropped::Data { name: Some("images.jpeg".into()), bytes: JPEG.to_vec() },
            Dropped::Link("data:image/png;base64,iVBORw0KGgoAAAANSUhEUg==".into()),
            Dropped::Data { name: None, bytes: Vec::new() },
        ];
        let saved = save(items, &folder);
        assert_eq!(saved.paths, [folder.join("images.jpeg"), folder.join("Image.png")]);
        assert_eq!(std::fs::read(&saved.paths[1]).unwrap(), PNG);
        assert_eq!(saved.problems, [NOTHING]);
        std::fs::remove_dir_all(&folder).unwrap();
    }

    #[test]
    fn temp_files_copied_others_stay() {
        let folder = folder("temp");
        let scratch = std::env::temp_dir().join(format!("notchbase-drag-{}.png", std::process::id()));
        std::fs::write(&scratch, PNG).unwrap();
        assert!(is_temporary(&scratch));
        let home = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml");
        assert!(!is_temporary(&home));
        let saved = save(vec![Dropped::File(scratch.clone()), Dropped::File(home.clone())], &folder);
        assert_eq!(saved.paths, [folder.join(scratch.file_name().unwrap()), home]);
        assert_eq!(std::fs::read(&saved.paths[0]).unwrap(), PNG);
        std::fs::remove_file(&scratch).unwrap();
        std::fs::remove_dir_all(&folder).unwrap();
    }

    #[test]
    fn temp_files_rescued_on_drop() {
        let folder = folder("rescue");
        let scratch = std::env::temp_dir().join(format!("notchbase-rescue-{}.png", std::process::id()));
        std::fs::write(&scratch, PNG).unwrap();
        let home = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml");
        let link = Dropped::Link("https://example.com/a.png".into());
        let items = vec![Dropped::File(scratch.clone()), Dropped::File(home.clone()), link.clone()];
        assert_eq!(rescue(items.clone(), None), items);
        let rescued = rescue(items, Some(&folder));
        std::fs::remove_file(&scratch).unwrap();
        assert_eq!(rescued, [Dropped::File(folder.join(scratch.file_name().unwrap())), Dropped::File(home), link]);
        std::fs::remove_dir_all(&folder).unwrap();
    }

    #[test]
    fn failed_links_are_explained() {
        let saved = save(vec![Dropped::Link("ftp://example.com/a.png".into())], &folder("links"));
        assert_eq!((saved.paths.len(), saved.problems.as_slice()), (0, [UNKNOWN_LINK.to_owned()].as_slice()));
    }

    #[test]
    fn links_fetch_like_a_browser() {
        let agent = agent();
        let config = agent.config();
        assert!(!config.https_only());
        assert!(config.http_status_as_error());
        assert!(config.max_redirects() > 0);
        assert_eq!(config.timeouts().global, Some(TIMEOUT));
        assert!(
            matches!(config.user_agent(), ureq::config::AutoHeaderValue::Provided(text) if text.starts_with("Mozilla/5.0 (compatible; notchbase/"))
        );
    }
}
