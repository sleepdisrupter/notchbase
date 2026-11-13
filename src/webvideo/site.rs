//! Which addresses are videos, with or without the scheme an address bar hides. `recognize` knows the video pages
//! of a few big sites by their shape, and `recognize_any` also takes any other web page for yt-dlp to search.

const SCHEMES: [&str; 2] = ["https://", "http://"];
/// Prefixes that leave the site the same, like `www.` and the mobile and music hosts.
const HOST_PREFIXES: [&str; 4] = ["www.", "m.", "music.", "mobile."];
const YOUTUBE_ID: usize = 11;
const YOUTUBE_PATHS: [&str; 4] = ["shorts", "live", "embed", "v"];
const INSTAGRAM_KINDS: [&str; 4] = ["reel", "reels", "tv", "p"];
const INSTAGRAM_NOT_PAGES: [&str; 2] = ["share", "explore"];
const LONGEST_CODE: usize = 64;
const NOT_THE_WEB: [&str; 3] = ["localhost", "127.0.0.1", "[::1]"];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Site {
    YouTube,
    Instagram,
    Facebook,
    Vimeo,
    TikTok,
    X,
    Twitch,
    Reddit,
    Dailymotion,
    Other,
}

impl Site {
    pub fn name(self) -> &'static str {
        match self {
            Site::YouTube => "YouTube",
            Site::Instagram => "Instagram",
            Site::Facebook => "Facebook",
            Site::Vimeo => "Vimeo",
            Site::TikTok => "TikTok",
            Site::X => "X",
            Site::Twitch => "Twitch",
            Site::Reddit => "Reddit",
            Site::Dailymotion => "Dailymotion",
            Site::Other => "this page",
        }
    }

    /// YouTube is never signed in to, because it answers borrowed sign-ins by restricting the account.
    pub fn wants_sign_in(self) -> bool {
        self != Site::YouTube
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Page {
    pub site: Site,
    pub id: String,
    pub url: String,
}

struct Address<'a> {
    host: String,
    path: &'a str,
    query: &'a str,
    whole: String,
}

impl Address<'_> {
    fn segments(&self) -> Vec<&str> {
        self.path.split('/').filter(|segment| !segment.is_empty()).collect()
    }
}

/// Splits an address into its host, without `www.` and the like, its path and its query.
fn address(text: &str) -> Option<Address<'_>> {
    let text = text.trim();
    if text.is_empty() || text.contains(char::is_whitespace) {
        return None;
    }
    let schemed = SCHEMES.iter().find_map(|scheme| strip_prefix_ignoring_case(text, scheme));
    let rest = schemed.unwrap_or(text);
    let (host, rest) = rest.split_at(rest.find(['/', '?', '#']).unwrap_or(rest.len()));
    let full_host = host.rsplit('@').next()?.to_ascii_lowercase();
    let host = full_host.split(':').next()?;
    let named = host.contains('.') && host.bytes().all(|byte| byte.is_ascii_alphanumeric() || b".-".contains(&byte));
    if !named || host.starts_with('.') || host.ends_with('.') || (schemed.is_none() && text.contains("://")) {
        return None;
    }
    let rest = rest.split('#').next().unwrap_or_default();
    let (path, query) = rest.split_once('?').unwrap_or((rest, ""));
    let bare = HOST_PREFIXES.iter().fold(host, |host, prefix| host.strip_prefix(prefix).unwrap_or(host));
    Some(Address { host: bare.to_owned(), path, query, whole: format!("https://{full_host}{rest}") })
}

/// A video page on a known site, read from its address. YouTube and Instagram come back canonical, without the
/// playlist, timestamp or tracking, so one video is never fetched twice at once.
pub fn recognize(text: &str) -> Option<Page> {
    let address = address(text)?;
    let segments = address.segments();
    let known = |site: Site, is_video: bool| is_video.then(|| page(site, &address));
    match address.host.as_str() {
        "youtu.be" => youtube(segments.first()?),
        "youtube.com" | "youtube-nocookie.com" => match segments.as_slice() {
            ["watch", ..] => youtube(parameter(address.query, "v")?),
            [kind, id, ..] if YOUTUBE_PATHS.contains(kind) => youtube(id),
            _ => None,
        },
        "instagram.com" | "instagr.am" => instagram(&segments),
        "fb.watch" => known(Site::Facebook, !segments.is_empty()),
        "facebook.com" | "web.facebook.com" => {
            let video = ["watch", "reel", "videos", "video.php"].iter().any(|kind| segments.contains(kind));
            known(Site::Facebook, video || matches!(segments.as_slice(), ["share", "v" | "r", _, ..]))
        }
        "vimeo.com" | "player.vimeo.com" => {
            let numbered = |segment: &&str| !segment.is_empty() && segment.bytes().all(|byte| byte.is_ascii_digit());
            known(Site::Vimeo, segments.iter().any(numbered))
        }
        "tiktok.com" => known(Site::TikTok, matches!(segments.as_slice(), [_, "video", _, ..] | ["t", _, ..])),
        "vm.tiktok.com" | "vt.tiktok.com" => known(Site::TikTok, !segments.is_empty()),
        "x.com" | "twitter.com" => known(Site::X, matches!(segments.as_slice(), [_, "status", _, ..])),
        "twitch.tv" => known(Site::Twitch, matches!(segments.as_slice(), ["videos", _, ..] | [_, "clip", _, ..])),
        "clips.twitch.tv" => known(Site::Twitch, !segments.is_empty()),
        "reddit.com" | "old.reddit.com" => known(Site::Reddit, matches!(segments.as_slice(), ["r", _, "comments", ..])),
        "v.redd.it" => known(Site::Reddit, !segments.is_empty()),
        "dailymotion.com" => known(Site::Dailymotion, matches!(segments.as_slice(), ["video", _, ..])),
        "dai.ly" => known(Site::Dailymotion, !segments.is_empty()),
        _ => None,
    }
}

