//! Extended displays on a Mac. A virtual display is a `CGVirtualDisplay` placed where the lent
//! screen sits, captured with ScreenCaptureKit and encoded with VideoToolbox. A lent screen is
//! covered by a window that shows the host's pictures.

mod capture;
mod keep_off;
mod screen;
mod viewer;

use std::time::Duration;

use super::{Ability, Displays, Feed, Hosted, Panel, Picture, PointerAt, PointerShape, RemoteSink, Showing, Want};
use crate::layout::Rect;

/// Virtual displays run at this rate: what every screen shows, and what the encoder keeps up with.
const HZ: u32 = 60;

pub struct Mac;

impl Displays for Mac {
    fn ability(&self) -> Ability {
        if screen::available() {
            Ability::Ready
        } else {
            Ability::No("This Mac's macOS is too old for extended displays".into())
        }
    }

    fn host(&self, want: &Want, feed: Feed) -> Result<Box<dyn Hosted>, String> {
        // As many points as the lent screen takes on the desk, so the pointer crosses its edges
        // where it crosses the screen's own when nothing is lent. A Retina-style screen gets a
        // Retina-style display, twice the pixels; the pictures go out at the screen's own.
        let points = want.units.map(|side| side.max(64) & !1);
        let hidpi =
            want.panel.scale >= 1.5 && points[0] * 2 <= super::MOST_UNITS[0] && points[1] * 2 <= super::MOST_UNITS[1];
        let pixels = if hidpi { points.map(|side| side * 2) } else { points };
        let size = super::encode_size(pixels, want.panel);
        let display = screen::Virtual::new(&format!("{} (Roam)", want.name), pixels, HZ, hidpi)?;
        display.place(want.at);
        let capture = capture::Capture::start(display.id(), size, HZ, feed)?;
        Ok(Box::new(Virtual { capture: Some(capture), display, size }))
    }

    fn panel(&self, rect: Rect) -> Option<Panel> {
        screen::panel(rect)
    }

    fn draws_pointer(&self) -> bool {
        true
    }

    fn show(&self, rect: Rect, size: [u32; 2], remote: RemoteSink) -> Result<Box<dyn Showing>, String> {
        let viewer = viewer::Viewer::open(rect, size, remote)?;
        Ok(Box::new(Lent { viewer, _keep_off: keep_off::KeepOff::start(rect) }))
    }
}

/// A lent screen: the host's pictures over it, and this Mac's own windows kept off it.
struct Lent {
    viewer: viewer::Viewer,
    _keep_off: keep_off::KeepOff,
}

impl Showing for Lent {
    fn picture(&mut self, picture: &Picture) -> Result<(), String> {
        self.viewer.picture(picture)
    }

    fn pointer(&mut self, at: PointerAt) {
        self.viewer.pointer(at);
    }

    fn shape(&mut self, shape: &PointerShape) {
        self.viewer.shape(shape);
    }

    fn take_pointer_lag(&mut self) -> Option<Duration> {
        self.viewer.take_pointer_lag()
    }
}

/// The virtual display while it's in use. Dropping it stops the capture, then the display.
struct Virtual {
    capture: Option<capture::Capture>,
    display: screen::Virtual,
    size: [u32; 2],
}

impl Hosted for Virtual {
    fn rect(&mut self) -> Option<Rect> {
        self.display.rect()
    }

    fn place(&mut self, at: [i32; 2]) {
        self.display.place(at);
    }

    fn size(&self) -> [u32; 2] {
        self.size
    }
}

impl Drop for Virtual {
    fn drop(&mut self) {
        drop(self.capture.take());
    }
}
