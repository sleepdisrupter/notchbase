//! What tools reach outside notchbase through: HTTP requests, and reading and writing text files.

use std::path::{Path, PathBuf};
use std::time::Duration;

use notchbase_sdk::protocol::{HttpRequest, HttpResponse};

const USER_AGENT: &str = notchbase_net::user_agent!();
const TIMEOUT: Duration = Duration::from_secs(30);
const LONGEST: Duration = Duration::from_secs(300);
const LARGEST: u64 = 16 * 1024 * 1024;
const HOME: &str = "~/";

pub fn fetch(request: &HttpRequest) -> Result<HttpResponse, String> {
    let agent = agent(timeout_for(request));
    let mut outgoing = ureq::http::Request::builder().method(request.method.as_str()).uri(request.url.as_str());
    for (name, value) in &request.headers {
        outgoing = outgoing.header(name.as_str(), value.as_str());
    }
    let outgoing = outgoing.body(request.body.clone().unwrap_or_default()).map_err(|err| err.to_string())?;
    let mut response = agent.run(outgoing).map_err(|err| describe(&err))?;
    let status = response.status().as_u16();
    let body = response.body_mut().with_config().limit(LARGEST).read_to_string().map_err(|err| describe(&err))?;
    Ok(HttpResponse { status, body })
}

/// 30 seconds unless the tool asks otherwise, and never more than 5 minutes.
fn timeout_for(request: &HttpRequest) -> Duration {
    request.timeout_ms.map_or(TIMEOUT, Duration::from_millis).min(LONGEST)
}

/// Every status comes back to the tool, errors included, so it decides what a 404 means.
fn agent(timeout: Duration) -> ureq::Agent {
    notchbase_net::config(timeout).user_agent(USER_AGENT).http_status_as_error(false).build().into()
}

pub fn read(path: &Path) -> Result<String, String> {
    std::fs::read_to_string(path).map_err(|err| format!("Couldn't read {}: {err}", path.display()))
}

pub fn write(path: &Path, text: &str) -> Result<String, String> {
    let failed = |err: std::io::Error| format!("Couldn't write {}: {err}", path.display());
    if let Some(folder) = path.parent() {
        std::fs::create_dir_all(folder).map_err(failed)?;
    }
    std::fs::write(path, text).map_err(failed)?;
    Ok(path.to_string_lossy().into_owned())
}

/// Where a tool's path points: `~/` is home, an absolute path stays, and the rest is in the tool's data folder.
pub fn resolve(data: &Path, home: Option<&Path>, path: &str) -> PathBuf {
    match (path.strip_prefix(HOME), home) {
        (Some(rest), Some(home)) => home.join(rest),
        _ if Path::new(path).is_absolute() => PathBuf::from(path),
        _ => data.join(path),
    }
}

fn describe(err: &ureq::Error) -> String {
    match err {
        ureq::Error::Timeout(_) => "It took too long to answer".into(),
        ureq::Error::HostNotFound | ureq::Error::ConnectionFailed | ureq::Error::Io(_) => "Can't reach it".into(),
        ureq::Error::BodyExceedsLimit(_) => "The answer is too big".into(),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_name_lands_in_tool_folder() {
        let (data, home) = (Path::new("/data/tools/notes"), Path::new("/Users/me"));
        assert_eq!(resolve(data, Some(home), "list.json"), data.join("list.json"));
        assert_eq!(resolve(data, Some(home), "cache/a.txt"), data.join("cache").join("a.txt"));
        assert_eq!(resolve(data, Some(home), "~/Documents/a.md"), home.join("Documents").join("a.md"));
        assert_eq!(
            resolve(data, None, "~/a.md"),
            data.join("~/a.md"),
            "without a home there is nowhere else to put it"
        );
        let anywhere = std::env::temp_dir().join("a.txt");
        assert_eq!(resolve(data, Some(home), &anywhere.to_string_lossy()), anywhere);
    }

    #[test]
    fn write_makes_folders_and_reads_back() {
        let file =
            std::env::temp_dir().join(format!("notchbase-outside-{}", std::process::id())).join("deep").join("a.txt");
        assert_eq!(write(&file, "kept").as_deref(), Ok(file.to_string_lossy().as_ref()));
        assert_eq!(read(&file).as_deref(), Ok("kept"));
        let _ = std::fs::remove_dir_all(file.parent().and_then(Path::parent).unwrap());
        assert!(read(&file).unwrap_err().starts_with("Couldn't read"));
    }

    #[test]
    fn bad_address_fails_without_network() {
        assert!(fetch(&HttpRequest::get("not an address")).is_err());
        assert!(fetch(&HttpRequest::new("NOT A METHOD", "https://example.com")).is_err());
    }

    fn provided(value: &ureq::config::AutoHeaderValue) -> Option<&str> {
        match value {
            ureq::config::AutoHeaderValue::Provided(text) => Some(text.as_str()),
            _ => None,
        }
    }

    #[test]
    fn default_wait_is_thirty_seconds() {
        assert_eq!(timeout_for(&HttpRequest::get("https://example.com")), TIMEOUT);
        let quick = HttpRequest::get("https://example.com").timeout(Duration::from_millis(1500));
        assert_eq!(timeout_for(&quick), Duration::from_millis(1500));
        let none = HttpRequest::get("https://example.com").timeout(Duration::ZERO);
        assert_eq!(timeout_for(&none), Duration::ZERO);
    }

    #[test]
    fn no_wait_beyond_five_minutes() {
        let slow = HttpRequest::get("https://example.com").timeout(Duration::from_secs(3600));
        assert_eq!(timeout_for(&slow), LONGEST);
        let endless = HttpRequest { timeout_ms: Some(u64::MAX), ..HttpRequest::get("https://example.com") };
        assert_eq!(timeout_for(&endless), LONGEST);
    }

    #[test]
    fn plain_http_sees_every_status() {
        let agent = agent(TIMEOUT);
        let config = agent.config();
        assert!(!config.https_only());
        assert!(!config.http_status_as_error());
        assert!(config.max_redirects() > 0);
        assert_eq!(config.timeouts().global, Some(TIMEOUT));
        assert_eq!(provided(config.user_agent()), Some(USER_AGENT));
        assert!(USER_AGENT.starts_with("notchbase/"));
    }

    #[test]
    fn network_trouble_reads_as_sentence() {
        assert_eq!(describe(&ureq::Error::Timeout(ureq::Timeout::Global)), "It took too long to answer");
        assert_eq!(describe(&ureq::Error::HostNotFound), "Can't reach it");
        assert_eq!(describe(&ureq::Error::ConnectionFailed), "Can't reach it");
        assert_eq!(describe(&ureq::Error::Io(std::io::ErrorKind::BrokenPipe.into())), "Can't reach it");
        assert_eq!(describe(&ureq::Error::BodyExceedsLimit(LARGEST)), "The answer is too big");
        let other = ureq::Error::BadUri("nowhere".into());
        assert_eq!(describe(&other), other.to_string());
    }
}
