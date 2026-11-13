//! The three programs downloads need, kept in `<data dir>/video-engine`: yt-dlp, Deno for YouTube's page script
//! and ffmpeg. Missing ones are fetched in parallel, checked against their SHA-256, run once and only then moved in.

use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, SystemTime};

use notchbase_net::{CopyError, Limits, Mode};

const FOLDER: &str = "video-engine";
const DOWNLOADER: &str = "yt-dlp";
const STAMP: &str = "yt-dlp.sha256";
const STAGING: &str = ".staging";
const RETIRED: &str = ".retired";
const USER_AGENT: &str = notchbase_net::user_agent!();
const TIMEOUT: Duration = Duration::from_secs(15 * 60);
const LIST_TIMEOUT: Duration = Duration::from_secs(20);
const LARGEST: u64 = 400 << 20;
const LARGEST_LIST: u64 = 1 << 20;
const DIGEST: usize = 64;
/// A yt-dlp last compared with the newest release longer ago than this is stale.
const LOOK_EVERY: Duration = Duration::from_secs(20 * 60 * 60);
/// How long a new program's first run may take, which on a Mac includes the system's first-launch check.
const SELF_CHECK: Duration = Duration::from_secs(60);
const YT_DLP: &str = "https://github.com/yt-dlp/yt-dlp/releases/latest/download/";
const YT_DLP_SUMS: &str = "SHA2-256SUMS";
const DENO: &str = "https://github.com/denoland/deno/releases/latest/download/";
const DENO_SUM: &str = ".sha256sum";
/// A pinned `ffmpeg-static` release, checked against the digests in `ASSETS`.
const FFMPEG: &str = "https://github.com/eugeneware/ffmpeg-static/releases/download/b6.1.1/";
const UNSUPPORTED: &str = "Video downloads aren't available on this system";
const NO_FOLDER: &str = "Couldn't find a place to keep the downloader";
const STOPPED: &str = "Cancelled";
const INTERRUPTED: &str = "The download was interrupted";
const TAMPERED: &str = "A download didn't match its checksum";
const NOT_UNPACKED: &str = "Couldn't unpack the downloader";
const NOT_WORKING: &str = "The downloader didn't start";

struct Assets {
    /// yt-dlp's unpacked build, which starts in 0.2 seconds where the single file takes 6.
    downloader: &'static str,
    downloader_exe: &'static str,
    ffmpeg: &'static str,
    ffmpeg_digest: &'static str,
    ffmpeg_exe: &'static str,
    deno: &'static str,
    deno_exe: &'static str,
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
const ASSETS: Option<Assets> = Some(Assets {
    downloader: "yt-dlp_macos.zip",
    downloader_exe: "yt-dlp_macos",
    ffmpeg: "ffmpeg-darwin-arm64.gz",
    ffmpeg_digest: "8923876afa8db5585022d7860ec7e589af192f441c56793971276d450ed3bbfa",
    ffmpeg_exe: "ffmpeg",
    deno: "deno-aarch64-apple-darwin.zip",
    deno_exe: "deno",
});

#[cfg(all(target_os = "macos", target_arch = "x86_64"))]
const ASSETS: Option<Assets> = Some(Assets {
    downloader: "yt-dlp_macos.zip",
    downloader_exe: "yt-dlp_macos",
    ffmpeg: "ffmpeg-darwin-x64.gz",
    ffmpeg_digest: "929b375c1182d956c51f7ac25e0b2b0411fb01f6f407aa15c9758efeb4242106",
    ffmpeg_exe: "ffmpeg",
    deno: "deno-x86_64-apple-darwin.zip",
    deno_exe: "deno",
});

#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
const ASSETS: Option<Assets> = Some(Assets {
    downloader: "yt-dlp_win.zip",
    downloader_exe: "yt-dlp.exe",
    ffmpeg: "ffmpeg-win32-x64.gz",
    ffmpeg_digest: "8883a3dffbd0a16cf4ef95206ea05283f78908dbfb118f73c83f4951dcc06d77",
    ffmpeg_exe: "ffmpeg.exe",
    deno: "deno-x86_64-pc-windows-msvc.zip",
    deno_exe: "deno.exe",
});

#[cfg(all(target_os = "windows", target_arch = "aarch64"))]
const ASSETS: Option<Assets> = Some(Assets {
    downloader: "yt-dlp_win_arm64.zip",
    downloader_exe: "yt-dlp.exe",
    ffmpeg: "ffmpeg-win32-x64.gz",
    ffmpeg_digest: "8883a3dffbd0a16cf4ef95206ea05283f78908dbfb118f73c83f4951dcc06d77",
    ffmpeg_exe: "ffmpeg.exe",
    deno: "deno-aarch64-pc-windows-msvc.zip",
    deno_exe: "deno.exe",
});

#[cfg(not(any(
    all(target_os = "macos", any(target_arch = "aarch64", target_arch = "x86_64")),
    all(target_os = "windows", any(target_arch = "aarch64", target_arch = "x86_64")),
)))]
const ASSETS: Option<Assets> = None;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Engine {
    pub downloader: PathBuf,
    pub runtime: PathBuf,
    pub tools: PathBuf,
}

