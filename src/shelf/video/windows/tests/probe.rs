use super::about;
use super::fixtures::Clip;
use crate::shelf::tests::fixtures::TempDir;
use crate::shelf::video::windows::probe;

#[test]
fn probe_reports_size_and_duration() {
    let dir = TempDir::new();
    let clip = Clip::SMALL.save(dir.path("clip.mp4"));
    let found = probe(&clip).unwrap();
    assert_eq!(found.dims, Some([320, 240]));
    assert!(about(found.seconds, 2.0), "{:?}", found.seconds);
}

#[test]
fn silent_video_still_probes() {
    let dir = TempDir::new();
    let clip = Clip { audio: false, frames: 15, ..Clip::SMALL }.save(dir.path("silent.mp4"));
    let found = probe(&clip).unwrap();
    assert_eq!(found.dims, Some([320, 240]));
    assert!(about(found.seconds, 0.5), "{:?}", found.seconds);
}

#[test]
fn sideways_videos_report_upright() {
    let dir = TempDir::new();
    let clip = Clip { rotation: 90, frames: 15, ..Clip::SMALL }.save(dir.path("portrait.mp4"));
    assert_eq!(probe(&clip).unwrap().dims, Some([240, 320]));
}
