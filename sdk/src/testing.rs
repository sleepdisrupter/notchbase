//! A harness for testing tools without a host: send messages, fake events, press controls and look
//! at the requests that come out.

use std::time::{Duration, Instant};

use crate::driver::{Driver, Waker};
use crate::protocol::{
    CalendarFeed, DownloadChange, Effects, Event, Focus, HandlerId, Icon, LiveNode, MonthNode, Node, NotchState,
    NowPlaying, Pointer, PointerKind, Power, Reply, Request, Sensors, Shape, Start, Surface, TileNode, UiValue, Value,
    Values, VolumeLevel, Wants,
};
use crate::runtime::Runtime;
use crate::settings::Settings;
use crate::tool::Tool;

const WAIT_AT_MOST: Duration = Duration::from_secs(5);
const WAIT_STEP: Duration = Duration::from_millis(5);

/// Runs a tool the way the host would, with every request kept for the test to inspect.
pub struct Harness<T: Tool> {
    runtime: Runtime<T>,
    last: Vec<Request>,
    asked: Vec<u64>,
    wants: Wants,
    notice_controls: Option<Node>,
}

#[derive(Clone, Debug)]
struct Control {
    surface: Surface,
    label: String,
    icon: Option<Icon>,
    handler: HandlerId,
    value: UiValue,
}

impl<T: Tool> Harness<T> {
    pub fn start() -> Self {
        Self::start_with(Vec::<(&str, Value)>::new())
    }

    pub fn start_with<K: Into<String>, V: Into<Value>>(settings: impl IntoIterator<Item = (K, V)>) -> Self {
        let settings: Values = settings.into_iter().map(|(key, value)| (key.into(), value.into())).collect();
        let mut harness = Self {
            runtime: Runtime::new(Waker::noop()),
            last: Vec::new(),
            asked: Vec::new(),
            wants: Wants::default(),
            notice_controls: None,
        };
        let effects = harness.runtime.start(Start { settings, ..Start::default() });
        harness.keep(effects);
        harness
    }

    pub fn model(&self) -> &T {
        self.runtime.tool().expect("the tool has started")
    }

    pub fn settings(&self) -> &Settings {
        self.runtime.settings()
    }

    pub fn requests(&self) -> &[Request] {
        &self.last
    }

    pub fn wants(&self) -> &Wants {
        &self.wants
    }

    pub fn saved(&self, key: &str) -> Option<&Value> {
        self.last.iter().rev().find_map(|request| match request {
            Request::SaveSetting { key: saved, value } if saved == key => Some(value),
            _ => None,
        })
    }

    pub fn send(&mut self, msg: T::Msg) -> &[Request] {
        let effects = self.runtime.send(msg);
        self.keep(effects)
    }

    pub fn event(&mut self, event: Event) -> &[Request] {
        let effects = self.runtime.handle(vec![event]);
        self.keep(effects)
    }

    pub fn shortcut(&mut self, id: &str) -> &[Request] {
        self.event(Event::Shortcut { id: id.to_owned() })
    }

    pub fn tick(&mut self, every: Duration) -> &[Request] {
        self.event(Event::Tick { every_ms: every.as_millis().clamp(16, u128::from(u64::MAX)) as u64 })
    }

    pub fn change_settings<K: Into<String>, V: Into<Value>>(
        &mut self,
        changes: impl IntoIterator<Item = (K, V)>,
    ) -> &[Request] {
        let mut values = self.runtime.settings().values().clone();
        values.extend(changes.into_iter().map(|(key, value)| (key.into(), value.into())));
        self.event(Event::Settings { values })
    }

    pub fn notch(&mut self, state: NotchState) -> &[Request] {
        self.event(Event::Notch { state })
    }

    pub fn media(&mut self, now_playing: Option<NowPlaying>) -> &[Request] {
        self.event(Event::Media { now_playing })
    }

    pub fn calendar(&mut self, feed: CalendarFeed) -> &[Request] {
        self.event(Event::Calendar { feed })
    }

