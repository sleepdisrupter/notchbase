//! Calendar: the next meeting in the notch with a button to join it, and a page with the days
//! ahead, a month view and an event editor. Events come from Apple Calendar or from calendar links.

mod browse;
mod clock;
mod day;
mod editor;
mod fetch;
mod ics;
mod join;
mod links;
mod live;
mod local;
mod meeting;
mod page;
mod paint;
mod prefs;
mod status;
mod system;
mod upcoming;

use std::collections::BTreeSet;
use std::marker::PhantomData;
use std::path::PathBuf;
use std::time::Duration;

use notchbase_sdk::column;
use notchbase_sdk::prelude::*;

use browse::Browse;
pub use clock::{Clock, SystemClock};
use day::Days;
use editor::{Change, Editor};
use fetch::Tag;
use ics::Feed;
use links::Links;
use local::Local;
use meeting::Meeting;
use paint::Paints;
use prefs::{Prefs, Source};
use status::Status;
use system::System;
use upcoming::Phase;

pub const ID: &str = "calendar";
const REFRESH: Duration = Duration::from_secs(60);
const WATCHED: Duration = Duration::from_secs(15);
const QUICK: Duration = Duration::from_secs(5);
const CALM: Duration = Duration::from_secs(30);
const SOON_MS: u64 = 60_000;
const BADGE: f32 = 18.0;
const ACCOUNTS: &str = "x-apple.systempreferences:com.apple.Internet-Accounts-Settings.extension";
const PRIVACY: &str = "x-apple.systempreferences:com.apple.preference.security?Privacy_Calendars";

pub struct Calendar<C: Clock = SystemClock> {
    prefs: Prefs,
    links: Links,
    system: System,
    meetings: Vec<Meeting>,
    days: Days,
    browse: Browse,
    local: Local,
    paints: Paints,
    hidden: BTreeSet<String>,
    nudged: BTreeSet<String>,
    editor: Option<Editor>,
    on_page: bool,
    held: bool,
    cache: Option<PathBuf>,
    generation: u64,
    fetching: usize,
    open: bool,
    clock_24h: bool,
    clock: PhantomData<C>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Msg {
    Configured(Settings),
    System(CalendarFeed),
    Loaded { link: String, generation: u64, result: Result<Feed, String>, cached: bool },
    Fetched { link: String, generation: u64, result: Result<Option<(Feed, Tag)>, String> },
    Refresh,
    Tick,
    Open,
    OpenSettings,
    Accounts,
    Privacy,
    Join(String),
    Hide(String),
    SkipSetup,
    Pick(i32, u8, u8),
    Shift(i32),
    View(usize),
    Tapped(String),
    Today,
    New,
    Edit(String),
    Editor(Change),
    Save,
    Delete,
    Discard,
    Stored(Result<(), String>),
    Notch(NotchState),
}

impl<C: Clock> Tool for Calendar<C> {
    type Msg = Msg;

    fn manifest() -> Manifest {
        Prefs::declare(
            Manifest::new(ID, "Calendar")
                .description("Your next meeting in the notch, with a button to join it")
                .version(env!("CARGO_PKG_VERSION"))
                .page("Calendar")
                .page_icon(Icon::Calendar)
                .header_tab(),
        )
    }

    fn init(context: &Context) -> (Self, Command<Msg>) {
        let prefs = Prefs::read(context.settings());
        let mut links = Links::default();
        links.set(&prefs.links);
        let days = Days::around(C::now(), &C::zone());
        let cache = Some(context.data_dir()).filter(|dir| !dir.as_os_str().is_empty());
        let mut calendar = Self {
            prefs,
            links,
            system: System::default(),
            meetings: Vec::new(),
            days,
            browse: Browse::at(days.today),
            hidden: BTreeSet::new(),
            nudged: BTreeSet::new(),
            editor: None,
            on_page: false,
            held: false,
            local: Local::open(cache.as_deref()),
            paints: Paints::open(cache.as_deref()),
            cache,
            generation: 0,
            fetching: 0,
            open: false,
            clock_24h: context.clock_24h(),
            clock: PhantomData,
        };
        calendar.refresh();
        let command = calendar.load(true);
        (calendar, command)
    }

    fn update(&mut self, msg: Msg) -> Command<Msg> {
        let command = self.handle(msg);
        command.and(self.hold()).and(self.paints.flush())
    }

