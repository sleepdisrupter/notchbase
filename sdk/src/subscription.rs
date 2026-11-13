//! Subscriptions: the events and feeds a tool listens to.

use std::rc::Rc;
use std::time::Duration;

use crate::protocol::{
    AudioInput, CalendarFeed, DeviceChange, DownloadChange, Event, Focus, NotchState, NowPlaying, Power,
    RecordingUpdate, Sensors, VolumeLevel, Wants,
};
use crate::settings::Settings;

type Map<T, Msg> = Rc<dyn Fn(T) -> Msg>;

pub(crate) enum Listener<Msg> {
    Every(u64, Map<(), Msg>),
    Shortcut(String, Map<(), Msg>),
    Settings(Map<Settings, Msg>),
    Recording(Map<RecordingUpdate, Msg>),
    AudioInputs(Map<Vec<AudioInput>, Msg>),
    Media(Map<Option<NowPlaying>, Msg>),
    Bluetooth(Map<DeviceChange, Msg>),
    Volume(Map<VolumeLevel, Msg>),
    Notch(Map<NotchState, Msg>),
    Calendar(Option<[u64; 2]>, Map<CalendarFeed, Msg>),
    Power(Map<Power, Msg>),
    ScreenLock(Map<bool, Msg>),
    Sensors(Map<Sensors, Msg>),
    Downloads(Map<DownloadChange, Msg>),
    Focus(Map<Focus, Msg>),
}

/// What a tool wants to hear about. Combine several with `batch` or `and`.
pub struct Subscription<Msg> {
    listeners: Vec<Listener<Msg>>,
}

impl<Msg> Default for Subscription<Msg> {
    fn default() -> Self {
        Self { listeners: Vec::new() }
    }
}

