use std::time::Duration;

use crate::prelude::*;

#[derive(Default)]
pub(super) struct Counter {
    pub(super) count: i32,
    pub(super) picked: Option<Color>,
    pub(super) ticks: u32,
    pub(super) loud: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) enum Msg {
    Add(i32),
    Pick,
    Picked(Option<Color>),
    Tick,
    Loud(bool),
    Settings(Settings),
    Later(i32),
}

impl Tool for Counter {
    type Msg = Msg;

    fn manifest() -> Manifest {
        Manifest::new("counter", "Counter")
            .shortcut("add", "Add One", Some("Ctrl+K"))
            .setting(SettingSpec::toggle("loud", "Loud", false))
            .setting(SettingSpec::number("step", "Step", [1.0, 10.0], 1.0, 1.0))
    }

    fn init(context: &Context) -> (Self, Command<Msg>) {
        (Self { loud: context.settings().bool("loud"), ..Self::default() }, Command::none())
    }

    fn update(&mut self, msg: Msg) -> Command<Msg> {
        match msg {
            Msg::Add(n) => self.count += n,
            Msg::Pick => return Command::pick_color(Msg::Picked),
            Msg::Picked(color) => self.picked = color,
            Msg::Tick => self.ticks += 1,
            Msg::Loud(on) => return Command::save_setting("loud", on),
            Msg::Settings(settings) => self.loud = settings.bool("loud"),
            Msg::Later(n) => return Command::perform(move || n * 2, Msg::Add),
        }
        Command::none()
    }

    fn subscriptions(&self) -> Subscription<Msg> {
        Subscription::batch([
            Subscription::every(Duration::from_secs(1), Msg::Tick),
            Subscription::shortcut("add", Msg::Add(1)),
            Subscription::settings(Msg::Settings),
        ])
    }

    fn tile(&self) -> Option<Tile<Msg>> {
        let accessory = crate::row![button("+5").on_press(Msg::Add(5)), toggle(self.loud).on_toggle(Msg::Loud)];
        Some(Tile::new("Counter").icon(Icon::Plus).on_press(Msg::Add(1)).shortcut("add").accessory(accessory))
    }

    fn page(&self) -> Option<Element<Msg>> {
        Some(crate::column![text(self.count.to_string()).display(), button("Pick").on_press(Msg::Pick)].into())
    }
}