    pub fn power(&mut self, power: Power) -> &[Request] {
        self.event(Event::Power { power })
    }

    pub fn screen_lock(&mut self, locked: bool) -> &[Request] {
        self.event(Event::ScreenLock { locked })
    }

    pub fn sensors(&mut self, sensors: Sensors) -> &[Request] {
        self.event(Event::Sensors { sensors })
    }

    pub fn download(&mut self, change: DownloadChange) -> &[Request] {
        self.event(Event::Download { change })
    }

    pub fn focus(&mut self, focus: Focus) -> &[Request] {
        self.event(Event::Focus { focus })
    }

    pub fn volume(&mut self, volume: f32, muted: bool) -> &[Request] {
        self.event(Event::Volume { level: VolumeLevel { volume, muted } })
    }

    pub fn answer(&mut self, reply: Reply) -> &[Request] {
        let id = self.asked.pop().expect("nothing is waiting for an answer");
        self.event(Event::Reply { id, reply })
    }

    pub fn wait_for(&mut self, done: impl Fn(&T) -> bool) -> &[Request] {
        let deadline = Instant::now() + WAIT_AT_MOST;
        let mut gathered = Vec::new();
        loop {
            let effects = self.runtime.handle(Vec::new());
            gathered.extend(self.keep(effects).iter().cloned());
            if done(self.model()) {
                self.last = gathered;
                return &self.last;
            }
            assert!(Instant::now() < deadline, "waited {WAIT_AT_MOST:?} and it never happened");
            std::thread::sleep(WAIT_STEP);
        }
    }

    pub fn tile(&mut self) -> Option<TileNode> {
        self.runtime.views().tile
    }

    pub fn page(&mut self) -> Option<Node> {
        self.runtime.views().page
    }

    pub fn live(&mut self) -> Option<LiveNode> {
        self.runtime.views().live
    }

    pub fn texts(&mut self) -> Vec<String> {
        let mut texts = Vec::new();
        for (_, node) in self.surfaces() {
            gather_texts(&node, &mut texts);
        }
        texts
    }

    pub fn shows(&mut self, text: &str) -> bool {
        self.texts().iter().any(|shown| shown.contains(text))
    }

    pub fn noticed(&self) -> Vec<String> {
        let mut texts = Vec::new();
        for request in &self.last {
            if let Request::Notice { notice } = request {
                let parts = [&notice.leading, &notice.trailing].into_iter().chain(&notice.below);
                for node in parts.chain(&notice.controls) {
                    gather_texts(node, &mut texts);
                }
            }
        }
        texts
    }

    pub fn press_tile(&mut self) -> &[Request] {
        let handler = self.tile().and_then(|tile| tile.on_press).expect("the tile can be pressed");
        self.ui(Surface::Tile, handler, UiValue::Press)
    }

    pub fn click_notice(&mut self, label: &str) -> &[Request] {
        let mut controls = Vec::new();
        if let Some(node) = &self.notice_controls {
            gather(node, &mut |node| controls.extend(control_of(Surface::Notice, node)));
        }
        let Some(control) = controls.iter().find(|control| control.label == label).cloned() else {
            let labels: Vec<String> = controls.iter().map(describe).collect();
            panic!("the last notice has no control \"{label}\"; these can be clicked: {}", labels.join(", "))
        };
        self.ui(control.surface, control.handler, control.value)
    }

    pub fn click(&mut self, label: &str) -> &[Request] {
        self.click_nth(label, 0)
    }

    pub fn click_nth(&mut self, label: &str, nth: usize) -> &[Request] {
        let control = self.control(|control| control.label == label, nth, &format!("\"{label}\""));
        self.ui(control.surface, control.handler, control.value)
    }

    pub fn click_icon(&mut self, icon: Icon, nth: usize) -> &[Request] {
        let control = self.control(|control| control.icon == Some(icon), nth, &format!("{icon:?}"));
        self.ui(control.surface, control.handler, control.value)
    }

