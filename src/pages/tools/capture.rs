//! Recording a new shortcut for a tool. Keys are read by where they sit on the keyboard, not by what they type.

use egui::Rect;

use crate::bridge::Action;
use crate::platform;
use crate::tools::shortcuts::keys::{UNSUPPORTED_HINT, hotkey_from_egui, merge_modifiers};
use crate::tools::shortcuts::{ShortcutRef, ShortcutView};
use crate::ui::element::Prompt;
use crate::ui::element::shortcut::KEYS_SLOP;
use crate::ui::motion::{self, Clock};
use crate::ui::spring::{POINT_PRECISION, Spring, SpringConfig};

const SHAKE: SpringConfig = SpringConfig::new(0.12, 0.2);
const SHAKE_KICK: f32 = 320.0;
const PULSE_PERIOD: f32 = 1.6;

pub struct KeyCapture {
    shake: Spring,
    clock: Clock,
    hint: Option<(ShortcutRef, &'static str)>,
    seen_problem: Option<String>,
}

impl Default for KeyCapture {
    fn default() -> Self {
        Self {
            shake: Spring::new(0.0, motion::HOVER, POINT_PRECISION),
            clock: Clock::default(),
            hint: None,
            seen_problem: None,
        }
    }
}

struct KeyPress {
    keys: [egui::Key; 2],
    modifiers: egui::Modifiers,
}

impl KeyCapture {
    pub fn prompt(&self) -> Prompt {
        Prompt { shake: self.shake.value, pulse: self.clock.pulse(PULSE_PERIOD), hint: self.hint }
    }

    pub fn step(&mut self, dt: f32) {
        self.clock.advance(dt);
        self.shake.step(dt);
    }

    pub fn snap(&mut self) {
        self.shake.snap_to(self.shake.target);
    }

    pub fn is_animating(&self) -> bool {
        !self.shake.is_settled()
    }

    pub fn capture(&mut self, ui: &egui::Ui, shortcuts: &[ShortcutView]) -> Option<Action> {
        self.notice_problems(shortcuts);
        let view = shortcuts.iter().find(|shortcut| shortcut.recording)?;
        let which = view.which;
        let presses: Vec<_> = ui.input(|input| input.events.iter().filter_map(key_press).collect());
        for press in presses {
            match press.keys[0] {
                egui::Key::Escape => return Some(Action::CancelRecording),
                egui::Key::Backspace | egui::Key::Delete if !view.keys.is_empty() => {
                    return Some(Action::ClearShortcut(which));
                }
                _ => {}
            }
            let modifiers = merge_modifiers(press.modifiers, platform::held_modifiers());
            match press.keys.iter().find_map(|key| hotkey_from_egui(*key, modifiers)) {
                Some(hotkey) => {
                    self.hint = None;
                    return Some(Action::SetShortcut(which, hotkey));
                }
                None => {
                    self.hint = Some((which, UNSUPPORTED_HINT));
                    self.shake();
                }
            }
        }
        None
    }

    pub fn clicked_outside(&self, ui: &egui::Ui, keys: &[Rect]) -> bool {
        let click = ui.input(|i| if i.pointer.primary_clicked() { i.pointer.interact_pos() } else { None });
        click.is_some_and(|at| !keys.iter().any(|area| area.expand(KEYS_SLOP).contains(at)))
    }

    fn notice_problems(&mut self, shortcuts: &[ShortcutView]) {
        let recording = shortcuts.iter().find(|shortcut| shortcut.recording);
        if recording.is_none_or(|shortcut| self.hint.is_some_and(|(which, _)| which != shortcut.which)) {
            self.hint = None;
        }
        let problem = recording.and_then(|shortcut| shortcut.problem.clone());
        if problem.is_some() && problem != self.seen_problem {
            self.hint = None;
            self.shake();
        }
        self.seen_problem = problem;
    }

