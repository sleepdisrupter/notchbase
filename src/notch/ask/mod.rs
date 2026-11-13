//! Questions from the coding assistants, shown as a card under the notch until they're answered.

mod card;
mod offers;
mod steps;

use std::sync::Arc;

use egui::text::{LayoutJob, TextWrapping};
use egui::{Align2, Color32, FontId, Galley, Id, Painter, Rect, Shape, Vec2, pos2, vec2};

use crate::assistant::{Provider, Question, Reply};
use crate::bridge::{Action, Ask, Choices};
use crate::platform::Style;
use crate::ui::buttons::{self, Button, Tone};
use crate::ui::canvas::Canvas;
use crate::ui::color::{self, label, secondary_label, tertiary_label, white};
use crate::ui::spring::{lerp, smoothstep};
use crate::ui::{controls, font};

use super::geometry::Geometry;
use super::moves::transition::Motion;
use super::{Mode, Notch};
pub(super) use steps::Answering;
use steps::Move;

const WIDTH: f32 = 360.0;
const NOTCH_WINGS: f32 = 160.0;
pub(in crate::notch) const PAD: f32 = 16.0;
const TOP: f32 = 13.0;
pub(in crate::notch) const SOURCE_H: f32 = 14.0;
pub(in crate::notch) const GAP: f32 = 7.0;
pub(in crate::notch) const BUTTON_H: f32 = 30.0;
pub(in crate::notch) const BUTTON_GAP: f32 = 8.0;
const BOTTOM: f32 = 14.0;
const DETAIL_PAD: Vec2 = vec2(9.0, 6.0);
const DETAIL_RADIUS: f32 = 8.0;
const TITLE_ROWS: usize = 3;
const DETAIL_ROWS: usize = 3;
const DOT_RADIUS: f32 = 3.5;
const ALLOW_W: f32 = 86.0;
const ALWAYS_W: f32 = 118.0;
const DENY_W: f32 = 76.0;
const STEP_W: f32 = 76.0;
const ISLAND_RADIUS: f32 = 26.0;
const NOTCH_RADIUS: f32 = 20.0;
const NOTCH_EAR: f32 = 8.0;
const LIVE_FROM: f32 = 0.6;

pub(in crate::notch) struct Text {
    pub(in crate::notch) title: Arc<Galley>,
    pub(in crate::notch) detail: Option<Arc<Galley>>,
}

#[derive(Clone)]
pub(super) struct AskLayout {
    id: u64,
    step: usize,
    pub(in crate::notch) title: Arc<Galley>,
    pub(in crate::notch) detail: Option<Arc<Galley>>,
    offers: usize,
    framed: bool,
}

/// The card last drawn, kept so its words can fade out with it once it's answered.
#[derive(Clone)]
pub(super) struct Shown {
    layout: AskLayout,
    ask: Ask,
    answering: Answering,
}

impl AskLayout {
    fn fits(&self, front: u64, step: usize) -> bool {
        self.id == front && self.step == step
    }

    fn content_height(&self) -> f32 {
        let well = if self.framed { 2.0 * DETAIL_PAD.y } else { 0.0 };
        let detail = self.detail.as_ref().map_or(0.0, |detail| GAP + detail.size().y + well);
        let offers = if self.offers == 0 { 0.0 } else { GAP + offers::height(self.offers) };
        TOP + SOURCE_H + GAP + self.title.size().y + detail + offers + 2.0 * GAP + BUTTON_H + BOTTOM
    }
}

impl Notch {
    pub fn show_ask(&mut self, ask: Option<Ask>) {
        if self.preview.is_some() || self.ask == ask {
            return;
        }
        let replaced = self.ask.as_ref().map(|a| a.id) != ask.as_ref().map(|a| a.id);
        self.ask = ask;
        if replaced {
            self.ask_layout = None;
        }
        let shown = self.card && self.ask.is_some() && self.mode == Mode::Ask;
        self.follow_card(Motion::Close);
        if shown {
            self.retarget(Motion::Resize);
        }
    }