    fn subscriptions(&self) -> Subscription<Msg> {
        let mut listening =
            vec![Subscription::settings(Msg::Configured), Subscription::every(self.tick_every(), Msg::Tick)];
        let linked = self.prefs.source == Source::Links && !self.links.entries.is_empty();
        if self.editor.is_some() || self.held || linked {
            listening.push(Subscription::notch(Msg::Notch));
        }
        match self.prefs.source {
            Source::Apple => {
                let (from, to) = self.reach(&C::zone());
                listening.push(Subscription::calendar_between(from, to, Msg::System));
            }
            Source::Links if linked => listening.push(Subscription::every(self.refresh_every(), Msg::Refresh)),
            Source::Links => {}
        }
        Subscription::batch(listening)
    }

    fn page(&self) -> Option<Element<Msg>> {
        if let Some(editor) = &self.editor {
            let monday_first = self.prefs.monday_first(self.clock_24h);
            return Some(editor.view(self.days.today, &self.destinations(editor), self.clock_24h, monday_first));
        }
        let zone = C::zone();
        let agenda = page::Agenda {
            days: self.days,
            browse: self.browse,
            monday_first: self.prefs.monday_first(self.clock_24h),
            zone: &zone,
            now: C::now(),
            meetings: &self.meetings,
            status: self.status(),
            problems: if self.prefs.source == Source::Links { self.links.problems() } else { Vec::new() },
            all_day: self.prefs.all_day,
            apple: self.prefs.source == Source::Apple,
            week: self.prefs.week,
            clock_24h: self.clock_24h,
        };
        Some(page::page(&agenda))
    }

    fn live(&self) -> Option<Live<Msg>> {
        self.phase(C::now()).map(|phase| live::live(phase, self.prefs.lead_ms))
    }
}

impl<C: Clock> Calendar<C> {
    fn handle(&mut self, msg: Msg) -> Command<Msg> {
        match msg {
            Msg::Configured(settings) => self.configure(&settings),
            Msg::System(feed) => {
                self.system = System::read(feed);
                self.refresh();
                Command::none()
            }
            Msg::Loaded { link, generation, result, cached } => {
                if generation == self.generation && self.links.loaded(&link, result, cached) {
                    self.refresh();
                }
                Command::none()
            }
            Msg::Fetched { link, generation, result } => {
                if generation != self.generation {
                    return Command::none();
                }
                self.fetching = self.fetching.saturating_sub(1);
                let changed = match result {
                    Ok(Some((feed, tag))) => {
                        self.links.fetched(&link, tag);
                        self.links.loaded(&link, Ok(feed), false)
                    }
                    Ok(None) => self.links.unchanged(&link),
                    Err(problem) => self.links.loaded(&link, Err(problem), false),
                };
                if changed {
                    self.refresh();
                }
                Command::none()
            }
            Msg::Refresh => self.check(),
            Msg::Tick => self.tick(),
            Msg::Open => Command::open_page(),
            Msg::OpenSettings => Command::open_settings(),
            Msg::Accounts => Command::open(ACCOUNTS),
            Msg::Privacy => Command::open(PRIVACY),
            Msg::Join(link) => Command::open(link),
            Msg::Hide(id) => {
                self.hidden.insert(id);
                Command::none()
            }
            Msg::SkipSetup => {
                self.prefs.skipped_setup = true;
                Command::save_setting(prefs::SKIPPED_SETUP, true)
            }
            Msg::Pick(year, month, day) => {
                self.browse.pick(year, month, day);
                self.refresh();
                Command::none()
            }
            Msg::Shift(steps) => {
                match self.prefs.week {
                    true => self.browse.shift_days(i64::from(steps) * browse::DAYS),
                    false => self.browse.shift(steps, self.days.today),
                }
                self.refresh();
                Command::none()
            }
            Msg::View(index) => {
                self.prefs.week = index == 0;
                Command::save_setting(prefs::VIEW, if self.prefs.week { "week" } else { prefs::MONTH_VIEW })
            }
            Msg::Tapped(id) => {
                let Some(meeting) = self.meetings.iter().find(|meeting| meeting.id == id).cloned() else {
                    return Command::none();
                };
                if meeting.editable {
                    return self.handle(Msg::Edit(id));
                }
                if let Some(link) = meeting.join.clone().filter(|_| meeting.end > C::now()) {
                    return Command::open(link);
                }
                let day = day::local(meeting.start, &C::zone()).date();
                self.browse.pick(i32::from(day.year()), day.month() as u8, day.day() as u8);
                self.refresh();
                Command::none()
            }
            Msg::Today => {
                self.browse = Browse::at(self.days.today);
                self.refresh();
                Command::none()
            }
            Msg::New => {
                let on = self.browse.selected;
                self.editor = Some(Editor::new(C::now(), &C::zone(), on));
                self.on_page = true;
                Command::none()
            }
            Msg::Edit(id) => {
                let meeting = self.meetings.iter().find(|meeting| meeting.id == id && meeting.editable);
                self.editor =
                    meeting.map(|meeting| Editor::editing(meeting, &C::zone(), self.paints.tone(&meeting.id)));
                self.on_page = true;
                Command::none()
            }
            Msg::Editor(change) => {
                let destinations = self.editor.as_ref().map(|editor| self.destinations(editor)).unwrap_or_default();
                if let Some(editor) = &mut self.editor {
                    editor.change(change, &destinations);
                }
                Command::none()
            }
            Msg::Save => {
                let draft =
                    self.editor.as_ref().and_then(|editor| editor.draft(&C::zone(), &self.destinations(editor)));
                match draft {
                    Some(draft) => {
                        let editor = self.editor.take();
                        let tone = editor.as_ref().and_then(Editor::color);
                        let id = editor.and_then(|editor| editor.id);
                        self.paints.choose(id.as_deref(), &meeting::named(&draft.title), draft.start, tone);
                        self.store(draft)
                    }
                    None => Command::none(),
                }
            }
            Msg::Delete => match self.editor.take().and_then(|editor| editor.id) {
                Some(id) if local::owns(&id) => {
                    let removed = self.local.delete(&id);
                    self.refresh();
                    removed
                }
                Some(id) => Command::delete_event(id),
                None => Command::none(),
            },
            Msg::Discard => {
                self.editor = None;
                Command::none()
            }
            Msg::Stored(Ok(())) => Command::none(),
            Msg::Stored(Err(problem)) => Command::notify(format!("Couldn't save your event: {problem}")),
            Msg::Notch(state) => {
                let looking = state.open && state.page.as_deref() == Some(ID);
                let was = self.open && self.on_page;
                let opened = state.open && !self.open;
                self.on_page = state.page.as_deref() == Some(ID);
                self.open = state.open;
                if opened || (looking && !was) { self.check() } else { Command::none() }
            }
        }
    }

