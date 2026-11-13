//! Reading links out of drags: which text is a link, the picture in dragged HTML, search-result wrappers, `data:`
//! and `file:` links, and names and extensions for what they hold.

use std::path::PathBuf;

const SCHEMES: [&str; 4] = ["http://", "https://", "data:", "file:"];
const WRAPPED_BY: [&str; 2] = ["imgurl", "mediaurl"];
const ENTITIES: [(&str, &str); 5] = [("&quot;", "\""), ("&#39;", "'"), ("&lt;", "<"), ("&gt;", ">"), ("&amp;", "&")];
const MIME_EXTENSIONS: [(&str, &str); 16] = [
    ("image/png", "png"),
    ("image/jpeg", "jpg"),
    ("image/jpg", "jpg"),
    ("image/gif", "gif"),
    ("image/webp", "webp"),
    ("image/svg+xml", "svg"),
    ("image/avif", "avif"),
    ("image/bmp", "bmp"),
    ("image/x-icon", "ico"),
    ("image/vnd.microsoft.icon", "ico"),
    ("image/heic", "heic"),
    ("image/tiff", "tiff"),
    ("application/pdf", "pdf"),
    ("video/mp4", "mp4"),
    ("video/webm", "webm"),
    ("audio/mpeg", "mp3"),
];

pub struct Inline {
    pub mime: String,
    pub bytes: Vec<u8>,
}

pub fn is_link(text: &str) -> bool {
    let text = text.trim();
    !text.contains(['\n', '\r']) && SCHEMES.iter().any(|scheme| starts_with_ignoring_case(text, scheme))
}

pub fn is_inline_or_file(link: &str) -> bool {
    let link = link.trim();
    starts_with_ignoring_case(link, "data:") || starts_with_ignoring_case(link, "file:")
}

pub fn first_link(text: &str) -> Option<String> {
    text.lines().map(str::trim).find(|line| !line.starts_with('#') && is_link(line)).map(str::to_owned)
}

pub fn image_source(html: &str) -> Option<String> {
    let lower = html.to_ascii_lowercase();
    let mut from = 0;
    while let Some(found) = lower[from..].find("<img") {
        let start = from + found;
        let end = lower[start..].find('>').map_or(lower.len(), |end| start + end);
        if let Some(source) = attribute(&html[start..end], &lower[start..end], "src").filter(|source| is_link(source)) {
            return Some(source);
        }
        from = end;
    }
    None
}

/// Google and Bing image results wrap the picture's address in `imgurl` or `mediaurl`, so take it out.
pub fn unwrap(url: &str) -> String {
    let url = url.trim();
    let query = url.split_once('?').map_or("", |(_, query)| query);
    let query = query.split('#').next().unwrap_or_default();
    query
        .split('&')
        .filter_map(|pair| pair.split_once('='))
        .filter(|(key, _)| WRAPPED_BY.contains(key))
        .map(|(_, value)| String::from_utf8_lossy(&percent_decode(value)).into_owned())
        .find(|inner| is_link(inner) && !inner.starts_with("file:"))
        .unwrap_or_else(|| url.to_owned())
}

/// A `file:` link as a path here. A host other than localhost makes a UNC path on Windows.
pub fn file_path(url: &str) -> Option<PathBuf> {
    let url = url.trim();
    if !starts_with_ignoring_case(url, "file:") {
        return None;
    }
    let after = &url[5..];
    let (host, path) = match after.strip_prefix("//") {
        Some(rest) if !rest.starts_with('/') => match rest.split_once('/') {
            Some((server, path)) if server.eq_ignore_ascii_case("localhost") => (false, path),
            _ => (true, rest),
        },
        _ => (false, after.trim_start_matches('/')),
    };
    let decoded = String::from_utf8_lossy(&percent_decode(path)).into_owned();
    Some(native_path(&decoded, host))
}

/// The last part of a link's path, decoded. Data links have no name.
pub fn name_of(url: &str) -> Option<String> {
    if starts_with_ignoring_case(url.trim(), "data:") {
        return None;
    }
    let path = url.trim().split(['?', '#']).next()?;
    let path = path.split_once("://").map_or(path, |(_, rest)| rest.split_once('/').map_or("", |(_, path)| path));
    let last = path.rsplit('/').next()?;
    let name = String::from_utf8_lossy(&percent_decode(last)).trim().to_owned();
    (!name.is_empty()).then_some(name)
}

