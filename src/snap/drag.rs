//! Tells a dragged window from a resized one. A press on a window becomes a drag once the window moves with the
//! cursor and keeps its size, or once a maximized window shrinks as it's pulled away.

use egui::{Pos2, Rect};

use super::{Grab, Handle};

/// How far the window has to move before it counts.
const MOVE_SLACK: f32 = 5.0;
/// A size change bigger than this means the window is being resized.
const SIZE_SLACK: f32 = 3.0;
/// How far the window may lag the cursor and still be following it.
const FOLLOW_SLACK: f32 = 80.0;

/// What the mouse is doing: nothing, a press, dragging a window, or just letting one go.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Drag {
    Idle,
    Pressed,
    Moving(Grab),
    Released(Grab),
}

/// What the watcher asks about windows. The app asks the system, and tests answer for it.
pub trait Probe {
    fn window_at(&self, point: Pos2) -> Option<Grab>;
    fn frame_of(&self, handle: Handle) -> Option<Rect>;
    fn fills_a_screen(&self, frame: Rect) -> bool;
}

/// Follows the mouse to spot a window being dragged.
#[derive(Default)]
pub struct Watcher {
    press: Option<Press>,
    moving: Option<Grab>,
}

#[derive(Clone, Copy)]
struct Press {
    grab: Option<Grab>,
    cursor: Pos2,
    maximized: bool,
}

impl Watcher {
    /// One step of the mouse. The window under a press is found once, when the press starts.
    pub fn poll(&mut self, down: bool, cursor: Option<Pos2>, probe: &impl Probe) -> Drag {
        let Some(cursor) = cursor.filter(|_| down) else {
            self.press = None;
            return match self.moving.take() {
                Some(grab) => Drag::Released(Grab { frame: probe.frame_of(grab.handle).unwrap_or(grab.frame), ..grab }),
                None => Drag::Idle,
            };
        };
        if let Some(moving) = self.moving {
            let frame = probe.frame_of(moving.handle).unwrap_or(moving.frame);
            self.moving = Some(Grab { frame, ..moving });
            return Drag::Moving(Grab { frame, ..moving });
        }
        let press = *self.press.get_or_insert_with(|| {
            let grab = probe.window_at(cursor);
            Press { grab, cursor, maximized: grab.is_some_and(|grab| probe.fills_a_screen(grab.frame)) }
        });
        let Some(grab) = press.grab else { return Drag::Pressed };
        let Some(now) = probe.frame_of(grab.handle) else { return Drag::Pressed };
        if is_move(&press, grab.frame, now, cursor) {
            self.moving = Some(Grab { frame: now, ..grab });
            return Drag::Moving(Grab { frame: now, ..grab });
        }
        Drag::Pressed
    }
}

