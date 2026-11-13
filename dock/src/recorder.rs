//! A flight recorder for minimizing and restoring, for finding glitches that only show on someone's real screen.
//! Armed by a number in `record-flights` in the data folder: each flight then films the screen (half size, as fast
//! as it can) from its start until a little after it ends, with a log of what the dock did at each moment, into
//! `.claude/work/dock/shots/` beside the repo when there is one. Off when the file is missing.

use std::io::Write as _;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant};

struct Rec {
    dir: PathBuf,
    start: Instant,
    until: Instant,
    log: Vec<String>,
}

static REC: Mutex<Option<Rec>> = Mutex::new(None);

fn armed_file() -> PathBuf {
    crate::lifecycle::data_dir().join("record-flights")
}

/// Where recordings go: the repo's work folder when the dock runs from a build in the repo.
fn out_dir() -> PathBuf {
    let from_repo = std::env::current_exe().ok().and_then(|exe| {
        exe.ancestors().find(|p| p.join(".claude/work/dock").is_dir()).map(|p| p.join(".claude/work/dock/shots"))
    });
    from_repo.unwrap_or_else(|| crate::lifecycle::data_dir().join("recordings"))
}

/// Starts recording a flight if armed, using up one of the armed recordings.
pub fn start(label: &str, screen: egui::Vec2) {
    let Ok(text) = std::fs::read_to_string(armed_file()) else { return };
    let left: u32 = text.trim().parse().unwrap_or(0);
    if left == 0 {
        return;
    }
    let _ = std::fs::write(armed_file(), (left - 1).to_string());
    let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs();
    let dir = out_dir().join(format!("rec-{label}-{stamp}"));
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    let start = Instant::now();
    {
        let mut rec = REC.lock().unwrap_or_else(|e| e.into_inner());
        if rec.is_some() {
            return;
        }
        *rec = Some(Rec { dir: dir.clone(), start, until: start + Duration::from_secs(3), log: Vec::new() });
    }
    let _ = std::thread::Builder::new().name("recorder".into()).spawn(move || film(dir, start, screen));
}

/// Keeps filming until `after` from now, then stops.
pub fn end_after(after: Duration) {
    if let Some(rec) = REC.lock().unwrap_or_else(|e| e.into_inner()).as_mut() {
        rec.until = Instant::now() + after;
    }
}

/// Notes what the dock is doing now, while recording.
pub fn event(what: impl FnOnce() -> String) {
    if let Some(rec) = REC.lock().unwrap_or_else(|e| e.into_inner()).as_mut() {
        let ms = rec.start.elapsed().as_secs_f64() * 1000.0;
        rec.log.push(format!("{ms:8.1} {}", what()));
    }
}

fn film(dir: PathBuf, start: Instant, screen: egui::Vec2) {
    let mut n = 0;
    loop {
        let until = REC.lock().unwrap_or_else(|e| e.into_inner()).as_ref().map(|r| r.until);
        if until.is_none_or(|u| Instant::now() > u) {
            break;
        }
        let at = start.elapsed().as_secs_f64() * 1000.0;
        if let Some(image) = crate::windows::screen_picture(screen, 478) {
            let name = format!("{n:03}-{at:06.1}ms.png");
            write_png(&dir.join(&name), &image);
            event(|| format!("frame {name}"));
            n += 1;
        }
    }
    let rec = REC.lock().unwrap_or_else(|e| e.into_inner()).take();
    if let Some(rec) = rec
        && let Ok(mut f) = std::fs::File::create(rec.dir.join("events.txt"))
    {
        for line in rec.log {
            let _ = writeln!(f, "{line}");
        }
    }
}

fn write_png(path: &std::path::Path, image: &egui::ColorImage) {
    let Ok(file) = std::fs::File::create(path) else { return };
    let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), image.size[0] as u32, image.size[1] as u32);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.set_compression(png::Compression::Fast);
    let Ok(mut writer) = encoder.write_header() else { return };
    let bytes: Vec<u8> = image.pixels.iter().flat_map(|c| c.to_array()).collect();
    let _ = writer.write_image_data(&bytes);
}
