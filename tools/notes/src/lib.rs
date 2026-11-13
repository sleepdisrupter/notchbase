//! Notes: jot things down, pin one to the notch and keep a Markdown copy in a folder.

mod export;
mod page;
mod prefs;
#[cfg(test)]
mod tests;

use std::time::Duration;

use notchbase_sdk::prelude::*;

use prefs::Prefs;

pub const ID: &str = "notes";
const OPEN: &str = "open";
const NOTES: &str = "notes";
const PINNED: &str = "pinned";
const COPIED_FOR: Duration = Duration::from_millis(1600);
const LIVE_CHARS: usize = 22;

pub struct Notes {
    notes: Vec<String>,
    pinned: Option<usize>,
    draft: String,
    holding: bool,
    prefs: Prefs,
    notch: NotchState,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Msg {
    Open,
    Draft(String),
    Add,
    Copy(usize),
    Pin(usize),
    Remove(usize),
    KeepOpen(bool),
    Exported(Result<(), String>),
    Configured(Settings),
    Notch(NotchState),
}

impl Tool for Notes {
    type Msg = Msg;

    fn manifest() -> Manifest {
        Manifest::new(ID, "Notes")
            .description("Jot things down, pin one to the notch and keep a Markdown copy")
            .version(env!("CARGO_PKG_VERSION"))
            .page("Notes")
            .shortcut(OPEN, "Open Notes", Some("Ctrl+Alt+N"))
            .setting(SettingSpec::number(prefs::LIMIT, "Keep at most", [5.0, 100.0], 5.0, 30.0).unit("notes"))
            .setting(SettingSpec::toggle(prefs::EXPORT, "Save to a file", false).detail("Keep notes.md up to date"))
            .setting(
                SettingSpec::folder(prefs::FOLDER, "Notes folder")
                    .placeholder("Documents/Notes")
                    .shown_when(prefs::EXPORT),
            )
            .setting(SettingSpec::toggle(prefs::DETAILS, "Show notch details", true).detail("What the notch is doing"))
            .setting(SettingSpec::data(NOTES, Value::Array(Vec::new())))
            .setting(SettingSpec::data(PINNED, Value::Null))
    }

    fn init(context: &Context) -> (Self, Command<Msg>) {
        let settings = context.settings();
        let notes = Self {
            notes: settings.parse(NOTES).unwrap_or_default(),
            pinned: settings.parse(PINNED),
            draft: String::new(),
            holding: false,
            prefs: Prefs::from(settings, context.dirs()),
            notch: NotchState::default(),
        };
        (notes, Command::none())
    }

    fn update(&mut self, msg: Msg) -> Command<Msg> {
        match msg {
            Msg::Open => Command::open_page(),
            Msg::Draft(text) => {
                self.draft = text;
                Command::none()
            }
            Msg::Add => self.add(),
            Msg::Copy(index) => self.copy(index),
            Msg::Pin(index) => {
                self.pinned = if self.pinned == Some(index) { None } else { Some(index) };
                self.save()
            }
            Msg::Remove(index) => self.remove(index),
            Msg::KeepOpen(on) => {
                self.holding = on;
                if on { Command::hold_open() } else { Command::release() }
            }
            Msg::Exported(Ok(())) => Command::none(),
            Msg::Exported(Err(err)) => Command::notify(err),
            Msg::Configured(settings) => {
                let exporting = self.prefs.exports();
                self.prefs = Prefs::from(&settings, &self.prefs.dirs);
                self.notes = settings.parse(NOTES).unwrap_or_default();
                self.pinned = settings.parse(PINNED);
                self.trim();
                // Turning the file on writes it straight away.
                if !exporting && self.prefs.exports() { self.export() } else { Command::none() }
            }
            Msg::Notch(state) => {
                self.notch = state;
                Command::none()
            }
        }
    }

    fn subscriptions(&self) -> Subscription<Msg> {
        let always =
            Subscription::batch([Subscription::shortcut(OPEN, Msg::Open), Subscription::settings(Msg::Configured)]);
        if self.prefs.details { always.and(Subscription::notch(Msg::Notch)) } else { always }
    }

    fn tile(&self) -> Option<Tile<Msg>> {
        let count = text(self.notes.len().to_string()).value().secondary();
        let tile = Tile::new("Notes").icon(Icon::Note).shortcut(OPEN).on_press(Msg::Open).accessory(count);
        Some(match self.pinned_note() {
            Some(note) => tile.footer(text(page::preview(note, LIVE_CHARS)).caption().secondary()),
            None => tile,
        })
    }

    fn page(&self) -> Option<Element<Msg>> {
        Some(page::page(self))
    }

    fn live(&self) -> Option<Live<Msg>> {
        let note = self.pinned_note()?;
        Some(Live::new(icon(Icon::Note).tone(Tone::Yellow), text(page::preview(note, LIVE_CHARS)).callout()))
    }
}

impl Notes {
    fn pinned_note(&self) -> Option<&String> {
        self.notes.get(self.pinned?)
    }

    fn add(&mut self) -> Command<Msg> {
        let note = std::mem::take(&mut self.draft).trim().to_owned();
        if note.is_empty() {
            return Command::none();
        }
        let pinned = self.pinned_note().cloned();
        self.notes.retain(|saved| *saved != note);
        self.notes.insert(0, note);
        self.pinned = pinned.and_then(|pinned| self.notes.iter().position(|saved| *saved == pinned));
        self.trim();
        self.save()
    }

    fn remove(&mut self, index: usize) -> Command<Msg> {
        if index >= self.notes.len() {
            return Command::none();
        }
        self.notes.remove(index);
        self.pinned = match self.pinned {
            Some(pinned) if pinned == index => None,
            Some(pinned) if pinned > index => Some(pinned - 1),
            other => other,
        };
        self.save()
    }

    fn copy(&self, index: usize) -> Command<Msg> {
        let Some(note) = self.notes.get(index) else { return Command::none() };
        let notice = Notice::new(icon(Icon::Checkmark).tone(Tone::Green), text("Copied").callout())
            .below(text(page::preview(note, 40)).caption().secondary())
            .duration(COPIED_FOR);
        Command::copy_text(note.clone()).and(Command::notice(notice))
    }

    fn trim(&mut self) {
        self.notes.truncate(self.prefs.limit);
        self.pinned = self.pinned.filter(|pinned| *pinned < self.notes.len());
    }

    fn save(&self) -> Command<Msg> {
        let pinned = self.pinned.map_or(Value::Null, Value::from);
        Command::save_setting(NOTES, self.notes.clone()).and(Command::save_setting(PINNED, pinned)).and(self.export())
    }

    fn export(&self) -> Command<Msg> {
        let Some(folder) = self.prefs.export_folder() else { return Command::none() };
        let notes = self.notes.clone();
        Command::perform(move || export::write(&folder, &notes), Msg::Exported)
    }
}

notchbase_sdk::export!(Notes);
