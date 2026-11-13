use std::sync::atomic::AtomicBool;

use super::fixtures::{BLUE, Clip, RED};
use crate::shelf::Task;
use crate::shelf::tests::fixtures::TempDir;
use crate::shelf::video::windows::frames;
use crate::shelf::video::{Frame, FrameSpec};

const GIF: FrameSpec = FrameSpec { max_side: 160, fps: 12.0, max_seconds: 30.0 };

pub(super) fn collect(clip: &std::path::Path, spec: FrameSpec) -> Vec<Frame> {
    let mut collected = Vec::new();
    frames(clip, spec, &mut Task::new(&mut |_| {}, &AtomicBool::new(false)), &mut |frame| {
        collected.push(frame);
        Ok(())
    })
    .unwrap();
    collected
}

pub(super) fn pixel(frame: &Frame, x: u32, y: u32) -> [u8; 4] {
    let at = ((y * frame.width + x) * 4) as usize;
    [frame.rgba[at], frame.rgba[at + 1], frame.rgba[at + 2], frame.rgba[at + 3]]
}

pub(super) fn looks_like(actual: [u8; 4], bgrx: [u8; 4]) -> bool {
    let expected = [bgrx[2], bgrx[1], bgrx[0]];
    actual.iter().zip(expected).all(|(got, want)| got.abs_diff(want) < 60)
}

#[test]
fn frames_spaced_scaled_and_opaque() {
    let dir = TempDir::new();
    let clip = Clip::SMALL.save(dir.path("clip.mp4"));
    let frames = collect(&clip, GIF);
    assert!((23..=25).contains(&frames.len()), "{}", frames.len());
    for (index, frame) in frames.iter().enumerate() {
        assert_eq!((frame.width, frame.height), (160, 120));
        assert_eq!(frame.rgba.len(), 160 * 120 * 4);
        assert!(frame.rgba.chunks_exact(4).all(|pixel| pixel[3] == 255));
        assert!((frame.seconds - index as f64 / 12.0).abs() < 1e-9);
    }
    let first = &frames[0];
    assert!(looks_like(pixel(first, 150, 10), RED), "{:?}", pixel(first, 150, 10));
    assert!(looks_like(pixel(first, 150, 110), BLUE), "{:?}", pixel(first, 150, 110));
}

#[test]
fn frames_stop_at_the_time_limit() {
    let dir = TempDir::new();
    let clip = Clip::SMALL.save(dir.path("clip.mp4"));
    let frames = collect(&clip, FrameSpec { max_seconds: 1.0, ..GIF });
    assert_eq!(frames.len(), 12);
}

#[test]
fn small_videos_are_never_enlarged() {
    let dir = TempDir::new();
    let clip = Clip { frames: 10, ..Clip::SMALL }.save(dir.path("clip.mp4"));
    let frames = collect(&clip, FrameSpec { max_side: 1000, ..GIF });
    assert!(frames.iter().all(|frame| (frame.width, frame.height) == (320, 240)));
}
