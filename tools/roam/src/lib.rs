//! Roam: one keyboard and mouse for every computer on your desk.
//!
//! Open Roam on each Mac or Windows PC on the same network and they find each other. Drag the
//! nearby ones onto the desk next to your screen, the way they stand in front of you, and the
//! cursor glides from one to the next at the edges. Keys, clicks, scrolling and copied text go
//! with it.

pub mod bulk;
pub mod crypto;
pub mod engine;
pub mod input;
pub mod keys;
pub mod layout;
pub mod link;
pub mod smooth;
pub mod wire;

use std::marker::PhantomData;
use std::sync::mpsc::Receiver;
use std::time::Duration;

use notchbase_sdk::column;
use notchbase_sdk::prelude::*;
use serde::{Deserialize, Serialize};

use engine::{Ask, Control, Engine, LinkState, PeerView, Placement, Prefs, Status, Transfer};
use layout::{Computer, Rect};
use wire::Os;

pub const ID: &str = "roam";
const OPEN: &str = "open";
const NAME: &str = "name";
const SMOOTHING: &str = "smoothing";
const CLIPBOARD: &str = "clipboard";
const FAST_WIFI: &str = "fast_wifi";
const MODIFIERS: &str = "modifiers";
const CORNERS: &str = "corners";
const ADDRESSES: &str = "addresses";
const DESK: &str = "desk";

const WIDTH: f32 = 340.0;
const HEIGHT: f32 = 170.0;
const PAD: f32 = 14.0;
const SNAP: f32 = 10.0;
const GLYPH: f32 = 14.0;
const RING: f32 = 13.0;
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
        let room = [f64::from(WIDTH - PAD * 2.0), f64::from(HEIGHT - PAD * 2.0 - 12.0)];
        let scale = (room[0] / bounds.w).min(room[1] / bounds.h).min(0.2);
        let used = [bounds.w * scale, bounds.h * scale];
        let offset = [(f64::from(WIDTH) - used[0]) / 2.0, (f64::from(HEIGHT) - 12.0 - used[1]) / 2.0];
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