impl Engine {
    pub fn ffmpeg(&self) -> PathBuf {
        self.tools.join(ASSETS.as_ref().map_or("ffmpeg", |assets| assets.ffmpeg_exe))
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Part {
    Downloader,
    Ffmpeg,
    Deno,
}

const PARTS: [Part; 3] = [Part::Downloader, Part::Ffmpeg, Part::Deno];
/// Rough download sizes, so the setup bar moves sensibly before each server gives the real one.
const ROUGH_SIZES: [u64; 3] = [54 << 20, 19 << 20, 39 << 20];

pub fn folder() -> Option<PathBuf> {
    crate::platform::data_dir().map(|dir| dir.join(FOLDER))
}

/// The engine, if all three programs are in place.
pub fn ready(folder: &Path) -> Option<Engine> {
    let assets = ASSETS.as_ref()?;
    let engine = engine_in(folder, assets);
    PARTS.iter().all(|part| installed(folder, assets, *part)).then_some(engine)
}

/// Fetches whatever is missing, reporting the share done across all of it.
pub fn install(folder: &Path, report: &(dyn Fn(f32) + Sync), stop: &AtomicBool) -> Result<Engine, String> {
    let assets = ASSETS.as_ref().ok_or(UNSUPPORTED)?;
    fs::create_dir_all(folder).map_err(|_| NO_FOLDER)?;
    let missing: Vec<usize> = (0..PARTS.len()).filter(|index| !installed(folder, assets, PARTS[*index])).collect();
    let received: [AtomicU64; 3] = Default::default();
    let totals: [AtomicU64; 3] = Default::default();
    for index in &missing {
        totals[*index].store(ROUGH_SIZES[*index], Ordering::Relaxed);
    }
    let tell = |index: usize, got: u64, total: Option<u64>| {
        received[index].store(got, Ordering::Relaxed);
        if let Some(total) = total {
            totals[index].store(total, Ordering::Relaxed);
        }
        let sum = |all: &[AtomicU64; 3]| all.iter().map(|one| one.load(Ordering::Relaxed)).sum::<u64>();
        report(notchbase_net::fraction(sum(&received), sum(&totals)));
    };
    let outcomes: Vec<Result<(), String>> = std::thread::scope(|scope| {
        let fetching: Vec<_> = missing
            .iter()
            .map(|&index| {
                let tell = &tell;
                scope.spawn(move || {
                    fetch_part(folder, assets, PARTS[index], &|got, total| tell(index, got, total), stop)
                })
            })
            .collect();
        fetching.into_iter().map(|thread| thread.join().unwrap_or_else(|_| Err(NOT_UNPACKED.into()))).collect()
    });
    outcomes.into_iter().collect::<Result<(), String>>()?;
    Ok(engine_in(folder, assets))
}

/// Whether yt-dlp is due a comparison with the newest release. A copy never compared is stale.
pub fn stale(folder: &Path) -> bool {
    let looked = fs::metadata(folder.join(STAMP)).and_then(|stamp| stamp.modified()).ok();
    looked.is_none_or(|at| SystemTime::now().duration_since(at).is_ok_and(|age| age > LOOK_EVERY))
}

/// Replaces yt-dlp if the newest release differs, and says whether it did.
pub fn refresh(folder: &Path) -> Result<bool, String> {
    let assets = ASSETS.as_ref().ok_or(UNSUPPORTED)?;
    let newest = listed_digest(&format!("{YT_DLP}{YT_DLP_SUMS}"), Some(assets.downloader))?;
    let stamp = folder.join(STAMP);
    if fs::read_to_string(&stamp).is_ok_and(|have| have.trim() == newest) {
        // Still the newest, so just note when we looked.
        let _ = fs::write(&stamp, &newest);
        return Ok(false);
    }
    fetch_part(folder, assets, Part::Downloader, &|_, _| {}, &AtomicBool::new(false))?;
    Ok(true)
}

fn engine_in(folder: &Path, assets: &Assets) -> Engine {
    Engine {
        downloader: folder.join(DOWNLOADER).join(assets.downloader_exe),
        runtime: folder.join(assets.deno_exe),
        tools: folder.to_path_buf(),
    }
}

fn installed(folder: &Path, assets: &Assets, part: Part) -> bool {
    match part {
        Part::Downloader => folder.join(DOWNLOADER).join(assets.downloader_exe).is_file(),
        Part::Ffmpeg => folder.join(assets.ffmpeg_exe).is_file(),
        Part::Deno => folder.join(assets.deno_exe).is_file(),
    }
}

fn fetch_part(
    folder: &Path,
    assets: &Assets,
    part: Part,
    report: &dyn Fn(u64, Option<u64>),
    stop: &AtomicBool,
) -> Result<(), String> {
    let (url, digest, name) = match part {
        Part::Downloader => {
            let digest = listed_digest(&format!("{YT_DLP}{YT_DLP_SUMS}"), Some(assets.downloader))?;
            (format!("{YT_DLP}{}", assets.downloader), digest, "downloader")
        }
        Part::Ffmpeg => (format!("{FFMPEG}{}", assets.ffmpeg), assets.ffmpeg_digest.to_owned(), "ffmpeg"),
        Part::Deno => {
            let digest = listed_digest(&format!("{DENO}{}{DENO_SUM}", assets.deno), None)?;
            (format!("{DENO}{}", assets.deno), digest, "deno")
        }
    };
    let archive = folder.join(format!("{STAGING}-{name}.part"));
    let staging = folder.join(format!("{STAGING}-{name}"));
    let placed = download(&url, &archive, &digest, report, stop).and_then(|()| {
        let _ = fs::remove_dir_all(&staging);
        fs::create_dir_all(&staging).map_err(|_| NOT_UNPACKED)?;
        match part {
            Part::Downloader => {
                unzip(&archive, &staging)?;
                works(&staging.join(assets.downloader_exe), "--version")?;
                swap_in(&staging, &folder.join(DOWNLOADER))?;
                fs::write(folder.join(STAMP), &digest).map_err(|_| NOT_UNPACKED.to_owned())
            }
            Part::Ffmpeg => {
                let unpacked = staging.join(assets.ffmpeg_exe);
                gunzip(&archive, &unpacked)?;
                works(&unpacked, "-version")?;
                fs::rename(&unpacked, folder.join(assets.ffmpeg_exe)).map_err(|_| NOT_UNPACKED.to_owned())
            }
            Part::Deno => {
                unzip(&archive, &staging)?;
                let unpacked = staging.join(assets.deno_exe);
                works(&unpacked, "--version")?;
                fs::rename(&unpacked, folder.join(assets.deno_exe)).map_err(|_| NOT_UNPACKED.to_owned())
            }
        }
    });
    let _ = fs::remove_file(&archive);
    let _ = fs::remove_dir_all(&staging);
    placed
}

/// Moves a new folder into place, putting the old one back if that fails.
fn swap_in(fresh: &Path, place: &Path) -> Result<(), String> {
    let retired = place.with_extension(RETIRED.trim_start_matches('.'));
    let _ = fs::remove_dir_all(&retired);
    if place.exists() {
        fs::rename(place, &retired).map_err(|_| NOT_UNPACKED)?;
    }
    let moved = fs::rename(fresh, place);
    if moved.is_err() && retired.exists() {
        let _ = fs::rename(&retired, place);
    }
    let _ = fs::remove_dir_all(&retired);
    moved.map_err(|_| NOT_UNPACKED.to_owned())
}

fn download(
    url: &str,
    to: &Path,
    digest: &str,
    report: &dyn Fn(u64, Option<u64>),
    stop: &AtomicBool,
) -> Result<(), String> {
    let mut response = agent(TIMEOUT).get(url).call().map_err(|err| describe(&err))?;
    let total = response.headers().get("content-length").and_then(|value| value.to_str().ok()?.parse::<u64>().ok());
    let body = response.body_mut().with_config().limit(LARGEST).reader();
    save_verified(body, to, digest, |done| report(done, total.filter(|total| *total > 0)), stop)
}

fn save_verified(
    body: impl Read,
    to: &Path,
    digest: &str,
    report: impl FnMut(u64),
    stop: &AtomicBool,
) -> Result<(), String> {
    let mut file = File::create(to).map_err(|_| NO_FOLDER)?;
    let limits = Limits { stop: Some(stop), ..Limits::default() };
    let got = notchbase_net::copy_hashed(body, &mut file, limits, report).map_err(|err| match err {
        CopyError::Stopped => STOPPED.to_owned(),
        CopyError::Write(err) => err.to_string(),
        CopyError::Read(_) | CopyError::TooLarge => INTERRUPTED.to_owned(),
    })?;
    file.sync_all().map_err(|err| err.to_string())?;
    if got.eq_ignore_ascii_case(digest) { Ok(()) } else { Err(TAMPERED.into()) }
}

fn listed_digest(url: &str, entry: Option<&str>) -> Result<String, String> {
    let mut response = agent(LIST_TIMEOUT).get(url).call().map_err(|err| describe(&err))?;
    let text = response.body_mut().with_config().limit(LARGEST_LIST).read_to_string().map_err(|err| describe(&err))?;
    digest_in(&text, entry).ok_or_else(|| TAMPERED.to_owned())
}

/// The checksum on `entry`'s line of a list, or the first in a single-checksum file, from shasum or PowerShell.
fn digest_in(text: &str, entry: Option<&str>) -> Option<String> {
    let hex = |line: &str| {
        line.split(|c: char| !c.is_ascii_hexdigit()).find(|word| word.len() == DIGEST).map(str::to_ascii_lowercase)
    };
    match entry {
        Some(entry) => text
            .lines()
            .find(|line| line.split_whitespace().any(|word| word.trim_start_matches('*') == entry))
            .and_then(hex),
        None => text.lines().find_map(hex),
    }
}

fn unzip(archive: &Path, into: &Path) -> Result<(), String> {
    let mut command = extractor(archive, into);
    let status = command.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).status();
    if status.is_ok_and(|status| status.success()) { Ok(()) } else { Err(NOT_UNPACKED.into()) }
}

#[cfg(target_os = "macos")]
fn extractor(archive: &Path, into: &Path) -> Command {
    let mut command = Command::new("/usr/bin/ditto");
    command.arg("-x").arg("-k").arg(archive).arg(into);
    command
}

#[cfg(not(target_os = "macos"))]
fn extractor(archive: &Path, into: &Path) -> Command {
    let mut command = Command::new("tar");
    command.arg("-xf").arg(archive).arg("-C").arg(into);
    super::quiet(&mut command);
    command
}

fn gunzip(archive: &Path, to: &Path) -> Result<(), String> {
    let mut packed = flate2::read::GzDecoder::new(File::open(archive).map_err(|_| NOT_UNPACKED)?);
    let mut file = File::create(to).map_err(|_| NOT_UNPACKED)?;
    std::io::copy(&mut packed, &mut file).map_err(|_| NOT_UNPACKED)?;
    file.sync_all().map_err(|_| NOT_UNPACKED)?;
    executable(to)
}

#[cfg(unix)]
fn executable(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt as _;
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).map_err(|_| NOT_UNPACKED.to_owned())
}

