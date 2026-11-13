//! The Settings page and its views: General, Assistant, Dock, Tools, Indicators and each tool's own settings.
//! A view slides in from the side it lies on.

mod app;
mod assistant;
mod catalog;
mod dock;
mod editing;
mod general;
pub(in crate::pages) mod rows;
mod tool;

use egui::vec2;

use super::PageCtx;
use crate::bridge::{Action, PanelData};
use crate::ui::canvas::{Canvas, Mark, fade};
use crate::ui::motion;
use crate::ui::spring::{Spring, UNIT_PRECISION};
use catalog::Kind;
use editing::Editing;
pub(super) use rows::{BACK_H, INSET, ROW_H, SECTION_H, back, card, card_of, pill, section};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
enum View {
    #[default]
    General,
    Assistant,
    Dock,
    Tools,
    Indicators,
    Tool(String),
}

impl View {
    fn depth(&self) -> u8 {
        match self {
            View::General => 0,
            View::Tool(_) => 2,
            View::Assistant | View::Dock | View::Tools | View::Indicators => 1,
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
    notifications: bool,
}

impl SettingsPage {
    pub fn height(&self, data: &PanelData) -> f32 {
        match &self.view {
            View::General => general::height(data),
            View::Assistant => assistant::height(),
            View::Dock => dock::height(),
            View::Tools => catalog::height(data.catalog, Kind::Tools),
            View::Indicators => catalog::height(data.catalog, Kind::Indicators),
            View::Tool(id) => self.tool_rows.height(data.tools.iter().find(|tool| tool.id == *id)),
        }
    }

    pub fn rest(&mut self, actions: &mut Vec<Action>) {
        self.editing.finish(actions);
        self.view = View::General;
        self.slide.enter.snap_to(1.0);
    }

    pub fn is_animating(&self) -> bool {
        !self.slide.enter.is_settled() || self.tool_rows.is_animating()
    }

    pub fn view_key(&self) -> String {
        format!("{:?}", self.view)
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
        self.go(View::Assistant);
    }

    pub fn show_dock(&mut self) {
        self.go(View::Dock);
    }

    pub fn show_tools(&mut self) {
        self.go(View::Tools);
    }

    pub fn show_tool(&mut self, id: &str) {
        self.go(View::Tool(id.to_owned()));
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
        let next = match &self.view {
            View::General => match general::ui(ctx, data, &mut actions) {
                general::Outcome::Stay => View::General,
                general::Outcome::ShowAssistant => View::Assistant,
                general::Outcome::ShowDock => View::Dock,
                general::Outcome::ShowTools => View::Tools,
                general::Outcome::ShowIndicators => View::Indicators,
                general::Outcome::ShowNotifications => {
                    self.notifications = true;
                    View::General
                }
            },
            View::Assistant => match assistant::ui(ctx, data, &mut actions) {
                assistant::Outcome::Stay => View::Assistant,
                assistant::Outcome::Back => View::General,
            },
            View::Dock => match dock::ui(ctx, data.dock, &mut actions) {
                dock::Outcome::Stay => View::Dock,
                dock::Outcome::Back => View::General,
            },
            View::Tools | View::Indicators => {
                let kind = if self.view == View::Tools { Kind::Tools } else { Kind::Indicators };
                match catalog::ui(ctx, data, kind, &mut actions) {
                    catalog::Outcome::Stay => self.view.clone(),
                    catalog::Outcome::Back => View::General,
                    catalog::Outcome::Settings(id) => View::Tool(id),
                }
            }
            View::Tool(id) => {
                let shown = data.tools.iter().find(|tool| tool.id == *id);
                let kind = data.catalog.iter().find(|info| info.id == *id).map_or(Kind::Tools, Kind::of);
                let editing = (&mut self.editing, &mut self.tool_rows);
                match (tool::ui(ctx, shown, kind.title(), editing, &mut actions), kind) {
                    (tool::Outcome::Stay, _) => View::Tool(id.clone()),
                    (tool::Outcome::Back, Kind::Tools) => View::Tools,
                    (tool::Outcome::Back, Kind::Indicators) => View::Indicators,
                }
            }
        };
        if next != self.view {
            self.editing.finish(&mut actions);
            self.go(next);
        }
        self.editing.end_frame(ctx.ui, &mut actions);
        actions
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bridge::ToolInfo;

    #[test]
    fn new_view_slides_from_its_side() {
        let mut page = SettingsPage::default();
        assert!(!page.is_animating());
        page.show_tools();
        assert!(page.is_animating() && page.slide.from > 0.0, "a deeper view comes in from the right");
        page.slide.enter.snap_to(1.0);
        page.show_tool("timer");
        assert!(page.slide.from > 0.0);
        page.go(View::Tools);
        assert!(page.slide.from < 0.0, "going back comes in from the left");
        page.go(View::Tools);
        assert_eq!(page.slide.enter.value, 0.0, "staying put starts nothing new");
        page.rest(&mut Vec::new());
        assert!(!page.is_animating() && page.view == View::General);
    }

    #[test]
    fn tools_view_grows_with_installs() {
        let one = [ToolInfo::default()];
        let three = [ToolInfo::default(), ToolInfo::default(), ToolInfo::default()];
        let tools = |catalog: &[ToolInfo]| catalog::height(catalog, Kind::Tools);
        assert!(tools(&three) > tools(&one));
        assert_eq!(tools(&[]), tools(&one));
    }

    #[test]
    fn background_tools_are_indicators() {
        let tool = |id: &str, background| ToolInfo { id: id.into(), background, ..ToolInfo::default() };
        let catalog = [tool("timer", false), tool("battery", true), tool("camera-mic", true), tool("notes", false)];
        assert_eq!((Kind::Tools.count(&catalog), Kind::Indicators.count(&catalog)), (2, 2));
        assert_eq!(Kind::of(&catalog[1]).title(), "Indicators");
        let only_tools = [tool("timer", false), tool("notes", false)];
        assert_eq!(catalog::height(&catalog, Kind::Tools), catalog::height(&only_tools, Kind::Tools));
        let indicators = |catalog: &[ToolInfo]| catalog::height(catalog, Kind::Indicators);
        assert_eq!(indicators(&catalog) - indicators(&only_tools), ROW_H, "an empty list still has one row");
        assert!(indicators(&catalog) < catalog::height(&catalog, Kind::Tools), "no folders or reload there");
    }

    #[test]
    fn assistant_view_reopens_on_general() {
        let mut page = SettingsPage::default();
        page.show_assistant();
        page.rest(&mut Vec::new());
        assert_eq!(page.view, View::General);
    }
}
