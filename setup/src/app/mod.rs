//! The installer window: one card that fades between pages, with the music dance along its bottom edge.

mod flow;
mod view;

use std::sync::{Arc, OnceLock};

use egui::{
    Color32, ColorImage, IconData, Id, Rect, Sense, Shape, TextureHandle, TextureOptions, Ui, Vec2, ViewportBuilder,
    ViewportCommand, pos2,
};
use notchbase_paint::canvas::Canvas;
use notchbase_paint::font;
use notchbase_paint::shape::{self, Blob};
use notchbase_paint::spring::lerp;
use notchbase_release::Version;

use crate::dance::{self, Dance};
use crate::install::{self, Installed, Job, Latest, Lookup, Options, Stage, Wake};
use crate::pages::{self, Scene, Task};
use crate::shot::Shot;
use crate::{brand, theme};

use flow::{PAGES, Page};
use image::imageops::FilterType;

const ICON_PX: u32 = 256;
const ICON_TEXTURE: &str = "notchbase";
const FADE: f32 = 0.24;
const LONGEST_FRAME: f32 = 0.1;
const SETTLED_PAGE: f32 = 0.95;
const SHOWN_PAGE: f32 = 0.01;
const PAGE_ENTRY_SCALE: f32 = 0.97;

pub fn options() -> eframe::NativeOptions {
    let icon = icon_image();
    let viewport = ViewportBuilder::default()
        .with_title(brand::TITLE)
        .with_inner_size([theme::WIDTH, theme::height(pages::rows()) + dance::ROOM])
        .with_decorations(false)
        .with_transparent(true)
        .with_resizable(false)
        .with_icon(Arc::new(IconData { rgba: icon.as_raw().clone(), width: icon.width(), height: icon.height() }));
    eframe::NativeOptions { viewport, centered: true, ..Default::default() }
}

fn icon_image() -> &'static image::RgbaImage {
    static ICON: OnceLock<image::RgbaImage> = OnceLock::new();
    ICON.get_or_init(|| {
        image::load_from_memory(brand::ICON)
            .map(|decoded| decoded.resize(ICON_PX, ICON_PX, FilterType::Lanczos3).to_rgba8())
            .unwrap_or_else(|_| image::RgbaImage::new(1, 1))
    })
}

fn pixel_size(points: Vec2, pixels_per_point: f32) -> [i32; 2] {
    let pixels = points * pixels_per_point;
    [pixels.x.round() as i32, pixels.y.round() as i32]
}

fn above_the_dance(window: Rect) -> Rect {
    Rect::from_min_max(window.min, pos2(window.max.x, window.max.y - dance::ROOM))
}

pub struct Setup {
    options: Options,
    lookup: Lookup,
    job: Option<Job>,
    task: Task,
    page: Page,
    stage: Stage,
    problem: String,
    shown: f32,
    version: Option<Version>,
    size: Option<u64>,
    installed: Option<Installed>,
    place: String,
    icon: TextureHandle,
    wake: Wake,
    shot: Option<Shot>,
    flattened: Option<[i32; 2]>,
    dance: Dance,
}

impl Setup {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        font::install(&cc.egui_ctx);
        let ctx = cc.egui_ctx.clone();
        let wake: Wake = Arc::new(move || ctx.request_repaint());
        let image = icon_image();
        let pixels =
            ColorImage::from_rgba_unmultiplied([image.width() as usize, image.height() as usize], image.as_raw());
        let icon = cc.egui_ctx.load_texture(ICON_TEXTURE, pixels, TextureOptions::LINEAR);
        let mut setup = Self {
            options: Options::default(),
            lookup: Lookup::start(Arc::clone(&wake)),
            job: None,
            task: Task::Install,
            page: Page::Welcome,
            stage: Stage::Downloading,
            problem: String::new(),
            shown: 0.0,
            version: None,
            size: None,
            installed: install::installed(),
            place: view::place(),
            icon,
            wake,
            shot: None,
            flattened: None,
            dance: Dance::still(),
        };
        match Shot::from_env() {
            Some(shot) => {
                setup.pin(shot.pin);
                setup.shot = Some(shot);
            }
            None => setup.dance = Dance::start(Arc::clone(&setup.wake)),
        }
        setup
    }
}

