//! One pass of the event loop: drive each notch window, hand the keyboard around and pick when to wake next.

use std::time::{Duration, Instant};

use egui::Pos2;
use notchbase_sdk::now_ms;
use winit::event_loop::{ActiveEventLoop, ControlFlow};

use crate::platform;
use crate::ui::gl_window::pace_frame;

use super::App;
use super::annotate::Hold;
use super::notches::window::{InputSwitch, NotchWindow};
use super::return_to::ReturnTo;

/// Tells the freeze recorder which step of the frame runs now.
macro_rules! step {
    ($name:literal) => {{
        static STAGE: crate::stall::Stage = crate::stall::Stage(concat!("frame: ", $name));
        crate::stall::enter(&STAGE);
    }};
}

const CURSOR_POLL: Duration = Duration::from_millis(20);
const NOTCH_TAKES_FOCUS_ON_CLICK: bool = cfg!(target_os = "macos");
const SECOND_MS: u64 = 1000;

#[derive(Clone, Copy)]
struct Frame {
    now: Instant,
    cursor: Option<Pos2>,
    recording: bool,
    keys: bool,
    pressing: bool,
    capturing: bool,
    catching: bool,
    annotator: Hold,
    page_changed: bool,
}

impl Frame {
    fn elsewhere(self) -> Self {
        Self { cursor: None, recording: false, catching: false, ..self }
    }
}

struct Schedule {
    wake_at: Instant,
}

impl Schedule {
    fn new(now: Instant) -> Self {
        Self { wake_at: now + CURSOR_POLL }
    }

    fn wake_by(&mut self, deadline: Option<Instant>) {
        if let Some(at) = deadline {
            self.wake_at = self.wake_at.min(at);
        }
    }
}

