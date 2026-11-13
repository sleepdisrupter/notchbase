use std::path::Path;
use std::sync::atomic::AtomicBool;

use super::fixtures::{BLUE, Clip, RED, save_wav};
use super::frames::{collect, looks_like, pixel};
use super::{about, streams};
use crate::shelf::Task;
use crate::shelf::tests::fixtures::TempDir;
use crate::shelf::video::windows::{probe, transcode};
use crate::shelf::video::{Container, FrameSpec, Quality};

fn convert(source: &Path, target: &Path, container: Container, quality: Quality) -> Vec<f32> {
    let mut reports = Vec::new();
    let mut report = |fraction: f32| reports.push(fraction);
    transcode(source, target, container, quality, &mut Task::new(&mut report, &AtomicBool::new(false))).unwrap();
    reports
}

#[test]
fn original_keeps_picture_and_sound() {
    let dir = TempDir::new();
    let clip = Clip::SMALL.save(dir.path("clip.mp4"));
    let target = dir.path(".notchbase-1-0.mp4");
    let reports = convert(&clip, &target, Container::Mp4, Quality::Original);
    let found = probe(&target).unwrap();
    assert_eq!(found.dims, Some([320, 240]));
    assert!(about(found.seconds, 2.0), "{:?}", found.seconds);
    let streams = streams(&target);
    assert!(streams.video && streams.audio);
    assert!(!reports.is_empty() && reports.windows(2).all(|pair| pair[0] <= pair[1]), "{reports:?}");
    assert!(reports.last().is_some_and(|last| *last > 0.9), "{reports:?}");
}

#[test]
fn compact_shrinks_big_to_1080p() {
    let dir = TempDir::new();
    let big = Clip { size: [2048, 1152], frames: 30, audio: false, bitrate: 20_000_000, grain: true, ..Clip::SMALL };
    let clip = big.save(dir.path("big.mp4"));
    let target = dir.path("small.mp4");
    convert(&clip, &target, Container::Mp4, Quality::Compact);
    assert_eq!(probe(&target).unwrap().dims, Some([1920, 1080]));
    let before = std::fs::metadata(&clip).unwrap().len();
    let after = std::fs::metadata(&target).unwrap().len();
    assert!(after < before, "{after} >= {before}");
    assert!(!streams(&target).audio);
}

#[test]
fn compact_leaves_small_videos() {
    let dir = TempDir::new();
    let clip = Clip { frames: 20, ..Clip::SMALL }.save(dir.path("clip.mp4"));
    let target = dir.path("compact.mp4");
    convert(&clip, &target, Container::Mp4, Quality::Compact);
    assert_eq!(probe(&target).unwrap().dims, Some([320, 240]));
}

#[test]
fn rotated_videos_stay_upright() {
    let dir = TempDir::new();
    let clip = Clip { rotation: 90, frames: 20, ..Clip::SMALL }.save(dir.path("portrait.mp4"));
    let target = dir.path("converted.mp4");
    convert(&clip, &target, Container::Mp4, Quality::Original);
    assert_eq!(probe(&target).unwrap().dims, Some([240, 320]));
    let frames = collect(&target, FrameSpec { max_side: 160, fps: 12.0, max_seconds: 1.0 });
    let first = frames.first().unwrap();
    assert_eq!((first.width, first.height), (120, 160));
    assert!(looks_like(pixel(first, 110, 80), RED), "{:?}", pixel(first, 110, 80));
    assert!(looks_like(pixel(first, 10, 80), BLUE), "{:?}", pixel(first, 10, 80));
}

#[test]
fn audio_extraction_writes_only_sound() {
    let dir = TempDir::new();
    let clip = Clip::SMALL.save(dir.path("clip.mp4"));
    let target = dir.path("sound.m4a");
    convert(&clip, &target, Container::M4a, Quality::Original);
    let found = probe(&target).unwrap();
    assert_eq!(found.dims, None);
    assert!(about(found.seconds, 2.0), "{:?}", found.seconds);
    let streams = streams(&target);
    assert!(streams.audio && !streams.video);
}

#[test]
fn odd_rates_resampled_for_aac() {
    let dir = TempDir::new();
    let wav = save_wav(dir.path("voice.wav"), 32_000, 1);
    let target = dir.path("voice.m4a");
    convert(&wav, &target, Container::M4a, Quality::Compact);
    assert!(about(probe(&target).unwrap().seconds, 1.0));
}

#[test]
fn full_hd_ignores_padding() {
    let dir = TempDir::new();
    let hd = Clip { size: [1920, 1080], frames: 6, audio: false, grain: true, ..Clip::SMALL };
    let clip = hd.save(dir.path("hd.mp4"));
    assert_eq!(probe(&clip).unwrap().dims, Some([1920, 1080]));
    let target = dir.path("copy.mp4");
    convert(&clip, &target, Container::Mp4, Quality::Original);
    assert_eq!(probe(&target).unwrap().dims, Some([1920, 1080]));
    let frames = collect(&target, FrameSpec { max_side: 480, fps: 12.0, max_seconds: 1.0 });
    assert!(!frames.is_empty() && frames.iter().all(|frame| (frame.width, frame.height) == (480, 270)));
}
