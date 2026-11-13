use super::fixtures::{BLUE, Clip, RED};
use super::frames::{collect, looks_like, pixel};
use crate::shelf::tests::fixtures::TempDir;
use crate::shelf::video::FrameSpec;

const ONE_IN_FIXED_POINT: i32 = 0x1_0000;

fn display_matrix(movie: &[u8]) -> [i32; 4] {
    let tag = movie.windows(4).position(|window| window == b"tkhd").unwrap();
    let times = if movie[tag + 4] == 1 { 32 } else { 20 };
    let start = tag + 8 + times + 16;
    let entry = |index: usize| {
        let at = start + index * 4;
        i32::from_be_bytes([movie[at], movie[at + 1], movie[at + 2], movie[at + 3]])
    };
    [entry(0), entry(1), entry(3), entry(4)]
}

#[test]
fn quarter_turn_stored_like_a_phone() {
    let dir = TempDir::new();
    let clip = Clip { rotation: 90, audio: false, frames: 5, ..Clip::SMALL }.save(dir.path("portrait.mp4"));
    let movie = std::fs::read(clip).unwrap();
    assert_eq!(display_matrix(&movie), [0, ONE_IN_FIXED_POINT, -ONE_IN_FIXED_POINT, 0]);
}

#[test]
fn sideways_frames_are_turned_upright() {
    let dir = TempDir::new();
    let clip = Clip { rotation: 90, frames: 12, ..Clip::SMALL }.save(dir.path("portrait.mp4"));
    let frames = collect(&clip, FrameSpec { max_side: 160, fps: 12.0, max_seconds: 30.0 });
    let first = frames.first().unwrap();
    assert_eq!((first.width, first.height), (120, 160));
    assert!(looks_like(pixel(first, 110, 80), RED), "{:?}", pixel(first, 110, 80));
    assert!(looks_like(pixel(first, 10, 80), BLUE), "{:?}", pixel(first, 10, 80));
}
