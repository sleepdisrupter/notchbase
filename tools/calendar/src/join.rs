//! Finds the join link in an event and names its service, like Zoom, Meet or Teams.

use notchbase_sdk::prelude::*;

const SERVICES: [(&str, &[&str], &str); 12] = [
    ("meet.google.com", &["/"], MEET),
    ("zoom.us", &["/j/", "/my/", "/w/", "/wc/", "/s/"], ZOOM),
    ("zoom.com", &["/j/", "/my/", "/w/", "/wc/", "/s/"], ZOOM),
    ("zoomgov.com", &["/j/", "/my/", "/w/", "/wc/", "/s/"], ZOOM),
    ("teams.microsoft.com", &["/l/meetup-join", "/meet/"], TEAMS),
    ("teams.live.com", &["/meet/"], TEAMS),
    ("webex.com", &["/meet/", "/join/", "/wbxmjs/", "/j.php"], WEBEX),
    ("whereby.com", &["/"], WHEREBY),
    ("around.co", &["/"], AROUND),
    ("meet.jit.si", &["/"], JITSI),
    ("chime.aws", &["/"], CHIME),
    ("facetime.apple.com", &["/join"], FACETIME),
];
const MEET: &str = "Meet";
const ZOOM: &str = "Zoom";
const TEAMS: &str = "Teams";
const WEBEX: &str = "Webex";
const WHEREBY: &str = "Whereby";
const AROUND: &str = "Around";
const JITSI: &str = "Jitsi";
const CHIME: &str = "Chime";
const FACETIME: &str = "FaceTime";
const SCHEMES: [(&str, &str); 5] = [
    ("zoommtg:", ZOOM),
    ("zoomus:", ZOOM),
    ("msteams:", TEAMS),
    ("facetime:", FACETIME),
    ("facetime-audio:", FACETIME),
];
const OPEN: &str = "Open";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Service {
    pub name: &'static str,
    pub color: Color,
    pub icon: Icon,
}

pub fn service(link: &str) -> Service {
    let lower = link.trim().to_ascii_lowercase();
    let named = SCHEMES
        .iter()
        .find(|(scheme, _)| lower.starts_with(scheme))
        .map(|(_, name)| *name)
        .or_else(|| meeting_service(&lower));
    match named {
        Some(name) => Service { name, color: accent(name), icon: Icon::Video },
        None => Service { name: OPEN, color: Tone::Gray.color(), icon: Icon::Link },
    }
}

pub fn direct(text: &str) -> Option<String> {
    let text = text.trim();
    if text.is_empty() || text.contains(char::is_whitespace) {
        return None;
    }
    let lower = text.to_ascii_lowercase();
    let schemed = lower.contains("://") || SCHEMES.iter().any(|(scheme, _)| lower.starts_with(scheme));
    match schemed {
        true => Some(text.to_owned()),
        false => text.contains('.').then(|| format!("https://{text}")),
    }
}

fn accent(name: &str) -> Color {
    match name {
        ZOOM => Color::rgb(45, 140, 255),
        MEET => Color::rgb(0, 137, 90),
        TEAMS => Color::rgb(91, 95, 199),
        WEBEX => Color::rgb(0, 160, 209),
        WHEREBY => Color::rgb(99, 75, 230),
        AROUND => Color::rgb(236, 88, 64),
        JITSI => Color::rgb(23, 116, 196),
        CHIME => Color::rgb(236, 138, 25),
        FACETIME => Color::rgb(52, 199, 89),
        _ => Tone::Gray.color(),
    }
}

const ENDS: [char; 9] = ['<', '>', '"', '\'', ')', ']', '|', '\\', '`'];

pub fn find(texts: &[&str]) -> Option<String> {
    texts.iter().find_map(|text| links(text).find(|link| is_meeting(link)))
}

