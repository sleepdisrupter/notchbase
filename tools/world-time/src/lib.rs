//! World time: your cities side by side, hour by hour, on a strip you can drag through the week.

mod clock;
mod page;
mod prefs;
mod strip;
mod zones;

use std::marker::PhantomData;
use std::time::Duration;

use jiff::tz::TimeZone;
use notchbase_sdk::prelude::*;

pub use clock::{Clock, SystemClock};
use page::View;
use prefs::Prefs;
use zones::Place;

pub const ID: &str = "world-time";
const NAME: &str = "World Time";
const OPEN: &str = "open";
const REACH: f32 = 168.0; // a week either way, in hours
const TICK: Duration = Duration::from_secs(5);

pub struct WorldTime<C: Clock = SystemClock> {
    prefs: Prefs,
    places: Vec<Place>,
    local: TimeZone,
    home: TimeZone,
    system_full_day: bool,
    pan: f32,
    chosen: i32,
    hover: Option<f32>,
    open: bool,
    clock: PhantomData<C>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Msg {
    Open,
    Tick,
    Pointer(Pointer),
    Scrolled(Pointer),
    Now,
    Notch(NotchState),
    Configured(Settings),
}

impl<C: Clock> Tool for WorldTime<C> {
    type Msg = Msg;

    fn manifest() -> Manifest {
        Prefs::declare(
            Manifest::new(ID, NAME)
                .description("Your cities side by side, hour by hour")
                .version(env!("CARGO_PKG_VERSION"))
                .page(NAME)
                .page_icon(Icon::Clock)
                .shortcut(OPEN, "Open World Time", Some("Ctrl+Alt+W")),
        )
    }

    fn init(context: &Context) -> (Self, Command<Msg>) {
        let mut tool = Self {
            prefs: Prefs::default(),
            places: Vec::new(),
            local: context.time_zone().and_then(zones::find).unwrap_or_else(C::zone),
            home: C::zone(),
            system_full_day: context.clock_24h(),
            pan: 0.0,
            chosen: 0,
            hover: None,
            open: false,
            clock: PhantomData,
        };
        tool.configure(context.settings());
        (tool, Command::none())
    }

    fn update(&mut self, msg: Msg) -> Command<Msg> {
        match msg {
            Msg::Open => return Command::open_page(),
            Msg::Tick => {}
            Msg::Pointer(pointer) => self.point(pointer),
            // Either scroll direction moves the strip sideways.
            Msg::Scrolled(pointer) => {
                self.slide(if pointer.dx.abs() >= pointer.dy.abs() { pointer.dx } else { pointer.dy })
            }
            Msg::Now => (self.pan, self.chosen) = (0.0, 0),
            Msg::Notch(state) => self.open = state.open,
            Msg::Configured(settings) => self.configure(&settings),
        }
        Command::none()
    }

    fn subscriptions(&self) -> Subscription<Msg> {
        let listening = [
            Subscription::shortcut(OPEN, Msg::Open),
            Subscription::settings(Msg::Configured),
            Subscription::notch(Msg::Notch),
        ];
        let ticking = self.open.then(|| Subscription::every(TICK, Msg::Tick));
        Subscription::batch(listening.into_iter().chain(ticking))
    }

    fn tile(&self) -> Option<Tile<Msg>> {
        let tile = Tile::new(NAME).icon(Icon::Clock).shortcut(OPEN).on_press(Msg::Open);
        Some(match self.view().glance() {
            Some(glance) => tile.footer(text(glance).subheadline().secondary()),
            None => tile,
        })
    }

    fn page(&self) -> Option<Element<Msg>> {
        Some(self.view().page())
    }
}

impl<C: Clock> WorldTime<C> {
    fn configure(&mut self, settings: &Settings) {
        self.prefs = Prefs::from(settings);
        let home =
            if self.prefs.home.is_empty() { Place::local(self.local.clone()) } else { Place::named(&self.prefs.home) };
        let mut places = vec![home];
        for city in self.prefs.cities.iter().map(|city| Place::named(city)) {
            if places.iter().all(|place| !place.name.eq_ignore_ascii_case(&city.name)) {
                places.push(city);
            }
        }
        // A home that can't be found falls back to this computer.
        self.home = places[0].zone.clone().unwrap_or_else(|| self.local.clone());
        self.places = places;
    }

    fn point(&mut self, pointer: Pointer) {
        let over = Some(pointer.x).filter(|over| over.is_finite());
        match pointer.kind {
            PointerKind::Hover => self.hover = over,
            PointerKind::Leave => self.hover = None,
            PointerKind::Drag => {
                self.slide(pointer.dx);
                self.hover = over;
            }
            PointerKind::Click => self.chosen = over.map_or(self.chosen, |over| page::hour_at(self.pan, over)),
            PointerKind::Press | PointerKind::Release | PointerKind::Scroll => {}
        }
    }

    fn slide(&mut self, by: f32) {
        if by.is_finite() {
            self.pan = (self.pan - by / page::CELL_W).clamp(-REACH, REACH);
        }
    }

    fn view(&self) -> View<'_> {
        View {
            places: &self.places,
            home: &self.home,
            now: C::now(),
            pan: self.pan,
            chosen: self.chosen,
            hover: self.hover,
            full_day: self.prefs.hours.full_day(self.system_full_day),
        }
    }
}

notchbase_sdk::export!(WorldTime);

#[cfg(test)]
mod tests;
