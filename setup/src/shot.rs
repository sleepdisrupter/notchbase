//! Screenshots of the installer for the docs. Set NOTCHBASE_SETUP_SHOT to a file and
//! NOTCHBASE_SETUP_PAGE to a page, and the window saves itself there once it settles.

use std::path::{Path, PathBuf};

use eframe::egui::{self, ColorImage, Event, UserData, ViewportCommand};

const SHOT: &str = "NOTCHBASE_SETUP_SHOT";
const PAGE: &str = "NOTCHBASE_SETUP_PAGE";
const SETTLE: f64 = 3.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pin {
    Welcome,
    Installed,
    Confirm,
    Working,
    Removing,
    Done,
    Failed,
}

impl Pin {
    fn named(name: &str) -> Self {
        match name.trim() {
            "installed" => Self::Installed,
            "confirm" => Self::Confirm,
            "working" => Self::Working,
            "removing" => Self::Removing,
            "done" => Self::Done,
            "failed" => Self::Failed,
            _ => Self::Welcome,
        }
    }
}

pub struct Shot {
    path: PathBuf,
    pub pin: Pin,
    asked: bool,
}

impl Shot {
    pub fn from_env() -> Option<Self> {
        let path = PathBuf::from(std::env::var_os(SHOT)?);
        let pin = Pin::named(&std::env::var(PAGE).unwrap_or_default());
        Some(Self { path, pin, asked: false })
    }

    pub fn take(&mut self, ctx: &egui::Context) {
        if !self.asked && ctx.input(|input| input.time) > SETTLE {
            self.asked = true;
            ctx.send_viewport_cmd(ViewportCommand::Screenshot(UserData::default()));
        }
        let taken = ctx.input(|input| {
            input.events.iter().find_map(|event| match event {
                Event::Screenshot { image, .. } => Some(image.clone()),
                _ => None,
            })
        });
        if let Some(image) = taken {
            if let Err(err) = save(&self.path, &image) {
                eprintln!("notchbase-setup: couldn't save {}: {err}", self.path.display());
            }
            ctx.send_viewport_cmd(ViewportCommand::Close);
        }
    }
}

fn save(path: &Path, image: &ColorImage) -> image::ImageResult<()> {
    let bytes: Vec<u8> = image.pixels.iter().flat_map(|pixel| pixel.to_srgba_unmultiplied()).collect();
    let [width, height] = image.size.map(|side| side as u32);
    image::save_buffer(path, &bytes, width, height, image::ExtendedColorType::Rgba8)
}

#[cfg(test)]
mod tests {
    use eframe::egui::Color32;

    use super::*;

    #[test]
    fn page_names_pin_their_page() {
        let named = ["installed", "confirm", "working", "removing", "done", "failed"].map(Pin::named);
        assert_eq!(named, [Pin::Installed, Pin::Confirm, Pin::Working, Pin::Removing, Pin::Done, Pin::Failed]);
        assert_eq!(Pin::named("  done\n"), Pin::Done, "surrounding space is ignored");
        assert_eq!(Pin::named(""), Pin::Welcome);
        assert_eq!(Pin::named("Done"), Pin::Welcome, "names are lowercase");
        assert_eq!(Pin::named("welcome"), Pin::Welcome);
    }

    #[test]
    fn screenshot_saves_as_png() {
        let path = std::env::temp_dir().join(format!("notchbase-setup-shot-{}.png", std::process::id()));
        let image = ColorImage::new([3, 2], vec![Color32::from_rgb(10, 20, 30); 6]);
        save(&path, &image).expect("a png");
        let read = image::open(&path).expect("readable").to_rgba8();
        assert_eq!(read.dimensions(), (3, 2));
        assert_eq!(read.get_pixel(2, 1).0, [10, 20, 30, 255]);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn missing_folder_fails_cleanly() {
        let path = std::env::temp_dir().join("notchbase-setup-no-such-folder").join("shot.png");
        assert!(save(&path, &ColorImage::new([1, 1], vec![Color32::BLACK])).is_err());
    }
}