#[derive(Clone, Copy, Debug, PartialEq)]
struct Drag {
    index: usize,
    grab: [f64; 2],
    view: View,
    moved: bool,
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
                self.selected = Some(spot.id.clone());
                self.desk.push(spot);
                self.desk_changed()
            }
            Msg::Remove(id) => {
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
                self.desk.retain(|s| s.id != id);
                self.selected = None;
                self.desk_changed()
            }
            Msg::Select(id) => {
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
            parts.push(card(
                row![
                    icon(Icon::Exclamation).tone(Tone::Orange),
                    text(problem.clone()).callout().wrap(),
                    spacer(),
                    button("Allow").on_press(Msg::Permission),
                ]
                .spacing(8.0)
                .align(Align::Center),
            ));
        }
        for ask in &self.status.asks {
            parts.push(ask_card(ask));
        }
        if let Some(transfer) = &self.status.transfer {
            parts.push(transfer_view(transfer));
        }
        if let Some(name) = &self.status.away_on {
            parts.push(
                row![
                    icon(Icon::Mouse).tone(Tone::Blue),
                    column![
                        text(format!("On {name}")).headline(),
                        text(if self.status.locked {
                            format!("Locked there · {HOME_KEYS} brings it back")
                        } else {
                            format!("{HOME_KEYS} brings it back")
                        })
                        .caption()
                        .secondary(),
                    ]
                    .spacing(2.0),
                    spacer(),
                    button("Bring back").on_press(Msg::Home),
                ]
                .spacing(8.0)
                .align(Align::Center)
                .into(),
            );
        }
        if let Some(name) = &self.status.controller {
            parts.push(
                row![
                    icon(Icon::Keyboard).tone(Tone::Green),
                    text(format!("Using {name}'s keyboard and mouse")).callout(),
                    spacer(),
                    button("Stop").on_press(Msg::Release),
                ]
                .spacing(8.0)
                .align(Align::Center)
                .into(),
            );
        }
        if !self.prefs.open && self.status.controller.is_none() {
            parts.push(
                row![
                    text("Hidden from other computers").callout().secondary(),
                    spacer(),
                    button("Show").on_press(Msg::Open(true)),
                ]
                .align(Align::Center)
                .into(),
            );
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
        self.drag.map_or_else(|| View::fit(&self.desk), |drag| drag.view)
    }

    fn pointer(&mut self, pointer: Pointer) -> Command<Msg> {
        match pointer.kind {
            PointerKind::Press => {
                let view = self.view();
                let at = view.to_desk(pointer.x, pointer.y);
                // The top-most spot under the pointer is the last drawn.
                let hit = self.desk.iter().rposition(|s| s.computer().rect().contains(at));
                if let Some(index) = hit {
                    let grab = [at[0] - self.desk[index].at[0], at[1] - self.desk[index].at[1]];
                    self.drag = Some(Drag { index, grab, view, moved: false });
                    self.selected = Some(self.desk[index].id.clone());
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
            PointerKind::Release | PointerKind::Leave => match self.drag.take() {
                Some(drag) if drag.moved => self.desk_changed(),
                _ => Command::none(),
            },
            PointerKind::Click => {
                if self.drag.take().is_some_and(|d| d.moved) {
                    return self.desk_changed();
                }
                let at = self.view().to_desk(pointer.x, pointer.y);
                let hit = self.desk.iter().rev().find(|s| s.computer().rect().contains(at)).map(|s| s.id.clone());
                self.selected = hit;
                Command::none()
            }
            _ => Command::none(),
        }
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
        let mut canvas = canvas::<Msg>(WIDTH, HEIGHT).stretch().on_pointer(Msg::Pointer);
        let away = self.status.away_on.clone();
        for spot in &self.desk {
            let peer = self.peer(&spot.id);
            let mine = spot.id == me;
            let connected = mine || peer.is_some_and(|p| p.link == LinkState::Connected);
            let selected = self.selected.as_deref() == Some(spot.id.as_str());
            let fill = if mine {
                Color::rgba(90, 150, 255, 140)
            } else if connected {
                Color::rgba(90, 210, 140, 115)
            } else {
                Color::white(0.12)
            };
            let computer = spot.computer();
            for display in computer.desk_displays() {
                let [x, y] = view.to_canvas([display.x, display.y]);
                let w = (display.w * view.scale) as f32;
                let h = (display.h * view.scale) as f32;
                let mut shape =
                    draw::rect(x + 0.75, y + 0.75, (w - 1.5).max(1.0), (h - 1.5).max(1.0)).radius(3.0).fill(fill);
                if selected {
                    shape = shape.stroke(1.5, Color::white(0.9));
                }
                canvas = canvas.push(shape);
            }
            let rect = computer.rect();
            let [cx, cy] = view.to_canvas([rect.x + rect.w / 2.0, rect.y + rect.h / 2.0]);
            let [_, bottom] = view.to_canvas([rect.x, rect.bottom()]);
            let label = if spot.name.chars().count() > 18 {
                format!("{}…", spot.name.chars().take(17).collect::<String>())
            } else {
                spot.name.clone()
            };
            let tone = if connected { Color::white(0.95) } else { Color::white(0.55) };
            canvas = canvas.push(draw::text(label, cx, cy).size(11.0).semibold().color(tone).anchor(Anchor::Center));
            let note = if mine {
                "This computer".to_owned()
            } else if peer.is_some_and(|p| p.link == LinkState::Connected) {
                spot.os.clone()
            } else {
                state_words(peer)
            };
            canvas =
                canvas.push(draw::text(note, cx, bottom + 2.0).size(9.0).color(Color::white(0.5)).anchor(Anchor::Top));
            if away.as_deref() == Some(spot.name.as_str()) || (mine && away.is_none()) {
                canvas = canvas.push(draw::circle(cx, cy + 10.0, 2.5).fill(Color::white(0.9)));
            }
        }
        column![canvas, text("Drag the screens to match your desk").caption().tertiary().align(Align::Center)]
            .spacing(4.0)
            .into()
    }

    fn selected_details(&self) -> Option<Element<Msg>> {
        let id = self.selected.clone()?;
        let spot = self.desk.iter().find(|s| s.id == id)?;
        if self.me().is_some_and(|m| m.id == id) {
            return None;
        }
        let peer = self.peer(&id);
        let mut actions: Vec<Element<Msg>> = Vec::new();
        if peer.is_some_and(|p| p.link == LinkState::Declined) {
            actions.push(button("Try again").on_press(Msg::Retry(id.clone())).into());
        }
        if peer.is_some_and(|p| p.paired) {
            actions.push(button("Forget").on_press(Msg::Forget(id.clone())).into());
        }
        actions.push(button("Remove").destructive().on_press(Msg::Remove(id.clone())).into());
        let mut line: Vec<Element<Msg>> = vec![
            column![text(spot.name.clone()).headline(), text(state_words(peer)).caption().secondary()]
                .spacing(2.0)
                .into(),
            spacer(),
        ];
        line.extend(actions);
        let scroll = row![
            text("Scroll speed").callout(),
            slider(spot.scroll as f32)
                .range(SCROLL_MIN, SCROLL_MAX)
                .step(SCROLL_STEP)
                .on_change(move |speed| Msg::Scroll(id.clone(), speed)),
            text(times(spot.scroll)).callout().secondary(),
        ]
        .spacing(8.0)
        .align(Align::Center);
        Some(card(column![row(line).spacing(6.0).align(Align::Center), scroll].spacing(10.0)))
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