    pub fn type_text(&mut self, placeholder: &str, text: &str) -> &[Request] {
        let (surface, handler) = self.input(placeholder, |node| match node {
            Node::TextInput { on_change, .. } => *on_change,
            _ => None,
        });
        self.ui(surface, handler, UiValue::Text(text.to_owned()))
    }

    pub fn submit(&mut self, placeholder: &str) -> &[Request] {
        let (surface, handler) = self.input(placeholder, |node| match node {
            Node::TextInput { on_submit, .. } => *on_submit,
            _ => None,
        });
        self.ui(surface, handler, UiValue::Press)
    }

    pub fn set_toggle(&mut self, nth: usize, on: bool) -> &[Request] {
        let mut found = Vec::new();
        for (surface, node) in self.surfaces() {
            gather(&node, &mut |node| {
                if let Node::Toggle { on_toggle: Some(handler), .. } = node {
                    found.push((surface, *handler));
                }
            });
        }
        let (surface, handler) = *found.get(nth).unwrap_or_else(|| panic!("there are {} toggles", found.len()));
        self.ui(surface, handler, UiValue::Bool(on))
    }

    pub fn select(&mut self, option: &str) -> &[Request] {
        let mut found = None;
        for (surface, node) in self.surfaces() {
            gather(&node, &mut |node| {
                let (labels, handler): (Vec<&str>, _) = match node {
                    Node::Segmented { options, on_select, .. } => {
                        (options.iter().map(String::as_str).collect(), *on_select)
                    }
                    Node::Dropdown { options, on_select, .. } => {
                        (options.iter().map(|choice| choice.label.as_str()).collect(), *on_select)
                    }
                    _ => return,
                };
                if let (Some(index), Some(handler), None) =
                    (labels.iter().position(|label| *label == option), handler, found)
                {
                    found = Some((surface, handler, index));
                }
            });
        }
        let (surface, handler, index) = found.unwrap_or_else(|| panic!("no choice called \"{option}\""));
        self.ui(surface, handler, UiValue::Index(index))
    }

    pub fn slide(&mut self, nth: usize, value: f32) -> &[Request] {
        let mut found = Vec::new();
        for (surface, node) in self.surfaces() {
            gather(&node, &mut |node| {
                if let Node::Slider { on_change: Some(handler), .. } = node {
                    found.push((surface, *handler));
                }
            });
        }
        let (surface, handler) = *found.get(nth).unwrap_or_else(|| panic!("there are {} sliders", found.len()));
        self.ui(surface, handler, UiValue::Number(value))
    }

    pub fn pan(&mut self, nth: usize, by: f32) -> &[Request] {
        let (surface, handler) = self.panned(nth, |on_pan, _| on_pan);
        self.ui(surface, handler, UiValue::Number(by))
    }

    pub fn tap_pan(&mut self, nth: usize, at: f32) -> &[Request] {
        let (surface, handler) = self.panned(nth, |_, on_tap| on_tap);
        self.ui(surface, handler, UiValue::Number(at))
    }

    fn panned(
        &mut self,
        nth: usize,
        handler: impl Fn(Option<HandlerId>, Option<HandlerId>) -> Option<HandlerId>,
    ) -> (Surface, HandlerId) {
        let mut found = Vec::new();
        for (surface, node) in self.surfaces() {
            gather(&node, &mut |node| {
                if let Node::Pan { on_pan, on_tap, .. } = node {
                    found.push(handler(*on_pan, *on_tap).map(|handler| (surface, handler)));
                }
            });
        }
        let pan = found.get(nth).unwrap_or_else(|| panic!("there are {} panning strips", found.len()));
        pan.unwrap_or_else(|| panic!("panning strip {nth} doesn't listen for this"))
    }

    pub fn pointer(&mut self, nth: usize, pointer: Pointer) -> &[Request] {
        let mut found = Vec::new();
        for (surface, node) in self.surfaces() {
            gather(&node, &mut |node| {
                if let Node::Canvas(canvas) = node {
                    let scrolls = pointer.kind == PointerKind::Scroll;
                    found.push((surface, if scrolls { canvas.on_scroll } else { canvas.on_pointer }));
                }
            });
        }
        let (surface, handler) = *found.get(nth).unwrap_or_else(|| panic!("there are {} canvases", found.len()));
        let handler = handler.unwrap_or_else(|| panic!("canvas {nth} doesn't listen for {:?}", pointer.kind));
        self.ui(surface, handler, UiValue::Pointer(pointer))
    }