    pub(super) fn ask_geometry(&self) -> Geometry {
        let content = self.ask_layout.as_ref().map_or(self.estimated_ask_height(), AskLayout::content_height);
        match self.style {
            Style::Notch => {
                Geometry { w: self.ask_width(), h: self.notch.y + content, r: NOTCH_RADIUS, ear: NOTCH_EAR }
            }
            Style::Island => Geometry { w: WIDTH, h: content, r: ISLAND_RADIUS, ear: 0.0 },
        }
    }

    pub(super) fn measure_ask(&mut self, painter: &Painter) {
        if let Some(ask) = &self.ask {
            self.answering.follow(ask);
        }
        let Some(front) = self.front() else { return };
        let step = self.answering.step();
        if self.ask_layout.as_ref().is_some_and(|layout| layout.fits(front, step)) {
            return;
        }
        self.ask_layout = self.ask_card_layout(painter, step);
        if self.mode == Mode::Ask {
            self.remeasured(Motion::Resize);
        }
    }

    fn ask_card_layout(&self, painter: &Painter, step: usize) -> Option<AskLayout> {
        let ask = self.ask.as_ref()?;
        let wrap = self.card_wrap();
        let question = asked(ask, step);
        let title = question.map_or(ask.title.as_str(), |question| question.text.as_str());
        let title = layout(painter, title, font::headline(), wrap, TITLE_ROWS, false);
        let detail = ask
            .detail
            .as_ref()
            .map(|detail| layout(painter, detail, font::mono(11.0), wrap - 2.0 * DETAIL_PAD.x, DETAIL_ROWS, true));
        let offers = question.map_or(0, |question| question.offers.len());
        Some(AskLayout { id: ask.id, step, title, detail, offers, framed: true })
    }

    pub(in crate::notch) fn card_wrap(&self) -> f32 {
        self.ask_width() - 2.0 * PAD
    }

    pub(in crate::notch) fn card_content(&self, body: Rect) -> Rect {
        let top = match self.style {
            Style::Notch => body.top() + self.notch.y,
            Style::Island => body.top(),
        };
        Rect::from_min_max(pos2(body.left() + PAD, top + TOP), pos2(body.right() - PAD, body.bottom() - BOTTOM))
    }

    pub(super) fn ask_ui(&mut self, ui: &egui::Ui, painter: &Painter, g: Geometry, body: Rect) -> Vec<Action> {
        let alpha = self.ask_alpha(g);
        let Some(front) = self.front() else {
            self.paint_answered_card(ui, painter, body);
            return Vec::new();
        };
        let Some(layout) = self.ask_layout.clone() else { return Vec::new() };
        if alpha <= 0.001 || !layout.fits(front, self.answering.step()) {
            return Vec::new();
        }
        let Some(ask) = self.ask.clone() else { return Vec::new() };
        let text = Text { title: layout.title.clone(), detail: layout.detail.clone() };
        self.card_shown = Some(Shown { layout, ask: ask.clone(), answering: self.answering.clone() });
        let content = self.card_content(self.body_rect(self.ask_geometry()));
        let mut canvas = Canvas::new(painter);
        let live = alpha > LIVE_FROM;
        let actions = ask_card(ui, &mut canvas, content, &text, &ask, &mut self.answering, live);
        let clip = painter.with_clip_rect(body.expand(1.0));
        canvas.paint(&clip, pos2(body.center().x, body.top()), lerp(0.96, 1.0, alpha), Vec2::ZERO, alpha);
        actions
    }

