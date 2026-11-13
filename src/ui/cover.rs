//! Album art that slides between songs. A song without its art yet keeps the old cover for up to 1.2 seconds.

use std::sync::Arc;
use std::time::{Duration, Instant};

use egui::{ColorImage, TextureHandle, TextureOptions};

use super::handover::{Handover, Part};

const WAIT: Duration = Duration::from_millis(1200);

#[derive(Clone)]
pub struct Art {
    pub image: Arc<ColorImage>,
    pub texture: Option<TextureHandle>,
}

pub struct Cover {
    name: &'static str,
    shown: Option<Art>,
    handover: Handover<Option<Art>>,
    due: Option<(Instant, f32)>,
}

impl Cover {
    pub fn new(name: &'static str, image: Option<&Arc<ColorImage>>) -> Self {
        let shown = image.map(|image| Art { image: image.clone(), texture: None });
        Self { name, shown, handover: Handover::default(), due: None }
    }

    pub fn song_changed(&mut self, image: Option<&Arc<ColorImage>>, direction: f32, now: Instant) -> bool {
        match image {
            None if self.shown.is_some() => {
                self.due = Some((now + WAIT, direction));
                false
            }
            image => self.swap(image, direction, now),
        }
    }

    pub fn art_changed(&mut self, image: Option<&Arc<ColorImage>>, now: Instant) -> bool {
        if image.is_none() && self.due.is_some() {
            return false;
        }
        let direction = self.due.map_or(0.0, |(_, direction)| direction);
        self.swap(image, direction, now)
    }

    pub fn tick(&mut self, now: Instant) -> bool {
        match self.due {
            Some((due, direction)) if now >= due => self.swap(None, direction, now),
            _ => false,
        }
    }

    pub fn wakeup(&self) -> Option<Instant> {
        self.due.map(|(due, _)| due)
    }

    pub fn awaits(&self) -> bool {
        self.due.is_some()
    }

    fn swap(&mut self, image: Option<&Arc<ColorImage>>, direction: f32, now: Instant) -> bool {
        self.due = None;
        let next = image.map(|image| Art { image: image.clone(), texture: None });
        let previous = std::mem::replace(&mut self.shown, next);
        self.handover.hand_over(previous, direction, now);
        true
    }

    pub fn upload(&mut self, ctx: &egui::Context) {
        if let Some(art) = self.shown.as_mut().filter(|art| art.texture.is_none()) {
            art.texture = Some(ctx.load_texture(self.name, (*art.image).clone(), TextureOptions::LINEAR));
        }
    }

    pub fn shown(&self) -> Option<&Art> {
        self.shown.as_ref()
    }

    pub fn leaving(&self, now: Instant) -> Option<(Option<&Art>, Part)> {
        self.handover.leaving(now).map(|(art, part)| (art.as_ref(), part))
    }

    pub fn arriving(&self, now: Instant) -> Part {
        self.handover.arriving(now)
    }

    pub fn is_moving(&self, now: Instant) -> bool {
        self.handover.is_moving(now)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn art() -> Arc<ColorImage> {
        Arc::new(ColorImage::new([2, 2], vec![egui::Color32::RED; 4]))
    }

    fn shows(cover: &Cover, image: &Arc<ColorImage>) -> bool {
        cover.shown().is_some_and(|art| Arc::ptr_eq(&art.image, image))
    }

    #[test]
    fn new_song_keeps_old_cover() {
        let (t0, old, new) = (Instant::now(), art(), art());
        let mut cover = Cover::new("test", Some(&old));
        assert!(!cover.song_changed(None, 1.0, t0), "no placeholder flash while the artwork is on its way");
        assert!(shows(&cover, &old) && cover.awaits() && !cover.is_moving(t0));
        assert_eq!(cover.wakeup(), Some(t0 + WAIT));
        assert!(cover.art_changed(Some(&new), t0 + Duration::from_millis(300)));
        assert!(shows(&cover, &new) && !cover.awaits());
        let later = t0 + Duration::from_millis(300);
        let (leaving, part) = cover.leaving(later).expect("the old cover slides out");
        assert!(leaving.is_some_and(|art| Arc::ptr_eq(&art.image, &old)) && part.alpha > 0.99);
        assert_eq!(cover.arriving(later), Part { alpha: 0.0, shift: 1.0 }, "the new one slides in the skip's way");
    }

    #[test]
    fn no_artwork_falls_back_to_placeholder() {
        let (t0, old) = (Instant::now(), art());
        let mut cover = Cover::new("test", Some(&old));
        cover.song_changed(None, -1.0, t0);
        assert!(!cover.tick(t0 + WAIT / 2));
        assert!(cover.tick(t0 + WAIT));
        assert!(cover.shown().is_none() && cover.wakeup().is_none());
        assert!(cover.arriving(t0 + WAIT).shift < -0.99);
    }

    #[test]
    fn first_artwork_fades_in_place() {
        let (t0, new) = (Instant::now(), art());
        let mut cover = Cover::new("test", None);
        assert!(cover.art_changed(Some(&new), t0));
        assert_eq!(cover.arriving(t0).shift, 0.0);
        assert!(cover.leaving(t0).is_some_and(|(art, _)| art.is_none()), "the placeholder fades out under it");
    }

    #[test]
    fn song_with_artwork_slides_at_once() {
        let (t0, old, new) = (Instant::now(), art(), art());
        let mut cover = Cover::new("test", Some(&old));
        assert!(cover.song_changed(Some(&new), 1.0, t0));
        assert!(shows(&cover, &new) && cover.is_moving(t0));
    }
}