fn links(text: &str) -> impl Iterator<Item = String> + '_ {
    text.match_indices("http").filter_map(|(at, _)| {
        let rest = &text[at..];
        let rest = rest.strip_prefix("https://").or_else(|| rest.strip_prefix("http://"))?;
        let end = rest.find(|c: char| c.is_whitespace() || ENDS.contains(&c)).unwrap_or(rest.len());
        let tail = rest[..end].trim_end_matches(['.', ',', ';', ':', '!', '?']);
        (!tail.is_empty()).then(|| format!("https://{}", tail.replace("&amp;", "&")))
    })
}

fn is_meeting(link: &str) -> bool {
    meeting_service(link).is_some()
}

fn meeting_service(link: &str) -> Option<&'static str> {
    let rest = link.trim_start_matches("https://").trim_start_matches("http://");
    let (host, path) = rest.split_at(rest.find(['/', '?', '#']).unwrap_or(rest.len()));
    let host = host.to_ascii_lowercase();
    let path = path.to_ascii_lowercase();
    SERVICES.iter().find_map(|(service, paths, name)| {
        let matches_host = host == *service || host.ends_with(&format!(".{service}"));
        (matches_host && paths.iter().any(|wanted| path.starts_with(wanted) && path.len() > 1)).then_some(*name)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_meeting_links() {
        let google = "Join with Google Meet: https://meet.google.com/abc-defg-hij\nLearn more: https://support.google.com/a/users/answer/9282720";
        assert_eq!(find(&[google]).as_deref(), Some("https://meet.google.com/abc-defg-hij"));
        let teams = "<a href=\"https://teams.microsoft.com/l/meetup-join/19%3ameeting_abc%40thread.v2/0?context=%7b%7d&amp;x=1\">Join</a>";
        assert_eq!(
            find(&[teams]).as_deref(),
            Some("https://teams.microsoft.com/l/meetup-join/19%3ameeting_abc%40thread.v2/0?context=%7b%7d&x=1")
        );
        let zoom = "Zoom: https://us02web.zoom.us/j/8123456789?pwd=Xyz. Download Zoom at https://zoom.us/download";
        assert_eq!(find(&[zoom]).as_deref(), Some("https://us02web.zoom.us/j/8123456789?pwd=Xyz"));
    }

    #[test]
    fn earlier_fields_win() {
        let url = "https://meet.google.com/aaa-bbbb-ccc";
        let notes = "https://acme.zoom.us/j/1";
        assert_eq!(find(&["", url, notes]).as_deref(), Some(url));
        assert_eq!(find(&["https://zoom.us/", "https://docs.google.com/document/d/1", "Room 4"]), None);
        assert_eq!(find(&["https://meet.google.com"]), None);
    }

    #[test]
    fn services_have_names_and_colors() {
        let zoom = service("https://us02web.zoom.us/j/8123456789");
        assert_eq!((zoom.name, zoom.icon), ("Zoom", Icon::Video));
        assert_eq!(service("https://meet.google.com/abc-defg-hij").name, "Meet");
        assert_eq!(service("https://teams.microsoft.com/l/meetup-join/19").name, "Teams");
        assert_eq!(service("https://facetime.apple.com/join#v=1&p=abc").name, "FaceTime");
        assert_eq!(service("facetime://someone@icloud.com").name, "FaceTime");
        assert_eq!(service("zoommtg://zoom.us/join?confno=1").name, "Zoom");
        assert_ne!(service("https://zoom.us/j/1").color, service("https://meet.google.com/a").color);
        let other = service("https://example.com/party");
        assert_eq!((other.name, other.icon), ("Open", Icon::Link));
    }

    #[test]
    fn typed_link_is_kept() {
        assert_eq!(direct(" zoom.us/j/123 ").as_deref(), Some("https://zoom.us/j/123"));
        assert_eq!(direct("https://example.com/a").as_deref(), Some("https://example.com/a"));
        assert_eq!(direct("facetime:someone@icloud.com").as_deref(), Some("facetime:someone@icloud.com"));
        assert_eq!(direct("Room 4"), None);
        assert_eq!(direct(""), None);
    }
}