    fn paint_answered_card(&mut self, ui: &egui::Ui, painter: &Painter, body: Rect) {
        let alpha = self.ask_fade.value.clamp(0.0, 1.0) * smoothstep(0.8, 1.0, self.attract.value);
        if alpha <= 0.001 {
            if self.ask_fade.is_settled() {
                self.card_shown = None;
            }
            return;
        }
        let Some(Shown { layout, ask, mut answering }) = self.card_shown.clone() else { return };
        let height = layout.content_height() + if self.style == Style::Notch { self.notch.y } else { 0.0 };
        let full = self.body_rect(Geometry { w: self.ask_width(), h: height, r: 0.0, ear: 0.0 });
        let content = self.card_content(full);
        let text = Text { title: layout.title, detail: layout.detail };
        let mut canvas = Canvas::new(painter);
        ask_card(ui, &mut canvas, content, &text, &ask, &mut answering, false);
        let clip = painter.with_clip_rect(body.expand(1.0));
        canvas.paint(&clip, pos2(body.center().x, body.top()), lerp(0.96, 1.0, alpha), Vec2::ZERO, alpha);
    }

    fn ask_width(&self) -> f32 {
        match self.style {
            Style::Notch => (self.notch.x + NOTCH_WINGS).max(WIDTH),
            Style::Island => WIDTH,
        }
    }

    fn estimated_ask_height(&self) -> f32 {
        TOP + SOURCE_H + GAP + 2.0 * 17.0 + 2.0 * GAP + BUTTON_H + BOTTOM
    }

    fn ask_alpha(&self, g: Geometry) -> f32 {
        let (target, idle) = (self.ask_geometry(), self.idle_geometry());
        let grown = ((g.h - idle.h) / (target.h - idle.h).max(1.0)).clamp(0.0, 1.0);
        self.ask_fade.value.clamp(0.0, 1.0)
            * smoothstep(0.55, 0.95, grown)
            * smoothstep(0.8, 1.0, self.attract.value)
            * self.clear_of_the_panel()
    }
}

fn ask_card(
    ui: &egui::Ui,
    canvas: &mut Canvas,
    content: Rect,
    text: &Text,
    ask: &Ask,
    answering: &mut Answering,
    live: bool,
) -> Vec<Action> {
    let step = answering.step();
    paint_source(canvas, content, ask, step);
    let mut y = content.top() + SOURCE_H + GAP;
    canvas.add(Shape::galley(pos2(content.left(), y), text.title.clone(), label()));
    y += text.title.size().y;
    if let Some(detail) = &text.detail {
        y += GAP;
        let size = detail.size() + 2.0 * DETAIL_PAD;
        let well = Rect::from_min_size(pos2(content.left(), y), vec2(content.width(), size.y));
        canvas.rect(well, DETAIL_RADIUS, white(0.07));
        canvas.add(Shape::galley(well.min + DETAIL_PAD, detail.clone(), secondary_label()));
    }
    let row = Rect::from_min_max(pos2(content.left(), content.bottom() - BUTTON_H), content.max);
    let id = Id::new(("notchbase.ask", ask.id));
    let reply = match &ask.choices {
        Choices::Questions(questions) => {
            let area = Rect::from_min_size(pos2(content.left(), y + GAP), vec2(content.width(), row.top() - y));
            let question = &questions[step];
            let picked = answering.picked(step).to_vec();
            let tapped = offers::ui(ui, canvas, area, id.with(("offer", step)), question, &picked, live);
            let footer = questions_footer(ui, canvas, row, ask, answering, live);
            tapped.map(Move::Tap).or(footer).and_then(|step| answering.take(questions, step))
        }
        Choices::Permission { always } => permission_footer(ui, canvas, row, ask, *always, live),
        Choices::Elsewhere => elsewhere_footer(ui, canvas, row, ask, live),
    };
    reply.map(|reply| Action::Answer(ask.id, reply)).into_iter().collect()
}

fn asked(ask: &Ask, step: usize) -> Option<&Question> {
    match &ask.choices {
        Choices::Questions(questions) => questions.get(step),
        Choices::Permission { .. } | Choices::Elsewhere => None,
    }
}

fn layout(painter: &Painter, text: &str, font: FontId, width: f32, rows: usize, anywhere: bool) -> Arc<Galley> {
    let mut job = LayoutJob::simple(text.to_owned(), font, label(), width);
    job.wrap = TextWrapping {
        max_width: width,
        max_rows: rows,
        break_anywhere: anywhere,
        overflow_character: Some('\u{2026}'),
    };
    painter.layout_job(job)
}

