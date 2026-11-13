//! Sites often send the biggest picture in a codec the system player can't show. So only codecs that play here are
//! asked for, and a video in any other is converted once it lands.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use super::engine::Engine;

/// Codec name prefixes yt-dlp may give for H.264, and on a Mac for HEVC too.
const ALWAYS: &[&str] =
    if cfg!(target_os = "macos") { &["avc", "h264", "hev", "hvc", "h265"] } else { &["avc", "h264"] };
const AV1: &str = "av01";
/// The same codecs by ffmpeg's names, for checking a file once it lands.
const PLAYED: &[&str] = if cfg!(target_os = "macos") { &["h264", "hevc"] } else { &["h264"] };
const STREAM: &str = "Video: ";
const COVER: &str = "attached pic";
const CONVERTED: &str = "converted";
const POLL: Duration = Duration::from_millis(100);
const NOT_CONVERTED: &str = "Couldn't make the video playable";

pub(super) fn native() -> String {
    let av1 = decodes_av1().then_some(AV1);
    ALWAYS.iter().copied().chain(av1).collect::<Vec<_>>().join("|")
}

fn plays(codec: &str) -> bool {
    PLAYED.contains(&codec) || (codec == "av1" && decodes_av1())
}

/// A Mac plays AV1 only when its chip decodes it.
#[cfg(target_os = "macos")]
fn decodes_av1() -> bool {
    const AV1_CODEC: u32 = u32::from_be_bytes(*b"av01");
    #[link(name = "VideoToolbox", kind = "framework")]
    unsafe extern "C" {
        fn VTIsHardwareDecodeSupported(codec: u32) -> u8;
    }
    unsafe { VTIsHardwareDecodeSupported(AV1_CODEC) != 0 }
}

#[cfg(not(target_os = "macos"))]
fn decodes_av1() -> bool {
    false
}

/// The first video stream's codec in what ffmpeg prints about a file, skipping cover art.
fn video_codec(report: &str) -> Option<String> {
    report.lines().filter(|line| !line.contains(COVER)).find_map(|line| {
        let codec = line.split_once(STREAM)?.1;
        let name: String = codec.chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '_').collect();
        (!name.is_empty()).then_some(name)
    })
}

fn probe(engine: &Engine, file: &Path) -> Option<String> {
    let mut command = Command::new(engine.ffmpeg());
    command.arg("-hide_banner").arg("-i").arg(file).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::piped());
    super::quiet(&mut command);
    let output = command.output().ok()?;
    video_codec(&String::from_utf8_lossy(&output.stderr))
}

/// Converts a video that won't play here: to HEVC through VideoToolbox on a Mac, H.264 elsewhere, with AAC sound.
pub(super) fn make_playable(engine: &Engine, file: &Path, stop: &AtomicBool) -> Result<PathBuf, String> {
    let Some(codec) = probe(engine, file) else { return Ok(file.to_path_buf()) };
    if plays(&codec) {
        return Ok(file.to_path_buf());
    }
    eprintln!("notchbase: converting a {codec} video to one this computer plays");
    let stem = file.file_stem().map(|stem| stem.to_string_lossy().into_owned()).unwrap_or_default();
    let target = file.with_file_name(format!("{stem}.{CONVERTED}.mp4"));
    let mut command = Command::new(engine.ffmpeg());
    command.args(["-hide_banner", "-loglevel", "error", "-y", "-i"]).arg(file);
    command.args(["-map", "0:v:0", "-map", "0:a?"]).args(encoder());
    command.args(["-c:a", "aac", "-b:a", "192k", "-movflags", "+faststart"]).arg(&target);
    command.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    super::quiet(&mut command);
    let mut child = command.spawn().map_err(|_| NOT_CONVERTED)?;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if !stop.load(Ordering::Relaxed) => std::thread::sleep(POLL),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = std::fs::remove_file(&target);
                return Err(NOT_CONVERTED.into());
            }
        }
    };
    if !status.success() || !target.is_file() {
        let _ = std::fs::remove_file(&target);
        return Err(NOT_CONVERTED.into());
    }
    let _ = std::fs::remove_file(file);
    let named = file.with_extension("mp4");
    Ok(if std::fs::rename(&target, &named).is_ok() { named } else { target })
}

fn encoder() -> &'static [&'static str] {
    if cfg!(target_os = "macos") {
        // QuickTime only plays HEVC tagged `hvc1`.
        &["-c:v", "hevc_videotoolbox", "-q:v", "62", "-tag:v", "hvc1"]
    } else {
        &["-c:v", "libx264", "-preset", "veryfast", "-crf", "19", "-pix_fmt", "yuv420p"]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codec_is_read_from_ffmpeg_output() {
        let merged = "  Duration: 00:00:19.06, start: 0.000000, bitrate: 312 kb/s\n  Stream #0:0[0x1](und): Video: h264 (Main) (avc1 / 0x31637661), yuv420p, 320x240\n  Stream #0:1[0x2](und): Audio: aac (LC) (mp4a / 0x6134706D), 44100 Hz";
        assert_eq!(video_codec(merged).as_deref(), Some("h264"));
        let reel = "  Stream #0:0: Video: vp9 (Profile 0) (vp09 / 0x39307076), yuv420p(tv), 1080x1920";
        assert_eq!(video_codec(reel).as_deref(), Some("vp9"));
        let song = "  Stream #0:0: Audio: aac (LC)\n  Stream #0:1: Video: mjpeg (Baseline), yuvj420p (attached pic)";
        assert_eq!(video_codec(song), None, "cover art is not a video");
    }

    #[test]
    fn asks_only_for_codecs_that_play() {
        let native = native();
        assert!(native.starts_with("avc|h264"), "{native}");
        assert!(!native.contains("vp"), "{native}");
        assert!(plays("h264") && !plays("vp9") && !plays("vp8"));
        assert_eq!(plays("av1"), native.contains(AV1), "AV1 is asked for exactly when it plays");
    }

    #[test]
    #[ignore = "needs the engine in NOTCHBASE_DATA_DIR; encodes a second of VP9 and converts it back"]
    fn unplayable_video_is_converted() {
        let engine = super::super::engine::ready(&super::super::engine::folder().unwrap()).expect("the engine");
        let folder = std::env::temp_dir().join(format!("notchbase-codecs-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        let file = folder.join("clip.webm");
        let made = Command::new(engine.ffmpeg())
            .args(["-loglevel", "error", "-y", "-f", "lavfi", "-i", "testsrc=d=1:s=320x240:r=30"])
            .args(["-f", "lavfi", "-i", "sine=d=1", "-c:v", "libvpx-vp9", "-c:a", "libopus"])
            .arg(&file)
            .status()
            .unwrap();
        assert!(made.success());
        assert_eq!(probe(&engine, &file).as_deref(), Some("vp9"));
        let playable = make_playable(&engine, &file, &AtomicBool::new(false)).unwrap();
        assert_eq!(playable, folder.join("clip.mp4"));
        assert!(plays(&probe(&engine, &playable).unwrap()) && !file.exists());
        assert_eq!(
            make_playable(&engine, &playable, &AtomicBool::new(false)).unwrap(),
            playable,
            "nothing to do twice"
        );
        std::fs::remove_dir_all(&folder).unwrap();
    }
}
