use std::fs::{self, File};
use std::io::BufReader;
use std::path::Path;
use std::sync::atomic::AtomicBool;

use image::AnimationDecoder;
use image::codecs::gif::GifDecoder;

use super::fixtures::Clip;
use crate::shelf::names::{file_name, lowercase_extension};
use crate::shelf::ops::{self, Outcome, Outputs, Source};
use crate::shelf::tests::fixtures::TempDir;
use crate::shelf::{Clip as Target, Kind, Op, Task};

fn run(op: Op, path: &Path, cancelled: bool) -> Outcome {
    let cancel = AtomicBool::new(cancelled);
    let mut report = |_: f32| {};
    let source = Source { path: path.to_path_buf(), kind: Kind::Video };
    ops::run(op, &[source], &Outputs::new(None), &mut Task::new(&mut report, &cancel))
}

fn names_in(dir: &TempDir) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(&dir.0).unwrap().map(|entry| file_name(&entry.unwrap().path())).collect();
    names.sort();
    names
}

#[test]
fn clip_converts_to_every_format() {
    let dir = TempDir::new();
    let clip = Clip::SMALL.save(dir.path("clip.mp4"));
    for (target, ext) in [(Target::Mp4, "mp4"), (Target::Gif, "gif"), (Target::M4a, "m4a")] {
        let outcome = run(Op::Encode(target), &clip, false);
        assert!(outcome.notice.ok, "{:?}", outcome.notice);
        let [output] = outcome.outputs.as_slice() else { panic!("{:?}", outcome.outputs) };
        assert_eq!(lowercase_extension(output), ext);
        assert!(fs::metadata(output).unwrap().len() > 0);
    }
    assert_eq!(names_in(&dir), ["clip 2.mp4", "clip.gif", "clip.m4a", "clip.mp4"]);
}

#[test]
fn gifs_keep_the_pace_of_the_clip() {
    let dir = TempDir::new();
    let clip = Clip::SMALL.save(dir.path("clip.mp4"));
    let outcome = run(Op::Encode(Target::Gif), &clip, false);
    let decoder = GifDecoder::new(BufReader::new(File::open(&outcome.outputs[0]).unwrap())).unwrap();
    let frames = decoder.into_frames().collect_frames().unwrap();
    assert!((22..=26).contains(&frames.len()), "{} frames", frames.len());
    let total_ms: u32 = frames.iter().map(|frame| frame.delay().numer_denom_ms().0).sum();
    assert!((1900..=2100).contains(&total_ms), "{total_ms} ms");
}

#[test]
fn compressing_saves_a_smaller_copy() {
    let dir = TempDir::new();
    let clip = Clip { size: [640, 480], grain: true, bitrate: 8_000_000, ..Clip::SMALL }.save(dir.path("clip.mp4"));
    let outcome = run(Op::Compress, &clip, false);
    assert!(outcome.notice.ok, "{:?}", outcome.notice);
    assert!(outcome.notice.text.ends_with("% smaller"), "{}", outcome.notice.text);
    assert!(fs::metadata(&outcome.outputs[0]).unwrap().len() < fs::metadata(&clip).unwrap().len());
}

#[test]
fn unshrinkable_clip_is_left_alone() {
    let dir = TempDir::new();
    let clip = Clip::SMALL.save(dir.path("clip.mp4"));
    let outcome = run(Op::Compress, &clip, false);
    assert_eq!((outcome.notice.text.as_str(), outcome.outputs.len()), ("Already compact", 0));
    assert_eq!(names_in(&dir), ["clip.mp4"]);
}

#[test]
fn cancelled_conversion_leaves_nothing() {
    let dir = TempDir::new();
    let clip = Clip::SMALL.save(dir.path("clip.mp4"));
    let outcome = run(Op::Encode(Target::Mp4), &clip, true);
    assert!(!outcome.notice.ok);
    assert_eq!(outcome.notice.text, "Cancelled");
    assert_eq!(names_in(&dir), ["clip.mp4"]);
}
