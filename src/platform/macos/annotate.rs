//! The Annotate window on a Mac: dark and frosted, with the title bar hidden over the picture, and opening on the
//! Space in use, even beside a full screen app.

use objc2::rc::Retained;
use objc2::{MainThreadMarker, MainThreadOnly as _};
use objc2_app_kit::{
    NSAppearance, NSAppearanceCustomization as _, NSAppearanceNameDarkAqua, NSAutoresizingMaskOptions, NSView,
    NSVisualEffectBlendingMode, NSVisualEffectMaterial, NSVisualEffectState, NSVisualEffectView, NSWindow,
    NSWindowCollectionBehavior, NSWindowOrderingMode,
};
use objc2_core_foundation::CGFloat;
use winit::dpi::LogicalSize;
use winit::platform::macos::WindowAttributesExtMacOS as _;
use winit::window::{Window, WindowAttributes, WindowLevel};

use super::appkit::ns_window;

const USUAL_TITLE_BAR: f32 = 28.0;
const TITLE: &str = "Annotate";
const FROST: NSVisualEffectMaterial = NSVisualEffectMaterial::HUDWindow;

pub fn annotate_window_attributes(size: egui::Vec2, least: egui::Vec2) -> WindowAttributes {
    WindowAttributes::default()
        .with_title(TITLE)
        .with_resizable(true)
        .with_visible(false)
        .with_transparent(true)
        .with_window_level(WindowLevel::AlwaysOnTop)
        .with_inner_size(points(size))
        .with_min_inner_size(points(least))
        .with_titlebar_transparent(true)
        .with_fullsize_content_view(true)
        .with_title_hidden(true)
        .with_accepts_first_mouse(false)
}

pub fn configure_annotate_window(window: &Window) {
    let Some(ns_window) = ns_window(window) else { return };
    wear_dark(&ns_window);
    open_on_the_space_in_use(&ns_window);
    if !is_frosted(&ns_window) {
        frost(&ns_window);
    }
}

pub fn annotate_title_inset(window: &Window) -> f32 {
    let Some(ns_window) = ns_window(window) else { return USUAL_TITLE_BAR };
    title_bar_height(ns_window.frame().size.height, ns_window.contentLayoutRect().size.height)
}

fn title_bar_height(window: CGFloat, clear_of_the_title_bar: CGFloat) -> f32 {
    let height = (window - clear_of_the_title_bar) as f32;
    if height.is_finite() && height > 0.0 { height } else { USUAL_TITLE_BAR }
}

fn points(size: egui::Vec2) -> LogicalSize<f64> {
    LogicalSize::new(f64::from(size.x), f64::from(size.y))
}

fn wear_dark(window: &NSWindow) {
    if window.appearance().is_some() {
        return;
    }
    let dark = NSAppearance::appearanceNamed(unsafe { NSAppearanceNameDarkAqua });
    window.setAppearance(dark.as_deref());
}

fn open_on_the_space_in_use(window: &NSWindow) {
    let behavior = beside_a_full_screen_app();
    if window.collectionBehavior() != behavior {
        window.setCollectionBehavior(behavior);
    }
}

fn beside_a_full_screen_app() -> NSWindowCollectionBehavior {
    NSWindowCollectionBehavior::FullScreenAuxiliary | NSWindowCollectionBehavior::MoveToActiveSpace
}

fn frame_of(window: &NSWindow) -> Option<(Retained<NSView>, Retained<NSView>)> {
    let content = window.contentView()?;
    let frame = unsafe { content.superview() }?;
    Some((frame, content))
}

fn is_frosted(window: &NSWindow) -> bool {
    let Some((frame, _)) = frame_of(window) else { return false };
    frame.subviews().iter().any(|view| view.downcast::<NSVisualEffectView>().is_ok_and(|effect| is_frost(&effect)))
}

fn is_frost(effect: &NSVisualEffectView) -> bool {
    effect.material() == FROST && effect.blendingMode() == NSVisualEffectBlendingMode::BehindWindow
}

fn frost(window: &NSWindow) {
    let (Some(mtm), Some((frame, content))) = (MainThreadMarker::new(), frame_of(window)) else { return };
    let effect = NSVisualEffectView::initWithFrame(NSVisualEffectView::alloc(mtm), content.frame());
    effect.setMaterial(FROST);
    effect.setBlendingMode(NSVisualEffectBlendingMode::BehindWindow);
    effect.setState(NSVisualEffectState::Active);
    effect.setAutoresizingMask(
        NSAutoresizingMaskOptions::ViewWidthSizable | NSAutoresizingMaskOptions::ViewHeightSizable,
    );
    frame.addSubview_positioned_relativeTo(&effect, NSWindowOrderingMode::Below, Some(&content));
}

#[cfg(test)]
mod tests {
    use super::*;

    const WINDOW: CGFloat = 700.0;

    #[test]
    fn opens_on_the_space_in_use() {
        let behavior = beside_a_full_screen_app();
        assert!(behavior.contains(NSWindowCollectionBehavior::FullScreenAuxiliary));
        assert!(behavior.contains(NSWindowCollectionBehavior::MoveToActiveSpace));
        let alone = NSWindowCollectionBehavior::FullScreenNone | NSWindowCollectionBehavior::FullScreenPrimary;
        assert!(!behavior.intersects(alone), "at most one full-screen behaviour may be named");
        assert!(!behavior.contains(NSWindowCollectionBehavior::CanJoinAllSpaces), "it lives on one space");
    }

    #[test]
    fn title_bar_height_from_window() {
        assert_eq!(title_bar_height(WINDOW, WINDOW - 32.0), 32.0);
        assert_eq!(title_bar_height(WINDOW, WINDOW - 28.0), 28.0);
        assert_eq!(title_bar_height(WINDOW + 0.5, WINDOW - 51.5), 52.0);
    }

    #[test]
    fn missing_title_bar_gets_usual() {
        assert_eq!(title_bar_height(WINDOW, WINDOW), USUAL_TITLE_BAR);
        assert_eq!(title_bar_height(WINDOW, WINDOW + 20.0), USUAL_TITLE_BAR);
        assert_eq!(title_bar_height(CGFloat::NAN, WINDOW), USUAL_TITLE_BAR);
        assert_eq!(title_bar_height(CGFloat::INFINITY, WINDOW), USUAL_TITLE_BAR);
    }
}
