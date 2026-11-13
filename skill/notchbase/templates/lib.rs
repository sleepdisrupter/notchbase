#[cfg(test)]
mod tests;

use std::time::Duration;

use notchbase_sdk::column;
use notchbase_sdk::prelude::*;

pub const ID: &str = "__ID__";
const OPEN: &str = "open";
const ITEMS: &str = "items";
const LOUD: &str = "loud";
const DONE_FOR: Duration = Duration::from_millis(1600);

pub struct __TYPE__ {
    items: Vec<String>,
    draft: String,
    loud: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Msg {
    Open,
    Draft(String),
    Add,
    Remove(usize),
    Configured(Settings),
}

impl Tool for __TYPE__ {
    type Msg = Msg;

    fn manifest() -> Manifest {
        Manifest::new(ID, "__NAME__")
            .description("__NAME__ in the notch")
            .version(env!("CARGO_PKG_VERSION"))
            .page("__NAME__")
            .shortcut(OPEN, "Open __NAME__", Some("Ctrl+Alt+K"))
            .setting(SettingSpec::toggle(LOUD, "Chime when adding", false).detail("Play a sound for each new item"))
            .setting(SettingSpec::data(ITEMS, Value::Array(Vec::new())))
    }

    fn init(context: &Context) -> (Self, Command<Msg>) {
        let settings = context.settings();
        let tool = Self { items: settings.parse(ITEMS).unwrap_or_default(), draft: String::new(), loud: settings.bool(LOUD) };
        (tool, Command::none())
    }

    fn update(&mut self, msg: Msg) -> Command<Msg> {
        match msg {
            Msg::Open => Command::open_page(),
            Msg::Draft(text) => {
                self.draft = text;
                Command::none()
            }
            Msg::Add => {
                let item = std::mem::take(&mut self.draft).trim().to_owned();
                if item.is_empty() {
                    return Command::none();
                }
                self.items.insert(0, item);
                let done = Notice::new(icon(Icon::Checkmark).tone(Tone::Green), text("Added").callout()).duration(DONE_FOR);
                let chime = if self.loud { Command::chime() } else { Command::none() };
                self.save().and(Command::notice(done)).and(chime)
            }
            Msg::Remove(index) => {
                if index < self.items.len() {
                    self.items.remove(index);
                }
                self.save()
            }
            Msg::Configured(settings) => {
                self.items = settings.parse(ITEMS).unwrap_or_default();
                self.loud = settings.bool(LOUD);
                Command::none()
            }
        }
    }

    fn subscriptions(&self) -> Subscription<Msg> {
        Subscription::batch([Subscription::shortcut(OPEN, Msg::Open), Subscription::settings(Msg::Configured)])
    }

    fn tile(&self) -> Option<Tile<Msg>> {
        let count = text(self.items.len().to_string()).value().secondary();
        Some(Tile::new("__NAME__").icon(Icon::Star).shortcut(OPEN).on_press(Msg::Open).accessory(count))
    }

    fn page(&self) -> Option<Element<Msg>> {
        let composer = row![
            text_input(&self.draft).placeholder("Add an item").on_change(Msg::Draft).on_submit(Msg::Add),
            button("Add").filled().enabled(!self.draft.trim().is_empty()).on_press(Msg::Add),
        ]
        .spacing(8.0);
        let items = if self.items.is_empty() {
            card(text("Nothing yet. Add one above.").secondary())
        } else {
            list(self.items.iter().enumerate().map(|(index, item)| {
                let remove = button("").icon(Icon::Xmark).plain().on_press(Msg::Remove(index));
                keyed(format!("{index}-{item}"), row![text(item.clone()), spacer(), remove].align(Align::Center))
            }))
        };
        Some(column![composer, items].spacing(10.0).into())
    }

    fn live(&self) -> Option<Live<Msg>> {
        None
    }
}

impl __TYPE__ {
    fn save(&self) -> Command<Msg> {
        Command::save_setting(ITEMS, self.items.clone())
    }
}

notchbase_sdk::export!(__TYPE__);
