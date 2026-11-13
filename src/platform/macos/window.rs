//! Sets up the notch, overlay and dim windows on a Mac: on every Space, in sRGB, at the right level and frame.

use objc2::runtime::NSObjectProtocol as _;
use objc2_app_kit::{
    NSColor, NSColorSpace, NSMainMenuWindowLevel, NSScreen, NSScreenSaverWindowLevel, NSWindow,
    NSWindowAnimationBehavior, NSWindowCollectionBehavior,
};
use objc2_core_foundation::{CGFloat, CGPoint, CGRect, CGSize};
use winit::dpi::{LogicalSize, PhysicalSize};
use winit::monitor::MonitorHandle;
use winit::platform::macos::WindowAttributesExtMacOS as _;
use winit::window::{Window, WindowAttributes, WindowLevel};

use super::appkit::{ns_screen, ns_window};
use crate::platform::Placement;

// Above the menu bar, where the hardware notch is.
const LEVELS_ABOVE_MAIN_MENU: isize = 3;

pub fn notch_window_attributes(_placement: &Placement, canvas: egui::Vec2) -> WindowAttributes {
    WindowAttributes::default()
        .with_title("notchbase")
        .with_decorations(false)
        .with_transparent(true)
        .with_resizable(false)
        .with_visible(false)
        .with_active(false)
        .with_window_level(WindowLevel::AlwaysOnTop)
        .with_inner_size(LogicalSize::new(canvas.x as f64, canvas.y as f64))
        .with_has_shadow(false)
        .with_accepts_first_mouse(true)
}

pub fn configure_notch_window(window: &Window, placement: &Placement, canvas: egui::Vec2) {
    let (Some(ns_window), Some(screen)) = (ns_window(window), ns_screen(&placement.monitor)) else {
        return;
    };
    float_everywhere(&ns_window);
    use_srgb(&ns_window);
    let level = NSMainMenuWindowLevel + LEVELS_ABOVE_MAIN_MENU;
    if ns_window.level() != level {
        ns_window.setLevel(level);
    }
    let frame = top_center_frame(&screen, canvas);
    if ns_window.frame() != frame {
        ns_window.setFrame_display(frame, true);
    }
}

pub fn overlay_window_attributes(monitor: &MonitorHandle) -> WindowAttributes {
    let size = monitor.size();
    WindowAttributes::default()
        .with_title("notchbase picker")
        .with_decorations(false)
        .with_transparent(true)
        .with_resizable(false)
        .with_visible(false)
        .with_window_level(WindowLevel::AlwaysOnTop)
        .with_inner_size(PhysicalSize::new(size.width, size.height))
        .with_position(monitor.position())
        .with_has_shadow(false)
        .with_accepts_first_mouse(true)
}

pub fn dim_window_attributes(monitor: &MonitorHandle) -> WindowAttributes {
    overlay_window_attributes(monitor).with_title("notchbase recording")
}

pub fn configure_dim_window(window: &Window, monitor: &MonitorHandle) {
    let (Some(ns_window), Some(screen)) = (ns_window(window), ns_screen(monitor)) else {
        return;
    };
    float_everywhere(&ns_window);
    use_srgb(&ns_window);
    // One below the notch, so the notch stays above the dim.
    let level = NSMainMenuWindowLevel + LEVELS_ABOVE_MAIN_MENU - 1;
    if ns_window.level() != level {
        ns_window.setLevel(level);
    }
    let frame = screen.frame();
    if ns_window.frame() != frame {
        ns_window.setFrame_display(frame, true);
    }
}

pub fn keep_on_top(_window: &Window) {}

pub fn raise_window(window: &Window) {
    if let Some(ns_window) = ns_window(window) {
        ns_window.orderFrontRegardless();
    }
}

pub fn configure_overlay_window(window: &Window, monitor: &MonitorHandle) {
    let (Some(ns_window), Some(screen)) = (ns_window(window), ns_screen(monitor)) else {
        return;
    };
    float_everywhere(&ns_window);
    use_srgb(&ns_window);
    if ns_window.level() != NSScreenSaverWindowLevel {
        ns_window.setLevel(NSScreenSaverWindowLevel);
    }
    let frame = screen.frame();
    if ns_window.frame() != frame {
        ns_window.setFrame_display(frame, true);
    }
}

fn top_center_frame(screen: &NSScreen, canvas: egui::Vec2) -> CGRect {
    let screen_frame = screen.frame();
    let (width, height) = (canvas.x as CGFloat, canvas.y as CGFloat);
    let x = (screen_frame.origin.x + (screen_frame.size.width - width) * 0.5).round();
    let y = screen_frame.origin.y + screen_frame.size.height - height;
    CGRect::new(CGPoint::new(x, y), CGSize::new(width, height))
}

fn use_srgb(window: &NSWindow) {
    let srgb = NSColorSpace::sRGBColorSpace();
    if !window.colorSpace().is_some_and(|space| space.isEqual(Some(&srgb))) {
        window.setColorSpace(Some(&srgb));
    }
}

fn float_everywhere(window: &NSWindow) {
    let behavior = NSWindowCollectionBehavior::CanJoinAllSpaces
        | NSWindowCollectionBehavior::Stationary
        | NSWindowCollectionBehavior::FullScreenAuxiliary
        | NSWindowCollectionBehavior::IgnoresCycle;
    if window.collectionBehavior() == behavior {
        return;
    }
    window.setCollectionBehavior(behavior);
    window.setHasShadow(false);
    window.setOpaque(false);
    window.setBackgroundColor(Some(&NSColor::clearColor()));
    window.setHidesOnDeactivate(false);
    window.setCanHide(false);
    window.setMovable(false);
    window.setAnimationBehavior(NSWindowAnimationBehavior::None);
}