fn paint_source(canvas: &mut Canvas, content: Rect, ask: &Ask, step: usize) {
    let y = content.top() + SOURCE_H * 0.5;
    canvas.circle(pos2(content.left() + DOT_RADIUS, y), DOT_RADIUS, tint(ask.provider));
    let left = content.left() + 2.0 * DOT_RADIUS + 6.0;
    let mut right = content.right();
    let aside = match &ask.choices {
        Choices::Questions(questions) if questions.len() > 1 => Some(format!("{} of {}", step + 1, questions.len())),
        _ if ask.waiting > 0 => Some(format!("+{} more", ask.waiting)),
        _ => None,
    };
    if let Some(aside) = aside {
        right =
            canvas.text(pos2(right, y), Align2::RIGHT_CENTER, aside, font::caption(), tertiary_label()).left() - 8.0;
    }
    let header = asked(ask, step).map(|question| question.header.as_str()).filter(|header| !header.is_empty());
    let source = match header {
        Some(header) => format!("{} \u{b7} {} \u{b7} {header}", ask.provider.title(), ask.context),
        None => format!("{} \u{b7} {}", ask.provider.title(), ask.context),
    };
    let source = controls::truncate(canvas, &source, font::section(), right - left);
    canvas.text(pos2(left, y), Align2::LEFT_CENTER, source, font::section(), secondary_label());
}

fn permission_footer(
    ui: &egui::Ui,
    canvas: &mut Canvas,
    row: Rect,
    ask: &Ask,
    always: bool,
    live: bool,
) -> Option<Reply> {
    let id = Id::new(("notchbase.ask", ask.id));
    let widths: &[f32] = if always { &[DENY_W, ALWAYS_W, ALLOW_W] } else { &[DENY_W, ALLOW_W] };
    let rects = buttons::trailing(row, widths, BUTTON_GAP);
    let mut offered = vec![("deny", "Deny", Tone::Plain, Reply::Deny)];
    if always {
        offered.push(("always", "Always allow", Tone::Plain, Reply::AllowAlways));
    }
    offered.push(("allow", "Allow", Tone::Primary, Reply::Allow));
    let mut reply = None;
    for ((key, title, tone, answer), rect) in offered.into_iter().zip(&rects) {
        let button = Button { rect: *rect, id: id.with(key), title, tone, enabled: true };
        if buttons::button(ui, canvas, &button, live) {
            reply = Some(answer);
        }
    }
    let elsewhere = format!("Answer in {}", ask.provider.title());
    let room = rects.first().map_or(row.right(), |first| first.left() - BUTTON_GAP);
    let linked = !always && buttons::link(ui, canvas, row, id.with("elsewhere"), &elsewhere, room, live);
    reply.or(linked.then_some(Reply::InApp))
}

fn questions_footer(
    ui: &egui::Ui,
    canvas: &mut Canvas,
    row: Rect,
    ask: &Ask,
    answering: &Answering,
    live: bool,
) -> Option<Move> {
    let Choices::Questions(questions) = &ask.choices else { return None };
    let id = Id::new(("notchbase.ask", ask.id));
    let step = answering.step();
    let multi = questions[step].multi;
    let last = step + 1 == questions.len();
    let mut offered = Vec::new();
    if step > 0 {
        offered.push(("back", "Back", Tone::Plain, true, Move::Back));
    }
    if multi {
        let title = if last { "Done" } else { "Next" };
        offered.push(("next", title, Tone::Primary, !answering.picked(step).is_empty(), Move::Next));
    }
    let widths: Vec<f32> = offered.iter().map(|(_, title, ..)| buttons::width(canvas, title, STEP_W)).collect();
    let rects = buttons::trailing(row, &widths, BUTTON_GAP);
    let mut chosen = None;
    for ((key, title, tone, enabled, step), rect) in offered.into_iter().zip(&rects) {
        let button = Button { rect: *rect, id: id.with(key), title, tone, enabled };
        if buttons::button(ui, canvas, &button, live) {
            chosen = Some(step);
        }
    }
    let elsewhere = format!("Answer in {}", ask.provider.title());
    let room = rects.first().map_or(row.right(), |first| first.left() - BUTTON_GAP);
    let linked = buttons::link(ui, canvas, row, id.with("elsewhere"), &elsewhere, room, live);
    chosen.or(linked.then_some(Move::Reply(Reply::InApp)))
}

