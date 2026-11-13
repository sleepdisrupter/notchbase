//! One run of yt-dlp: the command line, its output followed line by line, and its complaint sorted into a reason.

use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::{Duration, Instant};

use super::codecs;
use super::engine::Engine;
use super::progress::{self, Line, Meta, Step, Tally};
use super::site::{Page, Site};

const POLL: Duration = Duration::from_millis(100);
/// A run that prints nothing for this long is stalled, unless it's joining picture and sound.
const QUIET_LIMIT: Duration = Duration::from_secs(5 * 60);
/// Titles are cut to 150 bytes, keeping the file name within what file systems allow.
const LONGEST_NAME: &str = "%(title).150B.%(ext)s";
const NOT_LIVE: &str = "!is_live & !is_upcoming";
const FRAGMENTS_AT_ONCE: &str = "8";
/// The best sound, AAC first, since an m4a download keeps it untouched.
const SOUND: &str = "ba[acodec~=?'^(mp4a|aac)']/ba/b";
/// What yt-dlp prints when the live filter skips a video.
const FILTERED: &str = "does not pass filter";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    Video,
    Audio,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct Request {
    pub(super) page: Page,
    pub(super) staging: PathBuf,
    pub(super) kind: Kind,
    pub(super) tallest: u32,
    pub(super) audio: String,
    /// The browser yt-dlp reads cookies from, once a site has asked for a sign-in.
    pub(super) cookies: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) enum Update {
    Found(Meta),
    Moving { share: f32, received: u64, speed: Option<f64>, eta: Option<f64> },
    Joining,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Reason {
    Cancelled,
    Gone,
    SignIn,
    NoVideo,
    Protected,
    Live,
    Offline,
    Stalled,
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Failure {
    pub(super) reason: Reason,
    pub(super) detail: String,
}

impl Failure {
    fn new(reason: Reason, detail: impl Into<String>) -> Self {
        Self { reason, detail: detail.into() }
    }

    /// What the notch says. It names the site and never the tool.
    pub(super) fn message(&self, site: Site) -> String {
        let site = site.name();
        match self.reason {
            Reason::Cancelled => "Download cancelled".into(),
            Reason::Gone => "That video isn't available".into(),
            Reason::SignIn => format!("{site} wants you signed in for this one"),
            Reason::NoVideo => "There's no video on that page".into(),
            Reason::Protected => "That video is copy-protected and can't be downloaded".into(),
            Reason::Live => "Live videos can't be downloaded yet".into(),
            Reason::Offline => format!("Can't reach {site}"),
            Reason::Stalled => "The download stopped moving".into(),
            Reason::Unknown => "Couldn't download that video".into(),
        }
    }
}

/// The command line, which ignores the user's own yt-dlp config and takes only the first video of a page.
pub(super) fn arguments(engine: &Engine, request: &Request) -> Vec<String> {
    let lossy = |path: &Path| path.to_string_lossy().into_owned();
    let mut arguments: Vec<String> = [
        "--ignore-config",
        "--no-playlist",
        "--playlist-items",
        "1",
        "--newline",
        "--no-quiet",
        "--no-mtime",
        "--color",
        "never",
        "--encoding",
        "utf-8",
        "--socket-timeout",
        "20",
        "--concurrent-fragments",
        FRAGMENTS_AT_ONCE,
        "--embed-metadata",
        "--match-filter",
        NOT_LIVE,
        "--progress-template",
        progress::STEP,
        "--print",
        &format!("before_dl:{}", progress::META),
        "--print",
        progress::FILE,
        "-o",
        LONGEST_NAME,
    ]
    .map(str::to_owned)
    .to_vec();
    arguments.extend(["--js-runtimes".into(), format!("deno:{}", lossy(&engine.runtime))]);
    arguments.extend(["--ffmpeg-location".into(), lossy(&engine.tools)]);
    arguments.extend(["-P".into(), lossy(&request.staging)]);
    match request.kind {
        Kind::Video => {
            arguments.extend(["-f".into(), picture_and_sound(&codecs::native())]);
            arguments.extend(["-S".into(), format!("res:{},fps,vcodec:h264,acodec:aac", request.tallest)]);
            arguments.extend(["--merge-output-format".into(), "mp4".into()]);
        }
        Kind::Audio => {
            arguments.extend(["-f".into(), SOUND.into(), "-x".into()]);
            arguments.extend(["--audio-format".into(), request.audio.clone()]);
            arguments.extend(["--audio-quality".into(), "0".into()]);
        }
    }
    if let Some(browser) = &request.cookies {
        arguments.extend(["--cookies-from-browser".into(), browser.clone()]);
    }
    arguments.extend(["--".into(), request.page.url.clone()]);
    arguments
}

/// A playable picture with AAC sound, then with any sound, then a playable single file, then whatever there is.
fn picture_and_sound(native: &str) -> String {
    let picture = format!("[vcodec~=?'^({native})']");
    format!("bv*{picture}+ba[acodec~=?'^(mp4a|aac)']/bv*{picture}+ba/b{picture}/bv*+ba/b")
}

pub(super) fn run(
    engine: &Engine,
    request: &Request,
    stop: &AtomicBool,
    tell: &mut dyn FnMut(Update),
) -> Result<PathBuf, Failure> {
    std::fs::create_dir_all(&request.staging).map_err(|err| Failure::new(Reason::Unknown, err.to_string()))?;
    let mut command = Command::new(&engine.downloader);
    command
        .args(arguments(engine, request))
        // yt-dlp is a Python program, so ask it for UTF-8 whatever the system's code page.
        .env("PYTHONIOENCODING", "utf-8")
        .env("PYTHONUTF8", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    super::quiet(&mut command);
    let mut child = command.spawn().map_err(|err| Failure::new(Reason::Unknown, err.to_string()))?;
    let (send, lines) = mpsc::channel::<String>();
    let readers = [child.stdout.take().map(boxed), child.stderr.take().map(boxed)];
    for reader in readers.into_iter().flatten() {
        let send = send.clone();
        let _ = std::thread::Builder::new().name("notchbase-video-out".into()).spawn(move || read_lines(reader, &send));
    }
    drop(send);
    let outcome = follow(&mut child, &lines, stop, tell);
    let _ = child.kill();
    let _ = child.wait();
    outcome
}

fn boxed(stream: impl Read + Send + 'static) -> Box<dyn Read + Send> {
    Box::new(stream)
}

fn read_lines(reader: Box<dyn Read + Send>, send: &mpsc::Sender<String>) {
    let mut reader = BufReader::new(reader);
    let mut raw = Vec::new();
    while reader.read_until(b'\n', &mut raw).is_ok_and(|read| read > 0) {
        if send.send(String::from_utf8_lossy(&raw).into_owned()).is_err() {
            return;
        }
        raw.clear();
    }
}

/// Reads yt-dlp's lines until it closes its output, and gives up on a cancel or a long silence.
fn follow(
    child: &mut Child,
    lines: &mpsc::Receiver<String>,
    stop: &AtomicBool,
    tell: &mut dyn FnMut(Update),
) -> Result<PathBuf, Failure> {
    let mut seen = Seen::default();
    let mut heard = Instant::now();
    loop {
        if stop.load(Ordering::Relaxed) {
            return Err(Failure::new(Reason::Cancelled, ""));
        }
        match lines.recv_timeout(POLL) {
            Ok(line) => {
                heard = Instant::now();
                if let Some(update) = seen.read(&line) {
                    tell(update);
                }
            }
            Err(RecvTimeoutError::Timeout) if !seen.joining && heard.elapsed() > QUIET_LIMIT => {
                return Err(Failure::new(Reason::Stalled, "no output"));
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }
    let ended_well = child.wait().is_ok_and(|status| status.success());
    seen.outcome(ended_well)
}

#[derive(Default)]
struct Seen {
    tally: Tally,
    file: Option<PathBuf>,
    problem: Option<String>,
    filtered: bool,
    joining: bool,
}

impl Seen {
    fn read(&mut self, line: &str) -> Option<Update> {
        match progress::parse(line) {
            Line::Meta(meta) => {
                self.tally.expect(meta.filesize_approx);
                Some(Update::Found(meta))
            }
            Line::Step(step) => {
                let Step { speed, eta, .. } = step;
                let share = self.tally.step(step);
                Some(Update::Moving { share, received: self.tally.received(), speed, eta })
            }
            Line::Merging => {
                self.joining = true;
                Some(Update::Joining)
            }
            Line::File(path) => {
                self.file = Some(path);
                None
            }
            Line::Error(problem) => {
                eprintln!("notchbase: yt-dlp: {problem}");
                self.problem.get_or_insert(problem);
                None
            }
            Line::Other => {
                self.filtered |= line.contains(FILTERED);
                None
            }
        }
    }

    fn outcome(self, ended_well: bool) -> Result<PathBuf, Failure> {
        match (self.file.filter(|file| file.is_file()), self.problem) {
            (Some(file), _) if ended_well => Ok(file),
            (_, Some(problem)) => Err(Failure::new(classify(&problem), problem)),
            _ if self.filtered => Err(Failure::new(Reason::Live, "a live video")),
            _ => Err(Failure::new(Reason::Unknown, "yt-dlp ended without a file")),
        }
    }
}

/// Sorts a yt-dlp complaint into a reason by the words in it.
pub(super) fn classify(problem: &str) -> Reason {
    let problem = problem.to_ascii_lowercase();
    let says = |phrases: &[&str]| phrases.iter().any(|phrase| problem.contains(phrase));
    if says(&["drm"]) {
        return Reason::Protected;
    }
    if says(&[
        "sign in",
        "log in",
        "login",
        "logged-in",
        "logged in",
        "cookies",
        "rate-limit",
        "private",
        "members-only",
        "age-restricted",
        "confirm your age",
    ]) {
        Reason::SignIn
    } else if says(&["no video formats", "no video in this", "there is no video", "unsupported url"]) {
        Reason::NoVideo
    } else if says(&["live event", "premieres in", "is not currently live", "will begin in"]) {
        Reason::Live
    } else if says(&["unavailable", "has been removed", "been deleted", "does not exist", "not found", "terminated"]) {
        Reason::Gone
    } else if says(&[
        "getaddrinfo",
        "name or service",
        "nodename nor servname",
        "timed out",
        "connection",
        "network",
        "unable to download",
        "temporary failure",
    ]) {
        Reason::Offline
    } else {
        Reason::Unknown
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn engine() -> Engine {
        Engine { downloader: "/e/yt-dlp/yt-dlp_macos".into(), runtime: "/e/deno".into(), tools: "/e".into() }
    }

    fn request(cookies: Option<&str>) -> Request {
        let page = super::super::site::recognize("https://youtu.be/jNQXAC9IVRw").unwrap();
        Request {
            page,
            staging: "/e/staging/7".into(),
            kind: Kind::Video,
            tallest: 2160,
            audio: "m4a".into(),
            cookies: cookies.map(str::to_owned),
        }
    }

    fn pair(arguments: &[String], flag: &str) -> Option<String> {
        arguments.iter().position(|argument| argument == flag).and_then(|at| arguments.get(at + 1)).cloned()
    }

    #[test]
    fn ytdlp_finds_tools_skips_user_config() {
        let arguments = arguments(&engine(), &request(None));
        assert_eq!(arguments[0], "--ignore-config");
        assert_eq!(pair(&arguments, "--js-runtimes").as_deref(), Some("deno:/e/deno"));
        assert_eq!(pair(&arguments, "--ffmpeg-location").as_deref(), Some("/e"));
        assert_eq!(pair(&arguments, "-P").as_deref(), Some("/e/staging/7"));
        assert_eq!(pair(&arguments, "-S").as_deref(), Some("res:2160,fps,vcodec:h264,acodec:aac"));
        assert_eq!(pair(&arguments, "--merge-output-format").as_deref(), Some("mp4"));
        assert_eq!(pair(&arguments, "--playlist-items").as_deref(), Some("1"), "a page of many videos gives one");
        let formats = pair(&arguments, "-f").unwrap();
        assert!(formats.starts_with("bv*[vcodec~=?'^(avc|h264"), "{formats}");
        assert!(formats.ends_with("/bv*+ba/b") && !formats.contains("vp"), "{formats}");
        assert!(!arguments.contains(&"--cookies-from-browser".to_owned()));
        let last_two = &arguments[arguments.len() - 2..];
        assert_eq!(
            last_two,
            ["--", "https://www.youtube.com/watch?v=jNQXAC9IVRw"],
            "the address can't be read as a flag"
        );
    }

    #[test]
    fn sound_only_asks_for_no_picture() {
        let request = Request { kind: Kind::Audio, audio: "mp3".into(), ..request(None) };
        let arguments = arguments(&engine(), &request);
        assert!(arguments.contains(&"-x".to_owned()) && !arguments.contains(&"--merge-output-format".to_owned()));
        assert_eq!(pair(&arguments, "--audio-format").as_deref(), Some("mp3"));
        assert_eq!(pair(&arguments, "-f").as_deref(), Some(SOUND));
        assert_eq!(pair(&arguments, "-S"), None);
    }

    #[test]
    fn signed_in_try_names_the_browser() {
        let arguments = arguments(&engine(), &request(Some("firefox:/Users/me/Library/Application Support/zen")));
        let browser = pair(&arguments, "--cookies-from-browser");
        assert_eq!(browser.as_deref(), Some("firefox:/Users/me/Library/Application Support/zen"));
    }

    #[test]
    fn complaints_sorted_into_next_steps() {
        let instagram = "[Instagram] DAbc: Instagram sent an empty media response. Check if this post is accessible in your browser without being logged-in. If it is not, then use --cookies-from-browser or --cookies for the authentication.";
        assert_eq!(classify(instagram), Reason::SignIn);
        assert_eq!(classify("[youtube] abc: Sign in to confirm you're not a bot."), Reason::SignIn);
        assert_eq!(classify("[youtube] abc: Private video. Sign in if you've been granted access"), Reason::SignIn);
        assert_eq!(classify("[youtube] xxxxxxxxxxx: This video is unavailable"), Reason::Gone);
        assert_eq!(classify("[Instagram] DAbc: No video formats found!"), Reason::NoVideo);
        assert_eq!(classify("[youtube] abc: This live event will begin in 3 hours."), Reason::Live);
        let offline =
            "Unable to download webpage: <urlopen error [Errno 8] nodename nor servname provided, or not known>";
        assert_eq!(classify(offline), Reason::Offline);
        assert_eq!(classify("[youtube] abc: Requested format is not available"), Reason::Unknown);
        let protected = "This format is DRM protected; Try selecting another format with --format";
        assert_eq!(classify(protected), Reason::Protected, "protection is respected, not worked around");
        let vimeo = "[vimeo] 76979871: The web client only works when logged-in. Use --cookies, --cookies-from-browser";
        assert_eq!(classify(vimeo), Reason::SignIn);
    }

    #[test]
    fn run_is_followed_to_its_file() {
        let folder = std::env::temp_dir().join(format!("notchbase-video-seen-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        let file = folder.join("Me at the zoo.mp4");
        std::fs::write(&file, b"video").unwrap();
        let mut seen = Seen::default();
        let mut updates = Vec::new();
        for line in [
            "[youtube] jNQXAC9IVRw: Downloading webpage".to_owned(),
            r#"NB|M|{"id": "jNQXAC9IVRw", "title": "Me at the zoo", "filesize_approx": 742369}"#.to_owned(),
            "NB|P|downloading|130048|433081|NA|5478494.7|0".to_owned(),
            "NB|P|finished|433081|433081|NA|2372174.1|NA".to_owned(),
            "NB|P|finished|309288|309288|NA|1981015.0|NA".to_owned(),
            r#"[Merger] Merging formats into "x.mp4""#.to_owned(),
            format!("NB|F|{}", file.display()),
        ] {
            updates.extend(seen.read(&line));
        }
        assert!(matches!(&updates[0], Update::Found(meta) if meta.title == "Me at the zoo"));
        assert!(matches!(updates[1], Update::Moving { share, speed: Some(_), .. } if share > 0.17 && share < 0.18));
        assert!(matches!(updates[3], Update::Moving { share, received: 742_369, .. } if share == 1.0));
        assert_eq!(updates[4], Update::Joining);
        assert_eq!(seen.outcome(true), Ok(file));
        std::fs::remove_dir_all(&folder).unwrap();
    }

    #[test]
    fn run_that_ends_badly_says_why() {
        let mut seen = Seen::default();
        seen.read("ERROR: [youtube] xxxxxxxxxxx: This video is unavailable");
        assert_eq!(seen.outcome(false).unwrap_err().reason, Reason::Gone);

        let mut live = Seen::default();
        live.read("[download] Some stream does not pass filter (!is_live & !is_upcoming), skipping ..");
        assert_eq!(live.outcome(true).unwrap_err().reason, Reason::Live);

        let mut missing = Seen::default();
        missing.read("NB|F|/definitely/not/here.mp4");
        assert_eq!(missing.outcome(true).unwrap_err().reason, Reason::Unknown);
    }

    #[test]
    fn message_names_site_never_tool() {
        let failure = Failure::new(Reason::SignIn, "use --cookies-from-browser");
        assert_eq!(failure.message(Site::Instagram), "Instagram wants you signed in for this one");
        assert_eq!(Failure::new(Reason::Offline, "").message(Site::YouTube), "Can't reach YouTube");
    }
}
