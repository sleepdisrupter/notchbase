//! Which page shows, the order of the tabs and the motion of switching from one page to another.

use std::collections::HashMap;

use crate::bridge::{Page, Tab};
use crate::config;
use crate::notch::moves::transition::Motion;
use crate::notch::{Mode, Notch};
use crate::ui::motion;
use crate::ui::spring::{Spring, UNIT_PRECISION, smoothstep};

pub(in crate::notch) type Heights = HashMap<Page, f32>;

const MEASURE_TOLERANCE: f32 = 0.01;
const ESTIMATE: f32 = 84.0;
const SOUND: &str = "sound";
const MEDIA: &str = "media";
const FIND_POSITION: i32 = -1;
const GONE_BY: f32 = 0.6;
const SHOWN_FROM: f32 = 0.2;
const SWITCHED: f32 = 0.999;

/// The page that shows, the one it is switching from and the springs between them.
pub(in crate::notch) struct Pager {
    pub(in crate::notch) page: Page,
    pub(in crate::notch) from: Page,
    pub(in crate::notch) tab: Spring,
    switch: Spring,
    heights: Heights,
    revealed: Option<Page>,
    order: Vec<Page>,
    expanded: bool,
}

pub(in crate::notch) fn start_page(tabs: &[Tab]) -> Page {
    let wanted = config::current().notch.start_page.clone();
    let keyed = |key: &str| tabs.iter().find(|tab| tab.key == key);
    keyed(&wanted)
        .or_else(|| folded_into(&wanted).and_then(keyed))
        .or_else(|| tabs.iter().find(|tab| tab.page == Page::Tools))
        .or_else(|| tabs.first())
        .map_or(Page::Tools, |tab| tab.page)
}

fn folded_into(key: &str) -> Option<&'static str> {
    (key == SOUND).then_some(MEDIA)
}

impl Pager {
    pub(in crate::notch) fn new(page: Page, tabs: &[Tab]) -> Self {
        let order: Vec<Page> = tabs.iter().map(|tab| tab.page).collect();
        let unit = |value: f32| Spring::new(value, motion::HOVER, UNIT_PRECISION);
        let tab = unit(index_in(&order, page).unwrap_or(0) as f32);
        Self {
            page,
            from: page,
            tab,
            switch: unit(1.0),
            heights: Heights::new(),
            revealed: None,
            order,
            expanded: false,
        }
    }

    pub(in crate::notch) fn height(&self, page: Page) -> f32 {
        self.heights.get(&page).copied().unwrap_or(ESTIMATE)
    }

    pub(in crate::notch) fn progress(&self) -> f32 {
        self.switch.value.clamp(0.0, 1.0)
    }

    pub(in crate::notch) fn direction(&self) -> f32 {
        (self.position(self.page) - self.position(self.from)).signum() as f32
    }

    pub(in crate::notch) fn show(&mut self, page: Page) {
        self.from = page;
        self.page = page;
        self.switch.snap_to(1.0);
        if let Some(tab) = self.tab_index(page) {
            self.tab.snap_to(tab);
        }
    }

    fn switch_to(&mut self, page: Page) -> bool {
        if page == self.page {
            return false;
        }
        let (from, progress) = self.handover(page);
        self.from = from;
        self.page = page;
        self.switch.snap_to(progress);
        self.switch.animate_to(1.0, motion::SMOOTH);
        if let Some(tab) = self.tab_index(page) {
            self.tab.animate_to(tab, motion::BOUNCY);
        }
        true
    }

    pub(in crate::notch) fn switching(&self) -> bool {
        self.progress() < SWITCHED && self.from != self.page
    }

    fn handover(&self, to: Page) -> (Page, f32) {
        if !self.switching() {
            return (self.page, 0.0);
        }
        let (leaving, coming) = (outgoing(self.progress()), incoming(self.progress()));
        match (to == self.from, leaving > coming) {
            (true, true) => (self.page, incoming_at(leaving)),
            (true, false) | (false, false) => (self.page, outgoing_at(coming)),
            (false, true) => (self.from, outgoing_at(leaving)),
        }
    }

    pub(super) fn set_tabs(&mut self, tabs: &[Tab]) -> bool {
        let order: Vec<Page> = tabs.iter().map(|tab| tab.page).collect();
        if order == self.order {
            return false;
        }
        self.order = order;
        let fallback = self.order.first().copied().unwrap_or(Page::Tools);
        let keep = |page: Page| {
            matches!(
                page,
                Page::Settings | Page::Notifications | Page::Tools | Page::Find | Page::WhatsNew | Page::Welcome
            ) || self.order.contains(&page)
        };
        if !keep(self.page) {
            self.show(fallback);
        } else if !keep(self.from) {
            self.from = self.page;
        }
        if let Some(tab) = self.tab_index(self.page) {
            self.tab.snap_to(tab);
        }
        true
    }

