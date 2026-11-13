//! Pictures for tools: 192 px thumbnails of the last 64 captures, full pixels of the last 4, and files by path.

use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::path::Path;
use std::sync::Arc;

use egui::ColorImage;
use notchbase_sdk::protocol::{ImageId, ImageSource};

use crate::capture::Screenshot;

const KEEP_FULL: usize = 4;
const KEEP_THUMBNAILS: usize = 64;
const THUMBNAIL_PX: usize = 192;

struct Stored {
    id: ImageId,
    full: Option<Arc<Screenshot>>,
    thumbnail: Arc<ColorImage>,
}

#[derive(Default)]
pub struct ImageStore {
    stored: VecDeque<Stored>,
    /// Thumbnails of files, loaded once. A file that couldn't load is remembered as missing.
    files: RefCell<HashMap<String, Option<Arc<ColorImage>>>>,
    next: u64,
}

impl ImageStore {
    pub fn insert(&mut self, shot: Screenshot) -> ImageId {
        self.next += 1;
        let id = ImageId(self.next);
        let thumbnail = Arc::new(shot.thumbnail(THUMBNAIL_PX));
        self.stored.push_back(Stored { id, full: Some(Arc::new(shot)), thumbnail });
        for stored in self.stored.iter_mut().rev().skip(KEEP_FULL) {
            stored.full = None;
        }
        while self.stored.len() > KEEP_THUMBNAILS {
            self.stored.pop_front();
        }
        id
    }

    /// A preview's stand-in picture under id 0, which no real capture uses.
    pub fn show_sample(&mut self, thumbnail: ColorImage) {
        let sample = Stored { id: ImageId(0), full: None, thumbnail: Arc::new(thumbnail) };
        self.stored.retain(|stored| stored.id != sample.id);
        self.stored.push_back(sample);
    }

    pub fn full(&self, id: ImageId) -> Option<Arc<Screenshot>> {
        self.stored.iter().find(|stored| stored.id == id).and_then(|stored| stored.full.clone())
    }

    pub fn thumbnail(&self, source: &ImageSource) -> Option<Arc<ColorImage>> {
        match source {
            ImageSource::Capture(id) => {
                self.stored.iter().find(|stored| stored.id == *id).map(|stored| stored.thumbnail.clone())
            }
            ImageSource::File(path) => self
                .files
                .borrow_mut()
                .entry(path.clone())
                .or_insert_with(|| built_in(path).unwrap_or_else(|| load_file(Path::new(path))))
                .clone(),
        }
    }
}

/// Pictures carried inside notchbase, by a `builtin:` name, at their full size so they stay sharp.
pub const DOCK_PICTURE: &str = "builtin:dock-intro";

fn built_in(path: &str) -> Option<Option<Arc<ColorImage>>> {
    let bytes: &[u8] = match path {
        DOCK_PICTURE => include_bytes!("../../assets/dock-intro.png"),
        _ => return None,
    };
    let image = image::load_from_memory(bytes).ok().map(|image| image.into_rgba8());
    Some(image.map(|image| {
        let size = [image.width() as usize, image.height() as usize];
        Arc::new(ColorImage::from_rgba_unmultiplied(size, image.as_raw()))
    }))
}

fn load_file(path: &Path) -> Option<Arc<ColorImage>> {
    let image = image::open(path).ok()?.thumbnail(THUMBNAIL_PX as u32, THUMBNAIL_PX as u32).into_rgba8();
    let size = [image.width() as usize, image.height() as usize];
    Some(Arc::new(ColorImage::from_rgba_unmultiplied(size, image.as_raw())))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shot(side: usize) -> Screenshot {
        Screenshot { width: side, height: side, rgba: vec![200; side * side * 4] }
    }

    #[test]
    fn only_latest_captures_keep_pixels() {
        let mut store = ImageStore::default();
        let ids: Vec<ImageId> = (0..6).map(|_| store.insert(shot(8))).collect();
        assert!(store.full(ids[0]).is_none() && store.full(ids[1]).is_none());
        assert!(ids[2..].iter().all(|&id| store.full(id).is_some()));
        assert!(store.thumbnail(&ImageSource::Capture(ids[0])).is_some());
        assert!(store.thumbnail(&ImageSource::Capture(ImageId(999))).is_none());
    }

    #[test]
    fn built_in_picture_keeps_its_pixels() {
        let store = ImageStore::default();
        let picture = store.thumbnail(&ImageSource::File(DOCK_PICTURE.into())).expect("bundled");
        assert!(picture.size[0] >= 600, "sharp at twice its size in the notch, not a 192 px thumbnail");
    }

    #[test]
    fn missing_file_is_remembered() {
        let store = ImageStore::default();
        let source = ImageSource::File("definitely/not/here.png".into());
        assert!(store.thumbnail(&source).is_none());
        assert!(store.files.borrow().contains_key("definitely/not/here.png"));
    }
}
