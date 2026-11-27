//! The Settings page: a home of tiles with a search over every setting, a view behind each tile, and each tool's own
//! settings. A view slides in from the side it lies on.

mod app;
mod assistant;
mod catalog;
mod dock;
mod editing;
mod find;
mod group;
mod home;
mod item;
mod news;
mod notch;
pub(in crate::pages) mod rows;
mod search;
mod setting;
mod tool;

use egui::vec2;

use super::PageCtx;
use crate::bridge::{Action, PanelData};
use crate::ui::canvas::{Canvas, Mark, fade};
use crate::ui::motion;
use crate::ui::spring::{Spring, UNIT_PRECISION};
use catalog::Kind;
use editing::Editing;
use group::Group;
use item::Went;
pub(super) use rows::{BACK_H, INSET, ROW_H, SECTION_H, back, card, card_of, pill, section};
use search::Search;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
enum View {
    #[default]
    Home,
    Group(Group),
    Tool(String),
}

impl View {
    fn depth(&self) -> u8 {
        match self {
            View::Home => 0,
            View::Group(_) => 1,
            View::Tool(_) => 2,
        }
    }
}

const SLIDE: f32 = 12.0;

struct Slide {
    enter: Spring,
    from: f32,
}

impl Default for Slide {
    fn default() -> Self {
        Self { enter: Spring::new(1.0, motion::SMOOTH, UNIT_PRECISION), from: 1.0 }
    }
}

impl Slide {
    fn start(&mut self, deeper: bool) {
        self.from = if deeper { 1.0 } else { -1.0 };
        self.enter.snap_to(0.0);
        self.enter.animate_to(1.0, motion::SMOOTH);
    }

    fn apply(&self, canvas: &mut Canvas, mark: Mark, entered: f32) {
        if entered >= 1.0 {
            return;
        }
        let offset = vec2(self.from * (1.0 - entered) * SLIDE, 0.0);
        for shape in canvas.since(mark) {
            shape.translate(offset);
            fade(shape, entered);
        }
    }
}

#[derive(Default)]
pub struct SettingsPage {
    view: View,
    slide: Slide,
    editing: Editing,
    tool_rows: tool::Shown,
    search: Search,
    notifications: bool,
}

impl SettingsPage {
    pub fn height(&self, data: &PanelData) -> f32 {
        match &self.view {
            View::Home => home::height(&self.search, data),
            View::Group(group) => group::height(*group, data),
            View::Tool(id) => self.tool_rows.height(data.tools.iter().find(|tool| tool.id == *id)),
        }
    }

    /// Back home with the search emptied, as the page is left.
    pub fn rest(&mut self, actions: &mut Vec<Action>) {
        self.editing.finish(actions);
        self.view = View::Home;
        self.search.clear();
        self.slide.enter.snap_to(1.0);
    }

    pub fn is_animating(&self) -> bool {
        !self.slide.enter.is_settled() || self.tool_rows.is_animating()
    }

    pub fn view_key(&self) -> String {
        match &self.view {
            View::Home if self.search.is_on() => "Search".to_owned(),
            view => format!("{view:?}"),
        }
    }

    fn go(&mut self, next: View) {
        if next != self.view {
            self.slide.start(next.depth() > self.view.depth());
            self.view = next;
        }
    }

    pub fn let_go(&mut self, actions: &mut Vec<Action>) {
        actions.extend(self.editing.let_go());
    }

    pub fn show_assistant(&mut self) {
        self.go(View::Group(Group::Assistant));
    }

    pub fn show_dock(&mut self) {
        self.go(View::Group(Group::Dock));
    }

    pub fn show_tools(&mut self) {
        self.go(View::Group(Group::Tools));
    }

    pub fn show_tool(&mut self, id: &str) {
        self.go(View::Tool(id.to_owned()));
    }

    /// Home, with the search showing what it finds for `query`.
    pub fn search_for(&mut self, query: &str) {
        self.go(View::Home);
        self.search.set(query.to_owned());
    }

    pub fn take_notifications(&mut self) -> bool {
        std::mem::take(&mut self.notifications)
    }

    pub fn ui(&mut self, ctx: &mut PageCtx<'_>, data: &PanelData) -> Vec<Action> {
        self.slide.enter.step(ctx.dt);
        let (mark, entered) = (ctx.canvas.mark(), self.slide.enter.value.clamp(0.0, 1.0));
        let actions = self.view_ui(ctx, data);
        self.slide.apply(ctx.canvas, mark, entered);
        actions
    }

    fn view_ui(&mut self, ctx: &mut PageCtx<'_>, data: &PanelData) -> Vec<Action> {
        let mut actions = Vec::new();
        let went = match &self.view {
            View::Home => match home::ui(ctx, data, &mut self.search, &mut self.editing, &mut actions) {
                Some(home::Go::Group(group)) => Some(View::Group(group)),
                Some(home::Go::Went(went)) => self.went(went),
                None => None,
            },
            View::Group(group) => match group::ui(ctx, *group, data, &mut self.editing, &mut actions) {
                group::Outcome::Stay => None,
                group::Outcome::Back => Some(View::Home),
                group::Outcome::Went(went) => self.went(went),
            },
            View::Tool(id) => {
                let shown = data.tools.iter().find(|tool| tool.id == *id);
                let kind = data.catalog.iter().find(|info| info.id == *id).map_or(Kind::Tools, Kind::of);
                let editing = (&mut self.editing, &mut self.tool_rows);
                match tool::ui(ctx, shown, kind.title(), editing, &mut actions) {
                    tool::Outcome::Stay => None,
                    tool::Outcome::Back if kind == Kind::Indicators => Some(View::Group(Group::Indicators)),
                    tool::Outcome::Back => Some(View::Group(Group::Tools)),
                }
            }
        };
        if let Some(next) = went.filter(|next| *next != self.view) {
            self.editing.finish(&mut actions);
            self.go(next);
        }
        self.editing.end_frame(ctx.ui, &mut actions);
        actions
    }

    /// The view a row's request leads to, if it stays in Settings.
    fn went(&mut self, went: Went) -> Option<View> {
        match went {
            Went::Tool(id) => Some(View::Tool(id)),
            Went::Notifications => {
                self.notifications = true;
                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_view_slides_from_its_side() {
        let mut page = SettingsPage::default();
        assert!(!page.is_animating());
        page.show_tools();
        assert!(page.is_animating() && page.slide.from > 0.0, "a deeper view comes in from the right");
        page.slide.enter.snap_to(1.0);
        page.show_tool("timer");
        assert!(page.slide.from > 0.0);
        page.go(View::Group(Group::Tools));
        assert!(page.slide.from < 0.0, "going back comes in from the left");
        page.go(View::Group(Group::Tools));
        assert_eq!(page.slide.enter.value, 0.0, "staying put starts nothing new");
        page.rest(&mut Vec::new());
        assert!(!page.is_animating() && page.view == View::Home);
    }

    #[test]
    fn leaving_forgets_search() {
        let mut page = SettingsPage::default();
        page.show_assistant();
        page.search.set("dock".into());
        assert_eq!(page.view_key(), "Group(Assistant)");
        page.view = View::Home;
        assert_eq!(page.view_key(), "Search", "results keep a scroll of their own");
        page.rest(&mut Vec::new());
        assert_eq!(page.view, View::Home);
        assert!(!page.search.is_on());
        assert_eq!(page.view_key(), "Home");
    }
}
