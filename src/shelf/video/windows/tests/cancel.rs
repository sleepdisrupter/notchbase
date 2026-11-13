use std::sync::atomic::{AtomicBool, Ordering};

use super::fixtures::Clip;
use crate::shelf::Task;
use crate::shelf::tests::fixtures::TempDir;
use crate::shelf::video::windows::{frames, transcode};
use crate::shelf::video::{Container, FrameSpec, MediaError, Quality};

fn released(path: &std::path::Path) -> bool {
    !path.exists() || std::fs::remove_file(path).is_ok()
}

#[test]
fn cancelled_task_writes_nothing() {
    let dir = TempDir::new();
    let clip = Clip { frames: 10, ..Clip::SMALL }.save(dir.path("clip.mp4"));
    let target = dir.path("out.mp4");
    let cancel = AtomicBool::new(true);
    let result = transcode(&clip, &target, Container::Mp4, Quality::Original, &mut Task::new(&mut |_| {}, &cancel));
    assert_eq!(result, Err(MediaError::Cancelled));
    assert!(released(&target));
}

#[test]
fn midway_cancel_releases_file() {
    let dir = TempDir::new();
    let clip = Clip::SMALL.save(dir.path("clip.mp4"));
    let target = dir.path("out.mp4");
    let cancel = AtomicBool::new(false);
    let mut report = |fraction: f32| {
        if fraction > 0.3 {
            cancel.store(true, Ordering::Relaxed);
        }
    };
    let result = transcode(&clip, &target, Container::Mp4, Quality::Original, &mut Task::new(&mut report, &cancel));
    assert_eq!(result, Err(MediaError::Cancelled));
    assert!(target.exists());
    assert!(released(&target));
}

#[test]
fn frame_cancel_stops_early() {
    let dir = TempDir::new();
    let clip = Clip::SMALL.save(dir.path("clip.mp4"));
    let cancel = AtomicBool::new(false);
    let spec = FrameSpec { max_side: 160, fps: 12.0, max_seconds: 30.0 };
    let mut seen = 0;
    let result = frames(&clip, spec, &mut Task::new(&mut |_| {}, &cancel), &mut |_| {
        seen += 1;
        if seen == 2 {
            cancel.store(true, Ordering::Relaxed);
        }
        Ok(())
    });
    assert_eq!(result, Err(MediaError::Cancelled));
    assert_eq!(seen, 2);
}
