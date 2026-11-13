//! Tool shortcuts: registering them, recording a new one, and pinning the notch while a tool holds the keyboard.

use std::time::Instant;

use global_hotkey::hotkey::HotKey;
use global_hotkey::{GlobalHotKeyEvent, HotKeyState};
use notchbase_sdk::protocol::Event;
use winit::event_loop::ActiveEventLoop;

use crate::app::App;
use crate::bridge::ToolSlot;
use crate::config::hotkey;
use crate::platform;
use crate::tools::bindings;
use crate::tools::shortcuts::{ShortcutRef, ShortcutView};

const SHORTCUTS_UNAVAILABLE: &str = "Keyboard shortcuts are unavailable";
const TAKEN_ELSEWHERE: &str = "In use by another app";
const SHORTCUT_NOT_SAVED: &str = "Couldn't save the shortcut";

impl App {
    pub(in crate::app) fn setup_hotkeys(&mut self) {
        match self.hotkeys.install(&self.proxy) {
            Ok(()) => self.register_shortcuts(),
            Err(err) => self.report(SHORTCUTS_UNAVAILABLE, err),
        }
    }

    pub(in crate::app) fn shortcut_views(&self) -> Vec<ShortcutView> {
        self.bindings
            .iter()
            .map(|binding| {
                let recording = self.keys == Some(binding.which);
                ShortcutView {
                    which: binding.which,
                    id: binding.id.clone(),
                    title: binding.title.clone(),
                    keys: binding.hotkey.map(|hotkey| platform::hotkey_labels(&hotkey)).unwrap_or_default(),
                    recording,
                    problem: self.shortcut_problem.clone().filter(|_| recording),
                }
            })
            .collect()
    }

    pub(in crate::app) fn on_hotkey(&mut self, event_loop: &ActiveEventLoop, event: GlobalHotKeyEvent) {
        if event.state != HotKeyState::Pressed {
            return;
        }
        if self.hotkeys.is_escape(event.id) {
            self.cancel_capture();
            return;
        }
        let pressed = self.bindings.iter().find(|binding| binding.hotkey.is_some_and(|hotkey| hotkey.id() == event.id));
        if let Some(binding) = pressed.cloned() {
            self.tools.send(binding.which.tool, Event::Shortcut { id: binding.id });
            self.pump_tools(event_loop, Instant::now());
        }
    }

    pub(in crate::app) fn start_recording(&mut self, which: ShortcutRef) {
        if self.capture_requested() {
            return;
        }
        if self.keys.is_none() {
            self.hotkeys.unregister_shortcuts();
            if !self.typing {
                self.remember_front_app();
            }
        }
        self.keys = Some(which);
        self.shortcut_problem = None;
        self.hold_keyboard();
    }

    pub(in crate::app) fn set_shortcut(&mut self, which: ShortcutRef, hotkey: HotKey) {
        if self.keys != Some(which) {
            return;
        }
        let other = self.bindings.iter().find(|binding| binding.which != which && binding.hotkey == Some(hotkey));
        if let Some(other) = other {
            self.shortcut_problem = Some(format!("In use by {}", other.title));
        } else if self.hotkeys.is_taken(hotkey) {
            self.shortcut_problem = Some(TAKEN_ELSEWHERE.to_owned());
        } else {
            self.save_shortcut(which, Some(hotkey));
        }
        self.redraw_notch();
    }

    pub(in crate::app) fn preview_shortcut(&mut self, taken: bool) {
        let Some(which) = self.bindings.get(1).map(|binding| binding.which) else { return };
        self.keys = Some(which);
        let other = self.bindings.first().map(|binding| binding.title.clone()).unwrap_or_default();
        self.shortcut_problem = taken.then(|| format!("In use by {other}"));
    }

    pub(in crate::app) fn clear_shortcut(&mut self, which: ShortcutRef) {
        if self.keys == Some(which) {
            self.save_shortcut(which, None);
        }
    }