    pub fn pick_day(&mut self, day: u8) -> &[Request] {
        let (surface, handler, year, month) = self.month();
        self.ui(surface, handler, UiValue::Index(MonthNode::pack(year, month, day)))
    }

    pub fn pick_date(&mut self, year: i32, month: u8, day: u8) -> &[Request] {
        let (surface, handler, _, _) = self.month();
        self.ui(surface, handler, UiValue::Index(MonthNode::pack(year, month, day)))
    }

    pub fn pick_color(&mut self, index: usize) -> &[Request] {
        let mut found = None;
        for (surface, node) in self.surfaces() {
            gather(&node, &mut |node| {
                if let (Node::Palette { on_select: Some(handler), .. }, None) = (node, found) {
                    found = Some((surface, *handler));
                }
            });
        }
        let (surface, handler) = found.unwrap_or_else(|| panic!("no palette to pick a color from"));
        self.ui(surface, handler, UiValue::Index(index))
    }

    pub fn scroll_calendar(&mut self, steps: i32) -> &[Request] {
        let mut found = None;
        for (surface, node) in self.surfaces() {
            gather(&node, &mut |node| {
                let handler = match node {
                    Node::Week(week) => week.on_scroll,
                    Node::Month(month) => month.on_scroll,
                    _ => None,
                };
                if let (Some(handler), None) = (handler, found) {
                    found = Some((surface, handler));
                }
            });
        }
        let (surface, handler) = found.unwrap_or_else(|| panic!("no week or month that scrolls"));
        self.ui(surface, handler, UiValue::Number(steps as f32))
    }

    pub fn tap_week_day(&mut self, year: i32, month: u8, day: u8) -> &[Request] {
        let (surface, handler) = self.week(|week| week.on_day);
        self.ui(surface, handler, UiValue::Index(MonthNode::pack(year, month, day)))
    }

    pub fn tap_week_event(&mut self, id: &str) -> &[Request] {
        let (surface, handler) = self.week(|week| week.on_event);
        self.ui(surface, handler, UiValue::Text(id.to_owned()))
    }

    fn week(&mut self, handler: impl Fn(&crate::protocol::WeekNode) -> Option<HandlerId>) -> (Surface, HandlerId) {
        let mut found = None;
        for (surface, node) in self.surfaces() {
            gather(&node, &mut |node| {
                if let (Node::Week(week), None) = (node, found) {
                    found = handler(week).map(|handler| (surface, handler));
                }
            });
        }
        found.unwrap_or_else(|| panic!("no week you can tap"))
    }

    fn month(&mut self) -> (Surface, HandlerId, i32, u8) {
        let mut found = None;
        for (surface, node) in self.surfaces() {
            gather(&node, &mut |node| {
                if let (Node::Month(month), None) = (node, found) {
                    found = month.on_select.map(|handler| (surface, handler, month.year, month.month));
                }
            });
        }
        found.unwrap_or_else(|| panic!("no month you can pick a day in"))
    }

    fn ui(&mut self, surface: Surface, handler: HandlerId, value: UiValue) -> &[Request] {
        self.event(Event::Ui { surface, handler, value })
    }

    fn keep(&mut self, effects: Effects) -> &[Request] {
        self.asked.extend(effects.requests.iter().filter_map(asks));
        for request in &effects.requests {
            if let Request::Notice { notice } = request {
                self.notice_controls = notice.controls.clone();
            }
        }
        self.wants = effects.wants;
        self.last = effects.requests;
        &self.last
    }

