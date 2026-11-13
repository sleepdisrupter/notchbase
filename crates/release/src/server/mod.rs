//! Finds the newest release on GitHub and downloads one, checking its hash.
//!
//! Every GitHub release carries a `latest.json` that lists a build for each platform. It is read as a plain download
//! from `releases/latest/download`, never through the GitHub API, which allows only 60 requests an hour without a
//! login.

use std::collections::BTreeMap;
use std::fs::File;
use std::io::Read;
use std::path::Path;
use std::time::Duration;

use notchbase_net::{CopyError, Limits, Mode};
use serde::Deserialize;

use crate::Release;

/// The repository whose releases this build updates from, unless `NOTCHBASE_RELEASES_REPO` names another at build
/// time.
pub const REPO: &str = "sleepdisrupter/notchbase";
/// The file every release carries that lists its builds.
pub const MANIFEST: &str = "latest.json";
const CHECK_TIMEOUT: Duration = Duration::from_secs(20);
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(15 * 60);
const LARGEST: u64 = 512 * 1024 * 1024;
const LARGEST_MANIFEST: u64 = 1024 * 1024;
const NOT_FOUND: u16 = 404;
const PART: &str = "part";
const UNEXPECTED: &str = "The update server sent something unexpected";
const MISMATCH: &str = "The update didn't match its checksum";
const INTERRUPTED: &str = "The download was interrupted";

/// One release's `latest.json`: its version and a build for each platform it has one for.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct Manifest {
    pub version: String,
    #[serde(default)]
    pub builds: BTreeMap<String, Build>,
}

/// One platform's build in `latest.json`.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct Build {
    pub url: String,
    pub sha256: String,
    #[serde(default)]
    pub size: Option<u64>,
}

impl Manifest {
    /// The release for `platform`, or nothing when this release has no build for it yet.
    pub fn release_for(mut self, platform: &str) -> Vec<Release> {
        let build = self.builds.remove(platform);
        build
            .map(|build| Release { version: self.version, url: build.url, sha256: build.sha256, size: build.size })
            .into_iter()
            .collect()
    }
}

#[derive(Clone, Debug)]
pub struct Server {
    manifest: String,
    user_agent: &'static str,
}

impl Server {
    /// Releases from `repo`, written as `owner/name`.
    pub fn new(repo: &str, user_agent: &'static str) -> Self {
        let repo = repo.trim().trim_matches('/');
        Self::at(&format!("https://github.com/{repo}/releases/latest/download/{MANIFEST}"), user_agent)
    }

    /// Releases listed by the `latest.json` at `manifest`.
    pub fn at(manifest: &str, user_agent: &'static str) -> Self {
        Self { manifest: manifest.to_owned(), user_agent }
    }

    /// Releases from the repository this build was made for.
    pub fn built_in(user_agent: &'static str) -> Self {
        Self::new(built_in_repo(), user_agent)
    }

    /// The newest release's build for `platform`. A repository with no release yet, or a release with no build for
    /// this platform, has nothing to offer, which is not a failure.
    pub fn releases(&self, platform: &str) -> Result<Vec<Release>, String> {
        let request = agent(CHECK_TIMEOUT, self.user_agent).get(&self.manifest);
        let mut response = match request.call() {
            Ok(response) => response,
            Err(ureq::Error::StatusCode(NOT_FOUND)) => return Ok(Vec::new()),
            Err(err) => return Err(describe(&err)),
        };
        let manifest: Manifest =
            response.body_mut().with_config().limit(LARGEST_MANIFEST).read_json().map_err(|_| UNEXPECTED.to_owned())?;
        Ok(manifest.release_for(platform))
    }
}

/// `NOTCHBASE_RELEASES_REPO` from the build, so a fork can update from its own releases, or notchbase's own.
pub fn built_in_repo() -> &'static str {
    match option_env!("NOTCHBASE_RELEASES_REPO") {
        Some(repo) if !repo.trim().is_empty() => repo,
        _ => REPO,
    }
}

pub fn download(release: &Release, to: &Path, user_agent: &str, progress: impl FnMut(f32)) -> Result<(), String> {
    let mut response = agent(DOWNLOAD_TIMEOUT, user_agent).get(&release.url).call().map_err(|err| describe(&err))?;
    let length = response.headers().get("content-length").and_then(|value| value.to_str().ok()?.parse().ok());
    let total = expected_size(release.size, length);
    place(response.body_mut().with_config().limit(LARGEST).reader(), release, to, total, progress)
}

fn expected_size(listed: Option<u64>, length: Option<u64>) -> Option<u64> {
    listed.or(length).filter(|total| *total > 0)
}

fn place(
    body: impl Read,
    release: &Release,
    to: &Path,
    total: Option<u64>,
    mut progress: impl FnMut(f32),
) -> Result<(), String> {
    // Download to a .part file and only rename it once the checksum matches.
    let part = to.with_extension(PART);
    let saved = save(body, &part, total, &mut progress);
    let checked =
        saved.and_then(
            |digest| {
                if digest.eq_ignore_ascii_case(release.sha256.trim()) { Ok(()) } else { Err(MISMATCH.into()) }
            },
        );
    let finished = checked.and_then(|()| std::fs::rename(&part, to).map_err(|err| err.to_string()));
    if finished.is_err() {
        let _ = std::fs::remove_file(&part);
    }
    finished
}

fn save(body: impl Read, to: &Path, total: Option<u64>, progress: &mut impl FnMut(f32)) -> Result<String, String> {
    let mut file = File::create(to).map_err(|err| err.to_string())?;
    let digest = notchbase_net::copy_hashed(body, &mut file, Limits::default(), |done| {
        if let Some(total) = total {
            progress(notchbase_net::fraction(done, total));
        }
    })
    .map_err(|err| match err {
        CopyError::Write(err) => err.to_string(),
        CopyError::Read(_) | CopyError::TooLarge | CopyError::Stopped => INTERRUPTED.to_owned(),
    })?;
    file.sync_all().map_err(|err| err.to_string())?;
    Ok(digest)
}

fn agent(timeout: Duration, user_agent: &str) -> ureq::Agent {
    notchbase_net::agent(timeout, user_agent, Mode::Follow)
}

fn describe(err: &ureq::Error) -> String {
    match err {
        ureq::Error::Timeout(_) => "The update server took too long".into(),
        ureq::Error::HostNotFound | ureq::Error::ConnectionFailed | ureq::Error::Io(_) => {
            "Can't reach the update server".into()
        }
        ureq::Error::BodyExceedsLimit(_) => "The update is too big".into(),
        ureq::Error::StatusCode(status) => format!("The update server said no (HTTP {status})"),
        _ => "Couldn't check for updates".into(),
    }
}

#[cfg(test)]
mod tests;
