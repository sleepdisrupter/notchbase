use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use windows::Win32::Graphics::Dxgi::{CreateDXGIFactory1, IDXGIFactory1};

use super::gdi::Grabber;
use super::session::Session;
use crate::record::job::{Job, Take};
use crate::record::{Display, Region};

fn primary() -> Display {
    let factory: IDXGIFactory1 = unsafe { CreateDXGIFactory1() }.unwrap();
    let adapter = unsafe { factory.EnumAdapters1(0) }.unwrap();
    let output = unsafe { adapter.EnumOutputs(0) }.unwrap();
    let rect = unsafe { output.GetDesc() }.unwrap().DesktopCoordinates;
    let size = [rect.right - rect.left, rect.bottom - rect.top].map(|side| side as u32);
    Display { id: 0, origin: [rect.left, rect.top], size, scale: 1.0 }
}

fn scratch(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("notchbase-record-{}-{name}.mp4", std::process::id()))
}

fn record_for(path: &Path, region: Region, desktop: bool, length: Duration) -> Take {
    let job = Job { path: path.to_path_buf(), region, microphone: false, desktop, microphone_id: None, max_fps: None };
    let mut session = Session::start(job, Arc::new(|| {})).unwrap();
    while session.started().is_none() {
        if let Some(result) = session.finished() {
            panic!("the recording never started: {:?}", result.err());
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    std::thread::sleep(length);
    session.stop();
    loop {
        if let Some(result) = session.finished() {
            return result.unwrap();
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn find(bytes: &[u8], tag: &[u8]) -> Option<usize> {
    bytes.windows(tag.len()).position(|window| window == tag).map(|at| at + tag.len())
}

fn be32(bytes: &[u8], at: usize) -> u32 {
    u32::from_be_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
}

fn movie_seconds(bytes: &[u8]) -> f64 {
    let mvhd = find(bytes, b"mvhd").unwrap();
    f64::from(be32(bytes, mvhd + 16)) / f64::from(be32(bytes, mvhd + 12))
}

fn video_size(bytes: &[u8]) -> [u32; 2] {
    let tkhd = find(bytes, b"tkhd").unwrap();
    [be32(bytes, tkhd + 76) >> 16, be32(bytes, tkhd + 80) >> 16]
}

#[test]
#[ignore = "records the real screen and desktop audio"]
fn area_with_desktop_audio_is_mp4() {
    let path = scratch("area");
    let region = Region::new(primary(), [100, 80], [641, 361]);
    let take = record_for(&path, region, true, Duration::from_millis(1500));
    let bytes = std::fs::read(&path).unwrap();
    std::fs::remove_file(&path).unwrap();
    assert!((1.3..2.2).contains(&take.seconds), "{take:?}");
    assert_eq!(take.note, None);
    assert!((movie_seconds(&bytes) - take.seconds).abs() < 0.3, "{}", movie_seconds(&bytes));
    assert_eq!(video_size(&bytes), [640, 360]);
    assert!(find(&bytes, b"vide").is_some() && find(&bytes, b"soun").is_some());
}

#[test]
#[ignore = "records the real screen"]
fn display_without_audio_is_video_only() {
    let path = scratch("display");
    let take = record_for(&path, Region::whole(primary()), false, Duration::from_millis(700));
    let bytes = std::fs::read(&path).unwrap();
    std::fs::remove_file(&path).unwrap();
    assert!(take.seconds > 0.5, "{take:?}");
    assert_eq!(video_size(&bytes), Region::whole(primary()).size);
    assert!(find(&bytes, b"soun").is_none());
}

#[test]
#[ignore = "reads the real screen"]
fn gdi_fallback_fills_whole_frames() {
    let region = Region::new(primary(), [0, 0], [320, 200]);
    let mut grabber = Grabber::open(&region).unwrap();
    let frame = grabber.frame();
    assert_eq!(frame.len(), 320 * 200 * 4);
    assert!(frame.chunks_exact(4).all(|px| px[3] == 255));
}