    pub(super) fn hold_reveal(&mut self, page: Page, expanded: bool, exposed: f32) -> f32 {
        let held = expanded && (self.revealed == Some(page) || exposed >= 1.0);
        self.revealed = held.then_some(page);
        if held { 1.0 } else { exposed }
    }

    pub(in crate::notch) fn remeasure(&mut self, heights: Heights) -> bool {
        let changed = heights.len() != self.heights.len()
            || heights.iter().any(|(page, height)| (self.height(*page) - height).abs() > MEASURE_TOLERANCE);
        if changed {
            self.heights = heights;
        }
        changed
    }

    pub(in crate::notch) fn tab_index(&self, page: Page) -> Option<f32> {
        let home = || matches!(page, Page::Tool(_)).then(|| index_in(&self.order, Page::Tools)).flatten();
        index_in(&self.order, page).or_else(home).map(|index| index as f32)
    }

    pub(in crate::notch) fn springs(&self) -> [&Spring; 2] {
        [&self.tab, &self.switch]
    }

    pub(in crate::notch) fn springs_mut(&mut self) -> [&mut Spring; 2] {
        [&mut self.tab, &mut self.switch]
    }

    pub(in crate::notch) fn reopened(&mut self, expanded: bool) -> bool {
        let was = std::mem::replace(&mut self.expanded, expanded);
        expanded && !was
    }

    fn position(&self, page: Page) -> i32 {
        if page == Page::Find {
            return FIND_POSITION;
        }
        let at = index_in(&self.order, page).unwrap_or(self.order.len());
        i32::try_from(at).unwrap_or(i32::MAX)
    }
}

fn index_in(order: &[Page], page: Page) -> Option<usize> {
    order.iter().position(|&p| p == page)
}

pub(in crate::notch) fn outgoing(progress: f32) -> f32 {
    1.0 - smoothstep(0.0, GONE_BY, progress)
}

pub(in crate::notch) fn incoming(progress: f32) -> f32 {
    smoothstep(SHOWN_FROM, 1.0, progress)
}

fn outgoing_at(opacity: f32) -> f32 {
    GONE_BY * unsmooth(1.0 - opacity)
}

fn incoming_at(opacity: f32) -> f32 {
    SHOWN_FROM + (1.0 - SHOWN_FROM) * unsmooth(opacity)
}

fn unsmooth(eased: f32) -> f32 {
    0.5 - ((1.0 - 2.0 * eased.clamp(0.0, 1.0)).asin() / 3.0).sin()
}

impl Notch {
    pub(in crate::notch) fn switch_to(&mut self, page: Page) {
        if self.pager.switch_to(page) && self.mode == Mode::Expanded {
            self.retarget(Motion::Resize);
        }
    }

    pub(in crate::notch) fn sync_tabs(&mut self, tabs: &[Tab]) {
        if self.tabs == tabs {
            return;
        }
        self.tabs = tabs.to_vec();
        if !self.pager.set_tabs(tabs) {
            return;
        }
        if let Some(page) = self.preview.and_then(|preview| preview.page()) {
            self.pager.show(page);
        }
        if self.mode == Mode::Expanded {
            self.retarget(Motion::Resize);
        }
    }
}

#[cfg(test)]
mod tests {
    use notchbase_sdk::protocol::Icon;

    use crate::bridge::ToolSlot;
    use crate::notch::tests::fixtures::{island, settle};

    use super::*;

    #[test]
    fn switch_morphs_height() {
        let mut n = island();
        n.set_mode(Mode::Expanded, Motion::Open);
        settle(&mut n);
        let tools = n.morph.h.target;
        n.pager.heights.insert(Page::Stats, 200.0);
        n.switch_to(Page::Stats);
        assert!(n.morph.h.target > tools);
        assert_eq!(n.pager.from, Page::Tools);
        settle(&mut n);
        assert_eq!(n.pager.tab_index(Page::Stats), Some(4.0));
        assert!((n.pager.tab.value - 4.0).abs() < 1e-3);
        assert!(!n.is_animating());
    }

    #[test]
    fn settles_exactly() {
        let mut pager = Pager::new(Page::Agents, &Tab::defaults());
        assert!(pager.remeasure(Heights::from([(Page::Agents, 226.4)])));
        assert!(pager.remeasure(Heights::from([(Page::Agents, 226.0)])));
        assert_eq!(pager.height(Page::Agents), 226.0);
        assert!(!pager.remeasure(Heights::from([(Page::Agents, 226.0)])));
    }

    #[test]
    fn media_leads_tools_starts() {
        let tabs = Tab::defaults();
        let pager = Pager::new(start_page(&tabs), &tabs);
        assert_eq!(tabs[0].page, Page::Media);
        assert_eq!(pager.page, Page::Tools);
        assert_eq!(Some(pager.tab.value), pager.tab_index(Page::Tools));
    }