impl App {
    pub(super) fn run_frame(&mut self, event_loop: &ActiveEventLoop) {
        let now = Instant::now();
        step!("follow_config");
        self.follow_config(event_loop, now);
        step!("follow_plugins");
        self.follow_plugins(event_loop, now);
        if std::mem::take(&mut self.rearrange) {
            self.arrange_notches(event_loop);
        }
        step!("panel.mixer.maintain");
        self.panel.mixer.maintain();
        step!("sync_forced_awake");
        self.sync_forced_awake(now);
        step!("sync_screen_recording");
        self.sync_screen_recording();
        step!("sync_recording_dim");
        self.sync_recording_dim(event_loop);
        step!("sync_annotator");
        self.sync_annotator(now);
        step!("sync_now_playing");
        self.sync_now_playing();
        step!("sync_volume");
        self.sync_volume(now);
        step!("sync_bluetooth");
        self.sync_bluetooth();
        step!("sync_calendar");
        self.sync_calendar();
        step!("sync_system");
        self.sync_system();
        step!("start_pending_capture");
        self.start_pending_capture(event_loop, now);
        step!("pump_tools");
        self.pump_tools(event_loop, now);
        step!("sync_updates");
        self.sync_updates(event_loop, now);
        step!("sync_agents");
        self.sync_agents();
        step!("sync_airdrops");
        self.sync_airdrops(now);
        step!("sync_launch");
        self.sync_launch(now);
        step!("sync_videos");
        self.sync_videos(now);
        step!("follow_notifications");
        self.follow_notifications();
        step!("follow_whats_new_page");
        self.follow_whats_new_page(now);
        step!("follow_welcome");
        self.follow_welcome(now);
        step!("follow_dock_intro");
        self.follow_dock_intro(now);
        step!("panel refresh");
        let assistant = self.assistant();
        let page_changed =
            self.panel.refresh(self.visible_page(), assistant, now) | self.tick_timers() | self.tick_recent();
        if self.typing && !self.notches.any_expanded() {
            self.release_typing();
        }
        if self.panel.expire_notice(now) {
            self.redraw_notch();
        }
        step!("drop_stale_pins");
        self.drop_stale_pins();
        step!("close_find_on_a_press_elsewhere");
        self.close_find_on_a_press_elsewhere();
        step!("keep_find_focused");
        self.keep_find_focused(now);
        step!("cursor and drags");
        let capturing = self.capturing();
        let holding_keys = self.keys.is_some() || self.typing;
        if !holding_keys {
            self.notches.follow_cursor();
        }
        let cursor = if capturing { None } else { self.notches.active().and_then(NotchWindow::cursor) };
        let catching = self.update_drags(now, cursor, capturing);
        step!("update_snap");
        self.update_snap(event_loop, now);
        let calm = holding_keys || capturing;
        let shy = !calm && self.config.config().notch.flee_from_keys;
        let (keys, pressing) = (shy && platform::any_key_held(), shy && platform::mouse_held());
        let annotator = self.annotating.keyboard();
        let frame = Frame {
            now,
            cursor,
            recording: holding_keys,
            keys,
            pressing,
            capturing,
            catching,
            annotator,
            page_changed,
        };
        step!("keep_notches_placed");
        let mut schedule = Schedule::new(now);
        if !frame.recording && self.keep_notches_placed(event_loop, now) {
            self.redraw_notch();
        }
        step!("drive_notches");
        self.drive_notches(&frame, &mut schedule);
        step!("replace_spent_notches");
        self.replace_spent_notches(event_loop);
        step!("advance_captures");
        self.advance_captures();
        schedule.wake_by(self.pending_capture_deadline());
        schedule.wake_by(self.panel.notice_expiry());
        schedule.wake_by(self.tool_wakeup());
        schedule.wake_by(self.timer_wakeup(now));
        schedule.wake_by(self.update_wakeup());
        schedule.wake_by(self.whats_new_wakeup());
        schedule.wake_by(self.welcome_wakeup());
        schedule.wake_by(self.dock_intro_wakeup());
        schedule.wake_by(self.agents_wakeup(now));
        schedule.wake_by(self.recent_wakeup(now));
        schedule.wake_by(self.airdrop_wakeup());
        schedule.wake_by(self.launch_wakeup());
        schedule.wake_by(self.snap_wakeup());
        schedule.wake_by(self.videos_wakeup());
        schedule.wake_by(self.annotating.wakeup());
        step!("pace_frame (waiting for vblank)");
        pace_frame();
        event_loop.set_control_flow(ControlFlow::WaitUntil(schedule.wake_at));
        crate::stall::sleep_until(schedule.wake_at);
    }

    fn shows_tool_timers(&self) -> bool {
        self.visible_page().is_some() && self.tools.shows_timers()
    }

    fn tick_timers(&mut self) -> bool {
        let second = now_ms() / SECOND_MS;
        let ticked = self.shows_tool_timers() && second != self.timer_second;
        self.timer_second = second;
        ticked
    }

    fn timer_wakeup(&self, now: Instant) -> Option<Instant> {
        self.shows_tool_timers().then(|| now + Duration::from_millis(SECOND_MS - now_ms() % SECOND_MS))
    }

    fn drive_notches(&mut self, frame: &Frame, schedule: &mut Schedule) {
        let active: Vec<bool> = (0..self.notches.iter().count()).map(|i| self.notches.is_active(i)).collect();
        let mut ways_back = Vec::new();
        for (i, nw) in self.notches.windows_mut() {
            let own = if active[i] { *frame } else { frame.elsewhere() };
            ways_back.extend(drive_notch(nw, &own, schedule));
        }
        ways_back.into_iter().for_each(|remembered| self.hand_focus_back(remembered));
    }
}

fn drive_notch(nw: &mut NotchWindow, frame: &Frame, schedule: &mut Schedule) -> Option<ReturnTo> {
    let mut redraw = nw.notch.fear(frame.keys, frame.pressing, frame.cursor) | nw.notch.tick(frame.now, frame.cursor)
        || frame.page_changed;
    let (input_changed, way_back) = sync_input(nw, frame);
    redraw |= input_changed;
    nw.follow_lock(crate::config::current().notch.lock_screen && nw.notch.above_lock_screen());
    redraw |= nw.take_due_repaint(frame.now);
    schedule.wake_by(nw.wake_deadline(frame.now));
    if redraw || nw.notch.is_animating() {
        nw.redraw();
    }
    way_back
}