/// Any web page that may have a video. YouTube and Instagram pages that aren't a single video are left out.
pub fn recognize_any(text: &str) -> Option<Page> {
    if let Some(page) = recognize(text) {
        return Some(page);
    }
    let address = address(text)?;
    let strict = ["youtube.com", "youtu.be", "youtube-nocookie.com", "instagram.com", "instagr.am"];
    if strict.contains(&address.host.as_str()) || NOT_THE_WEB.contains(&address.host.as_str()) {
        return None;
    }
    Some(page(Site::Other, &address))
}

fn page(site: Site, address: &Address<'_>) -> Page {
    Page { site, id: address.whole.clone(), url: address.whole.clone() }
}

fn youtube(id: &str) -> Option<Page> {
    (id.len() == YOUTUBE_ID && id.bytes().all(is_code)).then(|| Page {
        site: Site::YouTube,
        id: id.to_owned(),
        url: format!("https://www.youtube.com/watch?v={id}"),
    })
}

fn instagram(segments: &[&str]) -> Option<Page> {
    if segments.first().is_some_and(|first| INSTAGRAM_NOT_PAGES.contains(first)) {
        return None;
    }
    let at = segments.iter().take(2).position(|segment| INSTAGRAM_KINDS.contains(segment))?;
    let (kind, id) = (segments[at], *segments.get(at + 1)?);
    if id.is_empty() || id.len() > LONGEST_CODE || !id.bytes().all(is_code) || (kind == "reels" && id == "audio") {
        return None;
    }
    let kind = if kind == "p" { "p" } else { "reel" };
    let url = format!("https://www.instagram.com/{kind}/{id}/");
    Some(Page { site: Site::Instagram, id: id.to_owned(), url })
}

fn parameter<'a>(query: &'a str, name: &str) -> Option<&'a str> {
    query.split('&').filter_map(|pair| pair.split_once('=')).find_map(|(key, value)| (key == name).then_some(value))
}

fn is_code(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_'
}