    #[test]
    fn sound_start_opens_tab_or_media() {
        let mut config = crate::config::Config::default();
        config.notch.start_page = SOUND.into();
        crate::config::publish(config);
        let mut tabs = Tab::defaults();
        assert_eq!(start_page(&tabs), Page::Media, "the mixer lives on the Media page");
        tabs.push(Tab::core(Page::Sound));
        assert_eq!(start_page(&tabs), Page::Sound, "a listed Sound tab still opens");
        crate::config::publish(crate::config::Config::default());
    }

    #[test]
    fn revealed_page_stays_while_growing() {
        let mut pager = Pager::new(Page::Tools, &Tab::defaults());
        assert_eq!(pager.hold_reveal(Page::Sound, true, 0.4), 0.4);
        assert_eq!(pager.hold_reveal(Page::Sound, true, 1.0), 1.0);
        assert_eq!(pager.hold_reveal(Page::Sound, true, 0.3), 1.0);
        assert_eq!(pager.hold_reveal(Page::Stats, true, 0.3), 0.3);
        assert_eq!(pager.hold_reveal(Page::Sound, true, 0.3), 0.3);
        pager.hold_reveal(Page::Sound, true, 1.0);
        assert_eq!(pager.hold_reveal(Page::Sound, false, 0.3), 0.3);
        assert_eq!(pager.hold_reveal(Page::Sound, true, 0.3), 0.3);
    }

    #[test]
    fn tool_pages_join_and_fall_back() {
        let mut tabs = Tab::defaults();
        let page = Page::Tool(ToolSlot(4));
        tabs.push(Tab { page, key: "focus-timer".into(), title: "Focus".into(), icon: Icon::Clock });
        let mut pager = Pager::new(Page::Tools, &Tab::defaults());
        assert!(pager.set_tabs(&tabs));
        pager.show(page);
        assert_eq!(pager.tab_index(page), Some(5.0));
        assert!(pager.set_tabs(&Tab::defaults()));
        assert_eq!(pager.page, Page::Media);
        assert!(!pager.set_tabs(&Tab::defaults()));
    }

    #[test]
    fn find_enters_and_leaves_left() {
        let mut pager = Pager::new(Page::Media, &Tab::defaults());
        assert!(pager.switch_to(Page::Find));
        assert_eq!(pager.direction(), -1.0, "even from the first tab");
        assert!(pager.switch_to(Page::Settings));
        assert_eq!(pager.direction(), 1.0);
        pager.show(Page::Find);
        assert!(pager.switch_to(Page::Media));
        assert_eq!(pager.direction(), 1.0, "the tab comes in from the right as Find leaves left");
    }

    #[test]
    fn hopping_on_never_flashes_back() {
        let shown = |pager: &Pager, page: Page| match page {
            _ if page == pager.page => incoming(pager.progress()),
            _ if page == pager.from && pager.switching() => outgoing(pager.progress()),
            _ => 0.0,
        };
        for frames in [1, 3, 6, 9, 14] {
            for next in [Page::Agents, Page::Tools] {
                let mut pager = Pager::new(Page::Tools, &Tab::defaults());
                pager.switch_to(Page::Shelf);
                for _ in 0..frames {
                    pager.switch.step(1.0 / 60.0);
                }
                let pages = [Page::Tools, Page::Shelf, Page::Agents];
                let before = pages.map(|page| shown(&pager, page));
                pager.switch_to(next);
                let after = pages.map(|page| shown(&pager, page));
                let brightest = (0..3).max_by(|a, b| before[*a].total_cmp(&before[*b])).unwrap_or(0);
                assert!((after[brightest] - before[brightest]).abs() < 0.01, "{frames} {next:?}: {before:?} {after:?}");
                for (was, now) in before.iter().zip(after) {
                    assert!(now < was + 0.2, "{frames} {next:?}: {before:?} -> {after:?}");
                }
            }
        }
        assert!((unsmooth(smoothstep(0.0, 1.0, 0.3)) - 0.3).abs() < 1e-4);
    }

    #[test]
    fn reopening_noticed_once() {
        let mut pager = Pager::new(Page::Tools, &Tab::defaults());
        assert!(pager.reopened(true));
        assert!(!pager.reopened(true));
        assert!(!pager.reopened(false));
        assert!(pager.reopened(true));
    }

    #[test]
    fn tabless_page_lights_tools() {
        let tabs = Tab::defaults();
        let mut pager = Pager::new(Page::Media, &tabs);
        pager.show(Page::Tool(ToolSlot(4)));
        assert_eq!(pager.tab_index(Page::Tool(ToolSlot(4))), pager.tab_index(Page::Tools));
        assert_eq!(pager.tab.value, 1.0);
    }
}