    fn surfaces(&mut self) -> Vec<(Surface, Node)> {
        let views = self.runtime.views();
        let mut surfaces = Vec::new();
        if let Some(tile) = views.tile {
            surfaces.extend(tile.accessory.into_iter().chain(tile.footer).map(|node| (Surface::Tile, node)));
        }
        surfaces.extend(views.page.map(|page| (Surface::Page, page)));
        if let Some(live) = views.live {
            surfaces.extend([(Surface::Live, live.leading), (Surface::Live, live.trailing)]);
            surfaces.extend(live.controls.map(|controls| (Surface::Live, controls)));
        }
        surfaces
    }

    fn control(&mut self, wanted: impl Fn(&Control) -> bool, nth: usize, name: &str) -> Control {
        let mut controls = Vec::new();
        for (surface, node) in self.surfaces() {
            gather(&node, &mut |node| controls.extend(control_of(surface, node)));
        }
        let matching: Vec<&Control> = controls.iter().filter(|control| wanted(control)).collect();
        match matching.get(nth) {
            Some(control) => (*control).clone(),
            None => {
                let labels: Vec<String> = controls.iter().map(describe).collect();
                panic!("no control {name} (number {nth}); these can be clicked: {}", labels.join(", "))
            }
        }
    }

    fn input(&mut self, placeholder: &str, handler: impl Fn(&Node) -> Option<HandlerId>) -> (Surface, HandlerId) {
        let mut found = None;
        for (surface, node) in self.surfaces() {
            gather(&node, &mut |node| {
                if let Node::TextInput { placeholder: shown, .. } = node
                    && shown == placeholder
                    && found.is_none()
                {
                    found = handler(node).map(|handler| (surface, handler));
                }
            });
        }
        found.unwrap_or_else(|| panic!("no text field with the placeholder \"{placeholder}\" that listens for this"))
    }
}

fn asks(request: &Request) -> Option<u64> {
    match request {
        Request::PickColor { reply }
        | Request::CaptureScreen { reply }
        | Request::Fetch { reply, .. }
        | Request::ReadFile { reply, .. } => Some(*reply),
        Request::SaveImage { reply, .. } | Request::WriteFile { reply, .. } => *reply,
        _ => None,
    }
}

fn control_of(surface: Surface, node: &Node) -> Option<Control> {
    let (label, icon, handler, value) = match node {
        Node::Button { label, icon, on_press: Some(handler), .. } => (label, *icon, *handler, UiValue::Press),
        Node::RoundButton { label, on_press: Some(handler), .. } => (label, None, *handler, UiValue::Press),
        Node::Chip { label, icon, on, on_toggle: Some(handler), .. } => (label, *icon, *handler, UiValue::Bool(!on)),
        Node::Checkbox { label, checked, on_toggle: Some(handler) } => (label, None, *handler, UiValue::Bool(!checked)),
        _ => return None,
    };
    Some(Control { surface, label: label.clone(), icon, handler, value })
}

fn describe(control: &Control) -> String {
    match (control.label.is_empty(), control.icon) {
        (true, Some(icon)) => format!("{icon:?} icon"),
        _ => format!("\"{}\"", control.label),
    }
}

fn gather(node: &Node, visit: &mut impl FnMut(&Node)) {
    visit(node);
    for child in node.children() {
        gather(child, visit);
    }
}

fn gather_drawn(shapes: &[Shape], texts: &mut Vec<String>) {
    for shape in shapes {
        match shape {
            Shape::Text { text, .. } => texts.push(text.clone()),
            Shape::Group { shapes, .. } | Shape::Run { parts: shapes, .. } => gather_drawn(shapes, texts),
            _ => {}
        }
    }
}

fn gather_texts(node: &Node, texts: &mut Vec<String>) {
    gather(node, &mut |node| match node {
        Node::Text(text) => texts.push(text.text.clone()),
        Node::Week(week) => {
            texts.extend(week.days.iter().flat_map(|day| day.events.iter().map(|event| event.title.clone())));
        }
        Node::Canvas(canvas) => gather_drawn(&canvas.shapes, texts),
        Node::Button { label, .. } | Node::RoundButton { label, .. } | Node::Chip { label, .. }
            if !label.is_empty() =>
        {
            texts.push(label.clone());
        }
        _ => {}
    });
}
