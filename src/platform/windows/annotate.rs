//! The Annotate window on Windows: an ordinary dark window with the app's icon and nothing over the picture.

use winit::dpi::LogicalSize;
use winit::platform::windows::{IconExtWindows as _, WindowAttributesExtWindows as _};
use winit::window::{Icon, Theme, Window, WindowAttributes};

const NO_TITLE_BAR_OVER_THE_PICTURE: f32 = 0.0;
const TITLE: &str = "Annotate";
const APP_ICON: u16 = 1;

pub fn annotate_window_attributes(size: egui::Vec2, least: egui::Vec2) -> WindowAttributes {
    WindowAttributes::default()
        .with_title(TITLE)
        .with_theme(Some(Theme::Dark))
        .with_resizable(true)
        .with_visible(false)
        .with_inner_size(points(size))
        .with_min_inner_size(points(least))
        .with_drag_and_drop(false)
        .with_window_icon(Icon::from_resource(APP_ICON, None).ok())
}

pub fn configure_annotate_window(_window: &Window) {}

pub fn annotate_title_inset(_window: &Window) -> f32 {
    NO_TITLE_BAR_OVER_THE_PICTURE
}

fn points(size: egui::Vec2) -> LogicalSize<f64> {
    LogicalSize::new(f64::from(size.x), f64::from(size.y))
}
