//! Every page's state in one place, with the calls the notch uses to route to a page, measure it and draw it.

use std::collections::HashMap;

use crate::bridge::{Action, FindView, Page, PanelData};
use crate::pages::PageCtx;
use crate::pages::agents::AgentsPage;
use crate::pages::find::FindPage;
use crate::pages::media::MediaPage;
use crate::pages::mixer::MixerPage;
use crate::pages::notifications::NotificationsPage;
use crate::pages::settings::SettingsPage;
use crate::pages::shelf::ShelfPage;
use crate::pages::stats::StatsPage;
use crate::pages::tool::ToolPages;
use crate::pages::tools::{Frame, KeyCapture, ToolsPage};
use crate::pages::welcome::WelcomePage;
use crate::pages::whats_new::WhatsNewPage;
use crate::platform::Style;
use crate::ui::element::Measure;
use crate::ui::spring::{POINT_PRECISION, Spring, SpringConfig};
use crate::whats_new::Release;

use super::pager::Heights;

const GLIDE: SpringConfig = SpringConfig::new(0.18, 1.0);
const SHELF_ESTIMATE: f32 = 76.0;
const MIXER_ESTIMATE: f32 = 60.0;
const MEDIA_ESTIMATE: f32 = 84.0;
pub(super) const LAYERS: usize = 2;

