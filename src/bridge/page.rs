//! The panel's pages and the tabs that open them.

use notchbase_sdk::protocol::Icon;

use crate::config::DEFAULT_TABS;

use super::ToolSlot;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Page {
    Media,
    Tools,
    Shelf,
    Sound,
    Stats,
    Agents,
    Settings,
    Notifications,
    Find,
    WhatsNew,
    Welcome,
    Tool(ToolSlot),
}

impl Page {
    pub const CORE: [Page; 6] = [Page::Media, Page::Tools, Page::Shelf, Page::Sound, Page::Stats, Page::Agents];

    pub fn core_title(self) -> &'static str {
        match self {
            Page::Tools => "Tools",
            Page::Shelf => "Shelf",
            Page::Sound => "Sound",
            Page::Media => "Media",
            Page::Stats => "Stats",
            Page::Agents => "Agents",
            Page::Settings => "Settings",
            Page::Notifications => "Notifications",
            Page::Find => "Find",
            Page::WhatsNew => "What's New",
            Page::Welcome => "Welcome",
            Page::Tool(_) => "",
        }
    }

    pub fn core_icon(self) -> Icon {
        match self {
            Page::Media => Icon::Note,
            Page::Tools => Icon::Grid,
            Page::Shelf => Icon::Tray,
            Page::Sound => Icon::Speaker,
            Page::Stats => Icon::Gauge,
            Page::Agents => Icon::Spark,
            Page::Settings => Icon::Gear,
            Page::Notifications => Icon::Bell,
            Page::Find => Icon::Viewfinder,
            Page::WhatsNew | Page::Welcome => Icon::Spark,
            Page::Tool(_) => Icon::Star,
        }
    }

    pub fn core_key(self) -> Option<&'static str> {
        Some(match self {
            Page::Media => "media",
            Page::Tools => "tools",
            Page::Shelf => "shelf",
            Page::Sound => "sound",
            Page::Stats => "stats",
            Page::Agents => "agents",
            Page::Settings => "settings",
            Page::Notifications => "notifications",
            Page::Find => "find",
            Page::WhatsNew => "whats-new",
            Page::Welcome => "welcome",
            Page::Tool(_) => return None,
        })
    }

    pub fn from_core_key(key: &str) -> Option<Page> {
        Page::CORE.into_iter().chain([Page::Settings, Page::Notifications]).find(|page| page.core_key() == Some(key))
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Tab {
    pub page: Page,
    pub key: String,
    pub title: String,
    pub icon: Icon,
}

impl Tab {
    pub fn core(page: Page) -> Self {
        let key = page.core_key().unwrap_or_default().to_owned();
        Self { page, key, title: page.core_title().to_owned(), icon: page.core_icon() }
    }

    pub fn defaults() -> Vec<Tab> {
        DEFAULT_TABS.into_iter().filter_map(Page::from_core_key).map(Tab::core).collect()
    }
}
