//! Puts notchbase's hooks into the assistant's config and takes them out again: settings.json for Claude Code,
//! hooks.json for Codex. Only our handlers are touched, and installing twice changes nothing.

mod claude;
mod codex;
mod exe;
mod hooks;
mod json_file;

#[cfg(test)]
mod tests;

use std::path::{Path, PathBuf};

use serde_json::{Map, Value};

use super::prompt::Provider;
use hooks::{Ours, Spec};

/// Which hooks to install. Progress alone leaves out the ones that answer, so the assistant is never held up.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hooks {
    Questions,
    Progress,
}

pub fn install(provider: Provider, hooks: Hooks) -> Result<(), String> {
    let file = existing_config(provider)?.join(file_name(provider));
    install_file(&file, &spec(provider, &exe::current()?).reaching(hooks))
}

pub fn uninstall(provider: Provider) -> Result<(), String> {
    let file = config_dir(provider).ok_or_else(no_home)?.join(file_name(provider));
    if !file.exists() {
        return Ok(());
    }
    uninstall_file(&file, ours(provider))
}

/// Whether the hooks are in place exactly as install would write them.
pub fn installed(provider: Provider, hooks: Hooks) -> bool {
    let Some(file) = config_dir(provider).map(|dir| dir.join(file_name(provider))) else {
        return false;
    };
    exe::current().is_ok_and(|exe| installed_file(&file, &spec(provider, &exe).reaching(hooks)))
}

fn install_file(file: &Path, spec: &Spec) -> Result<(), String> {
    edit(file, |root| hooks::apply(root, spec))
}

fn uninstall_file(file: &Path, ours: Ours) -> Result<(), String> {
    edit(file, |root| hooks::strip(root, ours))
}

fn installed_file(file: &Path, spec: &Spec) -> bool {
    let Ok(doc) = json_file::load(file) else {
        return false;
    };
    let mut root = doc.root.clone();
    doc.existed && hooks::apply(&mut root, spec).is_ok() && root == doc.root
}

fn edit(file: &Path, change: impl FnOnce(&mut Map<String, Value>) -> Result<(), String>) -> Result<(), String> {
    let doc = json_file::load(file)?;
    let mut root = doc.root.clone();
    change(&mut root).map_err(|reason| format!("{reason} in {}, so it was left untouched", file.display()))?;
    if doc.existed && root == doc.root {
        return Ok(());
    }
    json_file::save(file, &root, &doc)
}

fn spec(provider: Provider, exe: &str) -> Spec {
    match provider {
        Provider::Claude => claude::spec(exe),
        Provider::Codex => codex::spec(exe),
    }
}

fn ours(provider: Provider) -> Ours {
    match provider {
        Provider::Claude => claude::ours,
        Provider::Codex => codex::ours,
    }
}

fn file_name(provider: Provider) -> &'static str {
    match provider {
        Provider::Claude => "settings.json",
        Provider::Codex => "hooks.json",
    }
}

fn existing_config(provider: Provider) -> Result<PathBuf, String> {
    let dir = config_dir(provider).ok_or_else(no_home)?;
    if !dir.is_dir() {
        let app = match provider {
            Provider::Claude => "Claude Code",
            Provider::Codex => "Codex",
        };
        return Err(format!("{app} isn't set up on this computer yet ({} doesn't exist)", dir.display()));
    }
    Ok(dir)
}

fn config_dir(provider: Provider) -> Option<PathBuf> {
    let (variable, folder) = match provider {
        Provider::Claude => ("CLAUDE_CONFIG_DIR", ".claude"),
        Provider::Codex => ("CODEX_HOME", ".codex"),
    };
    let home = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
    std::env::var_os(variable)
        .filter(|dir| !dir.to_string_lossy().trim().is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os(home).filter(|dir| !dir.is_empty()).map(|dir| PathBuf::from(dir).join(folder)))
}

fn no_home() -> String {
    "Couldn't find your home folder".to_owned()
}