/// A move keeps the window's size and follows the cursor. A maximized window counts too when it shrinks as it's
/// pulled, since the system gives it back its old size.
fn is_move(press: &Press, before: Rect, now: Rect, cursor: Pos2) -> bool {
    let shifted = now.min - before.min;
    if shifted.length() < MOVE_SLACK {
        return press.maximized && now.size() != before.size() && (cursor - press.cursor).length() >= MOVE_SLACK;
    }
    let travelled = cursor - press.cursor;
    let same_size =
        (now.width() - before.width()).abs() <= SIZE_SLACK && (now.height() - before.height()).abs() <= SIZE_SLACK;
    let follows = (shifted - travelled).length() <= FOLLOW_SLACK;
    (same_size && follows) || (press.maximized && now.width() < before.width() - SIZE_SLACK)
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use egui::{pos2, vec2};

    use super::*;

    struct Fake {
        frame: Cell<Rect>,
        window: Option<Grab>,
        maximized: bool,
    }

    const HANDLE: Handle = Handle { id: 7, pid: 100 };

    fn frame(x: f32, y: f32, w: f32, h: f32) -> Rect {
        Rect::from_min_size(pos2(x, y), vec2(w, h))
    }

    fn scene(maximized: bool) -> Fake {
        let start = frame(200.0, 150.0, 800.0, 600.0);
        Fake { frame: Cell::new(start), window: Some(Grab { handle: HANDLE, frame: start }), maximized }
    }

    impl Probe for Fake {
        fn window_at(&self, _: Pos2) -> Option<Grab> {
            self.window
        }

        fn frame_of(&self, _: Handle) -> Option<Rect> {
            Some(self.frame.get())
        }

        fn fills_a_screen(&self, _: Rect) -> bool {
            self.maximized
        }
    }

    #[test]
    fn window_following_cursor_is_dragged() {
        let scene = scene(false);
        let mut watcher = Watcher::default();
        assert_eq!(watcher.poll(true, Some(pos2(300.0, 160.0)), &scene), Drag::Pressed);
        scene.frame.set(frame(260.0, 190.0, 800.0, 600.0));
        let drag = watcher.poll(true, Some(pos2(360.0, 200.0)), &scene);
        assert!(matches!(drag, Drag::Moving(grab) if grab.handle == HANDLE && grab.frame.min == pos2(260.0, 190.0)));
    }

    #[test]
    fn still_click_is_not_a_drag() {
        let scene = scene(false);
        let mut watcher = Watcher::default();
        for x in [300.0, 320.0, 400.0] {
            assert_eq!(watcher.poll(true, Some(pos2(x, 300.0)), &scene), Drag::Pressed);
        }
    }

    #[test]
    fn resizing_a_window_is_not_dragging_it() {
        let scene = scene(false);
        let mut watcher = Watcher::default();
        watcher.poll(true, Some(pos2(1000.0, 750.0)), &scene);
        scene.frame.set(frame(200.0, 150.0, 900.0, 700.0));
        assert_eq!(watcher.poll(true, Some(pos2(1100.0, 850.0)), &scene), Drag::Pressed);
        scene.frame.set(frame(140.0, 90.0, 860.0, 660.0));
        assert_eq!(watcher.poll(true, Some(pos2(1040.0, 790.0)), &scene), Drag::Pressed);
    }

    #[test]
    fn window_moving_itself_isnt_a_drag() {
        let scene = scene(false);
        let mut watcher = Watcher::default();
        watcher.poll(true, Some(pos2(300.0, 160.0)), &scene);
        scene.frame.set(frame(900.0, 600.0, 800.0, 600.0));
        assert_eq!(watcher.poll(true, Some(pos2(302.0, 160.0)), &scene), Drag::Pressed);
    }

    #[test]
    fn maximized_window_drag_counts() {
        let scene = scene(true);
        let mut watcher = Watcher::default();
        watcher.poll(true, Some(pos2(500.0, 20.0)), &scene);
        scene.frame.set(frame(260.0, 0.0, 700.0, 500.0));
        let drag = watcher.poll(true, Some(pos2(600.0, 60.0)), &scene);
        assert!(matches!(drag, Drag::Moving(_)));
    }

    #[test]
    fn drag_lasts_until_release() {
        let scene = scene(false);
        let mut watcher = Watcher::default();
        watcher.poll(true, Some(pos2(300.0, 160.0)), &scene);
        scene.frame.set(frame(260.0, 190.0, 800.0, 600.0));
        watcher.poll(true, Some(pos2(360.0, 200.0)), &scene);
        assert!(matches!(watcher.poll(true, Some(pos2(900.0, 700.0)), &scene), Drag::Moving(_)));
        assert!(matches!(watcher.poll(false, None, &scene), Drag::Released(grab) if grab.handle == HANDLE));
        assert_eq!(watcher.poll(false, None, &scene), Drag::Idle);
    }

    #[test]
    fn pressing_on_nothing_is_just_a_press() {
        let mut scene = scene(false);
        scene.window = None;
        let mut watcher = Watcher::default();
        assert_eq!(watcher.poll(true, Some(pos2(5.0, 5.0)), &scene), Drag::Pressed);
        assert_eq!(watcher.poll(false, None, &scene), Drag::Idle);
    }
}
