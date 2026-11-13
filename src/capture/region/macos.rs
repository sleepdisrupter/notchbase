//! Screenshots on macOS. The live picker chooses the area, then `screencapture` grabs it into a temporary
//! PNG, which is read back and deleted.

use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};

use winit::event::WindowEvent;
use winit::event_loop::ActiveEventLoop;
use winit::window::WindowId;

use super::PickerOutcome;
use super::live;
use crate::capture::Screenshot;
use crate::record::Region;

const SCREENCAPTURE: &str = "/usr/sbin/screencapture";

/// Keeps the temporary file names apart within a run.
static SHOTS: AtomicU32 = AtomicU32::new(0);

pub struct Picker {
    live: live::Picker,
    grab: Option<Grab>,
    outcome: Option<PickerOutcome>,
}

impl Picker {
    pub fn new() -> Self {
        Self { live: live::Picker::new(), grab: None, outcome: None }
    }

    pub fn is_active(&self) -> bool {
        self.live.is_active() || self.grab.is_some()
    }

    pub fn start(&mut self, event_loop: &ActiveEventLoop) -> Result<(), String> {
        if self.grab.is_some() {
            return Ok(());
        }
        self.live.start(event_loop)
    }

    pub fn cancel(&mut self) {
        self.live.cancel();
        if self.grab.take().is_some() {
            self.outcome = Some(PickerOutcome::Cancelled);
        }
    }

    pub fn owns(&self, id: WindowId) -> bool {
        self.live.owns(id)
    }

    pub fn window_event(&mut self, id: WindowId, event: &WindowEvent) {
        self.live.window_event(id, event);
        self.follow_live();
    }

    pub fn about_to_wait(&mut self) {
        self.live.about_to_wait();
        self.follow_live();
        if let Some(grab) = self.grab.take_if(|grab| grab.has_exited()) {
            self.outcome = Some(grab.finish().map_or(PickerOutcome::Cancelled, PickerOutcome::Region));
        }
    }

    pub fn take_outcome(&mut self) -> Option<PickerOutcome> {
        self.outcome.take()
    }

    fn follow_live(&mut self) {
        match self.live.take_outcome() {
            Some(PickerOutcome::Area(region)) => match Grab::start(&region) {
                Ok(grab) => self.grab = Some(grab),
                Err(err) => {
                    eprintln!("notchbase: screenshot failed: {err}");
                    self.outcome = Some(PickerOutcome::Cancelled);
                }
            },
            Some(outcome) => self.outcome = Some(outcome),
            None => {}
        }
    }
}

struct Grab {
    screencapture: Child,
    file: PathBuf,
}

impl Grab {
    fn start(region: &Region) -> Result<Self, String> {
        let file = std::env::temp_dir().join(format!(
            "notchbase-shot-{}-{}.png",
            std::process::id(),
            SHOTS.fetch_add(1, Ordering::Relaxed)
        ));
        let screencapture = Command::new(SCREENCAPTURE)
            // Silently, as a PNG, of just that rectangle.
            .arg("-x")
            .arg("-tpng")
            .arg(format!("-R{}", rect_in_points(region).map(|side| side.to_string()).join(",")))
            .arg(&file)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|err| format!("couldn't run screencapture: {err}"))?;
        Ok(Self { screencapture, file })
    }

    fn has_exited(&mut self) -> bool {
        !matches!(self.screencapture.try_wait(), Ok(None))
    }

    fn finish(self) -> Option<Screenshot> {
        let image = image::open(&self.file).ok()?.into_rgba8();
        let (width, height) = (image.width() as usize, image.height() as usize);
        (width > 0 && height > 0).then(|| Screenshot { width, height, rgba: image.into_raw() })
    }
}

impl Drop for Grab {
    fn drop(&mut self) {
        let _ = self.screencapture.kill();
        let _ = self.screencapture.wait();
        let _ = std::fs::remove_file(&self.file);
    }
}

/// The selection in global points, as `screencapture -R` wants it.
fn rect_in_points(region: &Region) -> [i64; 4] {
    let scale = region.display.scale.max(1.0);
    let point = |pixels: f64| (pixels / scale).round() as i64;
    let x = f64::from(region.display.origin[0]) + f64::from(region.origin[0]);
    let y = f64::from(region.display.origin[1]) + f64::from(region.origin[1]);
    let [width, height] = region.size.map(|side| point(f64::from(side)).max(1));
    [point(x), point(y), width, height]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::record::Display;

    #[test]
    fn selection_becomes_global_points() {
        let display = Display { id: 2, origin: [-2880, 400], size: [2880, 1800], scale: 2.0 };
        let region = Region { display, origin: [200, 100], size: [801, 400] };
        assert_eq!(rect_in_points(&region), [-1340, 250, 401, 200]);
        let whole =
            Region { display: Display { scale: 1.0, origin: [0, 0], ..display }, origin: [0, 0], size: [1440, 900] };
        assert_eq!(rect_in_points(&whole), [0, 0, 1440, 900]);
    }
}