#[cfg(not(unix))]
fn executable(_: &Path) -> Result<(), String> {
    Ok(())
}

/// Runs the program once to prove it starts here, which also gets the Mac's first-launch check out of the way.
fn works(program: &Path, flag: &str) -> Result<(), String> {
    let mut command = Command::new(program);
    command.arg(flag).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    super::quiet(&mut command);
    let mut child = command.spawn().map_err(|_| NOT_WORKING)?;
    let started = std::time::Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return if status.success() { Ok(()) } else { Err(NOT_WORKING.into()) },
            Ok(None) if started.elapsed() < SELF_CHECK => std::thread::sleep(Duration::from_millis(50)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(NOT_WORKING.into());
            }
        }
    }
}

fn agent(timeout: Duration) -> ureq::Agent {
    notchbase_net::agent(timeout, USER_AGENT, Mode::Follow)
}

fn describe(err: &ureq::Error) -> String {
    match err {
        ureq::Error::Timeout(_) => "Setting up took too long".into(),
        ureq::Error::HostNotFound | ureq::Error::ConnectionFailed | ureq::Error::Io(_) => {
            "Can't reach the internet to set up downloads".into()
        }
        ureq::Error::StatusCode(status) => format!("Setting up downloads failed (HTTP {status})"),
        _ => "Couldn't set up downloads".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const YT_DLP_LIST: &str = "\
66674953fe251b89f4d08c5f0e35e0728679bd67ab3d7d05c0562af101dd3e7a  yt-dlp.exe
0f192b7ec147ab6288885d6351d9ab67367640029b4377576ef46dd79cf7b202  yt-dlp_macos
07e54b0865303c864006925913bce2604f8ee8cc6f18699bac9c309f9328a6d8  yt-dlp_macos.zip
30b4c14aafab6082becff7881e41b76df46dc43ea7633479410a91e29da492bf  yt-dlp_win.zip
";

    fn scratch(name: &str) -> PathBuf {
        let folder = std::env::temp_dir().join(format!("notchbase-engine-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&folder);
        fs::create_dir_all(&folder).unwrap();
        folder
    }

    #[test]
    fn checksum_list_finds_exact_file() {
        let zip = digest_in(YT_DLP_LIST, Some("yt-dlp_macos.zip"));
        assert_eq!(zip.as_deref(), Some("07e54b0865303c864006925913bce2604f8ee8cc6f18699bac9c309f9328a6d8"));
        let bare = digest_in(YT_DLP_LIST, Some("yt-dlp_macos"));
        assert_eq!(bare.as_deref(), Some("0f192b7ec147ab6288885d6351d9ab67367640029b4377576ef46dd79cf7b202"));
        assert_eq!(digest_in(YT_DLP_LIST, Some("yt-dlp_linux")), None);
    }

    #[test]
    fn one_sum_reads_shasum_or_powershell() {
        let shasum =
            "5cd46d6268f6f78f5d88bdc7159d20bd44cdaa4b3303474839f87ec6fe7ae25c  deno-aarch64-apple-darwin.zip\n";
        let powershell = "\r\nAlgorithm : SHA256\r\nHash      : A0C3101B4158D1DFB7D6A78A7BF0F3DE80C96BB423C152BEEC8BEB22786F2238\r\nPath      : D:\\a\\deno\\deno-x86_64-pc-windows-msvc.zip\r\n";
        let digest = digest_in(shasum, None);
        assert_eq!(digest.as_deref(), Some("5cd46d6268f6f78f5d88bdc7159d20bd44cdaa4b3303474839f87ec6fe7ae25c"));
        let digest = digest_in(powershell, None);
        assert_eq!(digest.as_deref(), Some("a0c3101b4158d1dfb7d6a78a7bf0f3de80c96bb423c152beec8beb22786f2238"));
        assert_eq!(digest_in("<html>Not Found</html>", None), None);
    }

    #[test]
    fn ready_only_with_all_three_programs() {
        let Some(assets) = ASSETS.as_ref() else { return };
        let folder = scratch("ready");
        assert_eq!(ready(&folder), None);
        fs::create_dir_all(folder.join(DOWNLOADER)).unwrap();
        fs::write(folder.join(DOWNLOADER).join(assets.downloader_exe), b"").unwrap();
        fs::write(folder.join(assets.ffmpeg_exe), b"").unwrap();
        assert_eq!(ready(&folder), None, "Deno is still missing");
        fs::write(folder.join(assets.deno_exe), b"").unwrap();
        let engine = ready(&folder).expect("ready");
        assert_eq!(engine.tools, folder);
        assert!(engine.downloader.starts_with(folder.join(DOWNLOADER)));
        fs::remove_dir_all(&folder).unwrap();
    }

    #[test]
    fn new_downloader_replaces_old_whole() {
        let folder = scratch("swap");
        let (place, fresh) = (folder.join(DOWNLOADER), folder.join("fresh"));
        fs::create_dir_all(&place).unwrap();
        fs::write(place.join("old"), b"").unwrap();
        fs::create_dir_all(&fresh).unwrap();
        fs::write(fresh.join("new"), b"").unwrap();
        swap_in(&fresh, &place).unwrap();
        assert!(place.join("new").is_file() && !place.join("old").exists() && !fresh.exists());
        assert_eq!(fs::read_dir(&folder).unwrap().count(), 1, "nothing is left lying around");
        fs::remove_dir_all(&folder).unwrap();
    }

    #[test]
    fn stale_unless_freshly_compared() {
        let folder = scratch("stale");
        assert!(stale(&folder));
        fs::write(folder.join(STAMP), "abc").unwrap();
        assert!(!stale(&folder));
        fs::remove_dir_all(&folder).unwrap();
    }

    const ABC: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";

    struct BreaksOff;

    impl std::io::Read for BreaksOff {
        fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
            Err(std::io::ErrorKind::ConnectionReset.into())
        }
    }

    #[test]
    fn matching_checksum_kept_in_any_case() {
        let folder = scratch("verified");
        let to = folder.join("part");
        let mut seen = Vec::new();
        let saved =
            save_verified(&b"abc"[..], &to, &ABC.to_uppercase(), |done| seen.push(done), &AtomicBool::new(false));
        assert_eq!(saved, Ok(()));
        assert_eq!(fs::read(&to).unwrap(), b"abc");
        assert_eq!(seen, [3]);
        fs::remove_dir_all(&folder).unwrap();
    }

    #[test]
    fn mismatched_download_is_tampered() {
        let folder = scratch("tampered");
        let saved = save_verified(&b"abd"[..], &folder.join("part"), ABC, |_| {}, &AtomicBool::new(false));
        assert_eq!(saved, Err(TAMPERED.to_owned()));
        fs::remove_dir_all(&folder).unwrap();
    }

    #[test]
    fn cancelled_download_stops_at_once() {
        let folder = scratch("cancelled");
        let saved = save_verified(&b"abc"[..], &folder.join("part"), ABC, |_| {}, &AtomicBool::new(true));
        assert_eq!(saved, Err(STOPPED.to_owned()));
        fs::remove_dir_all(&folder).unwrap();
    }

    #[test]
    fn broken_download_is_interrupted() {
        let folder = scratch("interrupted");
        let saved =
            save_verified((&b"ab"[..]).chain(BreaksOff), &folder.join("part"), ABC, |_| {}, &AtomicBool::new(false));
        assert_eq!(saved, Err(INTERRUPTED.to_owned()));
        fs::remove_dir_all(&folder).unwrap();
    }

    #[test]
    fn download_with_nowhere_to_go_says_so() {
        let folder = scratch("nowhere");
        let to = folder.join("missing").join("part");
        assert_eq!(save_verified(&b"abc"[..], &to, ABC, |_| {}, &AtomicBool::new(false)), Err(NO_FOLDER.to_owned()));
        fs::remove_dir_all(&folder).unwrap();
    }

    #[test]
    fn network_trouble_reads_as_sentence() {
        assert_eq!(describe(&ureq::Error::Timeout(ureq::Timeout::Global)), "Setting up took too long");
        let offline = "Can't reach the internet to set up downloads";
        assert_eq!(describe(&ureq::Error::HostNotFound), offline);
        assert_eq!(describe(&ureq::Error::ConnectionFailed), offline);
        assert_eq!(describe(&ureq::Error::Io(std::io::ErrorKind::ConnectionReset.into())), offline);
        assert_eq!(describe(&ureq::Error::StatusCode(404)), "Setting up downloads failed (HTTP 404)");
        assert_eq!(describe(&ureq::Error::TooManyRedirects), "Couldn't set up downloads");
    }

    #[test]
    fn fetches_over_https_with_redirects() {
        let agent = agent(LIST_TIMEOUT);
        let config = agent.config();
        assert!(config.https_only());
        assert!(config.http_status_as_error());
        assert!(config.max_redirects() > 0);
        assert_eq!(config.timeouts().global, Some(LIST_TIMEOUT));
        assert!(
            matches!(config.user_agent(), ureq::config::AutoHeaderValue::Provided(text) if text.as_str() == USER_AGENT)
        );
    }

    #[test]
    #[ignore = "downloads about 110 MB from GitHub"]
    fn real_engine_installs_and_runs() {
        let folder = scratch("real");
        let engine = install(&folder, &|share| eprintln!("{:.0}%", share * 100.0), &AtomicBool::new(false)).unwrap();
        assert_eq!(ready(&folder), Some(engine));
        assert_eq!(refresh(&folder), Ok(false), "it was just installed, so it is the newest");
        fs::remove_dir_all(&folder).unwrap();
    }
}
