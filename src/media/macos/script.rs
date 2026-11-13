//! Runs AppleScript through `osascript`, and downloads Spotify's artwork over HTTPS.

use std::process::{Command, Stdio};
use std::time::Duration;

const OSASCRIPT: &str = "/usr/bin/osascript";
const LARGEST_DOWNLOAD: u64 = 8 << 20;
const TIMEOUT: Duration = Duration::from_secs(10);

pub fn run(script: &str) -> Option<String> {
    let output =
        Command::new(OSASCRIPT).arg("-e").arg(script).stdin(Stdio::null()).stderr(Stdio::null()).output().ok()?;
    output.status.success().then(|| String::from_utf8_lossy(&output.stdout).into_owned())
}

pub fn download(url: &str) -> Option<Vec<u8>> {
    let mut response = agent().get(url.trim()).call().ok()?;
    if response.status().as_u16() != 200 {
        return None;
    }
    response.body_mut().with_config().limit(LARGEST_DOWNLOAD).read_to_vec().ok()
}

fn agent() -> ureq::Agent {
    notchbase_net::config(TIMEOUT).https_only(true).build().into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn artwork_uses_https_and_ureq_agent() {
        let agent = agent();
        let config = agent.config();
        assert!(config.https_only());
        assert!(config.http_status_as_error());
        assert_eq!(config.timeouts().global, Some(TIMEOUT));
        assert!(matches!(config.user_agent(), ureq::config::AutoHeaderValue::Default));
    }
}