fn strip_prefix_ignoring_case<'a>(text: &'a str, prefix: &str) -> Option<&'a str> {
    text.get(..prefix.len()).filter(|start| start.eq_ignore_ascii_case(prefix)).map(|_| &text[prefix.len()..])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(text: &str) -> Option<(Site, String)> {
        recognize(text).map(|page| (page.site, page.id))
    }

    fn youtube(id: &str) -> Option<(Site, String)> {
        Some((Site::YouTube, id.to_owned()))
    }

    fn instagram(id: &str) -> Option<(Site, String)> {
        Some((Site::Instagram, id.to_owned()))
    }

    #[test]
    fn youtube_shapes_name_the_same_video() {
        for address in [
            "https://www.youtube.com/watch?v=jNQXAC9IVRw",
            "https://www.youtube.com/watch?list=PL123&v=jNQXAC9IVRw&t=42s#comments",
            "https://youtu.be/jNQXAC9IVRw?si=abc",
            "https://www.youtube.com/shorts/jNQXAC9IVRw",
            "https://www.youtube.com/live/jNQXAC9IVRw?feature=share",
            "https://www.youtube.com/embed/jNQXAC9IVRw",
            "https://m.youtube.com/watch?v=jNQXAC9IVRw",
            "https://music.youtube.com/watch?v=jNQXAC9IVRw",
            "HTTPS://WWW.YouTube.com/watch?v=jNQXAC9IVRw",
            "  https://www.youtube.com/watch?v=jNQXAC9IVRw\n",
        ] {
            assert_eq!(id(address), youtube("jNQXAC9IVRw"), "{address}");
        }
        assert_eq!(
            recognize("https://youtu.be/jNQXAC9IVRw").unwrap().url,
            "https://www.youtube.com/watch?v=jNQXAC9IVRw",
            "the list, the time and the tracking are left behind"
        );
    }

    #[test]
    fn hidden_scheme_still_reads() {
        assert_eq!(id("youtube.com/watch?v=jNQXAC9IVRw"), youtube("jNQXAC9IVRw"), "Chrome");
        assert_eq!(id("www.youtube.com/watch?v=jNQXAC9IVRw"), youtube("jNQXAC9IVRw"), "Firefox");
        assert_eq!(id("instagram.com/reel/DAbc_12-x/"), instagram("DAbc_12-x"));
    }

    #[test]
    fn reels_posts_tv_are_instagram() {
        assert_eq!(id("https://www.instagram.com/reel/DAbc_12-x/?igsh=xyz"), instagram("DAbc_12-x"));
        assert_eq!(id("https://www.instagram.com/reels/DAbc_12-x/"), instagram("DAbc_12-x"));
        assert_eq!(id("https://www.instagram.com/someone/reel/DAbc_12-x/"), instagram("DAbc_12-x"));
        assert_eq!(id("https://www.instagram.com/tv/DAbc_12-x"), instagram("DAbc_12-x"));
        assert_eq!(recognize("https://instagram.com/reels/DAbc/").unwrap().url, "https://www.instagram.com/reel/DAbc/");
        assert_eq!(recognize("https://www.instagram.com/p/DAbc/").unwrap().url, "https://www.instagram.com/p/DAbc/");
    }

    #[test]
    fn non_video_pages_are_left_alone() {
        for address in [
            "",
            "https://www.youtube.com/",
            "https://www.youtube.com/feed/subscriptions",
            "https://www.youtube.com/@someone/videos",
            "https://www.youtube.com/playlist?list=PL123",
            "https://www.youtube.com/watch?v=short",
            "https://www.youtube.com/results?search_query=jNQXAC9IVRw",
            "https://www.instagram.com/",
            "https://www.instagram.com/someone/",
            "https://www.instagram.com/reels/",
            "https://www.instagram.com/reels/audio/123/",
            "https://www.instagram.com/share/reel/BAbc",
            "https://www.instagram.com/explore/tags/reel/x",
            "https://example.com/watch?v=jNQXAC9IVRw",
            "https://notyoutube.com/watch?v=jNQXAC9IVRw",
            "how to download youtube.com/watch?v=jNQXAC9IVRw",
            "file:///Users/me/youtube.com/watch",
        ] {
            assert_eq!(recognize(address), None, "{address}");
        }
    }

    #[test]
    fn other_video_pages_known_by_shape() {
        for (address, site) in [
            ("https://vimeo.com/76979871", Site::Vimeo),
            ("https://vimeo.com/channels/staffpicks/76979871", Site::Vimeo),
            ("https://www.facebook.com/watch/?v=10153231379946729", Site::Facebook),
            ("https://www.facebook.com/reel/1234567890", Site::Facebook),
            ("https://www.facebook.com/someone/videos/1234567890/", Site::Facebook),
            ("https://fb.watch/abc123/", Site::Facebook),
            ("https://www.tiktok.com/@someone/video/7123456789012345678", Site::TikTok),
            ("https://x.com/someone/status/1234567890", Site::X),
            ("https://twitter.com/someone/status/1234567890?s=20", Site::X),
            ("https://www.twitch.tv/videos/123456789", Site::Twitch),
            ("https://clips.twitch.tv/SomeCleverClipName", Site::Twitch),
            ("https://www.reddit.com/r/videos/comments/abc123/a_title/", Site::Reddit),
            ("https://www.dailymotion.com/video/x8abcde", Site::Dailymotion),
        ] {
            assert_eq!(recognize(address).map(|page| page.site), Some(site), "{address}");
        }
        assert_eq!(recognize("https://vimeo.com/"), None);
        assert_eq!(recognize("https://www.facebook.com/someone"), None);
        assert_eq!(recognize("https://x.com/someone"), None);
        let page = recognize("vimeo.com/76979871#t=10").unwrap();
        assert_eq!(page.url, "https://vimeo.com/76979871", "the scheme is put back and the fragment dropped");
    }

    #[test]
    fn any_other_web_page_is_worth_a_look() {
        let page = recognize_any("https://www.example.com/watch?v=abc123&t=4").unwrap();
        assert_eq!((page.site, page.url.as_str()), (Site::Other, "https://www.example.com/watch?v=abc123&t=4"));
        assert_eq!(recognize_any("example.com/clip").unwrap().url, "https://example.com/clip");
        assert_eq!(recognize_any("https://x.com/someone").map(|page| page.site), Some(Site::Other));
        assert_eq!(recognize_any("https://youtu.be/jNQXAC9IVRw").map(|page| page.site), Some(Site::YouTube));
        for address in [
            "https://www.youtube.com/feed/subscriptions",
            "https://www.youtube.com/playlist?list=PL123",
            "https://www.instagram.com/someone/",
            "http://localhost:3000/video",
            "http://127.0.0.1:8080/",
            "about:blank",
            "chrome://settings",
            "file:///Users/me/video.html",
            "moz-extension://abc/page.html",
            "how to download a video",
            "justaword",
            "",
        ] {
            assert_eq!(recognize_any(address), None, "{address}");
        }
        assert!(Site::Vimeo.wants_sign_in() && Site::Other.wants_sign_in() && !Site::YouTube.wants_sign_in());
    }
}