pub(in crate::notch) struct Notes(pub(in crate::notch) Option<&'static Release>);

impl Default for Notes {
    fn default() -> Self {
        Self(crate::whats_new::latest())
    }
}

#[derive(Default)]
pub(in crate::notch) struct Pages {
    pub(in crate::notch) tools: ToolsPage,
    pub(in crate::notch) capture: KeyCapture,
    tool_pages: ToolPages,
    shelf: ShelfPage,
    mixer: MixerPage,
    media: MediaPage,
    stats: StatsPage,
    agents: AgentsPage,
    settings: SettingsPage,
    notifications: NotificationsPage,
    pub(in crate::notch) find: FindPage,
    whats_new: WhatsNewPage,
    welcome: WelcomePage,
    pub(in crate::notch) notes: Notes,
    pub(in crate::notch) natural: Heights,
    pub(in crate::notch) scroll: HashMap<Page, f32>,
    glide: Option<(Page, Spring)>,
    settings_view: String,
    settings_scroll: HashMap<String, f32>,
    pub(in crate::notch) drawn: [Option<Page>; LAYERS],
}

impl Pages {
    pub(in crate::notch) fn estimated_heights(&self) -> Heights {
        Heights::from([
            (Page::Media, MEDIA_ESTIMATE),
            (Page::Tools, ToolsPage::estimate()),
            (Page::Shelf, SHELF_ESTIMATE),
            (Page::Sound, MIXER_ESTIMATE),
            (Page::Stats, StatsPage::estimate()),
            (Page::Agents, AgentsPage::estimate()),
        ])
    }

    pub(in crate::notch) fn measure(&self, data: &PanelData, width: f32, measure: &Measure) -> Heights {
        let mut heights = Heights::from([
            (Page::Media, self.media.height(data.now_playing, data.mixer, width)),
            (Page::Tools, self.tools.height(data.tools)),
            (Page::Shelf, self.shelf.height(data.shelf, width)),
            (Page::Sound, self.mixer.height(data.mixer, width)),
            (Page::Stats, self.stats.height(data.stats, data.assistant)),
            (Page::Agents, self.agents.height(data.agents)),
            (Page::Settings, self.settings.height(data, width, measure.canvas)),
            (Page::Notifications, self.notifications.height(data)),
            (Page::Find, self.find.height(data)),
            (
                Page::WhatsNew,
                self.whats_new.height(self.notes.0, width, beside_camera(measure.env.style), measure.canvas),
            ),
            (Page::Welcome, WelcomePage::height(measure.canvas)),
        ]);
        for tool in data.tools.iter().filter(|tool| tool.page.is_some()) {
            let measure = Measure { slot: tool.slot, ..*measure };
            heights.insert(Page::Tool(tool.slot), self.tool_pages.height(Some(tool), &measure, width));
        }
        heights
    }

    pub(in crate::notch) fn is_animating(&self) -> bool {
        self.tools.is_animating()
            || self.capture.is_animating()
            || self.tool_pages.is_animating()
            || self.glide.is_some()
            || self.drawn.into_iter().flatten().any(|page| self.animates(page))
    }

    pub(in crate::notch) fn step(&mut self, dt: f32) {
        self.tools.step(dt);
        self.tool_pages.step(dt);
        self.capture.step(dt);
        if let Some((_, offset)) = &mut self.glide {
            offset.step(dt);
            if offset.is_settled() {
                self.glide = None;
            }
        }
    }

    pub(in crate::notch) fn snap(&mut self) {
        self.tools.snap();
        self.tool_pages.snap();
        self.capture.snap();
        self.glide = None;
    }

    pub(super) fn shown_scroll(&self, page: Page, target: f32) -> f32 {
        match &self.glide {
            Some((gliding, offset)) if *gliding == page => offset.value,
            _ => target,
        }
    }

    pub(super) fn glide_to(&mut self, page: Page, from: f32, to: f32) {
        match &mut self.glide {
            Some((gliding, offset)) if *gliding == page => offset.animate_to(to, GLIDE),
            _ => {
                let mut offset = Spring::new(from, GLIDE, POINT_PRECISION);
                offset.animate_to(to, GLIDE);
                self.glide = Some((page, offset));
            }
        }
    }

    pub(super) fn stop_gliding(&mut self, page: Page) {
        if self.glide.as_ref().is_some_and(|(gliding, _)| *gliding == page) {
            self.glide = None;
        }
    }

    fn animates(&self, page: Page) -> bool {
        match page {
            Page::Tools => self.tools.is_animating(),
            Page::Shelf => self.shelf.is_animating(),
            Page::Sound => self.mixer.is_animating(),
            Page::Media => self.media.is_animating(),
            Page::Stats => self.stats.is_animating(),
            Page::Agents => self.agents.is_animating(),
            Page::Find => self.find.is_animating(),
            Page::WhatsNew => self.whats_new.is_animating(),
            Page::Welcome => self.welcome.is_animating(),
            Page::Settings => self.settings.is_animating(),
            Page::Notifications => self.notifications.is_animating(),
            Page::Tool(_) => false,
        }
    }

    pub(in crate::notch) fn show_assistant_settings(&mut self) {
        self.settings.show_assistant();
    }

    pub(in crate::notch) fn show_dock_settings(&mut self) {
        self.settings.show_dock();
    }

    pub(in crate::notch) fn search_settings(&mut self, query: &str) {
        self.settings.search_for(query);
    }

    pub(in crate::notch) fn unfold_mixer(&mut self) {
        self.media.unfold_mixer();
    }

    pub(in crate::notch) fn open_agent_row(&mut self, id: u64) {
        self.agents.pin_open(id);
    }

    pub(in crate::notch) fn stage_formats(&mut self) {
        self.shelf.stage_formats();
    }

    pub(in crate::notch) fn stage_airdrop(&mut self) {
        self.shelf.stage_airdrop();
    }

    pub(in crate::notch) fn show_tool_list(&mut self) {
        self.settings.show_tools();
    }

    pub(in crate::notch) fn show_tool_settings(&mut self, id: &str) {
        self.settings.show_tool(id);
    }

    pub(in crate::notch) fn pin_welcome(&mut self, view: crate::bridge::WelcomeView) {
        self.welcome.pin(view);
    }

    pub(in crate::notch) fn pin_find(&mut self, view: FindView, selected: Option<usize>) {
        self.find.pin(view, selected);
    }

    pub(super) fn keep_settings_scroll(&mut self) {
        let view = self.settings.view_key();
        if view == self.settings_view {
            return;
        }
        let left = std::mem::replace(&mut self.settings_view, view);
        self.stop_gliding(Page::Settings);
        let offset = self.scroll.remove(&Page::Settings).unwrap_or(0.0);
        self.settings_scroll.insert(left, offset);
        if let Some(kept) = self.settings_scroll.get(&self.settings_view) {
            self.scroll.insert(Page::Settings, *kept);
        }
    }

    pub(super) fn route(&mut self) -> Option<Page> {
        if self.settings.take_notifications() {
            return Some(Page::Notifications);
        }
        self.notifications.take_back().then_some(Page::Settings)
    }

    pub(super) fn take_changes_done(&mut self) -> bool {
        self.whats_new.take_done()
    }

    pub(super) fn take_welcome_done(&mut self) -> Option<bool> {
        self.welcome.take_done()
    }

    pub(in crate::notch) fn holds_open(&self, page: Page) -> bool {
        matches!(page, Page::Settings | Page::Notifications | Page::Welcome)
    }

    pub(in crate::notch) fn hide(&mut self) -> Vec<Action> {
        let mut actions = Vec::new();
        self.tools.rest(&mut actions);
        self.tool_pages.rest(&mut actions);
        self.settings.rest(&mut actions);
        self.shelf.rest();
        self.find.rest(&mut actions);
        self.whats_new.rest();
        self.welcome.rest();
        self.scroll.remove(&Page::WhatsNew);
        self.glide = None;
        self.drawn = [None; LAYERS];
        actions
    }

    pub(in crate::notch) fn let_go(&mut self) -> Vec<Action> {
        let mut committed = Vec::new();
        self.tools.let_go();
        self.tool_pages.let_go();
        self.settings.let_go(&mut committed);
        self.find.drop_quietly();
        committed
    }

    pub(in crate::notch) fn ui(
        &mut self,
        page: Page,
        ctx: &mut PageCtx<'_>,
        data: &PanelData,
        frame: Frame<'_>,
    ) -> Vec<Action> {
        match page {
            Page::Tools => self.tools.ui(ctx, data.tools, frame),
            Page::Tool(slot) => self.tool_pages.ui(ctx, data.tool(slot), frame),
            Page::Shelf => self.shelf.ui(ctx, data.shelf, data.shelf_notice),
            Page::Sound => self.mixer.ui(ctx, data.mixer),
            Page::Media => self.media.ui(ctx, data.now_playing, data.mixer),
            Page::Stats => self.stats.ui(ctx, data.stats, data.assistant),
            Page::Agents => self.agents.ui(ctx, data.agents, data.assistant),
            Page::Settings => self.settings.ui(ctx, data),
            Page::Notifications => self.notifications.ui(ctx, data),
            Page::Find => self.find.ui(ctx, data),
            Page::WhatsNew => self.whats_new.ui(ctx, self.notes.0, beside_camera(frame.env.style)),
            Page::Welcome => self.welcome.ui(ctx, data.welcome),
        }
    }

    pub(in crate::notch) fn natural(&self, page: Page, shown: f32) -> f32 {
        self.natural.get(&page).copied().unwrap_or(shown).max(shown)
    }
}

fn beside_camera(style: Style) -> bool {
    style == Style::Notch
}