fn sync_input(nw: &mut NotchWindow, frame: &Frame) -> (bool, Option<ReturnTo>) {
    let every_click = takes_every_click(frame, nw.notch.finding(), nw.pointer_held());
    let wants_input = every_click || frame.cursor.is_some_and(|c| nw.notch.wants_input(c));
    let mut way_back = None;
    let pointer_left = match nw.set_wants_input(wants_input) {
        Some(InputSwitch::ToDesktop) => {
            way_back = nw.leave_hover().filter(|_| hands_focus_back(frame));
            true
        }
        Some(InputSwitch::ToNotch) => {
            platform::keep_on_top(nw.window());
            if remembers_who_had_the_keyboard(frame) {
                nw.remember_hover_app(frame.annotator);
            }
            false
        }
        None => false,
    };
    let pointer_moved = nw.follow_pointer(frame.cursor);
    (pointer_left || pointer_moved, way_back)
}

fn takes_every_click(frame: &Frame, finding: bool, held: bool) -> bool {
    frame.catching || (frame.recording && (!finding || held))
}

fn remembers_who_had_the_keyboard(frame: &Frame) -> bool {
    NOTCH_TAKES_FOCUS_ON_CLICK && !frame.catching
}

fn hands_focus_back(frame: &Frame) -> bool {
    !frame.capturing && !frame.recording
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame() -> Frame {
        Frame {
            now: Instant::now(),
            cursor: None,
            recording: false,
            keys: false,
            pressing: false,
            capturing: false,
            catching: false,
            annotator: Hold::NoAnnotator,
            page_changed: false,
        }
    }

    #[test]
    fn remembers_in_hands_back_out() {
        for annotator in [Hold::NoAnnotator, Hold::Elsewhere, Hold::Annotator, Hold::Lent] {
            let idle = Frame { annotator, ..frame() };
            assert!(hands_focus_back(&idle), "{annotator:?}");
            assert_eq!(remembers_who_had_the_keyboard(&idle), NOTCH_TAKES_FOCUS_ON_CLICK, "{annotator:?}");
            assert!(!hands_focus_back(&Frame { capturing: true, ..idle }), "{annotator:?}");
            assert!(!remembers_who_had_the_keyboard(&Frame { catching: true, ..idle }), "{annotator:?}");
        }
    }

    #[test]
    fn click_beside_notch_goes_through() {
        let typing = Frame { recording: true, ..frame() };
        assert!(!takes_every_click(&typing, true, false), "only the clicks on the notch are taken");
        assert!(takes_every_click(&typing, true, true), "a press that started on the notch is kept until let go");
        assert!(takes_every_click(&typing, false, false), "any other field still takes every click");
        assert!(takes_every_click(&Frame { catching: true, ..frame() }, true, false), "a file drag is still caught");
        assert!(!takes_every_click(&frame(), false, false));
        assert!(!hands_focus_back(&typing), "and the pointer leaving the notch leaves the keyboard with Find");
    }

    #[test]
    fn idle_frame_polls_cursor() {
        let now = Instant::now();
        let mut schedule = Schedule::new(now);
        schedule.wake_by(None);
        assert_eq!(schedule.wake_at, now + CURSOR_POLL);
    }

    #[test]
    fn earliest_deadline_wins() {
        let now = Instant::now();
        let mut schedule = Schedule::new(now);
        schedule.wake_by(Some(now + Duration::from_millis(8)));
        schedule.wake_by(Some(now + Duration::from_millis(12)));
        schedule.wake_by(Some(now + CURSOR_POLL * 3));
        assert_eq!(schedule.wake_at, now + Duration::from_millis(8));
    }
}
