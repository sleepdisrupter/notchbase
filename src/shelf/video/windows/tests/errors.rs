use std::sync::atomic::AtomicBool;

use super::fixtures::Clip;
use crate::shelf::Task;
use crate::shelf::tests::fixtures::TempDir;
use crate::shelf::video::windows::{can_write, frames, probe, transcode};
use crate::shelf::video::{Container, FrameSpec, MediaError, Quality};

const SPEC: FrameSpec = FrameSpec { max_side: 160, fps: 12.0, max_seconds: 30.0 };

#[test]
fn only_mpeg4_containers_can_be_written() {
    assert!(can_write(Container::Mp4));
    assert!(can_write(Container::M4a));
    assert!(!can_write(Container::Mov));
}

#[test]
fn unreadable_files_are_unsupported() {
    let dir = TempDir::new();
    let garbage = dir.path("garbage.mp4");
    std::fs::write(&garbage, b"this is not a movie at all, just some words").unwrap();
    assert_eq!(probe(&garbage), None);
    assert_eq!(probe(&dir.path("missing.mp4")), None);
    let target = dir.path("out.mp4");
    let result = transcode(
        &garbage,
        &target,
        Container::Mp4,
        Quality::Original,
        &mut Task::new(&mut |_| {}, &AtomicBool::new(false)),
    );
    assert_eq!(result, Err(MediaError::Unsupported));
    let result = frames(&garbage, SPEC, &mut Task::new(&mut |_| {}, &AtomicBool::new(false)), &mut |_| Ok(()));
    assert_eq!(result, Err(MediaError::Unsupported));
}

#[test]
fn audio_extraction_needs_a_soundtrack() {
    let dir = TempDir::new();
    let silent = Clip { audio: false, frames: 10, ..Clip::SMALL }.save(dir.path("silent.mp4"));
    let target = dir.path("out.m4a");
    let result = transcode(
        &silent,
        &target,
        Container::M4a,
        Quality::Original,
        &mut Task::new(&mut |_| {}, &AtomicBool::new(false)),
    );
    assert_eq!(result, Err(MediaError::NoAudio));
}

#[test]
fn frame_consumer_errors_stop_decoding() {
    let dir = TempDir::new();
    let clip = Clip::SMALL.save(dir.path("clip.mp4"));
    let mut seen = 0;
    let result = frames(&clip, SPEC, &mut Task::new(&mut |_| {}, &AtomicBool::new(false)), &mut |_| {
        seen += 1;
        if seen == 3 { Err(MediaError::Failed) } else { Ok(()) }
    });
    assert_eq!(result, Err(MediaError::Failed));
    assert_eq!(seen, 3);
}
