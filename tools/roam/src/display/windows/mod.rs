//! Extended displays on Windows. A virtual display comes from the Virtual Display Driver: its
//! monitor is turned on at the lent screen's size, captured with desktop duplication and
//! encoded with Media Foundation. A lent screen is covered by a window that decodes and draws
//! the host's pictures.

mod decoder;
mod duplicate;
mod encoder;
mod media;
mod monitor;
mod vdd;
mod viewer;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use windows::Win32::Foundation::POINT;
use windows::Win32::Graphics::Gdi::{
    DEVMODEW, ENUM_CURRENT_SETTINGS, EnumDisplaySettingsW, GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO,
    MONITORINFOEXW, MonitorFromPoint,
};
use windows::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
use windows::core::PCWSTR;

use super::{Ability, Displays, Feed, Hosted, Panel, Picture, RemoteSink, Showing, Want, h264, nv12};
use crate::layout::Rect;

/// Virtual displays run at this rate: what every screen shows, and what encoders keep up with.
const HZ: u32 = 60;
/// How long Windows may take to give a display just turned on to desktop duplication.
const OPEN_WAIT: Duration = Duration::from_secs(5);
/// A still screen still sends a picture this often, so the other side knows it's alive.
const REFRESH_EVERY: Duration = Duration::from_secs(1);

pub struct Windows;

impl Windows {
    /// Turns off any driver screen left on: from a run that ended without tidying up, or the
    /// driver's own at start-up. Roam only turns them on while one is in use.
    pub fn new() -> Self {
        let _ = std::thread::Builder::new().name("roam-display-tidy".into()).spawn(|| {
            if let Err(why) = monitor::detach_all() {
                eprintln!("roam: couldn't turn off the virtual display: {why}");
            }
        });
        Self
    }
}

impl Displays for Windows {
    fn ability(&self) -> Ability {
        if vdd::installed() { Ability::Ready } else { Ability::NeedsDriver }
    }

    fn install(&self) -> Result<(), String> {
        vdd::install()?;
        // The driver turns its monitor on as it starts; it stays off until it's wanted.
        let _ = monitor::detach_all();
        Ok(())
    }

    fn host(&self, want: &Want, feed: Feed) -> Result<Box<dyn Hosted>, String> {
        let (source, size) = ready_source(super::picture_size(want.panel))?;
        monitor::attach(&source, size, HZ, want.at)?;
        let stop = Arc::new(AtomicBool::new(false));
        let failed = Arc::new(Mutex::new(None));
        let (opened, open) = mpsc::channel();
        let thread = {
            let (stop, failed, feed) = (stop.clone(), failed.clone(), feed.clone());
            let name = source.name;
            std::thread::Builder::new()
                .name("roam-display-capture".into())
                .spawn(move || {
                    let result = capture(&name, &feed, &stop, &opened);
                    if let Err(why) = result {
                        if let Ok(mut slot) = failed.lock() {
                            *slot = Some(why.clone());
                        }
                        let _ = opened.send(Err(why));
                    }
                    feed.stop();
                })
                .map_err(|err| err.to_string())?
        };
        // Dropped on the way out if it never starts, which turns the monitor off again.
        let mut shown = Virtual { source, size, stop, failed, thread: Some(thread) };
        match open.recv_timeout(OPEN_WAIT + Duration::from_secs(5)) {
            Ok(Ok(size)) => {
                shown.size = size;
                Ok(Box::new(shown))
            }
            Ok(Err(why)) => Err(why),
            Err(_) => Err("The virtual display didn't start".into()),
        }
    }

    fn panel(&self, rect: Rect) -> Option<Panel> {
        panel_at(rect)
    }

    fn show(&self, rect: Rect, size: [u32; 2], remote: RemoteSink) -> Result<Box<dyn Showing>, String> {
        Ok(Box::new(viewer::Viewer::open(rect, size, remote)?))
    }
}

/// A driver monitor, and the size to show a `want`-sized screen at: the nearest the driver
/// offers, since asking it for new sizes can knock it over. One that's off goes first, so a
/// monitor the person turned on for something else stays theirs.
fn ready_source(want: [u32; 2]) -> Result<(monitor::Source, [u32; 2]), String> {
    let sources = monitor::virtual_sources();
    let off = sources.iter().filter(|s| !s.attached);
    let on = sources.iter().filter(|s| s.attached);
    let ordered: Vec<monitor::Source> = off.chain(on).copied().collect();
    for source in &ordered {
        let sizes: Vec<[u32; 2]> = monitor::modes(source).iter().map(|m| [m[0], m[1]]).collect();
        if let Some(size) = super::closest_size(&sizes, want) {
            return Ok((*source, size));
        }
    }
    // Windows may not list the sizes of a monitor that's off: try the size wanted, and let
    // turning it on say whether it works.
    ordered
        .first()
        .map(|source| (*source, want))
        .ok_or_else(|| "The display driver hasn't made its screen. Try again, and Roam reinstalls it".to_owned())
}

