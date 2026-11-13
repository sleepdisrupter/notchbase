//! Runs a `Tool`: routes events to messages, applies commands, and keeps the handler tables that
//! match a press in a view back to its message.

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex, PoisonError};

use crate::command::{Command, Effect};
use crate::driver::{Driver, Emitter, Waker};
use crate::element::Handlers;
use crate::protocol::{Effects, Event, Manifest, NoticeNode, Reply, Request, Start, Surface, Views};
use crate::settings::Settings;
use crate::subscription::Subscription;
use crate::tool::{Context, Tool};

type Answer<Msg> = Box<dyn FnOnce(Reply) -> Option<Msg>>;
type Inbox<Msg> = Arc<Mutex<Vec<Msg>>>;

pub struct Runtime<T: Tool> {
    tool: Option<T>,
    waker: Waker,
    inbox: Inbox<T::Msg>,
    asks: HashMap<u64, Answer<T::Msg>>,
    next_ask: u64,
    handlers: HashMap<Surface, Handlers<T::Msg>>,
    subscription: Subscription<T::Msg>,
    settings: Settings,
    requests: Vec<Request>,
    revision: u64,
}

/// A bundled tool as the driver the host talks to.
pub fn boxed<T: Tool>(waker: Waker) -> Box<dyn Driver> {
    Box::new(Runtime::<T>::new(waker))
}

impl<T: Tool> Runtime<T> {
    pub fn new(waker: Waker) -> Self {
        Self {
            tool: None,
            waker,
            inbox: Arc::default(),
            asks: HashMap::new(),
            next_ask: 1,
            handlers: HashMap::new(),
            subscription: Subscription::none(),
            settings: Settings::default(),
            requests: Vec::new(),
            revision: 0,
        }
    }

    pub fn tool(&self) -> Option<&T> {
        self.tool.as_ref()
    }

    pub fn settings(&self) -> &Settings {
        &self.settings
    }

    pub fn send(&mut self, msg: T::Msg) -> Effects {
        self.run(Command::message(msg));
        self.settle()
    }

    fn run(&mut self, first: Command<T::Msg>) {
        let mut queue = VecDeque::new();
        self.apply(first, &mut queue);
        while let Some(msg) = queue.pop_front() {
            let Some(tool) = &mut self.tool else { return };
            let command = tool.update(msg);
            self.revision += 1;
            self.apply(command, &mut queue);
        }
    }

    fn apply(&mut self, command: Command<T::Msg>, queue: &mut VecDeque<T::Msg>) {
        for effect in command.effects {
            match effect {
                Effect::Message(msg) => queue.push_back(msg),
                Effect::Request(request) => {
                    match request.as_ref() {
                        Request::SaveSetting { key, value } => self.settings.set(key, value.clone()),
                        Request::Notice { .. } => self.forget_notice_controls(),
                        _ => {}
                    }
                    self.requests.push(*request);
                }
                Effect::Ask { build, answer } => {
                    let id = self.next_ask;
                    self.next_ask += 1;
                    self.asks.insert(id, answer);
                    self.requests.push(build(id));
                }
                Effect::Notice { notice, controls } => {
                    let mut handlers = Handlers::default();
                    let controls = Some(controls.lower(&mut handlers));
                    self.handlers.insert(Surface::Notice, handlers);
                    self.requests.push(Request::Notice { notice: NoticeNode { controls, ..notice.into_node() } });
                }
                Effect::Task(task) => self.spawn(task),
            }
        }
    }

    fn forget_notice_controls(&mut self) {
        self.handlers.remove(&Surface::Notice);
    }

    fn spawn(&self, task: Box<dyn FnOnce(Emitter<T::Msg>) + Send>) {
        let (inbox, waker) = (self.inbox.clone(), self.waker.clone());
        let emitter = Emitter::new(move |msg| {
            inbox.lock().unwrap_or_else(PoisonError::into_inner).push(msg);
            waker.wake();
        });
        run_task(T::manifest().id, move || task(emitter));
    }

    fn route(&mut self, event: Event) -> Vec<T::Msg> {
        match event {
            Event::Ui { surface, handler, value } => {
                self.handlers.get(&surface).and_then(|handlers| handlers.dispatch(handler, value)).into_iter().collect()
            }
            Event::Reply { id, reply } => self.asks.remove(&id).and_then(|answer| answer(reply)).into_iter().collect(),
            Event::Settings { values } => {
                self.settings = Settings::over(T::manifest().defaults(), values.clone());
                self.subscription.route(&Event::Settings { values }, &self.settings)
            }
            event => self.subscription.route(&event, &self.settings),
        }
    }

    fn settle(&mut self) -> Effects {
        let pending = std::mem::take(&mut *self.inbox.lock().unwrap_or_else(PoisonError::into_inner));
        for msg in pending {
            self.run(Command::message(msg));
        }
        if let Some(tool) = &self.tool {
            self.subscription = tool.subscriptions();
        }
        Effects {
            requests: std::mem::take(&mut self.requests),
            wants: self.subscription.wants(),
            revision: self.revision,
        }
    }
}

impl<T: Tool> Driver for Runtime<T> {
    fn manifest(&self) -> Manifest {
        T::manifest()
    }

    fn start(&mut self, start: Start) -> Effects {
        self.settings = Settings::over(T::manifest().defaults(), start.settings.clone());
        let (tool, command) = T::init(&Context::new(start, self.settings.clone()));
        self.tool = Some(tool);
        self.revision += 1;
        self.run(command);
        self.settle()
    }

    fn handle(&mut self, events: Vec<Event>) -> Effects {
        for event in events {
            for msg in self.route(event) {
                self.run(Command::message(msg));
            }
        }
        self.settle()
    }

    fn views(&mut self) -> Views {
        let Some(tool) = &self.tool else { return Views::default() };
        let mut tile_handlers = Handlers::default();
        let mut page_handlers = Handlers::default();
        let mut live_handlers = Handlers::default();
        let views = Views {
            tile: tool.tile().map(|tile| tile.lower(&mut tile_handlers)),
            page: tool.page().map(|page| page.lower(&mut page_handlers)),
            live: tool.live().map(|live| live.lower(&mut live_handlers)),
        };
        self.handlers.insert(Surface::Tile, tile_handlers);
        self.handlers.insert(Surface::Page, page_handlers);
        self.handlers.insert(Surface::Live, live_handlers);
        views
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn run_task(id: String, task: impl FnOnce() + Send + 'static) {
    let spawned = std::thread::Builder::new().name(format!("notchbase-tool-{id}")).spawn(task);
    if let Err(err) = spawned {
        eprintln!("notchbase: {id} couldn't start a task: {err}");
    }
}

#[cfg(target_arch = "wasm32")]
fn run_task(_: String, task: impl FnOnce() + Send + 'static) {
    task();
}