    fn shake(&mut self) {
        self.shake.velocity = if self.shake.velocity > 0.0 { -SHAKE_KICK } else { SHAKE_KICK };
        self.shake.animate_to(0.0, SHAKE);
    }
}

fn key_press(event: &egui::Event) -> Option<KeyPress> {
    let mac = cfg!(target_os = "macos");
    let command = egui::Modifiers { ctrl: !mac, mac_cmd: mac, command: true, ..Default::default() };
    let clipboard = |key| Some(KeyPress { keys: [key; 2], modifiers: command });
    match event {
        egui::Event::Key { key, physical_key, pressed: true, repeat: false, modifiers } => {
            Some(KeyPress { keys: [physical_key.unwrap_or(*key), *key], modifiers: *modifiers })
        }
        egui::Event::Copy => clipboard(egui::Key::C),
        egui::Event::Cut => clipboard(egui::Key::X),
        egui::Event::Paste(_) => clipboard(egui::Key::V),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use global_hotkey::hotkey::{Code, HotKey, Modifiers};

    use super::*;
    use crate::bridge::ToolSlot;

    const WHICH: ShortcutRef = ShortcutRef { tool: ToolSlot(1), index: 0 };

    fn capture(page: &mut KeyCapture, keys: &[&str], events: Vec<egui::Event>) -> Option<Action> {
        let ctx = egui::Context::default();
        let keys = keys.iter().map(|key| (*key).to_owned()).collect();
        let view = ShortcutView {
            which: WHICH,
            id: "capture".into(),
            title: "Screenshot".into(),
            keys,
            recording: true,
            problem: None,
        };
        let input = egui::RawInput { events, ..Default::default() };
        let mut captured = None;
        let mut output = ctx.run_ui(input, |ui| captured = page.capture(ui, std::slice::from_ref(&view)));
        output.textures_delta.clear();
        captured
    }

    fn press(key: egui::Key, physical_key: Option<egui::Key>, modifiers: egui::Modifiers) -> egui::Event {
        egui::Event::Key { key, physical_key, pressed: true, repeat: false, modifiers }
    }

    #[test]
    fn unmapped_keys_shake() {
        let mut page = KeyCapture::default();
        let plain = egui::Modifiers::NONE;
        assert_eq!(capture(&mut page, &["S"], vec![press(egui::Key::F35, None, plain)]), None);
        assert_ne!(page.shake.velocity, 0.0);
        assert_eq!(page.hint, Some((WHICH, UNSUPPORTED_HINT)));
        let escape = capture(&mut page, &["S"], vec![press(egui::Key::Escape, None, plain)]);
        assert_eq!(escape, Some(Action::CancelRecording));
    }

    #[test]
    fn clipboard_chords_become_shortcuts() {
        let mut page = KeyCapture::default();
        let set = capture(&mut page, &[], vec![egui::Event::Copy]);
        let Some(Action::SetShortcut(WHICH, hotkey)) = set else { panic!("{set:?}") };
        assert_eq!(hotkey.key, Code::KeyC);
        assert!(!hotkey.mods.is_empty());
        let pasted = capture(&mut page, &[], vec![egui::Event::Paste("x".into())]);
        assert!(matches!(pasted, Some(Action::SetShortcut(..))));
    }

    #[test]
    fn unknown_key_uses_typed_one() {
        let mut page = KeyCapture::default();
        let set = capture(&mut page, &[], vec![press(egui::Key::K, Some(egui::Key::F35), egui::Modifiers::NONE)]);
        assert!(matches!(set, Some(Action::SetShortcut(WHICH, hotkey)) if hotkey.key == Code::KeyK));
    }

    #[test]
    fn delete_removes_existing_shortcut() {
        let mut page = KeyCapture::default();
        let plain = egui::Modifiers::NONE;
        let cleared = capture(&mut page, &["Ctrl", "S"], vec![press(egui::Key::Backspace, None, plain)]);
        assert_eq!(cleared, Some(Action::ClearShortcut(WHICH)));
        let cleared = capture(&mut page, &["Ctrl", "S"], vec![press(egui::Key::Delete, None, plain)]);
        assert_eq!(cleared, Some(Action::ClearShortcut(WHICH)));
        let backspace = capture(&mut page, &[], vec![press(egui::Key::Backspace, None, plain)]);
        assert!(matches!(backspace, Some(Action::SetShortcut(..))), "an empty shortcut can be set to Backspace");
    }

    #[test]
    fn keys_read_by_position() {
        let mut page = KeyCapture::default();
        let ctrl_shift = egui::Modifiers { ctrl: true, shift: true, ..Default::default() };
        let typed = press(egui::Key::Exclamationmark, Some(egui::Key::Num1), ctrl_shift);
        let expected = HotKey::new(Some(Modifiers::CONTROL | Modifiers::SHIFT), Code::Digit1);
        assert_eq!(capture(&mut page, &[], vec![typed]), Some(Action::SetShortcut(WHICH, expected)));
        assert_eq!(page.hint, None);
    }
}
