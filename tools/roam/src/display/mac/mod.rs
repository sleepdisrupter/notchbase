//! Extended displays on a Mac. A virtual display is a `CGVirtualDisplay` placed where the lent
//! screen sits, captured with ScreenCaptureKit and encoded with VideoToolbox. A lent screen is
//! covered by a window that shows the host's pictures.

mod capture;
mod screen;
mod viewer;

use super::{Ability, Displays, Feed, Hosted, Panel, RemoteSink, Showing, Want};
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
        let size = super::picture_size(want.panel);
        // A Retina-style screen gets a Retina-style display: the same pixels, half the points.
        let hidpi = want.panel.scale >= 1.5;
        let display = screen::Virtual::new(&format!("{} (Roam)", want.name), size, HZ, hidpi)?;
        display.place(want.at);
        let capture = capture::Capture::start(display.id(), size, HZ, feed)?;
        Ok(Box::new(Virtual { capture: Some(capture), display, size }))
    }

    fn panel(&self, rect: Rect) -> Option<Panel> {
        screen::panel(rect)
    }

    fn show(&self, rect: Rect, size: [u32; 2], remote: RemoteSink) -> Result<Box<dyn Showing>, String> {
        Ok(Box::new(viewer::Viewer::open(rect, size, remote)?))
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