    fn hold(&mut self) -> Command<Msg> {
        let wanted = self.editor.is_some() && self.on_page;
        if wanted == self.held {
            return Command::none();
        }
        self.held = wanted;
        if wanted { Command::hold_open() } else { Command::release() }
    }

    fn configure(&mut self, settings: &Settings) -> Command<Msg> {
        let prefs = Prefs::read(settings);
        let switched = prefs.source != self.prefs.source;
        let relinked = self.links.set(&prefs.links);
        self.prefs = prefs;
        if switched {
            self.editor = None;
        }
        self.refresh();
        if relinked || switched { self.load(true) } else { Command::none() }
    }

    fn load(&mut self, remembered: bool) -> Command<Msg> {
        if self.prefs.source != Source::Links {
            return Command::none();
        }
        self.generation += 1;
        self.fetching = self.links.entries.len();
        let generation = self.generation;
        Command::batch(self.links.entries.iter().flat_map(|entry| {
            let cached = remembered.then(|| remember(entry.link.clone(), self.cache.clone(), generation));
            cached.into_iter().chain([download(entry, self.cache.clone(), generation)])
        }))
    }

    fn refresh_every(&self) -> Duration {
        if self.on_page && self.open { WATCHED } else { REFRESH }
    }

    fn check(&mut self) -> Command<Msg> {
        if self.fetching > 0 { Command::none() } else { self.load(false) }
    }

    fn tick(&mut self) -> Command<Msg> {
        let now = C::now();
        let days = Days::around(now, &C::zone());
        if days != self.days {
            self.browse.follow(self.days.today, days.today);
            self.refresh();
        }
        let starting: Vec<Meeting> =
            upcoming::starting(&self.meetings, now, &self.nudged).into_iter().cloned().collect();
        self.nudged.extend(starting.iter().map(|meeting| meeting.id.clone()));
        if !self.prefs.nudge {
            return Command::none();
        }
        Command::batch(starting.iter().map(|meeting| Command::notice(nudge(meeting))))
    }

