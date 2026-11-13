//! Downloads calendar links and keeps a copy for when the network is down. It sends the last ETag,
//! so an unchanged calendar costs almost nothing.

use std::path::{Path, PathBuf};
use std::time::Duration;

use ureq::tls::{RootCerts, TlsConfig, TlsProvider};

use crate::ics::Feed;

const TIMEOUT: Duration = Duration::from_secs(20);
const LARGEST: u64 = 32 * 1024 * 1024;
const USER_AGENT: &str = concat!("notchbase-calendar/", env!("CARGO_PKG_VERSION"));
const NOT_A_LINK: &str = "Paste the whole address, starting with https://";
const UNREACHABLE: &str = "Couldn't reach it";
const TOO_BIG: &str = "That calendar is too big";
const GONE: &str = "The link wasn't found. It may have been reset";
const REFUSED: &str = "The link was refused";
const NOT_MODIFIED: u16 = 304;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Tag {
    etag: Option<String>,
    modified: Option<String>,
}

pub fn load(link: &str, cache: Option<&Path>, tag: &Tag) -> Result<Option<(Feed, Tag)>, String> {
    let Some((text, tag)) = download(link, tag)? else { return Ok(None) };
    let feed = Feed::parse(&text)?;
    if let Some(file) = cache.map(|dir| file_for(dir, link)) {
        let _ = std::fs::create_dir_all(file.parent().unwrap_or(Path::new(".")));
        let _ = std::fs::write(file, text);
    }
    Ok(Some((feed, tag)))
}

pub fn remembered(link: &str, cache: Option<&Path>) -> Result<Feed, String> {
    let file = cache.map(|dir| file_for(dir, link)).ok_or_else(String::new)?;
    let text = std::fs::read_to_string(file).map_err(|err| err.to_string())?;
    Feed::parse(&text)
}

fn download(link: &str, tag: &Tag) -> Result<Option<(String, Tag)>, String> {
    if !link.starts_with("https://") && !link.starts_with("http://") {
        return Err(NOT_A_LINK.to_owned());
    }
    let mut request = agent().get(link);
    if let Some(etag) = &tag.etag {
        request = request.header("If-None-Match", etag);
    }
    if let Some(modified) = &tag.modified {
        request = request.header("If-Modified-Since", modified);
    }
    let mut response = match request.call() {
        Err(ureq::Error::StatusCode(NOT_MODIFIED)) => return Ok(None),
        other => other.map_err(|err| described(&err))?,
    };
    if response.status().as_u16() == NOT_MODIFIED {
        return Ok(None);
    }
    let header = |name: &str| response.headers().get(name).and_then(|value| value.to_str().ok()).map(str::to_owned);
    let tag = Tag { etag: header("etag"), modified: header("last-modified") };
    let text = response.body_mut().with_config().limit(LARGEST).read_to_string().map_err(|err| described(&err))?;
    Ok(Some((text, tag)))
}

fn agent() -> ureq::Agent {
    let tls = TlsConfig::builder().provider(TlsProvider::NativeTls).root_certs(RootCerts::PlatformVerifier).build();
    ureq::Agent::config_builder().timeout_global(Some(TIMEOUT)).user_agent(USER_AGENT).tls_config(tls).build().into()
}

fn described(err: &ureq::Error) -> String {
    match err {
        ureq::Error::StatusCode(404 | 410) => GONE.to_owned(),
        ureq::Error::StatusCode(401 | 403) => REFUSED.to_owned(),
        ureq::Error::StatusCode(status) => format!("The calendar didn't load (HTTP {status})"),
        ureq::Error::BodyExceedsLimit(_) => TOO_BIG.to_owned(),
        _ => UNREACHABLE.to_owned(),
    }
}

fn file_for(dir: &Path, link: &str) -> PathBuf {
    let hash = link
        .bytes()
        .fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3));
    dir.join("feeds").join(format!("{hash:016x}.ics"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_link_has_a_cache_file() {
        let dir = Path::new("cache");
        let a = file_for(dir, "https://calendar.google.com/a/basic.ics");
        assert_ne!(a, file_for(dir, "https://calendar.google.com/b/basic.ics"));
        assert_eq!(a, file_for(dir, "https://calendar.google.com/a/basic.ics"));
        assert!(a.starts_with(dir.join("feeds")));
    }

    #[test]
    fn only_web_addresses_are_fetched() {
        assert_eq!(load("calendar.google.com/basic.ics", None, &Tag::default()), Err(NOT_A_LINK.to_owned()));
    }

    #[test]
    fn saved_copy_is_read_back() {
        let dir = std::env::temp_dir().join(format!("notchbase-calendar-{}", std::process::id()));
        let link = "https://example.com/team.ics";
        let file = file_for(&dir, link);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(&file, "BEGIN:VCALENDAR\nX-WR-CALNAME:Team\nEND:VCALENDAR").unwrap();
        assert_eq!(remembered(link, Some(&dir)).unwrap().name, "Team");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
