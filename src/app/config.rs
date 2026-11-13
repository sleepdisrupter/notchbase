//! Follows config.json while the app runs and applies whatever changed.

use std::time::Instant;

use winit::event_loop::ActiveEventLoop;

use crate::assistant::{Hooks, Provider};
use crate::config::{Config, Refresh};

use super::App;
use super::tools::tabs;

impl App {
    pub(super) fn follow_config(&mut self, event_loop: &ActiveEventLoop, now: Instant) {
        match self.config.refresh(now) {
            Refresh::Unchanged => {}
            Refresh::Broken(problem) => self.notify(&problem),
            Refresh::Changed(previous) => self.apply_config(event_loop, &previous),
        }
    }

    fn apply_config(&mut self, event_loop: &ActiveEventLoop, previous: &Config) {
        let current = self.config.config().clone();
        let enabled = |config: &Config| {
            config.tools.iter().filter(|(_, tool)| !tool.enabled).map(|(id, _)| id.clone()).collect::<Vec<_>>()
        };
        if enabled(previous) != enabled(&current) {
            self.reload_tools(event_loop);
        } else {
            if previous.layout.tools != current.layout.tools {
                self.reorder_tools();
            }
            self.tools.configure(&current);
            let shortcuts = |config: &Config| {
                config.tools.iter().map(|(id, tool)| (id.clone(), tool.shortcuts.clone())).collect::<Vec<_>>()
            };
            if shortcuts(previous) != shortcuts(&current) {
                self.rebind_shortcuts();
            }
        }
        self.tabs = tabs(&current, self.tools.hosts());
        if previous.displays.all != current.displays.all {
            self.rearrange = true;
        }
        self.follow_auto_update(previous);
        self.follow_find(previous);
        self.follow_whats_new(previous);
        if previous.volume.indicator != current.volume.indicator
            || previous.brightness.indicator != current.brightness.indicator
        {
            self.start_volume_indicator();
        }
        if hooked(previous) != hooked(&current) {
            self.connect_assistant(hooked(previous).map(|(provider, _)| provider), true);
        }
        self.redraw_notch();
    }
}

pub(super) fn hooked(config: &Config) -> Option<(Provider, Hooks)> {
    let provider = assistant_of(config)?;
    match (config.assistant.questions_in_notch, config.assistant.progress_in_notch) {
        (true, _) => Some((provider, Hooks::Questions)),
        (false, true) => Some((provider, Hooks::Progress)),
        (false, false) => None,
    }
}

pub(super) fn assistant_of(config: &Config) -> Option<Provider> {
    let provider = match config.assistant.provider.as_str() {
        "chatgpt" | "codex" => Provider::Codex,
        _ => Provider::Claude,
    };
    config.assistant.enabled.then_some(provider)
}

pub(super) fn provider_key(provider: Provider) -> &'static str {
    match provider {
        Provider::Claude => "claude",
        Provider::Codex => "chatgpt",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assistant_read_from_config() {
        let mut config = Config::default();
        assert_eq!(assistant_of(&config), Some(Provider::Claude));
        config.assistant.provider = "codex".into();
        assert_eq!(assistant_of(&config), Some(Provider::Codex));
        config.assistant.enabled = false;
        assert_eq!(assistant_of(&config), None);
        assert_eq!(provider_key(Provider::Codex), "chatgpt");
    }

    #[test]
    fn hooks_follow_the_switches() {
        let mut config = Config::default();
        assert_eq!(hooked(&config), Some((Provider::Claude, Hooks::Questions)));
        config.assistant.questions_in_notch = false;
        assert_eq!(hooked(&config), Some((Provider::Claude, Hooks::Progress)));
        config.assistant.progress_in_notch = false;
        assert_eq!(hooked(&config), None);
    }
}
