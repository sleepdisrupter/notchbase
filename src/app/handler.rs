//! winit's event handler: opens the notch windows, routes window and user events, and sets up previews.

use std::time::{Duration, Instant};

use notchbase_sdk::protocol::{HandlerId, Icon, Node, Surface, UiValue, Value};
use winit::application::ApplicationHandler;
use winit::event::{StartCause, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow};
use winit::window::WindowId;

use crate::bridge::{Page, ToolSlot};
use crate::capture::CaptureMode;
use crate::stall::{self, OUTSIDE, Stage};

use super::App;
use super::events::UserEvent;
use super::preview::preview_name;

const PREVIEW_SHELF_FILES_VAR: &str = "NOTCHBASE_SHELF_FILES";
/// How often notchbase looks again for a screen while it has none to open its notch on.
const SCREEN_RETRY: Duration = Duration::from_secs(1);

static USER_EVENT: Stage = Stage("a message from a worker (user_event)");
static WINDOW_EVENT: Stage = Stage("a window event");
static PAINT: Stage = Stage("painting a notch");
static FRAME: Stage = Stage("the frame (about_to_wait)");

impl ApplicationHandler<UserEvent> for App {
    fn new_events(&mut self, _event_loop: &ActiveEventLoop, _cause: StartCause) {
        stall::woke();
    }

    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if !self.notches.is_empty() || self.waiting_for_screen {
            return;
        }
        if let Err(err) = self.open_notches(event_loop) {
            // Locked, or with every display off, macOS can list no screens at all. Quitting here
            // would leave no notchbase once the person is back, so wait for a screen instead.
            eprintln!("notchbase: no notch window yet ({err}); trying again until a screen appears");
            self.waiting_for_screen = true;
            wait_for_screen(event_loop);
            return;
        }
        self.start_up(event_loop);
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: UserEvent) {
        stall::during(&USER_EVENT, &OUTSIDE, || self.handle_user_event(event_loop, event));
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        let stage = if event == WindowEvent::RedrawRequested { &PAINT } else { &WINDOW_EVENT };
        stall::during(stage, &OUTSIDE, || self.handle_window_event(event_loop, id, event));
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        stall::enter(&FRAME);
        if self.waiting_for_screen {
            if self.open_notches(event_loop).is_err() {
                wait_for_screen(event_loop);
                return;
            }
            self.waiting_for_screen = false;
            self.start_up(event_loop);
        }
        self.run_frame(event_loop);
    }
}

fn wait_for_screen(event_loop: &ActiveEventLoop) {
    let at = Instant::now() + SCREEN_RETRY;
    event_loop.set_control_flow(ControlFlow::WaitUntil(at));
    stall::sleep_until(at);
}

impl App {
    fn handle_user_event(&mut self, event_loop: &ActiveEventLoop, event: UserEvent) {
        match event {
            UserEvent::HotKey(event) => self.on_hotkey(event_loop, event),
            UserEvent::Wake => {
                super::events::woke();
                self.wake(event_loop);
            }
            UserEvent::ScreenshotNotCopied(err) => self.screenshot_not_copied(&err),
            UserEvent::FolderPicked(slot, key, path) => {
                self.set_tool_setting(slot, key, Value::String(path.to_string_lossy().into_owned()));
            }
            UserEvent::LidFloor { enabled, ok } => self.lid_floor_result(enabled, ok),
            UserEvent::Summon => self.summon_find(),
            UserEvent::Quit => event_loop.exit(),
        }
    }