pub fn inline(url: &str) -> Option<Inline> {
    let url = url.trim();
    if !starts_with_ignoring_case(url, "data:") {
        return None;
    }
    let (header, payload) = url[5..].split_once(',')?;
    let encoded = header.to_ascii_lowercase().ends_with(";base64");
    let mime = header.split(';').next().unwrap_or_default().trim().to_ascii_lowercase();
    let raw = percent_decode(payload);
    let bytes = if encoded { base64(&String::from_utf8_lossy(&raw))? } else { raw };
    Some(Inline { mime: if mime.is_empty() { "text/plain".to_owned() } else { mime }, bytes })
}

pub fn extension_for_mime(mime: &str) -> Option<&'static str> {
    let mime = mime.split(';').next().unwrap_or_default().trim().to_ascii_lowercase();
    MIME_EXTENSIONS.iter().find_map(|(known, extension)| (*known == mime).then_some(*extension))
}

fn attribute(tag: &str, lower: &str, name: &str) -> Option<String> {
    let bytes = lower.as_bytes();
    let mut from = 0;
    while let Some(found) = lower[from..].find(name) {
        let at = from + found;
        from = at + name.len();
        let separated = at > 0 && bytes[at - 1].is_ascii_whitespace();
        let rest = lower[from..].trim_start();
        if !separated || !rest.starts_with('=') {
            continue;
        }
        let value_at = lower.len() - rest[1..].trim_start().len();
        return Some(unescape(&quoted(&tag[value_at..])));
    }
    None
}

fn quoted(value: &str) -> String {
    match value.chars().next() {
        Some(quote @ ('"' | '\'')) => value[1..].split(quote).next().unwrap_or_default().to_owned(),
        _ => value.split(|c: char| c.is_whitespace() || c == '>').next().unwrap_or_default().to_owned(),
    }
}

fn unescape(text: &str) -> String {
    ENTITIES.iter().fold(text.to_owned(), |text, (entity, plain)| text.replace(entity, plain))
}

fn native_path(decoded: &str, host: bool) -> PathBuf {
    if !cfg!(windows) {
        return PathBuf::from(format!("/{decoded}"));
    }
    let backslashed = decoded.replace('/', "\\");
    if host { PathBuf::from(format!("\\\\{backslashed}")) } else { PathBuf::from(backslashed) }
}

fn percent_decode(text: &str) -> Vec<u8> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let hex = bytes.get(i + 1..i + 3).and_then(|pair| std::str::from_utf8(pair).ok());
        match (bytes[i], hex.and_then(|pair| u8::from_str_radix(pair, 16).ok())) {
            (b'%', Some(byte)) => {
                out.push(byte);
                i += 3;
            }
            (byte, _) => {
                out.push(byte);
                i += 1;
            }
        }
    }
    out
}

/// Standard or URL-safe base64, stopping at padding and skipping whitespace.
fn base64(text: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(text.len() * 3 / 4);
    let (mut buffer, mut bits) = (0u32, 0u32);
    for byte in text.bytes() {
        let value = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            b'=' => break,
            byte if byte.is_ascii_whitespace() => continue,
            _ => return None,
        };
        buffer = (buffer << 6) | u32::from(value);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buffer >> bits) as u8);
            buffer &= (1 << bits) - 1;
        }
    }
    Some(out)
}