    fn refresh(&mut self) {
        let zone = C::zone();
        self.days = Days::around(C::now(), &zone);
        let (from, to) = self.reach(&zone);
        let mut groups = match self.prefs.source {
            Source::Apple => vec![self.system.meetings(from, to)],
            Source::Links => self.links.meetings(from, to, &zone),
        };
        groups.push(self.local.meetings(from, to));
        self.meetings = meeting::merge(groups);
        self.paints.paint(&mut self.meetings);
        let present: BTreeSet<&String> = self.meetings.iter().map(|meeting| &meeting.id).collect();
        self.hidden.retain(|id| present.contains(id));
        self.nudged.retain(|id| present.contains(id));
    }

    fn reach(&self, zone: &jiff::tz::TimeZone) -> (u64, u64) {
        let (start, end) = self.browse.range(self.prefs.monday_first(self.clock_24h), zone);
        (start.min(self.days.today_ms), end.max(self.days.after_ms))
    }

    fn phase(&self, now: u64) -> Option<Phase<'_>> {
        upcoming::phase(&self.meetings, now, self.prefs.lead_ms, &self.hidden)
    }

    fn tick_every(&self) -> Duration {
        let now = C::now();
        let reach = self.prefs.lead_ms.max(SOON_MS);
        let close = self
            .meetings
            .iter()
            .any(|meeting| meeting.timed() && meeting.start + SOON_MS > now && meeting.start <= now + reach);
        if close || self.phase(now).is_some() { QUICK } else { CALM }
    }

    fn status(&self) -> Status {
        match (self.prefs.source, &self.system) {
            (Source::Links, _) if self.links.entries.is_empty() && self.prefs.skipped_setup => Status::Ready,
            (Source::Links, _) if self.links.entries.is_empty() => Status::Setup,
            (Source::Links, _) if self.links.waiting() && self.meetings.is_empty() => Status::Loading,
            (Source::Links, _) => Status::Ready,
            (Source::Apple, System::Ready { .. }) => Status::Ready,
            (Source::Apple, System::Denied) => Status::Denied,
            (Source::Apple, System::Unavailable) => Status::Unavailable,
            (Source::Apple, System::Unknown | System::Waiting) => Status::Waiting,
        }
    }

    fn destinations(&self, editor: &Editor) -> Vec<CalendarInfo> {
        let apple = match self.prefs.source {
            Source::Apple => self.system.writable().to_vec(),
            Source::Links => Vec::new(),
        };
        let keeps_local = editor.id.as_deref().is_some_and(local::owns);
        if apple.is_empty() || keeps_local { apple.into_iter().chain([local::calendar()]).collect() } else { apple }
    }

    fn store(&mut self, draft: EventDraft) -> Command<Msg> {
        let was_local = draft.id.as_deref().is_some_and(local::owns);
        let command = match (was_local, draft.calendar_id == local::CALENDAR_ID) {
            (_, true) => {
                let moved = draft.id.clone().filter(|id| !local::owns(id));
                let kept = EventDraft { id: draft.id.clone().filter(|id| local::owns(id)), ..draft };
                self.local.save(&kept, C::now()).and(moved.map_or_else(Command::none, Command::delete_event))
            }
            (true, false) => {
                let id = draft.id.clone().unwrap_or_default();
                Command::save_event(EventDraft { id: None, ..draft }).and(self.local.delete(&id))
            }
            (false, false) => Command::save_event(draft),
        };
        self.refresh();
        command
    }
}

fn remember(link: String, cache: Option<PathBuf>, generation: u64) -> Command<Msg> {
    let key = link.clone();
    Command::perform(
        move || fetch::remembered(&link, cache.as_deref()),
        move |result| Msg::Loaded { link: key, generation, result, cached: true },
    )
}

fn download(entry: &links::Linked, cache: Option<PathBuf>, generation: u64) -> Command<Msg> {
    let (link, tag) = (entry.link.clone(), entry.tag.clone());
    let key = link.clone();
    Command::perform(
        move || fetch::load(&link, cache.as_deref(), &tag),
        move |result| Msg::Fetched { link: key, generation, result },
    )
}

fn nudge(meeting: &Meeting) -> Notice {
    let words: Element<()> =
        column![text(live::shortened(&meeting.title)).callout(), text("Starting now").caption().secondary()]
            .spacing(1.0)
            .into();
    Notice::new(badge(Icon::Calendar, Tone::Blue, BADGE), words).shake()
}

notchbase_sdk::export!(Calendar);

#[cfg(test)]
mod tests;