    fn handle_window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        if let WindowEvent::Focused(focused) = event {
            self.annotating.another_window_focused(id, focused);
        }
        if self.owns_notch(id) {
            self.notch_window_event(event_loop, id, event);
        } else if self.snap.owns(id) {
            self.snap.window_event(&event);
        } else if self.recording_dim.owns(id) {
            self.recording_dim.window_event(&event);
        } else if self.annotating.owns(id) {
            self.annotating.window_event(&event);
        } else {
            self.capture_window_event(id, &event);
        }
    }

    /// Everything that starts once the first notch is open.
    fn start_up(&mut self, event_loop: &ActiveEventLoop) {
        for message in std::mem::take(&mut self.startup_messages) {
            self.notify(&message);
        }
        self.setup_hotkeys();
        self.start_tools(event_loop);
        self.start_assistant();
        self.announce_changes();
        self.open_welcome_at_start();
        self.start_preview(event_loop);
    }

    fn wake(&mut self, event_loop: &ActiveEventLoop) {
        self.sync_screen_recording();
        self.pump_tools(event_loop, Instant::now());
        self.panel.stats.sync();
        self.sync_shelf(Instant::now());
        self.sync_prompts();
        self.sync_shows();
        self.sync_find();
        self.sync_system_key();
        self.redraw_notch();
    }

    fn notch_window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::RedrawRequested => self.paint_notch(event_loop, id),
            WindowEvent::CloseRequested => {}
            event => {
                self.track_drop(id, &event);
                if let Some(nw) = self.notches.window_mut(id) {
                    nw.on_event(&event);
                }
                if event == WindowEvent::Focused(false) {
                    self.notch_lost_focus(id);
                }
            }
        }
    }

    fn start_preview(&mut self, event_loop: &ActiveEventLoop) {
        if let Some(history) = self.notches.iter().find_map(|nw| nw.notch.previewed_history()) {
            self.panel.stats.pin_history(history);
        }
        if let Some((storage, _, _)) = self.notches.iter().find_map(|nw| nw.notch.previewed_storage()) {
            self.panel.stats.pin_storage(storage);
        }
        if let Some((by, apps)) = self.notches.iter().find_map(|nw| nw.notch.previewed_apps()) {
            self.panel.stats.pin_apps(by, apps);
        }
        match preview_name().as_deref() {
            Some("picker") => self.request_capture(event_loop, CaptureMode::Color),
            Some("screenshot") => self.request_capture(event_loop, CaptureMode::Region),
            Some("shot") => self.tools.images.show_sample(crate::notch::sample_thumbnail()),
            Some("annotate") => self.preview_annotator(event_loop),
            Some("shelf" | "shelf-airdrop" | "airdrop") => self.preview_shelf(),
            Some("recording" | "recording-tools") => self.preview_screen_recording(),
            Some("recording-area") => {
                self.preview_screen_recording();
                self.preview_recording_dim(event_loop);
            }
            Some(preview @ ("shortcut" | "shortcut-taken")) => self.preview_shortcut(preview == "shortcut-taken"),
            Some("calendar") => {
                self.preview_tool_page(::calendar::ID);
            }
            Some(preview @ ("calendar-new" | "calendar-dates")) => {
                if let Some(slot) = self.preview_tool_page(::calendar::ID) {
                    self.press_page_icon(event_loop, slot, Icon::Plus);
                    if preview == "calendar-dates" {
                        self.press_page_icon(event_loop, slot, Icon::ChevronDown);
                    }
                }
            }
            _ => {}
        }
    }

    fn preview_tool_page(&mut self, id: &str) -> Option<ToolSlot> {
        let slot = self.tools.hosts().iter().find(|host| host.id() == id).map(|host| host.slot)?;
        self.update_notch(|notch| notch.open_page(Page::Tool(slot)));
        self.hold_for(slot, true);
        Some(slot)
    }

    fn press_page_icon(&mut self, event_loop: &ActiveEventLoop, slot: ToolSlot, wanted: Icon) {
        let page = self.tools.views().iter().find(|view| view.slot == slot).and_then(|view| view.page.clone());
        let handler = page.as_ref().and_then(|page| icon_button(page, wanted));
        if let Some(handler) = handler {
            self.tool_event(event_loop, slot, Surface::Page, handler, UiValue::Press);
        }
    }

    fn preview_shelf(&mut self) {
        if let Some(files) = std::env::var_os(PREVIEW_SHELF_FILES_VAR) {
            self.panel.shelf.add(std::env::split_paths(&files).collect());
        }
    }
}

fn icon_button(node: &Node, wanted: Icon) -> Option<HandlerId> {
    match node {
        Node::Button { icon: Some(icon), on_press: Some(handler), .. } if *icon == wanted => Some(*handler),
        other => other.children().into_iter().find_map(|child| icon_button(child, wanted)),
    }
}
