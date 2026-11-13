//! The blur behind the open panel and the smoke that darkens glass panes. The blur is an NSVisualEffectView under
//! the GL surface, masked to the outline the notch asks for.

use egui::Pos2;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{MainThreadMarker, MainThreadOnly as _, msg_send};
use objc2_app_kit::{
    NSAutoresizingMaskOptions, NSView, NSVisualEffectBlendingMode, NSVisualEffectMaterial, NSVisualEffectState,
    NSVisualEffectView, NSWindowOrderingMode,
};
use objc2_core_foundation::{CFRetained, CGFloat};
use objc2_core_graphics::{CGColor, CGMutablePath};
use objc2_foundation::{NSNumber, NSString, ns_string};
use objc2_quartz_core::{CALayer, CAShapeLayer, CATransaction};
use winit::window::Window;

use crate::ui::erase::{Pane, SMOKE};

use super::appkit::ns_window;

pub const BLURS_BEHIND: bool = true;
const BLUR_FILTER: &str = "gaussianBlur";
const BLUR_RADIUS: f64 = 8.0;

pub struct Backdrop {
    view: Option<(Retained<NSView>, Retained<CAShapeLayer>)>,
    shown: Vec<Vec<Pos2>>,
    smoke: Option<Retained<CALayer>>,
    layers: Vec<Retained<CAShapeLayer>>,
    panes: Vec<Pane>,
}

impl Backdrop {
    pub fn new(window: &Window) -> Self {
        let view = behind(window);
        let smoke = view.as_ref().and_then(|(view, _)| above(view));
        Self { view, shown: Vec::new(), smoke, layers: Vec::new(), panes: Vec::new() }
    }

    pub fn shape(&mut self, outlines: Vec<Vec<Pos2>>, panes: &[Pane]) {
        if let Some(layer) = self.view.as_ref().and_then(|(view, _)| view.layer()) {
            clear_glass(&layer);
        }
        self.dim(panes);
        if outlines == self.shown {
            return;
        }
        let Some((view, mask)) = &self.view else { return };
        CATransaction::begin();
        CATransaction::setDisableActions(true);
        mask.setPath(Some(&path(outlines.iter(), view.frame().size.height)));
        view.setHidden(outlines.is_empty());
        CATransaction::commit();
        self.shown = outlines;
    }

    fn dim(&mut self, panes: &[Pane]) {
        if panes == self.panes {
            return;
        }
        let (Some((view, _)), Some(smoke)) = (&self.view, &self.smoke) else { return };
        let height = view.frame().size.height;
        CATransaction::begin();
        CATransaction::setDisableActions(true);
        while self.layers.len() < panes.len() * SMOKE.len() {
            let layer = darken(SMOKE[self.layers.len() % SMOKE.len()].0);
            smoke.addSublayer(&layer);
            self.layers.push(layer);
        }
        for (index, layer) in self.layers.iter().enumerate() {
            let pane = panes.get(index / SMOKE.len());
            layer.setHidden(pane.is_none());
            if let Some(pane) = pane {
                layer.setPath(Some(&path([&pane.outline], height)));
                layer.setOpacity(SMOKE[index % SMOKE.len()].1 * pane.strength);
            }
        }
        CATransaction::commit();
        self.panes = panes.to_vec();
    }
}

fn path<'a>(outlines: impl IntoIterator<Item = &'a Vec<Pos2>>, height: CGFloat) -> CFRetained<CGMutablePath> {
    let path = CGMutablePath::new();
    for outline in outlines {
        let Some((first, rest)) = outline.split_first() else { continue };
        let at = |point: &Pos2| (CGFloat::from(point.x), height - CGFloat::from(point.y));
        let (x, y) = at(first);
        unsafe { CGMutablePath::move_to_point(Some(&path), std::ptr::null(), x, y) };
        for point in rest {
            let (x, y) = at(point);
            unsafe { CGMutablePath::add_line_to_point(Some(&path), std::ptr::null(), x, y) };
        }
        CGMutablePath::close_subpath(Some(&path));
    }
    path
}

fn darken(gray: f32) -> Retained<CAShapeLayer> {
    let gray = CGFloat::from(gray);
    let layer = CAShapeLayer::new();
    layer.setFillColor(Some(&CGColor::new_srgb(gray, gray, gray, 1.0)));
    unsafe { layer.setCompositingFilter(Some(ns_string!("darkenBlendMode"))) };
    layer
}

/// The view's own tint would turn the glass flat gray, so every layer but the blur is hidden.
fn clear_glass(layer: &CALayer) {
    let Some(sublayers) = (unsafe { layer.sublayers() }) else { return };
    for sub in sublayers.iter() {
        let class = unsafe { (*(&*sub as *const CALayer as *const AnyObject)).class() };
        if class.name().to_string_lossy().contains("Backdrop") {
            soften(&sub);
        } else if unsafe { sub.sublayers() }.is_some_and(|inner| inner.count() > 0) {
            clear_glass(&sub);
        } else if sub.opacity() != 0.0 {
            sub.setOpacity(0.0);
        }
    }
}

fn soften(layer: &CALayer) {
    let Some(filters) = layer.filters() else { return };
    let blurs = filters.iter().any(|filter| unsafe {
        let name: Option<Retained<NSString>> = msg_send![&*filter, name];
        name.is_some_and(|name| name.to_string() == BLUR_FILTER)
    });
    if blurs {
        let radius = NSNumber::new_f64(BLUR_RADIUS);
        let _: () =
            unsafe { msg_send![layer, setValue: &*radius, forKeyPath: ns_string!("filters.gaussianBlur.inputRadius")] };
    }
}

fn behind(window: &Window) -> Option<(Retained<NSView>, Retained<CAShapeLayer>)> {
    let mtm = MainThreadMarker::new()?;
    let ns_window = ns_window(window)?;
    let content = ns_window.contentView()?;
    let frame: Retained<NSView> = unsafe { content.superview() }?;
    let effect = NSVisualEffectView::initWithFrame(NSVisualEffectView::alloc(mtm), content.frame());
    effect.setMaterial(NSVisualEffectMaterial::HUDWindow);
    effect.setBlendingMode(NSVisualEffectBlendingMode::BehindWindow);
    effect.setState(NSVisualEffectState::Active);
    let view: Retained<NSView> = Retained::into_super(effect);
    view.setAutoresizingMask(
        NSAutoresizingMaskOptions::ViewWidthSizable | NSAutoresizingMaskOptions::ViewHeightSizable,
    );
    view.setWantsLayer(true);
    view.setHidden(true);
    let mask = CAShapeLayer::new();
    let layer = view.layer()?;
    unsafe { layer.setMask(Some(&mask)) };
    frame.addSubview_positioned_relativeTo(&view, NSWindowOrderingMode::Below, Some(&content));
    Some((view, mask))
}

/// The smoke gets a view of its own, since inside the masked blur its darken blend paints flat gray.
fn above(blur: &NSView) -> Option<Retained<CALayer>> {
    let mtm = MainThreadMarker::new()?;
    let frame = unsafe { blur.superview() }?;
    let view = NSView::initWithFrame(NSView::alloc(mtm), blur.frame());
    view.setAutoresizingMask(blur.autoresizingMask());
    view.setWantsLayer(true);
    frame.addSubview_positioned_relativeTo(&view, NSWindowOrderingMode::Above, Some(blur));
    view.layer()
}
