//! The system watchers (battery, lock screen, sensors, downloads, Focus, menus) and keeping awake, each
//! switched on only while something needs it.

mod bluetooth;
mod calendar;
mod volume;

use std::time::Instant;

use crate::bridge::{Action, NotificationKind, NotificationView, ToolSlot};
use crate::platform::{self, KnownDir};
use crate::system::awake::Awake;
use crate::system::downloads::Downloads;
use crate::system::focus::{FocusWatch, Reading};
use crate::system::menus::{MenuWatch, edge_in};
use crate::system::permissions::{self, Access};
use crate::system::power::Battery;
use crate::system::sensors::SensorWatch;
use crate::system::session::ScreenLock;

use super::App;
use super::preview::previewing;

const NOT_AWAKE: &str = "Couldn't keep this computer awake";
const NO_LID: &str = "Couldn't keep this Mac awake with the lid closed";
const FULL_DISK_ACCESS: &str = "x-apple.systempreferences:com.apple.preference.security?Privacy_AllFiles";
const ASK_FOR_FOCUS: &str = "Allow Full Disk Access for notchbase to show your Focus";
const NOT_ASKED: &str = "Couldn't remember asking for Full Disk Access";
const NO_SETTINGS: &str = "Couldn't open System Settings";
const PERMISSION_KEY: &str = "full-disk-access";
const PERMISSION_TITLE: &str = "Show your Focus";
const PERMISSION_DETAIL: &str =
    "macOS only shows apps your Focus with Full Disk Access. Without it, notchbase skips Focus.";
const OPEN_SETTINGS: &str = "Open Settings";

#[derive(Default)]
pub(super) struct System {
    battery: Option<Battery>,
    lock: Option<ScreenLock>,
    locked: bool,
    sensors: Option<SensorWatch>,
    downloads: Option<Downloads>,
    focus: Option<FocusWatch>,
    menus: Option<MenuWatch>,
    menu_items: Vec<egui::Rect>,
    focus_hidden: bool,
    asked_this_run: bool,
    awake: Awake<ToolSlot>,
    lid_floor: bool,
    lid_busy: bool,
    lid_tried: Option<bool>,
}

impl App {
    pub(super) fn sync_system(&mut self) {
        if previewing() {
            return;
        }
        self.sync_power();
        self.sync_screen_lock();
        self.sync_sensors();
        self.sync_downloads();
        self.sync_focus();
        self.sync_menus();
    }

    pub(super) fn permission_notification(&self) -> Option<NotificationView> {
        (self.system.focus_hidden && self.system.asked_this_run).then(|| NotificationView {
            key: PERMISSION_KEY.to_owned(),
            kind: NotificationKind::Permission,
            title: PERMISSION_TITLE.to_owned(),
            detail: PERMISSION_DETAIL.to_owned(),
            progress: None,
            action: Some((OPEN_SETTINGS.to_owned(), Action::AllowFullDiskAccess)),
        })
    }

    pub(super) fn allow_full_disk_access(&mut self) {
        if let Err(err) = platform::open(FULL_DISK_ACCESS) {
            self.report(NO_SETTINGS, err);
        }
    }

    fn sync_menus(&mut self) {
        let wanted = cfg!(target_os = "macos") && self.config.config().notch.dodge_menus;
        follow(&mut self.system.menus, wanted, MenuWatch::start);
        if let Some(items) = self.system.menus.as_ref().and_then(MenuWatch::take) {
            self.system.menu_items = items;
        }
        let items = if wanted { self.system.menu_items.as_slice() } else { &[] };
        for (_, nw) in self.notches.windows_mut() {
            let width = nw.notch.canvas_size().x;
            let edge = platform::window_origin(nw.window()).and_then(|origin| edge_in(items, origin, width));
            nw.notch.set_menu_edge(edge);
        }
    }

    fn sync_focus(&mut self) {
        follow(&mut self.system.focus, self.tools.wants_focus(), FocusWatch::start);
        match self.system.focus.as_ref().and_then(FocusWatch::take) {
            Some(Reading::Seen(focus)) => {
                self.system.focus_hidden = false;
                self.tools.feed_focus(focus);
            }
            Some(Reading::Unreadable) => {
                self.system.focus_hidden = true;
                self.ask_for_full_disk_access();
            }
            None => {}
        }
    }

