//! Roam: one keyboard and mouse for every computer on your desk.
//!
//! Open Roam on each Mac or Windows PC on the same network and they find each other. Drag the
//! nearby ones onto the desk next to your screen, the way they stand in front of you, and the
//! cursor glides from one to the next at the edges. Keys, clicks, scrolling and copied text go
//! with it.

pub mod align;
pub mod bulk;
pub mod crypto;
pub mod display;
pub mod engine;
pub mod input;
pub mod keys;
pub mod layout;
pub mod link;
pub mod priority;
pub mod smooth;
pub mod wire;

use std::marker::PhantomData;
use std::sync::mpsc::Receiver;
use std::time::Duration;

use notchbase_sdk::column;
use notchbase_sdk::prelude::*;
use serde::{Deserialize, Serialize};

use display::Ability;
use engine::{Ask, Control, Engine, ExtendedView, LinkState, PeerView, Placement, Prefs, Status, Transfer};
use layout::{Computer, Rect};
use wire::Os;

pub const ID: &str = "roam";
const OPEN: &str = "open";
const NAME: &str = "name";
const SMOOTHING: &str = "smoothing";
const CLIPBOARD: &str = "clipboard";
const FAST_WIFI: &str = "fast_wifi";
const ROAM: &str = "roam";
const MODIFIERS: &str = "modifiers";
const CORNERS: &str = "corners";
const ADDRESSES: &str = "addresses";
const DESK: &str = "desk";

const WIDTH: f32 = 340.0;
const HEIGHT: f32 = 200.0;
const PAD: f32 = 14.0;
const SNAP: f32 = 10.0;
const GLYPH: f32 = 14.0;
const RING: f32 = 13.0;
/// How many arrangements back Undo goes.
const UNDO_DEPTH: usize = 30;
const ASK_FOR: Duration = Duration::from_secs(30);
const SCROLL_MIN: f32 = 0.25;
const SCROLL_MAX: f32 = 4.0;
const SCROLL_STEP: f32 = 0.25;
const HOME_KEYS: &str = if cfg!(target_os = "macos") { "Control Option Shift H" } else { "Ctrl Alt Shift H" };

/// A computer on the desk, remembered so it's drawn even while it's off.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Spot {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub os: String,
    pub displays: Vec<Rect>,
    pub ppu: f64,
    pub at: [f64; 2],
    /// Scroll multiplier for this computer, when it's the one being used from here.
    #[serde(default = "same_speed", skip_serializing_if = "is_same_speed")]
    pub scroll: f64,
}

fn same_speed() -> f64 {
    1.0
}

#[allow(clippy::trivially_copy_pass_by_ref)]
fn is_same_speed(scroll: &f64) -> bool {
    (*scroll - 1.0).abs() < f64::EPSILON
}

impl Spot {
    fn computer(&self) -> Computer {
        Computer::new(self.id.clone(), self.displays.clone(), self.ppu, self.at)
    }

    fn from_view(peer: &PeerView, at: [f64; 2]) -> Self {
        Spot {
            id: peer.id.clone(),
            name: peer.name.clone(),
            os: peer.os.word().into(),
            displays: if peer.displays.is_empty() {
                vec![Rect::new(0.0, 0.0, 1440.0, 900.0)]
            } else {
                peer.displays.clone()
            },
            ppu: peer.ppu,
            at,
            scroll: 1.0,
        }
    }
}

/// How the desk fits the canvas: canvas = (desk - origin) * scale + offset.
#[derive(Clone, Copy, Debug, PartialEq)]
struct View {
    scale: f64,
    origin: [f64; 2],
    offset: [f64; 2],
}

impl View {
    fn fit(spots: &[Spot]) -> Self {
        let rects: Vec<Rect> = spots.iter().map(|s| s.computer().rect()).collect();
        let bounds = layout::bounds(&rects);
        let room = [f64::from(WIDTH - PAD * 2.0), f64::from(HEIGHT - PAD * 2.0)];
        let scale = (room[0] / bounds.w).min(room[1] / bounds.h).min(0.2);
        let used = [bounds.w * scale, bounds.h * scale];
        let offset = [(f64::from(WIDTH) - used[0]) / 2.0, (f64::from(HEIGHT) - used[1]) / 2.0];
        View { scale, origin: [bounds.x, bounds.y], offset }
    }

    fn to_canvas(self, [x, y]: [f64; 2]) -> [f32; 2] {
        [
            ((x - self.origin[0]) * self.scale + self.offset[0]) as f32,
            ((y - self.origin[1]) * self.scale + self.offset[1]) as f32,
        ]
    }

    fn to_desk(self, x: f32, y: f32) -> [f64; 2] {
        [
            (f64::from(x) - self.offset[0]) / self.scale + self.origin[0],
            (f64::from(y) - self.offset[1]) / self.scale + self.origin[1],
        ]
    }
}

#[derive(Clone, Debug, PartialEq)]
struct Drag {
    index: usize,
    grab: [f64; 2],
    view: View,
    moved: bool,
    /// The desk before the drag, for Undo.
    before: Vec<Spot>,
}

/// What starts the engine: the real one, or none in tests.
pub trait Backend: 'static {
    fn start(context: &Context, prefs: Prefs, desk: Vec<Placement>) -> Option<(Engine, Receiver<Status>)>;
}

pub struct Native;

impl Backend for Native {
    fn start(context: &Context, prefs: Prefs, desk: Vec<Placement>) -> Option<(Engine, Receiver<Status>)> {
        Some(Engine::start(context.data_dir(), input::native(), prefs, desk, engine::PORT))
    }
}

pub struct Roam<B: Backend = Native> {
    engine: Option<Engine>,
    status: Status,
    prefs: Prefs,
    desk: Vec<Spot>,
    drag: Option<Drag>,
    selected: Option<String>,
    asked: Vec<String>,
    draft: String,
    /// The screen right-clicked on the desk: whose, and which of its displays.
    menu: Option<(String, u8)>,
    /// The computer under the pointer on the desk.
    hover: Option<String>,
    /// Earlier arrangements, newest last, for Undo.
    history: Vec<Vec<Spot>>,
    /// The computer last nudged, so a run of nudges is one step of Undo.
    nudged: Option<String>,
    backend: PhantomData<B>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Msg {
    Status(Box<Status>),
    Configured(Settings),
    Pointer(Pointer),
    Add(String),
    Remove(String),
    Allow(String),
    Decline(String),
    Retry(String),
    Forget(String),
    Select(Option<String>),
    Home,
    Release,
    Permission,
    Open(bool),
    Draft(String),
    Connect,
    Unlist(String),
    Scroll(String, f32),
    /// A right click on the desk.
    Menu(Pointer),
    /// Make that computer's display an extended display of this one.
    Extend(String, u8),
    /// Make every one of that computer's screens not already one an extended display.
    ExtendAll(String),
    /// Give that computer's screen back (`None`: every one of its screens).
    Unextend(String, Option<u8>),
    /// Take this computer's screens back from the computer using them as displays.
    StopLending,
    /// Put the desk back the way it was before the last change.
    Undo,
    /// Put line-up guides over every screen on the desk, or take them down.
    LineUp(bool),
    /// Move a computer on the desk by this many points.
    Nudge(String, [f64; 2]),
    /// Line a computer's screen up with the one it sits against: their starts, middles or ends.
    Level(String, Level),
    /// Take down the line-up guides another computer put on this one's screens.
    HideGuides,
}

/// Which edges of two screens side by side (or one above the other) to line up.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Level {
    /// Tops, or left edges.
    Start,
    Middle,
    /// Bottoms, or right edges.
    End,
}

/// The screen of another computer that a computer's screen sits against, the longest such edge.
#[derive(Clone, Debug, PartialEq)]
struct Facing {
    name: String,
    /// Both screens on the desk: the computer's, and the one it sits against.
    mine: Rect,
    theirs: Rect,
    /// Side by side (so they line up up and down), rather than one above the other.
    beside: bool,
}