fn starts_with_ignoring_case(text: &str, prefix: &str) -> bool {
    text.get(..prefix.len()).is_some_and(|start| start.eq_ignore_ascii_case(prefix))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn links_need_one_line_and_a_scheme() {
        assert!(is_link("https://example.com/a.png"));
        assert!(is_link("  DATA:image/png;base64,AAAA "));
        assert!(!is_link("javascript:alert(1)"));
        assert!(!is_link("https://a.com\nhttps://b.com"));
        assert!(!is_link("just some text"));
        assert_eq!(first_link("# comment\r\nhttps://a.com/x.jpg\r\nTitle").as_deref(), Some("https://a.com/x.jpg"));
    }

    #[test]
    fn first_image_source_in_html() {
        let html = r#"<html><body><!--StartFragment--><A HREF="/imgres?x=1"><IMG class=a SRC="https://i.example.com/cat.jpg?w=1&amp;h=2" alt=""></A>"#;
        assert_eq!(image_source(html).as_deref(), Some("https://i.example.com/cat.jpg?w=1&h=2"));
        assert_eq!(
            image_source("<img data-src='x' src='data:image/png;base64,AAA='>").as_deref(),
            Some("data:image/png;base64,AAA=")
        );
        assert_eq!(image_source("<img src=https://a.com/b.png width=3>").as_deref(), Some("https://a.com/b.png"));
        assert_eq!(image_source("<a href='https://a.com'>no image</a>"), None);
    }

    #[test]
    fn search_results_unwrap_to_image() {
        let google = "https://www.google.com/imgres?imgurl=https%3A%2F%2Fcdn.example.com%2Fdog.png&imgrefurl=https%3A%2F%2Fexample.com";
        assert_eq!(unwrap(google), "https://cdn.example.com/dog.png");
        let bing = "https://www.bing.com/images/search?view=detailV2&mediaurl=http%3A%2F%2Fx.org%2Fa.webp";
        assert_eq!(unwrap(bing), "http://x.org/a.webp");
        assert_eq!(
            unwrap("https://example.com/a.png?imgurl=file%3A%2F%2F%2Fetc"),
            "https://example.com/a.png?imgurl=file%3A%2F%2F%2Fetc"
        );
    }

    #[test]
    fn data_links_decode_to_bytes() {
        let png = inline("data:image/png;base64,iVBORw0KGgo=").unwrap();
        assert_eq!((png.mime.as_str(), png.bytes.as_slice()), ("image/png", b"\x89PNG\r\n\x1a\n".as_slice()));
        let svg = inline("data:image/svg+xml,%3Csvg%3E%3C/svg%3E").unwrap();
        assert_eq!(svg.bytes, b"<svg></svg>");
        assert_eq!(inline("data:,hi").unwrap().mime, "text/plain");
        assert!(inline("https://a.com").is_none());
        assert!(inline("data:image/png;base64,***").is_none());
    }

    #[test]
    fn names_come_from_last_segment() {
        assert_eq!(name_of("https://a.com/pics/My%20Cat.jpg?x=1#top").as_deref(), Some("My Cat.jpg"));
        assert_eq!(name_of("https://encrypted-tbn0.gstatic.com/images?q=tbn:abc").as_deref(), Some("images"));
        assert_eq!(name_of("https://a.com/"), None);
        assert_eq!(name_of("https://a.com"), None);
        assert_eq!(name_of("data:image/png;base64,AAAA"), None);
    }

    #[test]
    fn mime_types_map_to_extensions() {
        assert_eq!(extension_for_mime("image/JPEG; charset=binary"), Some("jpg"));
        assert_eq!(extension_for_mime("image/svg+xml"), Some("svg"));
        assert_eq!(extension_for_mime("text/html"), None);
    }

    #[test]
    fn file_links_become_paths() {
        assert!(file_path("https://a.com/x").is_none());
        let path = file_path("file:///C:/Users/me/My%20Pic.png").unwrap();
        if cfg!(windows) {
            assert_eq!(path, PathBuf::from(r"C:\Users\me\My Pic.png"));
            assert_eq!(file_path("file://server/share/a.png").unwrap(), PathBuf::from(r"\\server\share\a.png"));
        } else {
            assert_eq!(path, PathBuf::from("/C:/Users/me/My Pic.png"));
            assert_eq!(file_path("file:///Users/me/a.png").unwrap(), PathBuf::from("/Users/me/a.png"));
        }
    }

    #[test]
    fn base64_takes_both_alphabets() {
        assert_eq!(base64("aGVsbG8=").unwrap(), b"hello");
        assert_eq!(base64("aGVsbG8").unwrap(), b"hello");
        assert_eq!(base64("-_8=").unwrap(), [0xfb, 0xff]);
        assert_eq!(base64("aGVs\nbG8=").unwrap(), b"hello");
    }
}