    fn ask_for_full_disk_access(&mut self) {
        if self.config.config().permissions.full_disk_asked || !permissions::may_ask(Access::FullDisk) {
            return;
        }
        if let Err(err) = self.config.update(|config| config.permissions.full_disk_asked = true) {
            self.report(NOT_ASKED, err);
        }
        self.system.asked_this_run = true;
        self.notify(ASK_FOR_FOCUS);
    }

    pub(super) fn sync_forced_awake(&mut self, now: Instant) {
        let forced = self.config.config().awake.force;
        if forced != self.system.awake.is_forced() && !self.system.awake.force(forced) {
            self.notify(NOT_AWAKE);
        }
        self.system.awake.renew(now);
        self.sync_lid_floor();
    }

    pub(super) fn keep_awake(&mut self, slot: ToolSlot, awake: bool) {
        if !self.system.awake.set(slot, awake) {
            self.notify(NOT_AWAKE);
        }
    }

    fn sync_lid_floor(&mut self) {
        if !cfg!(target_os = "macos") {
            return;
        }
        let wanted = self.system.awake.is_awake() && self.config.config().awake.hold_lid_closed;
        if wanted == self.system.lid_floor || self.system.lid_busy || self.system.lid_tried == Some(wanted) {
            return;
        }
        self.system.lid_busy = true;
        self.system.lid_tried = Some(wanted);
        self.request_lid_floor(wanted);
    }

    #[cfg(target_os = "macos")]
    fn request_lid_floor(&mut self, disabled: bool) {
        let send = super::events::sender(&self.proxy);
        crate::system::awake::request_lid_floor(disabled, move |ok| {
            send(super::events::UserEvent::LidFloor { enabled: disabled, ok })
        });
    }

    #[cfg(not(target_os = "macos"))]
    fn request_lid_floor(&mut self, _disabled: bool) {}

    pub(super) fn lid_floor_result(&mut self, enabled: bool, ok: bool) {
        self.system.lid_busy = false;
        if ok {
            self.system.lid_floor = enabled;
            self.system.lid_tried = None;
        } else if enabled {
            self.notify(NO_LID);
        }
    }

    pub(super) fn mute_microphone(&mut self, muted: bool) {
        self.system.sensors.get_or_insert_with(SensorWatch::start).mute(muted);
    }

    fn let_go_before_locking(&mut self) {
        if self.keys.is_some() {
            self.stop_recording();
        }
        if self.typing {
            self.release_typing();
        }
        self.cancel_capture();
    }

    pub(super) fn let_computer_sleep(&mut self) {
        self.system.awake.release_all();
    }

    fn sync_power(&mut self) {
        follow(&mut self.system.battery, self.tools.wants_power(), Battery::start);
        if let Some(power) = self.system.battery.as_ref().and_then(Battery::take) {
            self.tools.feed_power(power);
        }
    }

    fn sync_screen_lock(&mut self) {
        let wanted = self.tools.wants_screen_lock() || self.config.config().notch.lock_screen;
        follow(&mut self.system.lock, wanted, ScreenLock::start);
        if let Some(locked) = self.system.lock.as_ref().and_then(ScreenLock::take) {
            self.system.locked = locked;
            self.tools.feed_screen_lock(locked);
            if locked {
                self.let_go_before_locking();
            }
            self.update_notch(|notch| notch.set_locked(locked));
        }
    }

    pub(super) fn screen_locked(&self) -> bool {
        self.system.lock.is_some() && self.system.locked
    }

    fn sync_sensors(&mut self) {
        follow(&mut self.system.sensors, self.tools.wants_sensors(), SensorWatch::start);
        if let Some(sensors) = self.system.sensors.as_ref().and_then(SensorWatch::take) {
            self.tools.feed_sensors(sensors);
        }
    }

    fn sync_downloads(&mut self) {
        let wanted = self.tools.wants_downloads();
        if wanted != self.system.downloads.is_some() {
            let folder = platform::known_dir(KnownDir::Downloads).filter(|_| wanted);
            self.system.downloads = folder.map(Downloads::start);
        }
        let changes = self.system.downloads.as_ref().map(Downloads::take_changes).unwrap_or_default();
        for change in changes {
            self.tools.feed_download(change);
        }
    }
}

fn follow<T>(service: &mut Option<T>, wanted: bool, start: impl FnOnce() -> T) {
    if wanted != service.is_some() {
        *service = wanted.then(start);
    }
}
