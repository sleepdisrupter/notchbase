//! Window snapping: a window dropped on a notch layout moves into that zone, and is checked again once it settles.

use std::time::{Duration, Instant};

use egui::Rect;
use winit::event::WindowEvent;
use winit::event_loop::ActiveEventLoop;
use winit::window::WindowId;

use crate::config::SnapConfig;
use crate::filedrag;
use crate::notch::Pick;
use crate::snap::{self, Drag, Ghost, Grab, Habits, Handle, Scene, Screen, Watcher, Zone};

use super::App;
use super::preview::previewing;

const NEEDS_ACCESS: &str = "Allow Accessibility for notchbase, then drag the window again";
const SETTLE_AFTER: Duration = Duration::from_millis(180);
const SETTLE_EVERY: Duration = Duration::from_millis(200);
const SETTLE_TRIES: u8 = 3;
const SETTLE_SLACK: f32 = 3.0;

#[derive(Default)]
pub(super) struct SnapState {
    watcher: Watcher,
    ghosts: Vec<Ghost>,
    target: Option<Target>,
    settle: Option<Settle>,
    asked: bool,
    active: bool,
    habits: Option<Habits>,
    app: Option<String>,
}

struct Target {
    screen: Screen,
    pick: Pick,
    rects: Vec<Rect>,
}

struct Settle {
    handle: Handle,
    rect: Rect,
    at: Instant,
    left: u8,
}

impl SnapState {
    pub(super) fn owns(&self, id: WindowId) -> bool {
        self.ghosts.iter().any(|ghost| ghost.owns(id))
    }

    pub(super) fn window_event(&mut self, event: &WindowEvent) {
        for ghost in &mut self.ghosts {
            ghost.window_event(event);
        }
    }

    pub(super) fn wakeup(&self) -> Option<Instant> {
        self.settle.as_ref().map(|settle| settle.at)
    }
}

impl App {
    pub(super) fn update_snap(&mut self, event_loop: &ActiveEventLoop, now: Instant) {
        let config = self.config.config().snap.clone();
        if !config.enabled || previewing() {
            self.snap_idle();
            return;
        }
        match self.snap.watcher.watch(filedrag::primary_button_down()) {
            Drag::Moving(grab) => self.snap_moving(event_loop, grab, &config),
            Drag::Released(grab) => self.snap_released(grab, now, &config),
            Drag::Idle | Drag::Pressed => self.snap_idle(),
        }
        self.snap_settle(now);
        self.snap.ghosts.retain(|ghost| !ghost.is_gone());
        for ghost in self.snap.ghosts.iter().filter(|ghost| ghost.is_animating()) {
            ghost.redraw();
        }
    }

    fn snap_idle(&mut self) {
        if !std::mem::take(&mut self.snap.active) {
            return;
        }
        self.snap.target = None;
        for (_, nw) in self.notches.windows_mut() {
            nw.notch.set_window_drag(false);
            nw.redraw();
        }
        self.snap.ghosts.iter_mut().for_each(Ghost::hide);
    }

    fn snap_moving(&mut self, event_loop: &ActiveEventLoop, grab: Grab, config: &SnapConfig) {
        if !snap::allowed() {
            if !std::mem::replace(&mut self.snap.asked, true) {
                snap::ask_for_access();
                self.notify(NEEDS_ACCESS);
            }
            return;
        }
        if !self.snap.active {
            self.snap.app = snap::app_name(grab.handle.pid);
        }
        self.snap.active = true;
        let habits = self.snap.habits.get_or_insert_with(Habits::load);
        let app = self.snap.app.as_deref();
        let usual_on = |screen: &Screen| app.and_then(|app| habits.usual(app, snap::class_of(screen)));
        let Some(nw) = self.notches.active_mut() else { return };
        let over = nw.cursor().filter(|at| nw.notch.drop_catch_zone().contains(*at));
        let notch_screen = snap::screen_of(&nw.placement().monitor).filter(|_| over.is_some());
        let (screen, pick) = match (over, notch_screen) {
            (Some(at), Some(screen)) => {
                nw.notch.set_window_drag(true);
                nw.notch.set_usual_pick(usual_on(&screen));
                let pick = nw.notch.pick_at(at);
                nw.notch.hover_pick(pick);
                let away = crate::platform::held_modifiers().shift;
                nw.notch.send_away(away);
                let screen = if away { snap::next_screen(&screen).unwrap_or(screen) } else { screen };
                (Some(screen), pick)
            }
            _ => {
                nw.notch.set_window_drag(false);
                (None, None)
            }
        };
        nw.redraw();
        let (Some(screen), Some(pick)) = (screen, pick) else {
            self.snap.target = None;
            self.snap.ghosts.iter_mut().for_each(Ghost::hide);
            return;
        };
        let unchanged = self.snap.target.as_ref().is_some_and(|t| t.screen.key == screen.key && t.pick == pick);
        if !unchanged {
            let rects = rects_for(&grab, &screen, pick, config.gap);
            self.snap.target = Some(Target { screen, pick, rects });
        }
        let Some(target) = &self.snap.target else { return };
        let scene = scene_of(target);
        self.show_ghost(event_loop, &screen, scene);
    }

