//! Global hotkeys: the tools' shortcuts, and Escape while something is listening for it.

use global_hotkey::hotkey::{Code, HotKey};
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager};
use winit::event_loop::EventLoopProxy;

use crate::tools::Binding;

use super::events::{self, UserEvent};

pub(super) struct Hotkeys {
    manager: Option<GlobalHotKeyManager>,
    registered: Vec<HotKey>,
    escape: HotKey,
    escape_registered: bool,
}

impl Hotkeys {
    pub(super) fn new() -> Self {
        Self {
            manager: None,
            registered: Vec::new(),
            escape: HotKey::new(None, Code::Escape),
            escape_registered: false,
        }
    }

    pub(super) fn install(&mut self, proxy: &EventLoopProxy<UserEvent>) -> global_hotkey::Result<()> {
        let manager = GlobalHotKeyManager::new()?;
        let send = events::sender(proxy);
        GlobalHotKeyEvent::set_event_handler(Some(move |event| send(UserEvent::HotKey(event))));
        self.manager = Some(manager);
        Ok(())
    }

    pub(super) fn register_shortcuts(&mut self, bindings: &[Binding]) -> Vec<(String, global_hotkey::Error)> {
        self.unregister_shortcuts();
        let Some(manager) = &self.manager else { return Vec::new() };
        let mut unavailable = Vec::new();
        for binding in bindings {
            let Some(hotkey) = binding.hotkey else { continue };
            if self.registered.contains(&hotkey) {
                continue;
            }
            match manager.register(hotkey) {
                Ok(()) => self.registered.push(hotkey),
                Err(err) => unavailable.push((binding.title.clone(), err)),
            }
        }
        unavailable
    }

    pub(super) fn unregister_shortcuts(&mut self) {
        let Some(manager) = &self.manager else { return };
        for hotkey in self.registered.drain(..) {
            let _ = manager.unregister(hotkey);
        }
    }

    pub(super) fn grab_escape(&mut self) -> global_hotkey::Result<()> {
        let Some(manager) = &self.manager else { return Ok(()) };
        if !self.escape_registered {
            manager.register(self.escape)?;
            self.escape_registered = true;
        }
        Ok(())
    }

    pub(super) fn release_escape(&mut self) {
        let Some(manager) = &self.manager else { return };
        if std::mem::take(&mut self.escape_registered) {
            let _ = manager.unregister(self.escape);
        }
    }

    pub(super) fn is_escape(&self, id: u32) -> bool {
        self.escape_registered && id == self.escape.id()
    }

    pub(super) fn is_taken(&self, hotkey: HotKey) -> bool {
        let Some(manager) = &self.manager else { return false };
        let taken = manager.register(hotkey).is_err();
        if !taken {
            let _ = manager.unregister(hotkey);
        }
        taken
    }
}