impl Facing {
    /// How far the computer's screen is past the other's along their shared edge, at their
    /// starts: down, or right.
    fn offset(&self) -> f64 {
        if self.beside { self.mine.y - self.theirs.y } else { self.mine.x - self.theirs.x }
    }

    /// The move that lines them up at `level`.
    fn move_for(&self, level: Level) -> [f64; 2] {
        let (mine, theirs) = if self.beside {
            ((self.mine.y, self.mine.h), (self.theirs.y, self.theirs.h))
        } else {
            ((self.mine.x, self.mine.w), (self.theirs.x, self.theirs.w))
        };
        let by = match level {
            Level::Start => theirs.0 - mine.0,
            Level::Middle => (theirs.0 + theirs.1 / 2.0) - (mine.0 + mine.1 / 2.0),
            Level::End => (theirs.0 + theirs.1) - (mine.0 + mine.1),
        };
        if self.beside { [0.0, by] } else { [by, 0.0] }
    }
}

fn prefs(settings: &Settings) -> Prefs {
    let addresses = settings
        .text(ADDRESSES)
        .split([',', ';', ' ', '\n'])
        .map(str::trim)
        .filter(|a| !a.is_empty())
        .map(str::to_owned)
        .collect();
    Prefs {
        open: settings.bool(OPEN),
        name: settings.text(NAME).trim().to_owned(),
        smoothing: settings.bool(SMOOTHING),
        corners: settings.bool(CORNERS),
        clipboard: settings.bool(CLIPBOARD),
        fast_wifi: settings.bool(FAST_WIFI),
        roam: settings.bool(ROAM),
        modifiers: match settings.text(MODIFIERS) {
            "position" => keys::Modifiers::Position,
            "shortcuts" => keys::Modifiers::Shortcuts,
            _ => keys::Modifiers::Same,
        },
        addresses,
    }
}

impl<B: Backend> Tool for Roam<B> {
    type Msg = Msg;

    fn manifest() -> Manifest {
        Manifest::new(ID, "Roam")
            .description("Use one keyboard and mouse across your Mac and Windows computers")
            .version(env!("CARGO_PKG_VERSION"))
            .page("Roam")
            .page_icon(Icon::Mouse)
            .setting(
                SettingSpec::toggle(OPEN, "Let nearby computers use this one", true)
                    .detail("Shows this computer to others on the network so they can share their keyboard and mouse"),
            )
            .setting(
                SettingSpec::toggle(ROAM, "Move the mouse and keyboard to other computers", true).detail(
                    "Turn off to keep the mouse and keyboard on this computer while extended displays stay on",
                ),
            )
            .setting(SettingSpec::text(NAME, "Name shown to other computers", "").placeholder("This computer's name"))
            .setting(
                SettingSpec::toggle(CLIPBOARD, "Share the clipboard", true)
                    .detail("Copied text, images and files go along with the cursor"),
            )
            .setting(
                SettingSpec::choice(
                    MODIFIERS,
                    // Each label names the PC key that is ⌘ on a Mac: the switch has 58 points a part.
                    "PC key that is ⌘ on a Mac",
                    [("same", "Win ⌘"), ("shortcuts", "Ctrl ⌘"), ("position", "Alt ⌘")],
                    "same",
                )
                .detail("Win ⌘ matches Deskflow"),
            )
            .setting(SettingSpec::toggle(FAST_WIFI, "Fast Wi-Fi", true).detail(
                "On a Mac, pauses AirDrop and Handoff while connected, since they take the Wi-Fi away twice a second. Asks for your password once",
            ))
            .setting(
                SettingSpec::toggle(SMOOTHING, "Smooth over Wi-Fi hiccups", true)
                    .detail("When Wi-Fi goes quiet for a moment, the cursor keeps gliding instead of freezing"),
            )
            .setting(
                SettingSpec::toggle(CORNERS, "Keep Mac screen corners for hot corners", true)
                    .detail("The cursor doesn't cross to another computer in the corners of a Mac's screen"),
            )
            .setting(
                SettingSpec::text(ADDRESSES, "Addresses of other computers", "")
                    .placeholder("192.168.1.20, desk-pc.local")
                    .detail("Only needed on networks that keep computers from finding each other"),
            )
            .setting(SettingSpec::data(DESK, Value::Array(Vec::new())))
    }

    fn init(context: &Context) -> (Self, Command<Msg>) {
        let settings = context.settings();
        let desk: Vec<Spot> = settings.parse(DESK).unwrap_or_default();
        let prefs = prefs(settings);
        let placements = placements(&desk);
        let started = B::start(context, prefs.clone(), placements);
        let (engine, pump) = match started {
            Some((engine, updates)) => (
                Some(engine),
                Command::spawn(move |emitter| {
                    while let Ok(status) = updates.recv() {
                        emitter.send(Msg::Status(Box::new(status)));
                    }
                }),
            ),
            None => (None, Command::none()),
        };
        let roam = Roam {
            engine,
            status: Status::default(),
            prefs,
            desk,
            drag: None,
            selected: None,
            asked: Vec::new(),
            draft: String::new(),
            menu: None,
            hover: None,
            history: Vec::new(),
            nudged: None,
            backend: PhantomData,
        };
        (roam, pump)
    }

    fn update(&mut self, msg: Msg) -> Command<Msg> {
        match msg {
            Msg::Status(status) => self.status_changed(*status),
            Msg::Configured(settings) => {
                let next = prefs(&settings);
                if next != self.prefs {
                    self.prefs = next.clone();
                    self.control(Control::Prefs(next));
                }
                let desk: Vec<Spot> = settings.parse(DESK).unwrap_or_default();
                if self.drag.is_none() && desk != self.desk {
                    self.desk = desk;
                    self.control(Control::Desk(placements(&self.desk)));
                }
                Command::none()
            }
            Msg::Pointer(pointer) => self.pointer(pointer),
            Msg::Add(id) => {
                let Some(peer) = self.status.peers.iter().find(|p| p.id == id).cloned() else { return Command::none() };
                self.ensure_me();
                let computers: Vec<Computer> = self.desk.iter().map(Spot::computer).collect();
                let spot = Spot::from_view(&peer, layout::free_spot(&computers));
                self.remember();
                self.selected = Some(spot.id.clone());
                self.desk.push(spot);
                self.desk_changed()
            }
            Msg::Remove(id) => {
                self.remember();
                self.desk.retain(|s| s.id != id);
                if self.selected.as_deref() == Some(&id) {
                    self.selected = None;
                }
                self.desk_changed()
            }
            Msg::Allow(id) => {
                self.control(Control::Allow(id));
                Command::dismiss_notice()
            }
            Msg::Decline(id) => {
                self.control(Control::Decline(id));
                Command::dismiss_notice()
            }
            Msg::Retry(id) => {
                self.control(Control::Retry(id));
                Command::none()
            }
            Msg::Forget(id) => {
                self.control(Control::Forget(id.clone()));
                self.remember();
                self.desk.retain(|s| s.id != id);
                self.selected = None;
                self.desk_changed()
            }
            Msg::Select(id) => {
                if id != self.selected {
                    self.menu = None;
                }
                self.selected = id;
                Command::none()
            }
            Msg::Home => {
                self.control(Control::Home);
                Command::none()
            }
            Msg::Release => {
                self.control(Control::Release);
                Command::none()
            }
            Msg::Permission => {
                self.control(Control::AskPermission);
                Command::none()
            }
            Msg::Open(open) => Command::save_setting(OPEN, open),
            Msg::Draft(draft) => {
                self.draft = draft;
                Command::none()
            }
            Msg::Connect => {
                let address = self.draft.trim().to_owned();
                self.draft.clear();
                if address.is_empty() || self.prefs.addresses.contains(&address) {
                    return Command::none();
                }
                let mut addresses = self.prefs.addresses.clone();
                addresses.push(address);
                self.set_addresses(addresses)
            }
            Msg::Unlist(address) => {
                let addresses = self.prefs.addresses.iter().filter(|a| **a != address).cloned().collect();
                self.set_addresses(addresses)
            }
            Msg::Scroll(id, speed) => {
                let speed = f64::from((speed / SCROLL_STEP).round() * SCROLL_STEP)
                    .clamp(f64::from(SCROLL_MIN), f64::from(SCROLL_MAX));
                let Some(spot) = self.desk.iter_mut().find(|s| s.id == id) else { return Command::none() };
                if (spot.scroll - speed).abs() < f64::EPSILON {
                    return Command::none();
                }
                spot.scroll = speed;
                self.desk_changed()
            }
            Msg::Menu(pointer) => {
                self.menu = self.screen_at(pointer);
                if let Some((id, _)) = &self.menu {
                    self.selected = Some(id.clone());
                }
                Command::none()
            }
            Msg::Extend(id, display) => {
                self.menu = None;
                self.control(Control::Extend { id, display });
                Command::none()
            }
            Msg::ExtendAll(id) => {
                self.menu = None;
                let count = self.desk.iter().find(|s| s.id == id).map_or(0, |s| s.displays.len());
                for display in 0..count {
                    let display = u8::try_from(display).unwrap_or(u8::MAX);
                    if self.extended(&id, usize::from(display)).is_none_or(|e| e.failed.is_some()) {
                        self.control(Control::Extend { id: id.clone(), display });
                    }
                }
                Command::none()
            }
            Msg::Unextend(id, display) => {
                self.menu = None;
                self.control(Control::Unextend { id, display });
                Command::none()
            }
            Msg::StopLending => {
                self.control(Control::StopLending);
                Command::none()
            }
            Msg::LineUp(on) => {
                self.control(Control::LineUp(on));
                Command::none()
            }
            Msg::Nudge(id, by) => self.nudge(&id, by),
            Msg::HideGuides => {
                self.control(Control::HideGuides);
                Command::none()
            }
            Msg::Level(id, level) => {
                let Some(facing) = self.facing(&id) else { return Command::none() };
                self.nudged = None;
                self.nudge(&id, facing.move_for(level))
            }
            Msg::Undo => {
                self.nudged = None;
                let Some(before) = self.history.pop() else { return Command::none() };
                self.desk = before;
                if self.selected.as_ref().is_some_and(|id| !self.desk.iter().any(|s| &s.id == id)) {
                    self.selected = None;
                }
                self.desk_changed()
            }
        }
    }

