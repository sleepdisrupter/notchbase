//! Which screens get a notch and in which style. A screen with a hardware notch gets the Notch style and any other
//! the Island. NOTCHBASE_NOTCH=200x32 pretends the main screen has one.

use winit::event_loop::ActiveEventLoop;
use winit::monitor::MonitorHandle;

use super::imp::{hardware_notch, virtual_display};

const SIMULATED_NOTCH: &str = "NOTCHBASE_NOTCH";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Style {
    Notch,
    Island,
}

#[derive(PartialEq)]
pub struct Placement {
    pub monitor: MonitorHandle,
    pub style: Style,
    pub notch_size: Option<egui::Vec2>,
}

pub fn placements(event_loop: &ActiveEventLoop, everywhere: bool) -> Vec<Placement> {
    let main = placement(event_loop);
    if !everywhere {
        return main.into_iter().collect();
    }
    // A virtual display shows on another computer's screen: a notch there would cover that computer's own.
    let mut all: Vec<Placement> =
        event_loop.available_monitors().filter(|monitor| !virtual_display(monitor)).map(place_on).collect();
    if let Some(main) = main
        && let Some(i) = all.iter().position(|placement| placement.monitor == main.monitor)
    {
        all.swap(0, i);
    }
    all
}

fn place_on(monitor: MonitorHandle) -> Placement {
    match hardware_notch(&monitor) {
        Some(size) => Placement { monitor, style: Style::Notch, notch_size: Some(size) },
        None => Placement { monitor, style: Style::Island, notch_size: None },
    }
}

fn simulated() -> Option<egui::Vec2> {
    let text = std::env::var(SIMULATED_NOTCH).ok()?;
    let (width, height) = text.trim().split_once(['x', 'X'])?;
    let size = egui::vec2(width.trim().parse().ok()?, height.trim().parse().ok()?);
    (size.x > 0.0 && size.y > 0.0).then_some(size)
}

fn placement(event_loop: &ActiveEventLoop) -> Option<Placement> {
    if let Some(size) = simulated() {
        let monitor = event_loop.primary_monitor().or_else(|| event_loop.available_monitors().next())?;
        return Some(Placement { monitor, style: Style::Notch, notch_size: Some(size) });
    }
    for monitor in event_loop.available_monitors() {
        if let Some(size) = hardware_notch(&monitor) {
            return Some(Placement { monitor, style: Style::Notch, notch_size: Some(size) });
        }
    }
    let monitor = event_loop.primary_monitor().or_else(|| event_loop.available_monitors().next())?;
    Some(Placement { monitor, style: Style::Island, notch_size: None })
}