impl eframe::App for Setup {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [0.0; 4]
    }

    fn ui(&mut self, ui: &mut Ui, frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let size = pixel_size(ui.max_rect().size(), ctx.pixels_per_point());
        if self.flattened != Some(size) {
            self.flattened = Some(size);
            crate::window::flatten(frame, size);
        }
        let dt = ctx.input(|input| input.stable_dt.min(LONGEST_FRAME));
        if self.shot.is_none() {
            self.advance(dt);
        }
        let latest = self.lookup.latest();
        let rect = above_the_dance(ui.max_rect());
        let wave = self.dance.wave(dt);
        let backdrop = ui.interact(rect, Id::new("setup.drag"), Sense::click_and_drag());
        if backdrop.drag_started() {
            ctx.send_viewport_cmd(ViewportCommand::StartDrag);
        }
        let blob =
            Blob { rect, top_radius: theme::RADIUS, bottom_radius: theme::RADIUS, ear: 0.0, bleed_top: false, wave };
        ui.painter().add(Shape::mesh(shape::fill_with(&blob, |_| Color32::BLACK, ui.painter().pixels_per_point())));

        let interactive = self.shot.is_none();
        let mut intents = Vec::new();
        if self.header(ui, rect, interactive) {
            intents.push(pages::Intent::Close);
        }
        self.tile(ui, rect, &latest);
        for page in PAGES {
            let alpha = ctx.animate_bool_with_time(Id::new(("setup.page", page)), self.page == page, FADE);
            if alpha <= SHOWN_PAGE {
                continue;
            }
            let mut canvas = Canvas::new(ui.painter());
            let scene = Scene { ui, rect, live: interactive && alpha > SETTLED_PAGE && self.page == page };
            intents.extend(self.page_ui(&scene, &mut canvas, page, &latest));
            let anchor = pos2(rect.center().x, rect.top() + theme::card_top());
            canvas.paint(ui.painter(), anchor, lerp(PAGE_ENTRY_SCALE, 1.0, alpha), Vec2::ZERO, alpha);
        }
        intents.extend(interactive.then(|| self.keys(&ctx, &latest)).flatten());
        for intent in intents {
            self.act(intent, &ctx);
        }
        if let Some(shot) = &mut self.shot {
            shot.take(&ctx);
        }
        if wave.is_some() || self.job.is_some() || self.shot.is_some() || matches!(latest, Latest::Checking) {
            ctx.request_repaint();
        }
    }
}

#[cfg(test)]
mod tests {
    use egui::vec2;

    use super::*;

    #[test]
    fn window_is_a_fixed_clear_card() {
        let options = options();
        let viewport = &options.viewport;
        assert_eq!(viewport.title.as_deref(), Some(brand::TITLE));
        assert_eq!(viewport.inner_size, Some(vec2(theme::WIDTH, theme::height(pages::rows()) + dance::ROOM)));
        assert_eq!(viewport.decorations, Some(false));
        assert_eq!(viewport.transparent, Some(true));
        assert_eq!(viewport.resizable, Some(false));
        assert!(options.centered);
    }

    #[test]
    fn window_icon_is_full_size() {
        let options = options();
        let icon = options.viewport.icon.expect("the window has an icon");
        assert_eq!((icon.width, icon.height), (ICON_PX, ICON_PX));
        assert_eq!(icon.rgba.len(), (ICON_PX * ICON_PX * 4) as usize);
        assert_eq!(icon.rgba, *icon_image().as_raw());
        assert!(icon.rgba.chunks(4).any(|pixel| pixel[3] > 0), "the mark is not blank");
    }

    #[test]
    fn icon_is_decoded_once() {
        assert!(std::ptr::eq(icon_image(), icon_image()));
    }

    #[test]
    fn size_is_in_whole_pixels() {
        assert_eq!(pixel_size(vec2(372.0, 400.0), 2.0), [744, 800]);
        assert_eq!(pixel_size(vec2(100.3, 100.25), 1.5), [150, 150]);
        assert_eq!(pixel_size(vec2(100.4, 0.5), 1.25), [126, 1]);
        assert_eq!(pixel_size(Vec2::ZERO, 2.0), [0, 0]);
    }

    #[test]
    fn card_leaves_room_for_the_dance() {
        let window = Rect::from_min_max(pos2(0.0, 0.0), pos2(372.0, 420.0));
        let card = above_the_dance(window);
        assert_eq!(card, Rect::from_min_max(pos2(0.0, 0.0), pos2(372.0, 420.0 - dance::ROOM)));
        assert_eq!(window.height() - card.height(), dance::ROOM);
    }
}