    fn subscriptions(&self) -> Subscription<Msg> {
        Subscription::settings(Msg::Configured)
    }

    fn tile(&self) -> Option<Tile<Msg>> {
        let tile = Tile::new("Roam").icon(Icon::Mouse);
        let words = self.summary();
        let tile = if self.status.away_on.is_some() || self.status.controller.is_some() {
            tile.symbol(icon(Icon::Mouse).tone(Tone::White)).well(Tone::Blue).tint(Tone::Blue)
        } else {
            tile
        };
        Some(tile.footer(text(words).subheadline().secondary()))
    }

    fn page(&self) -> Option<Element<Msg>> {
        let mut parts: Vec<Element<Msg>> = Vec::new();
        if let Some(problem) = &self.status.problem {
            parts.push(banner(Icon::Exclamation, Tone::Orange, problem.clone(), None, ("Allow", Msg::Permission)));
        }
        for ask in &self.status.asks {
            parts.push(ask_card(ask));
        }
        if let Some(transfer) = &self.status.transfer {
            parts.push(transfer_view(transfer));
        }
        if let Some(name) = &self.status.away_on {
            let detail = if self.status.locked {
                format!("Locked there · {HOME_KEYS} brings it back")
            } else {
                format!("{HOME_KEYS} brings it back")
            };
            parts.push(banner(Icon::Mouse, Tone::Blue, format!("On {name}"), Some(detail), ("Bring back", Msg::Home)));
        }
        if let Some(name) = &self.status.controller {
            parts.push(banner(
                Icon::Keyboard,
                Tone::Green,
                format!("Using {name}'s keyboard and mouse"),
                None,
                ("Stop", Msg::Release),
            ));
        }
        parts.extend(extended_banners(&self.status.extended));
        if let Some(first) = self.status.lending.first() {
            let me = self.me().map_or(1, |m| m.displays.len());
            let numbers: Vec<String> =
                self.status.lending.iter().map(|l| (u32::from(l.display) + 1).to_string()).collect();
            let (which, displays, them) = match numbers.len() {
                1 if me > 1 => (format!("screen {}", numbers[0]), "a display", "it"),
                1 => ("screen".to_owned(), "a display", "it"),
                _ if numbers.len() >= me => ("screens".to_owned(), "displays", "them"),
                _ => (format!("screens {}", and_list(&numbers)), "displays", "them"),
            };
            parts.push(banner(
                Icon::Display,
                Tone::Green,
                format!("{} is using this computer's {which} as {displays}", first.to),
                Some(format!("Your mouse and keyboard work on {them} as usual")),
                ("Stop", Msg::StopLending),
            ));
        }
        if let Some(name) = &self.status.guided_by {
            parts.push(banner(
                Icon::Viewfinder,
                Tone::Blue,
                format!("{name} is lining up the screens"),
                Some("Its guides are on this computer's screens until it's done there".into()),
                ("Hide", Msg::HideGuides),
            ));
        }
        if !self.prefs.open && self.status.controller.is_none() {
            parts.push(banner(
                Icon::Lock,
                Tone::Secondary,
                "Hidden from other computers".into(),
                Some("They can't find or use this computer".into()),
                ("Show", Msg::Open(true)),
            ));
        }
        parts.push(self.desk_view());
        if let Some(details) = self.selected_details() {
            parts.push(details);
        }
        parts.push(self.nearby());
        parts.push(self.by_address());
        Some(column(parts).spacing(10.0).into())
    }

    fn live(&self) -> Option<Live<Msg>> {
        if let Some(transfer) = &self.status.transfer {
            let live = Live::new(
                ring(transfer.share()).size(RING).tone(Tone::Blue),
                text(format!("{} · {}", transfer.what, percent(transfer))).callout().secondary(),
            );
            return Some(if self.status.away_on.is_some() {
                live.controls(button("").icon(Icon::Xmark).on_press(Msg::Home))
            } else {
                live
            });
        }
        if let Some(name) = &self.status.away_on {
            let live =
                Live::new(icon(Icon::Mouse).size(GLYPH).tone(Tone::Blue), text(name.clone()).callout().secondary());
            return Some(live.controls(button("").icon(Icon::Xmark).on_press(Msg::Home)));
        }
        if self.status.here
            && let Some(name) = &self.status.controller
        {
            let live =
                Live::new(icon(Icon::Keyboard).size(GLYPH).tone(Tone::Green), text(name.clone()).callout().secondary());
            return Some(live);
        }
        None
    }
}

fn placements(desk: &[Spot]) -> Vec<Placement> {
    desk.iter().map(|s| Placement { id: s.id.clone(), at: s.at, scroll: s.scroll }).collect()
}

fn ask_card(ask: &Ask) -> Element<Msg> {
    let (title, yes, no) = if ask.controlling {
        (format!("Check that {} shows this code", ask.name), "It matches", "Cancel")
    } else {
        (format!("{} wants to use this computer's keyboard and mouse", ask.name), "Allow", "Decline")
    };
    card(
        column![
            text(title).callout().wrap(),
            text(ask.code.clone()).display(),
            row![
                button(no).on_press(Msg::Decline(ask.id.clone())),
                spacer(),
                button(yes).filled().on_press(Msg::Allow(ask.id.clone())),
            ]
            .spacing(8.0),
        ]
        .spacing(6.0),
    )
}

/// A copy on its way, for the page: what it is, who with, and how far along.
fn transfer_view(transfer: &Transfer) -> Element<Msg> {
    let title = if transfer.incoming {
        format!("Getting {} from {}", transfer.what, transfer.with)
    } else {
        format!("Sending {} to {}", transfer.what, transfer.with)
    };
    card(
        column![
            row![
                icon(Icon::Download).tone(Tone::Blue),
                text(title).callout().wrap(),
                spacer(),
                text(format!("{} of {}", size_words(transfer.done), size_words(transfer.total))).caption().secondary(),
            ]
            .spacing(8.0)
            .align(Align::Center),
            progress(transfer.share()).tone(Tone::Blue),
        ]
        .spacing(6.0),
    )
}