    fn show_ghost(&mut self, event_loop: &ActiveEventLoop, screen: &Screen, scene: Scene) {
        for ghost in self.snap.ghosts.iter_mut().filter(|ghost| ghost.screen_key() != screen.key) {
            ghost.hide();
        }
        if !self.snap.ghosts.iter().any(|ghost| ghost.screen_key() == screen.key) {
            let monitor =
                event_loop.available_monitors().find(|m| snap::screen_of(m).is_some_and(|s| s.key == screen.key));
            let Some(monitor) = monitor else { return };
            match Ghost::open(event_loop, &monitor, *screen) {
                Ok(ghost) => self.snap.ghosts.push(ghost),
                Err(err) => return eprintln!("notchbase: couldn't preview the layout: {err}"),
            }
            for (_, nw) in self.notches.windows_mut() {
                crate::platform::raise_window(nw.window());
            }
        }
        if let Some(ghost) = self.snap.ghosts.iter_mut().find(|ghost| ghost.screen_key() == screen.key) {
            ghost.show(scene);
        }
    }

    fn snap_released(&mut self, grab: Grab, now: Instant, config: &SnapConfig) {
        let target = self.snap.target.take();
        self.snap.active = true;
        self.snap_idle();
        let Some(target) = target else { return };
        if let (Some(app), Some(habits)) = (self.snap.app.as_deref(), self.snap.habits.as_mut()) {
            habits.learn(app, snap::class_of(&target.screen), target.pick);
            habits.save();
        }
        let outcome = match target.pick {
            Pick::Zone(_) | Pick::Fill => snap::place(grab.handle, target.rects[0]).map(|()| 1),
            Pick::Auto => snap::lay_out(grab.handle, &target.screen, config.gap),
        };
        match outcome {
            Ok(_) => {
                self.snap.settle = Some(Settle {
                    handle: grab.handle,
                    rect: target.rects[0],
                    at: now + SETTLE_AFTER,
                    left: SETTLE_TRIES,
                });
            }
            Err(problem) => self.notify(&problem),
        }
    }

    fn snap_settle(&mut self, now: Instant) {
        let Some(settle) = &mut self.snap.settle else { return };
        if now < settle.at {
            return;
        }
        let off = snap::frame_of(settle.handle).is_some_and(|frame| !near(frame, settle.rect));
        if off {
            let _ = snap::place(settle.handle, settle.rect);
        }
        settle.left -= 1;
        settle.at = now + SETTLE_EVERY;
        if settle.left == 0 || !off {
            self.snap.settle = None;
        }
    }

    pub(super) fn snap_wakeup(&self) -> Option<Instant> {
        self.snap.wakeup()
    }
}

fn near(a: Rect, b: Rect) -> bool {
    (a.min - b.min).length() <= SETTLE_SLACK && (a.size() - b.size()).length() <= SETTLE_SLACK
}

fn rects_for(grab: &Grab, screen: &Screen, pick: Pick, gap: f32) -> Vec<Rect> {
    let gap = screen.gap(gap);
    match pick {
        Pick::Zone(zone) => vec![zone.rect(screen.work, gap)],
        Pick::Fill => {
            let taken: Vec<Rect> = snap::windows_on(screen)
                .into_iter()
                .filter(|other| other.handle != grab.handle)
                .map(|other| other.frame)
                .collect();
            vec![snap::free_space(screen.work, gap, &taken).unwrap_or_else(|| Zone::Full.rect(screen.work, gap))]
        }
        Pick::Auto => {
            let others = snap::windows_on(screen).into_iter().filter(|other| other.handle != grab.handle).count();
            snap::tile(others + 1, screen.work, gap)
        }
    }
}

fn scene_of(target: &Target) -> Scene {
    Scene { target: target.rects.first().copied(), others: target.rects.iter().skip(1).copied().collect() }
}

#[cfg(test)]
mod tests {
    use egui::{pos2, vec2};

    use super::*;

    fn screen() -> Screen {
        let frame = Rect::from_min_size(pos2(0.0, 0.0), vec2(1500.0, 950.0));
        Screen { key: 1, frame, work: Rect::from_min_max(pos2(0.0, 33.0), frame.max), scale: 1.0 }
    }

    #[test]
    fn zone_is_one_rect_in_work_area() {
        let grab = Grab { handle: Handle { id: 1, pid: 2 }, frame: screen().work };
        let rects = rects_for(&grab, &screen(), Pick::Zone(Zone::Left), 8.0);
        assert_eq!(rects.len(), 1);
        assert!(screen().work.contains_rect(rects[0]));
    }

    #[test]
    fn preview_marks_target_first() {
        let screen = screen();
        let rects = snap::tile(3, screen.work, 8.0);
        let target = Target { screen, pick: Pick::Auto, rects: rects.clone() };
        let scene = scene_of(&target);
        assert_eq!(scene.target, Some(rects[0]));
        assert_eq!(scene.others, rects[1..]);
    }

    #[test]
    fn short_window_is_off() {
        let rect = Rect::from_min_size(pos2(10.0, 40.0), vec2(700.0, 800.0));
        assert!(near(rect, rect.translate(vec2(2.0, 1.0))));
        assert!(!near(rect, rect.translate(vec2(40.0, 0.0))));
    }
}
