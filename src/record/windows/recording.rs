//! The recording loop on Windows: plan the output, then write each frame as it falls due and pump audio between.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use notchbase_wasapi::{Audio, RATE};

use super::mixer::Mixer;
use super::nv12::Converter;
use super::runtime::{Runtime, performance_ticks};
use super::screen::Screen;
use super::timeline::{Frames, TICKS_PER_SECOND};
use super::writer::{Video, Writer};
use crate::record::job::{Job, Take};

const MAX_SIDE: u32 = 4096;
const MAX_PIXELS: u64 = 4096 * 2304;
const SMOOTH_PIXELS: u64 = 2560 * 1600;
const BITS_PER_PIXEL_PER_FRAME: f64 = 0.1;
const MIN_BITRATE: f64 = 2.0e6;
const MAX_BITRATE: f64 = 40.0e6;
const NOTHING_RECORDED: &str = "Nothing was recorded";
const MIN_FPS: u32 = 1;
const MAX_FPS: u32 = 120;

pub(super) fn record(job: &Job, stop: &AtomicBool, started: &dyn Fn(Instant)) -> Result<Take, String> {
    let _runtime = Runtime::start()?;
    let mut screen = Screen::open(&job.region)?;
    let video = plan(job.region.size, job.max_fps);
    let mut sources = Vec::new();
    if job.microphone {
        sources.push(Audio::microphone(job.microphone_id.as_deref())?);
    }
    if job.desktop {
        sources.push(Audio::desktop()?);
    }
    let mut track = (!sources.is_empty()).then(|| Mixer::new(sources));
    let writer = Writer::create(&job.path, video, track.as_ref().map(|_| RATE))?;
    let mut converter = Converter::new(job.region.size, video.size);
    let start = Instant::now();
    if let Some(track) = &mut track {
        track.start(performance_ticks())?;
    }
    started(start);

    let mut frames = Frames::new(start, video.fps);
    let mut note = None;
    let mut recorded = false;
    let outcome = loop {
        let stopping = stop.load(Ordering::Relaxed);
        if !stopping && let Err(err) = screen.wait(frames.deadline()) {
            break Err(err);
        }
        if let Some(track) = &mut track
            && let Err(err) = track.pump(&writer, ticks_since(start), &mut note)
        {
            break Err(err);
        }
        if let Some(index) = frames.due(Instant::now()) {
            let (time, next) = (frames.time(index), frames.time(index + 1));
            if let Err(err) = writer.video(converter.convert(screen.frame()), time, next - time) {
                break Err(err);
            }
            recorded = true;
        }
        if stopping {
            break Ok(());
        }
    };

    // A failure after the first frame still saves what was recorded, with the error as its note.
    if !recorded {
        drop(writer);
        let _ = std::fs::remove_file(&job.path);
        return Err(outcome.err().unwrap_or_else(|| NOTHING_RECORDED.into()));
    }
    let end = frames.end();
    if let Some(track) = &mut track {
        track.finish(&writer, end)?;
    }
    writer.finish()?;
    Ok(Take { seconds: end as f64 / TICKS_PER_SECOND as f64, note: outcome.err().or(note) })
}

/// Halves outputs over 4096 a side or 4096 by 2304 in all, and drops to 30 fps above 2560 by 1600.
fn plan(crop: [u32; 2], max_fps: Option<u32>) -> Video {
    let [width, height] = crop;
    let oversized = width.max(height) > MAX_SIDE || u64::from(width) * u64::from(height) > MAX_PIXELS;
    let size = if oversized { crop.map(|side| (side / 2) & !1) } else { crop };
    let pixels = u64::from(size[0]) * u64::from(size[1]);
    let fps = max_fps.map_or(if pixels <= SMOOTH_PIXELS { 60 } else { 30 }, |fps| fps.clamp(MIN_FPS, MAX_FPS));
    let bitrate = (BITS_PER_PIXEL_PER_FRAME * pixels as f64 * f64::from(fps)).clamp(MIN_BITRATE, MAX_BITRATE);
    Video { size, fps, bitrate: bitrate as u32 }
}

fn ticks_since(start: Instant) -> i64 {
    (start.elapsed().as_nanos() / 100) as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn big_screens_are_halved_and_slowed() {
        assert_eq!(plan([1920, 1080], None), Video { size: [1920, 1080], fps: 60, bitrate: 12_441_600 });
        assert_eq!(plan([3840, 2160], None).fps, 30);
        assert_eq!(plan([3840, 2160], None).size, [3840, 2160]);
        assert_eq!(plan([5120, 2880], None).size, [2560, 1440]);
        assert_eq!(plan([2160, 3840], None).size, [2160, 3840]);
        assert_eq!(plan([3840, 2160], Some(60)).fps, 60);
        assert_eq!(plan([1920, 1080], Some(30)).fps, 30);
        assert_eq!(plan([64, 64], None).bitrate, MIN_BITRATE as u32);
        assert_eq!(plan([7680, 4320], None).bitrate, 24_883_200);
    }
}