/// The screens on their way to being, or being, extended displays of this computer: one card
/// each, but one for all the screens of a computer that are up.
fn extended_banners(extended: &[ExtendedView]) -> Vec<Element<Msg>> {
    let mut parts = Vec::new();
    let mut done: Vec<&str> = Vec::new();
    for view in extended {
        if view.failed.is_some() || view.working.is_some() {
            parts.push(extended_view(view));
            continue;
        }
        if done.contains(&view.id.as_str()) {
            continue;
        }
        done.push(&view.id);
        let up: Vec<&ExtendedView> = extended.iter().filter(|e| e.id == view.id && e.running).collect();
        if up.len() < 2 {
            parts.push(extended_view(view));
            continue;
        }
        let numbers: Vec<String> = up.iter().map(|e| (u32::from(e.display) + 1).to_string()).collect();
        parts.push(banner(
            Icon::Display,
            Tone::Blue,
            format!("{}'s screens {} are displays of this computer", view.name, and_list(&numbers)),
            Some("Arrange them in Display settings like any monitor".into()),
            ("Disconnect all", Msg::Unextend(view.id.clone(), None)),
        ));
    }
    parts
}

/// "1", "1 and 2", "1, 2 and 3".
fn and_list(items: &[String]) -> String {
    match items {
        [] => String::new(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

/// A screen on its way to being, or being, an extended display of this computer.
fn extended_view(extended: &ExtendedView) -> Element<Msg> {
    let (id, display) = (extended.id.clone(), Some(extended.display));
    let screen = &extended.screen;
    if let Some(why) = &extended.failed {
        return card(
            row![
                icon(Icon::Exclamation).tone(Tone::Orange),
                column![
                    text(format!("{screen} didn't become a display")).callout().wrap(),
                    text(why.clone()).caption().secondary().wrap(),
                ]
                .spacing(2.0),
                spacer(),
                button("OK").on_press(Msg::Unextend(id, display)),
            ]
            .spacing(8.0)
            .align(Align::Center),
        );
    }
    let (title, detail, action) = match &extended.working {
        Some(working) => (format!("Making {screen} a display"), format!("{working}…"), "Cancel"),
        None => (
            format!("{screen} is a display of this computer"),
            "Arrange it in Display settings like any monitor".to_owned(),
            "Disconnect",
        ),
    };
    banner(Icon::Display, Tone::Blue, title, Some(detail), (action, Msg::Unextend(id, display)))
}

/// A card at the top of the page for what's going on now: an icon, what it is, a word more,
/// and the one thing to do about it.
fn banner(glyph: Icon, tone: Tone, title: String, detail: Option<String>, (label, msg): (&str, Msg)) -> Element<Msg> {
    let mut words: Vec<Element<Msg>> = vec![text(title).callout().wrap().into()];
    if let Some(detail) = detail {
        words.push(text(detail).caption().secondary().wrap().into());
    }
    card(
        row![badge(glyph, tone, 28.0), column(words).spacing(2.0), spacer(), button(label).on_press(msg)]
            .spacing(10.0)
            .align(Align::Center),
    )
}

fn percent(transfer: &Transfer) -> String {
    format!("{}%", (transfer.share() * 100.0).floor() as u32)
}

/// A size the way people read it: 840 KB, 12.4 MB, 3.1 GB.
fn size_words(bytes: u64) -> String {
    const KB: f64 = 1000.0;
    let bytes = bytes as f64;
    if bytes < KB * KB {
        format!("{} KB", (bytes / KB).ceil() as u64)
    } else if bytes < KB * KB * KB {
        format!("{:.1} MB", bytes / (KB * KB))
    } else {
        format!("{:.2} GB", bytes / (KB * KB * KB))
    }
}

fn state_words(peer: Option<&PeerView>) -> String {
    let Some(peer) = peer else { return "Not around".into() };
    if !peer.nearby {
        return "Not around".into();
    }
    match peer.link {
        LinkState::Connected => match peer.rtt {
            Some(rtt) if rtt < 1.0 => "Connected · under 1 ms".into(),
            Some(rtt) => format!("Connected · {} ms", rtt.round() as u32),
            None => "Connected".into(),
        },
        LinkState::Pairing => "Checking the code".into(),
        LinkState::Connecting => "Connecting".into(),
        LinkState::Declined => "Said no".into(),
        LinkState::Busy => "Used by another computer".into(),
        LinkState::Closed => "Not sharing".into(),
        LinkState::None if !peer.open => "Not sharing".into(),
        LinkState::None => "Waiting".into(),
    }
}

impl<B: Backend> Roam<B> {
    fn control(&self, control: Control) {
        if let Some(engine) = &self.engine {
            engine.send(control);
        }
    }

    fn set_addresses(&mut self, addresses: Vec<String>) -> Command<Msg> {
        self.prefs.addresses = addresses;
        self.control(Control::Prefs(self.prefs.clone()));
        Command::save_setting(ADDRESSES, self.prefs.addresses.join(", "))
    }

    fn me(&self) -> Option<&PeerView> {
        self.status.me.as_ref()
    }

    fn peer(&self, id: &str) -> Option<&PeerView> {
        self.status.peers.iter().find(|p| p.id == id)
    }

    fn summary(&self) -> String {
        if let Some(transfer) = &self.status.transfer {
            let verb = if transfer.incoming { "Getting" } else { "Sending" };
            return format!("{verb} {} · {}", transfer.what, percent(transfer));
        }
        if let Some(name) = &self.status.away_on {
            return format!("On {name}");
        }
        if let Some(name) = &self.status.controller {
            return format!("Used by {name}");
        }
        let connected = self.status.peers.iter().filter(|p| p.link == LinkState::Connected).count();
        match connected {
            0 if !self.prefs.open => "Hidden".into(),
            0 => "No computers yet".into(),
            1 => "1 computer".into(),
            n => format!("{n} computers"),
        }
    }

    /// Puts this computer on the desk if it isn't there, and keeps its screens up to date.
    fn ensure_me(&mut self) -> bool {
        let Some(me) = self.me().cloned() else { return false };
        match self.desk.iter_mut().find(|s| s.id == me.id) {
            Some(spot) => refresh(spot, &me),
            None => {
                self.desk.insert(0, Spot::from_view(&me, [0.0, 0.0]));
                true
            }
        }
    }

    fn status_changed(&mut self, status: Status) -> Command<Msg> {
        let new_asks: Vec<Ask> =
            status.asks.iter().filter(|ask| !ask.controlling && !self.asked.contains(&ask.id)).cloned().collect();
        self.asked = status.asks.iter().map(|a| a.id.clone()).collect();
        let landed = match (&status.landed, &self.status.landed) {
            (Some(new), Some(old)) if new.serial == old.serial => None,
            (new, _) => new.as_ref().map(|l| l.text.clone()),
        };
        self.status = status;
        let mut changed = !self.desk.is_empty() && self.ensure_me();
        let peers = self.status.peers.clone();
        for peer in &peers {
            if let Some(spot) = self.desk.iter_mut().find(|s| s.id == peer.id) {
                changed |= refresh(spot, peer);
            }
        }
        let mut command = if changed && self.drag.is_none() { self.desk_changed() } else { Command::none() };
        for ask in new_asks {
            let notice = Notice::new(
                icon(Icon::Keyboard).size(GLYPH).tone(Tone::Blue),
                text(format!("{} wants to use this computer · {}", ask.name, ask.code)).callout(),
            )
            .duration(ASK_FOR)
            .size(NoticeSize::Medium);
            let controls = row![
                button("Decline").on_press(Msg::Decline(ask.id.clone())),
                button("Allow").filled().on_press(Msg::Allow(ask.id.clone())),
            ]
            .spacing(6.0);
            command = command.and(Command::notice_with_controls(notice, controls));
        }
        if let Some(text) = landed {
            command = command.and(Command::notify(text));
        }
        command
    }

    fn desk_changed(&mut self) -> Command<Msg> {
        self.control(Control::Desk(placements(&self.desk)));
        let value = serde_json::to_value(&self.desk).unwrap_or(Value::Array(Vec::new()));
        Command::save_setting(DESK, value)
    }

    fn view(&self) -> View {
        self.drag.as_ref().map_or_else(|| View::fit(&self.desk), |drag| drag.view)
    }

    /// Keeps the desk as it is now, for Undo.
    fn remember(&mut self) {
        self.nudged = None;
        self.history.push(self.desk.clone());
        if self.history.len() > UNDO_DEPTH {
            self.history.remove(0);
        }
    }

    fn pointer(&mut self, pointer: Pointer) -> Command<Msg> {
        match pointer.kind {
            PointerKind::Press => {
                let view = self.view();
                let at = view.to_desk(pointer.x, pointer.y);
                // The top-most spot under the pointer is the last drawn.
                let hit = self.computer_at(at).and_then(|id| self.desk.iter().position(|s| s.id == id));
                if let Some(index) = hit {
                    let grab = [at[0] - self.desk[index].at[0], at[1] - self.desk[index].at[1]];
                    let before = self.desk.clone();
                    self.drag = Some(Drag { index, grab, view, moved: false, before });
                    self.selected = Some(self.desk[index].id.clone());
                    self.menu = None;
                }
                Command::none()
            }
            PointerKind::Drag => {
                let Some(drag) = self.drag.as_mut() else { return Command::none() };
                let at = drag.view.to_desk(pointer.x, pointer.y);
                let wanted = [at[0] - drag.grab[0], at[1] - drag.grab[1]];
                let reach = f64::from(SNAP) / drag.view.scale;
                let index = drag.index;
                drag.moved = true;
                let computers: Vec<Computer> = self.desk.iter().map(Spot::computer).collect();
                let snapped = layout::snap(&computers, index, wanted, reach);
                if let Some(spot) = self.desk.get_mut(index) {
                    spot.at = snapped;
                }
                Command::none()
            }
            PointerKind::Release | PointerKind::Leave => {
                if pointer.kind == PointerKind::Leave {
                    self.hover = None;
                }
                match self.drag.take() {
                    Some(drag) if drag.moved => self.moved(drag),
                    _ => Command::none(),
                }
            }
            PointerKind::Hover => {
                let at = self.view().to_desk(pointer.x, pointer.y);
                self.hover = self.computer_at(at);
                Command::none()
            }
            PointerKind::Click => {
                if let Some(drag) = self.drag.take().filter(|d| d.moved) {
                    return self.moved(drag);
                }
                let at = self.view().to_desk(pointer.x, pointer.y);
                let hit = self.computer_at(at);
                if hit != self.selected {
                    self.menu = None;
                }
                self.selected = hit;
                Command::none()
            }
            _ => Command::none(),
        }
    }

    /// A drag that moved a computer: keep the desk from before it for Undo, and save.
    fn moved(&mut self, drag: Drag) -> Command<Msg> {
        if drag.before == self.desk {
            return Command::none();
        }
        self.nudged = None;
        self.history.push(drag.before);
        if self.history.len() > UNDO_DEPTH {
            self.history.remove(0);
        }
        self.desk_changed()
    }

    /// Moves computer `id` by `by` points: a run of nudges to the same one is one step of Undo.
    fn nudge(&mut self, id: &str, by: [f64; 2]) -> Command<Msg> {
        if by.iter().all(|v| v.abs() < 1e-9) || !self.desk.iter().any(|s| s.id == id) {
            return Command::none();
        }
        if self.nudged.as_deref() != Some(id) {
            self.remember();
            self.nudged = Some(id.to_owned());
        }
        if let Some(spot) = self.desk.iter_mut().find(|s| s.id == id) {
            spot.at = [spot.at[0] + by[0], spot.at[1] + by[1]];
        }
        self.desk_changed()
    }

    /// The screen of another computer that computer `id` sits against: of the pairs of their
    /// screens that touch, the one sharing the longest edge.
    fn facing(&self, id: &str) -> Option<Facing> {
        const TOUCH: f64 = 1.0;
        let spot = self.desk.iter().find(|s| s.id == id)?;
        let mine = spot.computer().desk_displays();
        let mut best: Option<(f64, Facing)> = None;
        for other in self.desk.iter().filter(|s| s.id != id) {
            for theirs in other.computer().desk_displays() {
                for a in &mine {
                    let beside = (a.right() - theirs.x).abs() < TOUCH || (theirs.right() - a.x).abs() < TOUCH;
                    let stacked = (a.bottom() - theirs.y).abs() < TOUCH || (theirs.bottom() - a.y).abs() < TOUCH;
                    for (touches, beside) in [(beside, true), (stacked, false)] {
                        if !touches {
                            continue;
                        }
                        let shared = if beside {
                            a.bottom().min(theirs.bottom()) - a.y.max(theirs.y)
                        } else {
                            a.right().min(theirs.right()) - a.x.max(theirs.x)
                        };
                        if shared > 0.0 && best.as_ref().is_none_or(|(most, _)| shared > *most) {
                            let facing = Facing { name: other.name.clone(), mine: *a, theirs, beside };
                            best = Some((shared, facing));
                        }
                    }
                }
            }
        }
        best.map(|(_, facing)| facing)
    }

    /// The colour a screen has in the line-up guides, while they're up.
    fn guide_colour(&self, id: &str, display: usize) -> Option<[u8; 3]> {
        if !self.status.lining_up {
            return None;
        }
        let found = self.status.coloured.iter().find(|c| c.id == id && usize::from(c.display) == display)?;
        Some(align::colour(found.colour))
    }

    /// The computer with a screen at `at` on the desk, the top-most one.
    fn computer_at(&self, at: [f64; 2]) -> Option<String> {
        self.desk
            .iter()
            .rev()
            .find(|s| s.computer().desk_displays().iter().any(|d| d.contains(at)))
            .map(|s| s.id.clone())
    }

    fn desk_view(&self) -> Element<Msg> {
        if self.desk.len() < 2 {
            let me = self.me().map_or_else(|| "This computer".into(), |m| m.name.clone());
            return card(
                column![
                    row![icon(Icon::Display).tone(Tone::Blue), text(me).headline()].spacing(8.0).align(Align::Center),
                    text("Add a computer below, then drag it to where it sits next to this one.")
                        .callout()
                        .secondary()
                        .wrap(),
                ]
                .spacing(6.0),
            );
        }
        let view = self.view();
        let me = self.me().map(|m| m.id.clone()).unwrap_or_default();
        let mut canvas = canvas::<Msg>(WIDTH, HEIGHT).stretch().on_pointer(Msg::Pointer).on_menu(Msg::Menu);
        // The desk itself, so the screens sit on something.
        canvas = canvas.push(
            draw::rect(0.5, 0.5, WIDTH - 1.0, HEIGHT - 1.0)
                .radius(12.0)
                .fill(Color::white(0.03))
                .stroke(1.0, Color::white(0.06)),
        );
        let dragging = self.drag.as_ref().filter(|d| d.moved).map(|d| d.index);
        for (index, spot) in self.desk.iter().enumerate() {
            canvas = canvas.extend(self.spot_shapes(spot, view, &me, dragging == Some(index)));
        }
        canvas = canvas.extend(self.crossings(view));
        if let Some(index) = dragging {
            canvas = canvas.extend(guides(&self.desk, index, view));
        }
        let hint = if self.drag.as_ref().is_some_and(|d| d.moved) {
            "Lines show where edges line up"
        } else if self.status.lining_up {
            "Every screen shows its colour · the bands are where the cursor crosses"
        } else if matches!(self.status.ability, Ability::No(_)) {
            "Drag the screens to match your desk"
        } else {
            "Drag the screens to match your desk · right-click one to use it as a display"
        };
        let mut footer: Vec<Element<Msg>> = vec![text(hint).caption().tertiary().wrap().into(), spacer()];
        if !self.history.is_empty() {
            footer.push(
                row![icon(Icon::Checkmark).size(10.0).tone(Tone::Green), text("Saved").caption().secondary()]
                    .spacing(3.0)
                    .align(Align::Center)
                    .into(),
            );
            footer.push(button("Undo").plain().on_press(Msg::Undo).into());
        }
        let line_up: Element<Msg> = if self.status.lining_up {
            button("Done").icon(Icon::Checkmark).filled().on_press(Msg::LineUp(false)).into()
        } else {
            button("Line up screens").icon(Icon::Viewfinder).on_press(Msg::LineUp(true)).into()
        };
        footer.push(line_up);
        let mut parts: Vec<Element<Msg>> = vec![canvas.into(), row(footer).spacing(6.0).align(Align::Center).into()];
        if let Some(controller) = &self.status.controller {
            parts.push(
                text(format!(
                    "{controller}'s mouse crosses by the arrangement in Roam on {controller}: line the screens up there"
                ))
                .caption()
                .secondary()
                .wrap()
                .into(),
            );
        }
        column(parts).spacing(4.0).into()
    }

    /// Where the cursor crosses from one computer to another: a line along each stretch of
    /// edge that leads over, in the colour of the screen it leads to while lining up.
    fn crossings(&self, view: View) -> Vec<Shape> {
        let computers: Vec<Computer> = self.desk.iter().map(Spot::computer).collect();
        let mut shapes = Vec::new();
        for (on, computer) in computers.iter().enumerate() {
            for (index, display) in computer.desk_displays().iter().enumerate() {
                for edge in layout::Edge::ALL {
                    for span in align::spans(&computers, on, index, edge) {
                        if span.computer == on {
                            continue;
                        }
                        let [r, g, b] =
                            self.guide_colour(&self.desk[span.computer].id, span.display).unwrap_or([255, 255, 255]);
                        let colour = Color::rgba(r, g, b, if self.status.lining_up { 255 } else { 200 });
                        // Just inside the screen's edge, as drawn.
                        let inset = 2.5 / view.scale;
                        let (from, to) = (span.from, span.to);
                        let [a, b] = match edge {
                            layout::Edge::Left => [
                                [display.x + inset, display.y + from * display.h],
                                [display.x + inset, display.y + to * display.h],
                            ],
                            layout::Edge::Right => [
                                [display.right() - inset, display.y + from * display.h],
                                [display.right() - inset, display.y + to * display.h],
                            ],
                            layout::Edge::Top => [
                                [display.x + from * display.w, display.y + inset],
                                [display.x + to * display.w, display.y + inset],
                            ],
                            layout::Edge::Bottom => [
                                [display.x + from * display.w, display.bottom() - inset],
                                [display.x + to * display.w, display.bottom() - inset],
                            ],
                        };
                        let (a, b) = (view.to_canvas(a), view.to_canvas(b));
                        shapes.push(draw::line([(a[0], a[1]), (b[0], b[1])]).stroke(2.0, colour));
                    }
                }
            }
        }
        shapes
    }

    /// One computer on the desk: each of its screens, its name on its main one, and where the
    /// cursor is.
    fn spot_shapes(&self, spot: &Spot, view: View, me: &str, dragging: bool) -> Vec<Shape> {
        let peer = self.peer(&spot.id);
        let mine = spot.id == me;
        let connected = mine || peer.is_some_and(|p| p.link == LinkState::Connected);
        let selected = self.selected.as_deref() == Some(spot.id.as_str());
        let hovered = self.hover.as_deref() == Some(spot.id.as_str());
        let tint = if mine {
            [90, 150, 255]
        } else if connected {
            [70, 200, 130]
        } else {
            [150, 150, 160]
        };
        let computer = spot.computer();
        let displays = computer.desk_displays();
        let several = displays.len() > 1;
        let mut shapes = Vec::new();
        for (index, display) in displays.iter().enumerate() {
            let [x, y] = view.to_canvas([display.x, display.y]);
            let w = (display.w * view.scale) as f32;
            let h = (display.h * view.scale) as f32;
            let [x, y, w, h] = [x + 1.0, y + 1.0, (w - 2.0).max(2.0), (h - 2.0).max(2.0)];
            // A screen lent here as a display is drawn as one of this computer's.
            let lent = self.extended(&spot.id, index).is_some_and(|e| e.failed.is_none());
            let guided = self.guide_colour(&spot.id, index);
            let [r, g, b] = guided.unwrap_or(if lent { [90, 150, 255] } else { tint });
            let strength = if connected || lent || guided.is_some() { 1.0 } else { 0.45 };
            let alpha = |a: f32| (a * strength) as u8;
            let fill = draw::linear(Color::rgba(r, g, b, alpha(175.0)), Color::rgba(r, g, b, alpha(120.0)), 90.0);
            let (width, edge) = if selected {
                (1.5, Color::white(0.9))
            } else if hovered || dragging {
                (1.0, Color::white(0.4))
            } else {
                (1.0, Color::white(0.1))
            };
            if dragging {
                shapes.push(draw::shadow(x, y, w, h).radius(4.0).blur(8.0).color(Color::rgba(0, 0, 0, 120)));
            }
            shapes.push(draw::rect(x, y, w, h).radius(4.0).fill(fill).stroke(width, edge));
            if several && w > 14.0 && h > 12.0 {
                shapes.push(
                    draw::text(format!("{}", index + 1), x + 4.0, y + 3.0)
                        .size(8.0)
                        .semibold()
                        .color(Color::white(0.6)),
                );
            }
            if lent && h > 34.0 && w > 50.0 {
                shapes.push(
                    draw::text("Extended display", x + w / 2.0, y + h - 4.0)
                        .size(8.0)
                        .color(Color::white(0.75))
                        .anchor(Anchor::Bottom),
                );
            }
        }
        // The name goes on the biggest screen, where it fits, never between screens.
        let Some(display) = displays.get(biggest_display(&spot.displays)) else { return shapes };
        let [x, y] = view.to_canvas([display.x, display.y]);
        let (w, h) = ((display.w * view.scale) as f32, (display.h * view.scale) as f32);
        let (cx, cy) = (x + w / 2.0, y + h / 2.0);
        // About how many characters fit across at each size.
        let fits_at = |size: f32| ((w - 10.0) / (size * 0.62)).max(0.0) as usize;
        let size = if spot.name.chars().count() <= fits_at(11.0) { 11.0 } else { 10.0 };
        let fits = fits_at(size);
        let tone = if connected { Color::white(0.95) } else { Color::white(0.6) };
        if fits >= 3 && h >= 16.0 {
            let note = if mine {
                "This computer".to_owned()
            } else if peer.is_some_and(|p| p.link == LinkState::Connected) {
                spot.os.clone()
            } else {
                state_words(peer)
            };
            let room_for_note = h >= 34.0;
            let name_y = if room_for_note { cy - 6.0 } else { cy };
            shapes.push(
                draw::text(shorten(&spot.name, fits), cx, name_y)
                    .size(size)
                    .semibold()
                    .color(tone)
                    .anchor(Anchor::Center),
            );
            if room_for_note {
                shapes.push(
                    draw::text(shorten(&note, fits_at(9.0)), cx, cy + 8.0)
                        .size(9.0)
                        .color(Color::white(0.65))
                        .anchor(Anchor::Center),
                );
            }
        }
        let away = self.status.away_on.as_deref();
        if away == Some(spot.name.as_str()) || (mine && away.is_none()) {
            shapes.push(
                draw::circle(x + w - 7.0, y + 7.0, 2.5).fill(Color::white(0.95)).stroke(1.0, Color::rgba(0, 0, 0, 90)),
            );
        }
        shapes
    }

    fn extended(&self, id: &str, display: usize) -> Option<&ExtendedView> {
        self.status.extended.iter().find(|e| e.id == id && usize::from(e.display) == display)
    }

    /// Another computer's display under a right click: its id and which display.
    fn screen_at(&self, pointer: Pointer) -> Option<(String, u8)> {
        let at = self.view().to_desk(pointer.x, pointer.y);
        let me = self.me().map(|m| m.id.clone()).unwrap_or_default();
        self.desk.iter().rev().filter(|s| s.id != me).find_map(|spot| {
            let index = spot.computer().desk_displays().iter().position(|d| d.contains(at))?;
            Some((spot.id.clone(), u8::try_from(index).ok()?))
        })
    }

    /// Whether `display` of computer `id` can become a display of this one: why not, if not.
    fn lend_blocked(&self, spot: &Spot) -> Option<String> {
        let peer = self.peer(&spot.id);
        let connected = peer.is_some_and(|p| p.link == LinkState::Connected);
        match &self.status.ability {
            Ability::No(why) => Some(why.clone()),
            _ if !connected => Some(format!("Connect to {} first", spot.name)),
            _ if !peer.is_some_and(|p| p.lends) => Some(format!("Update notchbase on {} first", spot.name)),
            _ => None,
        }
    }

    /// The selected computer: how it's doing, what can be done with it and with each of its
    /// screens, and its scroll speed. A right-clicked screen is the one picked out.
    fn selected_details(&self) -> Option<Element<Msg>> {
        let id = self.selected.clone()?;
        let spot = self.desk.iter().find(|s| s.id == id)?;
        if self.me().is_some_and(|m| m.id == id) {
            return None;
        }
        let peer = self.peer(&id);
        let connected = peer.is_some_and(|p| p.link == LinkState::Connected);
        let mut actions: Vec<Element<Msg>> = Vec::new();
        if peer.is_some_and(|p| p.link == LinkState::Declined) {
            actions.push(button("Try again").on_press(Msg::Retry(id.clone())).into());
        }
        if peer.is_some_and(|p| p.paired) {
            actions.push(button("Forget").on_press(Msg::Forget(id.clone())).into());
        }
        actions.push(button("Remove").destructive().on_press(Msg::Remove(id.clone())).into());
        let glyph = if spot.os == Os::Windows.word() { Icon::Display } else { Icon::Device };
        let mut header: Vec<Element<Msg>> = vec![
            icon(glyph).tone(if connected { Tone::Green } else { Tone::Secondary }).into(),
            column![text(spot.name.clone()).headline(), text(state_words(peer)).caption().secondary()]
                .spacing(2.0)
                .into(),
            spacer(),
        ];
        header.extend(actions);
        header.push(button("").icon(Icon::Xmark).plain().on_press(Msg::Select(None)).into());
        let mut parts: Vec<Element<Msg>> = vec![row(header).spacing(6.0).align(Align::Center).into()];

        // Each screen, and whether it can be a display of this computer.
        let blocked = self.lend_blocked(spot);
        let picked = self.menu.as_ref().filter(|(m, _)| *m == id).map(|(_, d)| usize::from(*d));
        let several = spot.displays.len() > 1;
        let many = peer.is_some_and(|p| p.lends_many);
        let up: Vec<usize> =
            (0..spot.displays.len()).filter(|i| self.extended(&id, *i).is_some_and(|e| e.failed.is_none())).collect();
        for (index, display) in spot.displays.iter().enumerate() {
            let display_index = u8::try_from(index).unwrap_or(u8::MAX);
            let name = if several {
                format!("{}'s screen {}", spot.name, index + 1)
            } else {
                format!("{}'s screen", spot.name)
            };
            let size = format!("{} × {}", display.w.round() as u32, display.h.round() as u32);
            let extended = self.extended(&id, index);
            let (detail, action): (String, Element<Msg>) = match extended {
                Some(extended) if extended.failed.is_none() => (
                    match &extended.working {
                        Some(working) => format!("{working}…"),
                        None => "A display of this computer now".into(),
                    },
                    button("Disconnect display")
                        .destructive()
                        .on_press(Msg::Unextend(id.clone(), Some(display_index)))
                        .into(),
                ),
                _ => {
                    let replaces = up.first().filter(|_| !many);
                    let detail = blocked.clone().unwrap_or_else(|| match (replaces, &self.status.ability) {
                        (Some(other), _) => format!(
                            "Takes the place of screen {}: update notchbase on {} to use several of its screens at once",
                            other + 1,
                            spot.name
                        ),
                        (None, Ability::NeedsDriver) => {
                            "Installs a free display driver first; Windows asks for permission once".into()
                        }
                        (None, _) if !up.is_empty() => {
                            format!("{size} · Becomes one more display of this computer, beside the others")
                        }
                        (None, _) => {
                            format!("{size} · Becomes another display of this computer, like a monitor plugged in")
                        }
                    });
                    let button = button("Turn into extended display")
                        .filled()
                        .enabled(blocked.is_none())
                        .on_press(Msg::Extend(id.clone(), display_index));
                    (detail, button.into())
                }
            };
            // Only the right-clicked screen shows its button when there are several; the
            // others offer theirs on a right-click.
            if several && picked != Some(index) && extended.is_none() {
                continue;
            }
            let lit = picked == Some(index) || extended.is_some_and(|e| e.failed.is_none());
            parts.push(divider());
            parts.push(
                column![
                    row![
                        icon(Icon::Display).tone(if lit { Tone::Blue } else { Tone::Secondary }),
                        text(name).callout(),
                    ]
                    .spacing(8.0)
                    .align(Align::Center),
                    text(detail).caption().secondary().wrap(),
                    action,
                ]
                .spacing(6.0)
                .into(),
            );
        }
        let free = spot.displays.len() - up.len();
        if several && blocked.is_none() && many && free >= 1 {
            parts.push(divider());
            let label = if up.is_empty() {
                format!("Use all {} screens as displays", spot.displays.len())
            } else if free == 1 {
                "Use the other screen too".to_owned()
            } else {
                format!("Use the other {free} screens too")
            };
            parts.push(
                row![
                    text(if picked.is_none() && up.is_empty() {
                        "Right-click one of its screens to use it as a display, or use them all"
                    } else {
                        "Each one becomes its own display of this computer"
                    })
                    .caption()
                    .tertiary()
                    .wrap(),
                    spacer(),
                    button(label).on_press(Msg::ExtendAll(id.clone())),
                ]
                .spacing(8.0)
                .align(Align::Center)
                .into(),
            );
        } else if several && picked.is_none() && up.is_empty() && !matches!(self.status.ability, Ability::No(_)) {
            parts
                .push(text("Right-click one of its screens to use it as a display").caption().tertiary().wrap().into());
        }
        parts.push(divider());
        parts.push(self.position(&id));
        parts.push(divider());
        parts.push(
            row![
                text("Scroll speed").callout(),
                slider(spot.scroll as f32)
                    .range(SCROLL_MIN, SCROLL_MAX)
                    .step(SCROLL_STEP)
                    .on_change(move |speed| Msg::Scroll(id.clone(), speed)),
                text(times(spot.scroll)).callout().secondary(),
            ]
            .spacing(8.0)
            .align(Align::Center)
            .into(),
        );
        Some(card(column(parts).spacing(10.0)))
    }

    /// Where the selected computer sits against its neighbour, and buttons to move it a point
    /// at a time or line their edges up.
    fn position(&self, id: &str) -> Element<Msg> {
        let Some(facing) = self.facing(id) else {
            return text("Drag it against another screen, then line their edges up here")
                .caption()
                .secondary()
                .wrap()
                .into();
        };
        let offset = facing.offset();
        let points = offset.abs().round() as i64;
        let words = if points == 0 {
            if facing.beside {
                format!("Its top is level with the top of {}'s screen", facing.name)
            } else {
                format!("Its left edge is level with {}'s screen", facing.name)
            }
        } else if facing.beside {
            let way = if offset > 0.0 { "below" } else { "above" };
            format!("Its top is {points} pt {way} the top of {}'s screen", facing.name)
        } else {
            let way = if offset > 0.0 { "right of" } else { "left of" };
            format!("Its left edge is {points} pt {way} {}'s", facing.name)
        };
        let nudge = |icon_for: Icon, by: f64, label: &str| -> Element<Msg> {
            let by = if facing.beside { [0.0, by] } else { [by, 0.0] };
            button(label).icon(icon_for).on_press(Msg::Nudge(id.to_owned(), by)).into()
        };
        let (back, ahead) =
            if facing.beside { (Icon::ChevronUp, Icon::ChevronDown) } else { (Icon::ChevronLeft, Icon::ChevronRight) };
        let (start, end) = if facing.beside { ("Tops", "Bottoms") } else { ("Lefts", "Rights") };
        let level = |label: &str, level: Level| -> Element<Msg> {
            button(label).plain().on_press(Msg::Level(id.to_owned(), level)).into()
        };
        column![
            text("Line up").callout(),
            text(words).caption().secondary().wrap(),
            row![nudge(back, -10.0, "10"), nudge(back, -1.0, "1"), nudge(ahead, 1.0, "1"), nudge(ahead, 10.0, "10")]
                .spacing(6.0)
                .align(Align::Center),
            row![
                text("Line up").caption().secondary(),
                level(start, Level::Start),
                level("Middles", Level::Middle),
                level(end, Level::End),
            ]
            .spacing(6.0)
            .align(Align::Center),
        ]
        .spacing(6.0)
        .into()
    }

    fn nearby(&self) -> Element<Msg> {
        let placed: Vec<&str> = self.desk.iter().map(|s| s.id.as_str()).collect();
        let waiting: Vec<&PeerView> =
            self.status.peers.iter().filter(|p| p.nearby && !placed.contains(&p.id.as_str())).collect();
        if waiting.is_empty() {
            let hint = if self.desk.len() < 2 {
                "No other computers yet. Open notchbase on another Mac or PC on this network."
            } else {
                "No other computers nearby"
            };
            return text(hint).caption().secondary().wrap().into();
        }
        let rows = waiting.into_iter().map(|peer| {
            let icon_for = if peer.os == Os::Windows { Icon::Display } else { Icon::Device };
            let detail = if peer.open { peer.os.word().to_owned() } else { state_words(Some(peer)) };
            keyed(
                peer.id.clone(),
                row![
                    icon(icon_for).tone(Tone::Secondary),
                    column![text(peer.name.clone()).callout(), text(detail).caption().secondary()].spacing(1.0),
                    spacer(),
                    button("Add").icon(Icon::Plus).enabled(peer.open).on_press(Msg::Add(peer.id.clone())),
                ]
                .spacing(8.0)
                .align(Align::Center),
            )
        });
        column![text("Nearby").section(), list(rows.collect::<Vec<_>>())].spacing(6.0).into()
    }
}

impl<B: Backend> Roam<B> {
    /// Reaching a computer by its address, for networks that hide computers from each other.
    fn by_address(&self) -> Element<Msg> {
        let mut parts: Vec<Element<Msg>> = vec![
            text("Connect by address").section().into(),
            row![
                text_input(&self.draft)
                    .placeholder("Other computer's IP address")
                    .on_change(Msg::Draft)
                    .on_submit(Msg::Connect),
                button("Connect").enabled(!self.draft.trim().is_empty()).on_press(Msg::Connect),
            ]
            .spacing(6.0)
            .align(Align::Center)
            .into(),
        ];
        for address in &self.prefs.addresses {
            parts.push(keyed(
                format!("address {address}"),
                row![
                    icon(Icon::Link).tone(Tone::Secondary),
                    text(address.clone()).callout(),
                    spacer(),
                    button("").icon(Icon::Xmark).plain().on_press(Msg::Unlist(address.clone())),
                ]
                .spacing(8.0)
                .align(Align::Center),
            ));
        }
        if !self.status.addresses.is_empty() {
            let mine = format!("This computer is {}", self.status.addresses.join(", "));
            parts.push(text(mine).caption().secondary().wrap().into());
        }
        column(parts).spacing(6.0).into()
    }
}

/// A computer's main screen: the one at its origin, or else its biggest.
fn main_display(displays: &[Rect]) -> usize {
    displays.iter().position(|d| d.contains([0.0, 0.0])).unwrap_or_else(|| biggest_display(displays))
}

/// The screen with the most room, for a computer's name.
fn biggest_display(displays: &[Rect]) -> usize {
    let area = |d: &Rect| d.w * d.h;
    (0..displays.len()).max_by(|&a, &b| area(&displays[a]).total_cmp(&area(&displays[b])).then(b.cmp(&a))).unwrap_or(0)
}

/// Words cut to `most` characters, with an ellipsis when cut.
fn shorten(words: &str, most: usize) -> String {
    if words.chars().count() <= most {
        return words.to_owned();
    }
    let kept: String = words.chars().take(most.saturating_sub(1)).collect();
    format!("{}…", kept.trim_end())
}

/// Lines where the dragged computer's screen edges meet or line up with another's, so it's
/// clear what it snapped to.
fn guides(desk: &[Spot], moving: usize, view: View) -> Vec<Shape> {
    const NEAR: f64 = 0.5;
    let Some(spot) = desk.get(moving) else { return Vec::new() };
    let mine = spot.computer().desk_displays();
    let others: Vec<Rect> = desk
        .iter()
        .enumerate()
        .filter(|(index, _)| *index != moving)
        .flat_map(|(_, s)| s.computer().desk_displays())
        .collect();
    let color = Color::rgba(255, 204, 0, 220);
    let mut lines: Vec<[f64; 4]> = Vec::new();
    for a in &mine {
        for b in &others {
            let (top, bottom) = (a.y.min(b.y), a.bottom().max(b.bottom()));
            let (left, right) = (a.x.min(b.x), a.right().max(b.right()));
            for x in [a.x, a.right()] {
                if [b.x, b.right()].iter().any(|bx| (bx - x).abs() < NEAR) {
                    lines.push([x, top, x, bottom]);
                }
            }
            for y in [a.y, a.bottom()] {
                if [b.y, b.bottom()].iter().any(|by| (by - y).abs() < NEAR) {
                    lines.push([left, y, right, y]);
                }
            }
        }
    }
    lines.dedup();
    lines
        .into_iter()
        .map(|[x1, y1, x2, y2]| {
            let from = view.to_canvas([x1, y1]);
            let to = view.to_canvas([x2, y2]);
            draw::line([(from[0], from[1]), (to[0], to[1])]).stroke(1.0, color)
        })
        .collect()
}

/// A multiplier as people read it: 1×, 1.5×, 0.25×.
fn times(speed: f64) -> String {
    let words = format!("{speed:.2}");
    let words = words.trim_end_matches('0').trim_end_matches('.');
    format!("{words}×")
}

/// Copies a computer's current screens and name into its spot. Whether anything changed.
fn refresh(spot: &mut Spot, peer: &PeerView) -> bool {
    let mut changed = false;
    if !peer.displays.is_empty() && (spot.displays != peer.displays || spot.ppu != peer.ppu) {
        // The main screen stays where it was on the desk when screens come, go or change
        // size, so the arrangement survives a monitor being unplugged or lent.
        let before = Computer::new("", spot.displays.clone(), spot.ppu, spot.at);
        let after = Computer::new("", peer.displays.clone(), peer.ppu, spot.at);
        let was = before.desk_displays().get(main_display(&spot.displays)).map(|d| [d.x, d.y]);
        let now = after.desk_displays().get(main_display(&peer.displays)).map(|d| [d.x, d.y]);
        if let (Some(was), Some(now)) = (was, now) {
            spot.at = [spot.at[0] + was[0] - now[0], spot.at[1] + was[1] - now[1]];
        }
        spot.displays = peer.displays.clone();
        spot.ppu = peer.ppu;
        changed = true;
    }
    if spot.name != peer.name && !peer.name.is_empty() {
        spot.name = peer.name.clone();
        changed = true;
    }
    let os = peer.os.word();
    if peer.os != Os::Other && spot.os != os {
        spot.os = os.into();
        changed = true;
    }
    changed
}

#[cfg(test)]
mod tests;
