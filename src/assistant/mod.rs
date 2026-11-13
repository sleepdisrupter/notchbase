//! Claude Code and Codex in the notch, through their hooks. The assistant runs `notchbase hook ...`, which hands the
//! event to this app's local server and prints the answer back.

mod board;
mod call;
mod endpoint;
mod hook;
mod install;
mod prompt;
mod runs;
mod server;
mod timing;
mod token;
mod wire;

#[cfg(test)]
mod tests;

use std::path::Path;

use crate::poll::Wake;

use board::Board;
use server::{HoldFor, Server};

pub use install::{Hooks, install, installed, uninstall};
pub use prompt::{Offer, Prompt, PromptKind, Provider, Question, Reply};
pub use runs::{Finished, Run};

/// Runs as a hook when the first argument is `hook`. None means this is an ordinary launch.
pub fn run_hook_from_args() -> Option<i32> {
    hook::run_from_args()
}

pub fn is_hook_launch(first: Option<&std::ffi::OsStr>) -> bool {
    hook::is_launch(first)
}

/// Tells the copy already running to show itself. False if none could be reached.
pub fn show_running() -> bool {
    crate::platform::data_dir().is_some_and(|dir| call::send(&dir, call::Request::Show))
}

pub struct Assistant {
    server: Option<Server>,
}

impl Assistant {
    /// Starts the local server. If it can't start, hooks find no one listening and leave the asking to the assistant.
    pub fn start(wake: Wake) -> Self {
        match crate::platform::data_dir() {
            Some(dir) => Self::start_in(&dir, wake, timing::hold),
            None => Self { server: None },
        }
    }

    fn start_in(dir: &Path, wake: Wake, hold_for: HoldFor) -> Self {
        Self { server: Server::start(dir, wake, hold_for) }
    }

    pub fn running(&self) -> bool {
        self.server.is_some()
    }

    pub fn version(&self) -> u64 {
        self.server.as_ref().map_or(0, |server| server.read(Board::version))
    }

    pub fn shows(&self) -> u64 {
        self.server.as_ref().map_or(0, Server::shows)
    }

    pub fn prompts(&self) -> Vec<Prompt> {
        self.server.as_ref().map(|server| server.read(Board::prompts)).unwrap_or_default()
    }

    pub fn runs(&self) -> Vec<Run> {
        self.server.as_ref().map(|server| server.read(Board::runs)).unwrap_or_default()
    }

    pub fn finished(&self) -> Vec<Finished> {
        self.server.as_ref().map(|server| server.read(Board::finished)).unwrap_or_default()
    }

    pub fn window_of(&self, id: u64) -> Option<u32> {
        self.server.as_ref().and_then(|server| server.read(|board| board.window_of(id)))
    }

    pub fn reply(&self, id: u64, reply: Reply) {
        if let Some(server) = &self.server {
            server.change(|board| board.reply(id, reply, notchbase_sdk::now_ms()));
        }
    }

    pub fn set_enabled(&self, provider: Provider, enabled: bool) {
        if let Some(server) = &self.server {
            server.change(|board| board.set_enabled(provider, enabled));
        }
    }
}

impl Drop for Assistant {
    fn drop(&mut self) {
        if let Some(server) = &self.server {
            // Releases every held prompt, so waiting hooks fall back to the assistant's own prompt.
            server.stop();
        }
    }
}