/// Captures the display and encodes it into `feed` until stopped. Answers the picture size
/// on `opened` once it's going.
fn capture(
    name: &[u16; 32],
    feed: &Feed,
    stop: &AtomicBool,
    opened: &mpsc::Sender<Result<[u32; 2], String>>,
) -> Result<(), String> {
    media::start()?;
    let started = Instant::now();
    let mut duplicator = loop {
        match duplicate::Duplicator::open(name) {
            Ok(duplicator) => break duplicator,
            Err(why) if started.elapsed() >= OPEN_WAIT => return Err(why),
            Err(_) => std::thread::sleep(Duration::from_millis(200)),
        }
    };
    let [width, height] = duplicator.size().map(|side| side as u32);
    if width % 2 != 0 || height % 2 != 0 {
        return Err(format!("The virtual display is an odd size ({width}x{height})"));
    }
    let size = [width, height];
    let mut converter = nv12::Converter::new(size, size);
    let mut encoder = encoder::Encoder::new(size, HZ)?;
    let _ = opened.send(Ok(size));
    let every = Duration::from_secs(1) / HZ;
    let mut next = Instant::now();
    let mut sent = Instant::now() - REFRESH_EVERY;
    while !stop.load(Ordering::Acquire) && !feed.stopped() {
        let now = Instant::now();
        next = if next + every < now { now + every } else { next + every };
        duplicator.wait(next)?;
        let key = feed.key_wanted();
        if !(duplicator.changed || key || sent.elapsed() >= REFRESH_EVERY) {
            continue;
        }
        if feed.busy() {
            if key {
                feed.ask_key();
            }
            continue;
        }
        let nv12 = converter.convert(duplicator.frame());
        for data in encoder.encode(nv12, key)? {
            feed.push(Picture { key: h264::is_key(&data), data });
        }
        sent = Instant::now();
    }
    Ok(())
}

/// The driver's monitor while it's in use. Dropping it stops the capture and turns the
/// monitor off.
struct Virtual {
    source: monitor::Source,
    size: [u32; 2],
    stop: Arc<AtomicBool>,
    failed: Arc<Mutex<Option<String>>>,
    thread: Option<JoinHandle<()>>,
}

impl Hosted for Virtual {
    fn rect(&mut self) -> Option<Rect> {
        monitor::rect(&self.source)
    }

    fn place(&mut self, at: [i32; 2]) {
        let _ = monitor::place(&self.source, at);
    }

    fn size(&self) -> [u32; 2] {
        self.size
    }

    fn failed(&self) -> Option<String> {
        self.failed.lock().ok().and_then(|f| f.clone())
    }
}

impl Drop for Virtual {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
        // Every driver screen goes, in case Windows turned on another with this one.
        if let Err(why) = monitor::detach(&self.source).and_then(|()| monitor::detach_all()) {
            eprintln!("roam: couldn't turn off the virtual display: {why}");
        }
    }
}

/// The screen at `rect`, as Windows has it set up now.
fn panel_at(rect: Rect) -> Option<Panel> {
    let center = POINT { x: (rect.x + rect.w / 2.0) as i32, y: (rect.y + rect.h / 2.0) as i32 };
    // SAFETY: plain lookups; `info` is sized as the call expects.
    unsafe {
        let monitor = MonitorFromPoint(center, MONITOR_DEFAULTTONEAREST);
        let mut info = MONITORINFOEXW::default();
        info.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;
        if !GetMonitorInfoW(monitor, (&mut info as *mut MONITORINFOEXW).cast::<MONITORINFO>()).as_bool() {
            return None;
        }
        let mut mode = DEVMODEW { dmSize: std::mem::size_of::<DEVMODEW>() as u16, ..Default::default() };
        let device = PCWSTR(info.szDevice.as_ptr());
        if !EnumDisplaySettingsW(device, ENUM_CURRENT_SETTINGS, &mut mode).as_bool() || mode.dmPelsWidth == 0 {
            return None;
        }
        let (mut dpi, mut unused) = (0, 0);
        let scale = if GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dpi, &mut unused).is_ok() && dpi > 0 {
            dpi as f32 / 96.0
        } else {
            1.0
        };
        Some(Panel {
            pixels: [mode.dmPelsWidth, mode.dmPelsHeight],
            hz: if mode.dmDisplayFrequency > 1 { mode.dmDisplayFrequency } else { HZ },
            scale,
        })
    }
}
