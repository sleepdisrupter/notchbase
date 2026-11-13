//! What the open panel shows, kept between frames, and the `PanelData` built from it.

use std::time::Instant;

use crate::assistant::Provider;
use crate::audio::Mixer;
use crate::bridge::{AgentsView, AppView, FindView, Locations, Page, PanelData, Tab, WelcomeView};
use crate::config::{self, Config};
use crate::media::NowPlaying;
use crate::poll::Wake;
use crate::shelf::{Notice, Shelf};
use crate::stats::Stats;
use crate::tools::Tools;
use crate::tools::shortcuts::ShortcutView;

use super::config::assistant_of;
use super::preview::previewing;

pub(super) struct Panel {
    pub(super) mixer: Mixer,
    pub(super) stats: Stats,
    pub(super) shelf: Shelf,
    pub(super) now_playing: Option<NowPlaying>,
    pub(super) agents: AgentsView,
    pub(super) find: FindView,
    pub(super) welcome: WelcomeView,
    pub(super) dock: crate::system::dock::DockLink,
    notice: Option<(Notice, Instant)>,
    last_refresh: Option<Instant>,
}

impl Panel {
    pub(super) fn new(wake: &Wake) -> Self {
        Self {
            mixer: Mixer::new(),
            stats: Stats::new(wake.clone(), !previewing()),
            shelf: Shelf::new(wake.clone()),
            now_playing: None,
            agents: AgentsView::default(),
            find: FindView::default(),
            welcome: WelcomeView::default(),
            dock: crate::system::dock::DockLink::default(),
            notice: None,
            last_refresh: None,
        }
    }

    pub(super) fn data<'a>(
        &'a self,
        shortcuts: &'a [ShortcutView],
        tabs: &'a [Tab],
        tools: &'a Tools,
        locations: &'a Locations,
        config: &Config,
        app: AppView,
    ) -> PanelData<'a> {
        PanelData {
            shortcuts,
            tabs,
            tools: tools.views(),
            catalog: tools.catalog(),
            locations,
            images: &tools.images,
            mixer: &self.mixer,
            now_playing: self.now_playing.as_ref(),
            stats: self.stats.snapshot(),
            shelf: &self.shelf,
            shelf_notice: self.notice.as_ref().map(|(notice, _)| notice),
            agents: &self.agents,
            find: &self.find,
            welcome: &self.welcome,
            typing: false,
            assistant: assistant_of(config),
            questions_in_notch: config.assistant.questions_in_notch,
            progress_in_notch: config.assistant.progress_in_notch,
            volume_indicator: config.volume.indicator,
            brightness_indicator: config.brightness.indicator,
            all_displays: config.displays.all,
            wake_at_edge: config.notch.wake_at_edge,
            flee_from_keys: config.notch.flee_from_keys,
            haptics: config.notch.haptics,
            dodge_menus: config.notch.dodge_menus,
            snap_windows: config.snap.enabled,
            lock_screen: config.notch.lock_screen,
            app,
            dock: self.dock.view(),
        }
    }

    pub(super) fn show_notice(&mut self, notice: Notice, now: Instant) {
        self.notice = Some((notice, now));
    }

    pub(super) fn notice_expiry(&self) -> Option<Instant> {
        self.notice.as_ref().map(|(_, shown)| *shown + config::current().notices.shelf())
    }

    pub(super) fn expire_notice(&mut self, now: Instant) -> bool {
        let expired = self.notice_expiry().is_some_and(|at| now >= at);
        if expired {
            self.notice = None;
        }
        expired
    }

    pub(super) fn refresh(&mut self, page: Option<Page>, assistant: Option<Provider>, now: Instant) -> bool {
        if page == Some(Page::Settings) {
            self.dock.refresh(now);
        }
        let Some(page) = page else {
            self.last_refresh = None;
            return false;
        };
        let due = self.last_refresh.is_none_or(|at| now.duration_since(at) >= config::current().stats.refresh());
        if due {
            self.last_refresh = Some(now);
            self.sample(page, assistant);
        }
        self.poll_changes(page, due)
    }

    fn sample(&mut self, page: Page, assistant: Option<Provider>) {
        match page {
            Page::Sound | Page::Tools | Page::Media => self.mixer.refresh(),
            Page::Stats => self.stats.sample(assistant),
            Page::Shelf => {
                self.shelf.prune_missing();
            }
            Page::Agents
            | Page::Settings
            | Page::Notifications
            | Page::Find
            | Page::WhatsNew
            | Page::Welcome
            | Page::Tool(_) => {}
        }
    }

    fn poll_changes(&mut self, page: Page, sampled: bool) -> bool {
        match page {
            Page::Sound | Page::Tools | Page::Media => self.mixer.poll(),
            Page::Stats => sampled,
            Page::Shelf => sampled || self.shelf.busy().is_some(),
            Page::Agents
            | Page::Settings
            | Page::Notifications
            | Page::Find
            | Page::WhatsNew
            | Page::Welcome
            | Page::Tool(_) => false,
        }
    }
}