fn elsewhere_footer(ui: &egui::Ui, canvas: &mut Canvas, row: Rect, ask: &Ask, live: bool) -> Option<Reply> {
    let elsewhere = format!("Answer in {}", ask.provider.title());
    let width = canvas.text_width(elsewhere.as_str(), font::headline()) + 2.0 * PAD;
    let rect = Rect::from_min_max(pos2(row.right() - width, row.top()), row.max);
    let id = Id::new(("notchbase.ask", ask.id)).with("elsewhere");
    let button = Button { rect, id, title: &elsewhere, tone: Tone::Primary, enabled: true };
    buttons::button(ui, canvas, &button, live).then_some(Reply::InApp)
}

fn tint(provider: Provider) -> Color32 {
    match provider {
        Provider::Claude => color::CLAUDE,
        Provider::Codex => color::CHATGPT,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::notch::tests::fixtures::{Rig, island};

    fn permission(id: u64) -> Ask {
        Ask {
            id,
            provider: Provider::Claude,
            context: "notchbase".into(),
            title: "Run this command?".into(),
            detail: Some("cargo test".into()),
            choices: Choices::Permission { always: true },
            waiting: 0,
        }
    }

    fn words(rig: &Rig, n: &mut Notch) -> usize {
        let (_, mut output) = rig.run(n, &[], Vec::new(), 1.0 / 60.0);
        output.textures_delta.clear();
        output.shapes.iter().filter(|clipped| matches!(clipped.shape, Shape::Text(_))).count()
    }

    #[test]
    fn answered_card_keeps_words() {
        let rig = Rig::new();
        let mut n = island();
        n.show_ask(Some(permission(1)));
        for _ in 0..60 {
            rig.frame(&mut n, &[], Vec::new());
        }
        let asked = words(&rig, &mut n);
        assert!(asked > 0);
        n.show_ask(None);
        assert_eq!((n.mode, n.front()), (Mode::Idle, None));
        let answered = words(&rig, &mut n);
        assert_eq!(answered, asked, "the words fade out with the card instead of vanishing");
        for _ in 0..60 {
            rig.frame(&mut n, &[], Vec::new());
        }
        assert_eq!(words(&rig, &mut n), 0);
        assert!(n.card_shown.is_none());
    }

    #[test]
    fn card_waits_for_page_fade() {
        let mut n = island();
        n.open_page(crate::bridge::Page::Tools);
        crate::notch::tests::fixtures::settle(&mut n);
        n.show_ask(Some(permission(1)));
        let mut overlap = 0.0f32;
        for _ in 0..60 {
            n.step(1.0 / 60.0);
            overlap = overlap.max(n.ask_alpha(n.morph.current()).min(n.panel_fade.value));
        }
        assert!(overlap < 0.1, "the page and the card are never both half there: {overlap}");
        assert!(n.ask_alpha(n.morph.current()) > 0.99);
    }

    #[test]
    fn card_keeps_pop_after_measure() {
        let rig = Rig::new();
        let mut n = island();
        n.show_ask(Some(permission(1)));
        assert!(n.ask_layout.is_none());
        rig.frame(&mut n, &[], Vec::new());
        assert!(n.ask_layout.is_some(), "the first frame lays it out");
        assert_eq!(n.morph.h.config, n.spring_configs(Motion::Pop).1);
        assert_eq!(n.morph.h.target, n.ask_geometry().h, "and it heads for its measured height");
    }
}