    pub(in crate::app) fn stop_recording(&mut self) {
        self.shortcut_problem = None;
        if self.keys.take().is_none() {
            return;
        }
        self.register_shortcuts();
        self.free_keyboard();
    }

    pub(in crate::app) fn set_typing(&mut self, on: bool) {
        if on == self.typing {
            return;
        }
        if on && self.keys.is_none() {
            self.remember_front_app();
        }
        self.typing = on;
        if on { self.hold_keyboard() } else { self.free_keyboard() }
    }

    pub(in crate::app) fn release_typing(&mut self) {
        self.set_typing(false);
    }

    pub(in crate::app) fn recording_lost_focus(&mut self) {
        if self.keys.is_some() || self.typing {
            self.front_app = None;
            self.typing = false;
            self.stop_recording();
            self.free_keyboard();
        }
    }

    pub(in crate::app) fn register_shortcuts(&mut self) {
        for (title, err) in self.hotkeys.register_shortcuts(&self.bindings) {
            self.report(&format!("{title} shortcut is unavailable"), err);
        }
    }

    pub(in crate::app) fn rebind_shortcuts(&mut self) {
        self.bindings = bindings(&self.tools, self.config.config());
        if self.keys.is_none() {
            self.register_shortcuts();
        }
        self.redraw_notch();
    }

    pub(in crate::app) fn hold_for(&mut self, slot: ToolSlot, held: bool) {
        if held {
            self.held_by.insert(slot);
        } else {
            self.held_by.remove(&slot);
        }
        let pinned = held || self.keys.is_some() || self.typing || !self.held_by.is_empty();
        self.update_notch(|notch| notch.set_pinned(pinned));
        self.redraw_notch();
    }

    pub(in crate::app) fn drop_stale_pins(&mut self) {
        let held = !self.held_by.is_empty();
        let holding = self.keys.is_some() || self.typing;
        let active: Vec<bool> = (0..self.notches.iter().count()).map(|i| self.notches.is_active(i)).collect();
        for (i, nw) in self.notches.windows_mut() {
            if nw.notch.pinned() && !pin_wanted(held, holding, active[i]) {
                nw.notch.set_pinned(false);
                nw.redraw();
            }
        }
    }

    pub(in crate::app) fn hold_keyboard(&mut self) {
        if let Some(nw) = self.notches.active_mut() {
            nw.notch.set_pinned(true);
            platform::begin_keyboard_focus(nw.window());
            nw.redraw();
        }
    }

    fn free_keyboard(&mut self) {
        if self.keys.is_some() || self.typing {
            return;
        }
        let held = !self.held_by.is_empty();
        if let Some(nw) = self.notches.active_mut() {
            nw.notch.set_pinned(held);
            platform::end_keyboard_focus(nw.window());
            nw.redraw();
        }
        self.restore_front_app();
    }

    fn save_shortcut(&mut self, which: ShortcutRef, hotkey: Option<HotKey>) {
        let Some(binding) = self.bindings.iter().find(|binding| binding.which == which).cloned() else { return };
        let text = hotkey.map(|hotkey| hotkey::format(&hotkey));
        let saved = self.config.update(|config| {
            config.tools.entry(binding.tool.clone()).or_default().shortcuts.insert(binding.id.clone(), text);
        });
        self.bindings = bindings(&self.tools, self.config.config());
        self.stop_recording();
        if let Err(err) = saved {
            self.report(SHORTCUT_NOT_SAVED, err);
        }
    }
}

fn pin_wanted(held_by_a_tool: bool, holding_the_keyboard: bool, active: bool) -> bool {
    held_by_a_tool || (holding_the_keyboard && active)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pinned_only_while_held() {
        assert!(pin_wanted(true, false, false) && pin_wanted(true, false, true), "a tool holds every notch open");
        assert!(pin_wanted(false, true, true), "typing or recording a shortcut holds the notch it happens in");
        assert!(!pin_wanted(false, true, false), "and not the notch of another display");
        assert!(!pin_wanted(false, false, true), "with nothing held, a pin left behind is dropped");
    }
}