impl<Msg: 'static> Subscription<Msg> {
    pub fn none() -> Self {
        Self::default()
    }

    pub fn batch(subscriptions: impl IntoIterator<Item = Subscription<Msg>>) -> Self {
        Self { listeners: subscriptions.into_iter().flat_map(|subscription| subscription.listeners).collect() }
    }

    pub fn and(mut self, other: Subscription<Msg>) -> Self {
        self.listeners.extend(other.listeners);
        self
    }

    /// Sends `msg` on a timer. Anything faster than 16 ms is treated as 16 ms.
    pub fn every(interval: Duration, msg: Msg) -> Self
    where
        Msg: Clone,
    {
        let every_ms = interval.as_millis().clamp(16, u128::from(u64::MAX)) as u64;
        Self::one(Listener::Every(every_ms, Rc::new(move |()| msg.clone())))
    }

    /// Sends `msg` when the shortcut with this id, declared in the manifest, is pressed.
    pub fn shortcut(id: impl Into<String>, msg: Msg) -> Self
    where
        Msg: Clone,
    {
        Self::one(Listener::Shortcut(id.into(), Rc::new(move |()| msg.clone())))
    }

    pub fn settings(map: impl Fn(Settings) -> Msg + 'static) -> Self {
        Self::one(Listener::Settings(Rc::new(map)))
    }

    pub fn recording(map: impl Fn(RecordingUpdate) -> Msg + 'static) -> Self {
        Self::one(Listener::Recording(Rc::new(map)))
    }

    pub fn audio_inputs(map: impl Fn(Vec<AudioInput>) -> Msg + 'static) -> Self {
        Self::one(Listener::AudioInputs(Rc::new(map)))
    }

    pub fn media(map: impl Fn(Option<NowPlaying>) -> Msg + 'static) -> Self {
        Self::one(Listener::Media(Rc::new(map)))
    }

    pub fn bluetooth(map: impl Fn(DeviceChange) -> Msg + 'static) -> Self {
        Self::one(Listener::Bluetooth(Rc::new(map)))
    }

    pub fn volume(map: impl Fn(VolumeLevel) -> Msg + 'static) -> Self {
        Self::one(Listener::Volume(Rc::new(map)))
    }

    pub fn notch(map: impl Fn(NotchState) -> Msg + 'static) -> Self {
        Self::one(Listener::Notch(Rc::new(map)))
    }

    pub fn calendar(map: impl Fn(CalendarFeed) -> Msg + 'static) -> Self {
        Self::one(Listener::Calendar(None, Rc::new(map)))
    }

    /// Like `calendar`, but asks for events between two times in Unix milliseconds.
    pub fn calendar_between(from: u64, to: u64, map: impl Fn(CalendarFeed) -> Msg + 'static) -> Self {
        Self::one(Listener::Calendar(Some([from.min(to), from.max(to)]), Rc::new(map)))
    }

    pub fn power(map: impl Fn(Power) -> Msg + 'static) -> Self {
        Self::one(Listener::Power(Rc::new(map)))
    }

    pub fn screen_lock(map: impl Fn(bool) -> Msg + 'static) -> Self {
        Self::one(Listener::ScreenLock(Rc::new(map)))
    }

    pub fn sensors(map: impl Fn(Sensors) -> Msg + 'static) -> Self {
        Self::one(Listener::Sensors(Rc::new(map)))
    }

    pub fn downloads(map: impl Fn(DownloadChange) -> Msg + 'static) -> Self {
        Self::one(Listener::Downloads(Rc::new(map)))
    }

    pub fn focus(map: impl Fn(Focus) -> Msg + 'static) -> Self {
        Self::one(Listener::Focus(Rc::new(map)))
    }

    pub fn map<Outer: 'static>(self, map: impl Fn(Msg) -> Outer + 'static) -> Subscription<Outer> {
        let map = Rc::new(map);
        let listeners = self
            .listeners
            .into_iter()
            .map(|listener| {
                let map = map.clone();
                match listener {
                    Listener::Every(ms, inner) => Listener::Every(ms, Rc::new(move |()| map(inner(())))),
                    Listener::Shortcut(id, inner) => Listener::Shortcut(id, Rc::new(move |()| map(inner(())))),
                    Listener::Settings(inner) => Listener::Settings(Rc::new(move |value| map(inner(value)))),
                    Listener::Recording(inner) => Listener::Recording(Rc::new(move |value| map(inner(value)))),
                    Listener::AudioInputs(inner) => Listener::AudioInputs(Rc::new(move |value| map(inner(value)))),
                    Listener::Media(inner) => Listener::Media(Rc::new(move |value| map(inner(value)))),
                    Listener::Bluetooth(inner) => Listener::Bluetooth(Rc::new(move |value| map(inner(value)))),
                    Listener::Volume(inner) => Listener::Volume(Rc::new(move |value| map(inner(value)))),
                    Listener::Notch(inner) => Listener::Notch(Rc::new(move |value| map(inner(value)))),
                    Listener::Calendar(range, inner) => {
                        Listener::Calendar(range, Rc::new(move |value| map(inner(value))))
                    }
                    Listener::Power(inner) => Listener::Power(Rc::new(move |value| map(inner(value)))),
                    Listener::ScreenLock(inner) => Listener::ScreenLock(Rc::new(move |value| map(inner(value)))),
                    Listener::Sensors(inner) => Listener::Sensors(Rc::new(move |value| map(inner(value)))),
                    Listener::Downloads(inner) => Listener::Downloads(Rc::new(move |value| map(inner(value)))),
                    Listener::Focus(inner) => Listener::Focus(Rc::new(move |value| map(inner(value)))),
                }
            })
            .collect();
        Subscription { listeners }
    }

    pub(crate) fn wants(&self) -> Wants {
        let mut wants = Wants::default();
        for listener in &self.listeners {
            match listener {
                Listener::Every(ms, _) => {
                    if !wants.every_ms.contains(ms) {
                        wants.every_ms.push(*ms);
                    }
                }
                Listener::Shortcut(..) => {}
                Listener::Settings(_) => wants.settings = true,
                Listener::Recording(_) => wants.recording = true,
                Listener::AudioInputs(_) => wants.audio_inputs = true,
                Listener::Media(_) => wants.media = true,
                Listener::Bluetooth(_) => wants.bluetooth = true,
                Listener::Volume(_) => wants.volume = true,
                Listener::Notch(_) => wants.notch = true,
                Listener::Calendar(range, _) => {
                    wants.calendar = true;
                    wants.calendar_range = widest(wants.calendar_range, *range);
                }
                Listener::Power(_) => wants.power = true,
                Listener::ScreenLock(_) => wants.screen_lock = true,
                Listener::Sensors(_) => wants.sensors = true,
                Listener::Downloads(_) => wants.downloads = true,
                Listener::Focus(_) => wants.focus = true,
            }
        }
        wants.every_ms.sort_unstable();
        wants
    }

    pub(crate) fn route(&self, event: &Event, settings: &Settings) -> Vec<Msg> {
        self.listeners
            .iter()
            .filter_map(|listener| match (listener, event) {
                (Listener::Every(ms, map), Event::Tick { every_ms }) if ms == every_ms => Some(map(())),
                (Listener::Shortcut(id, map), Event::Shortcut { id: pressed }) if id == pressed => Some(map(())),
                (Listener::Settings(map), Event::Settings { .. }) => Some(map(settings.clone())),
                (Listener::Recording(map), Event::Recording { update }) => Some(map(update.clone())),
                (Listener::AudioInputs(map), Event::AudioInputs { inputs }) => Some(map(inputs.clone())),
                (Listener::Media(map), Event::Media { now_playing }) => Some(map(now_playing.clone())),
                (Listener::Bluetooth(map), Event::Bluetooth { change }) => Some(map(change.clone())),
                (Listener::Volume(map), Event::Volume { level }) => Some(map(*level)),
                (Listener::Notch(map), Event::Notch { state }) => Some(map(state.clone())),
                (Listener::Calendar(_, map), Event::Calendar { feed }) => Some(map(feed.clone())),
                (Listener::Power(map), Event::Power { power }) => Some(map(*power)),
                (Listener::ScreenLock(map), Event::ScreenLock { locked }) => Some(map(*locked)),
                (Listener::Sensors(map), Event::Sensors { sensors }) => Some(map(sensors.clone())),
                (Listener::Downloads(map), Event::Download { change }) => Some(map(change.clone())),
                (Listener::Focus(map), Event::Focus { focus }) => Some(map(focus.clone())),
                _ => None,
            })
            .collect()
    }

    fn one(listener: Listener<Msg>) -> Self {
        Self { listeners: vec![listener] }
    }
}

/// The range covering both, when two calendar subscriptions ask for different ones.
pub fn widest(a: Option<[u64; 2]>, b: Option<[u64; 2]>) -> Option<[u64; 2]> {
    match (a, b) {
        (Some([a_from, a_to]), Some([b_from, b_to])) => Some([a_from.min(b_from), a_to.max(b_to)]),
        (one, None) | (None, one) => one,
    }
}
