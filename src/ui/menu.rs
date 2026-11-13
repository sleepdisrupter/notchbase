//! A glass pop-up menu that opens at the pointer and stays in bounds. A lit plate glides to the item under the pointer.

use std::f32::consts::TAU;

use egui::{Align2, Color32, Id, Mesh, Pos2, Rect, Sense, Shape, Vec2, pos2, vec2};

use super::canvas::{Canvas, Mark, fade, scale_about};
use super::color::{label, tertiary_label, white};
use super::controls;
use super::font;
use super::glass::{self, Glass};
use super::glyph::{self, Glyph};
use super::motion;
use super::shape;
use super::spring::{POINT_PRECISION, Spring, SpringConfig, UNIT_PRECISION, lerp};

const WIDTH_LEAST: f32 = 164.0;
const ITEM_H: f32 = 30.0;
const PAD: f32 = 5.0;
const RADIUS: f32 = 13.0;
const ITEM_RADIUS: f32 = RADIUS - PAD;
const GLYPH: f32 = 14.0;
const GLYPH_X: f32 = 15.0;
const TEXT_X: f32 = 31.0;
const TRAILING: f32 = 14.0;
const EDGE: f32 = 4.0;
const ARRIVES_FROM: f32 = 0.82;
const LEAVES_TO: f32 = 0.95;
const GONE: f32 = 0.02;
const POP: SpringConfig = SpringConfig::new(0.28, 0.6);
const LEADS: SpringConfig = SpringConfig::new(0.16, 0.86);
const TRAILS: SpringConfig = SpringConfig::new(0.3, 0.78);
const BODY: Glass = Glass { tint: Color32::from_rgb(8, 8, 10), sheen: 0.1, specular: 0.7, edge: 0.6, clear: 0.0 };
const LIT: Glass = Glass { tint: Color32::from_rgb(46, 46, 50), sheen: 0.08, specular: 0.5, edge: 0.55, clear: 0.0 };
const SHADE: u8 = 120;
const LIGHT: f32 = 0.085;
const LIGHT_REACH: f32 = 80.0;
const RINGS: u32 = 6;
const SPOKES: u32 = 28;
const GLINT: f32 = 0.34;
const GLINT_REACH: f32 = 46.0;
const GLINT_WIDTH: f32 = 1.6;
const MENU_ID: &str = "notchbase.menu";

#[derive(Clone, Debug, PartialEq)]
pub struct Item<T> {
    pub words: &'static str,
    pub glyph: Glyph,
    pub choice: Option<T>,
}

struct Lit {
    over: Option<usize>,
    top: Spring,
    bottom: Spring,
    glow: Spring,
}

impl Default for Lit {
    fn default() -> Self {
        Self {
            over: None,
            top: Spring::new(0.0, LEADS, POINT_PRECISION),
            bottom: Spring::new(0.0, LEADS, POINT_PRECISION),
            glow: Spring::new(0.0, motion::HOVER, UNIT_PRECISION),
        }
    }
}

impl Lit {
    fn follow(&mut self, over: Option<usize>) {
        let Some(index) = over else {
            self.over = None;
            self.glow.animate_to(0.0, motion::HOVER);
            return;
        };
        if self.over == Some(index) {
            return;
        }
        let top = PAD + ITEM_H * index as f32;
        if self.glow.value <= GONE {
            self.top.snap_to(top);
            self.bottom.snap_to(top + ITEM_H);
        } else {
            let down = top > self.top.target;
            self.top.animate_to(top, if down { TRAILS } else { LEADS });
            self.bottom.animate_to(top + ITEM_H, if down { LEADS } else { TRAILS });
        }
        self.over = Some(index);
        self.glow.animate_to(1.0, motion::HOVER);
    }

    fn step(&mut self, dt: f32) {
        self.top.step(dt);
        self.bottom.step(dt);
        self.glow.step(dt);
    }

    fn is_moving(&self) -> bool {
        !(self.top.is_settled() && self.bottom.is_settled() && self.glow.is_settled())
    }
}

struct Open<T> {
    at: Pos2,
    items: Vec<Item<T>>,
    shown: Spring,
    leaving: bool,
    stood: Option<Rect>,
    lit: Lit,
}

pub struct Menu<T> {
    open: Option<Open<T>>,
}

impl<T> Default for Menu<T> {
    fn default() -> Self {
        Self { open: None }
    }
}

pub fn item_id(index: usize) -> Id {
    Id::new((MENU_ID, index))
}

pub fn height_of(items: usize) -> f32 {
    2.0 * PAD + ITEM_H * items as f32
}

impl<T: Clone> Menu<T> {
    pub fn open(&mut self, at: Pos2, items: Vec<Item<T>>) {
        let mut shown = Spring::new(0.0, POP, UNIT_PRECISION);
        shown.animate_to(1.0, POP);
        self.open = Some(Open { at, items, shown, leaving: false, stood: None, lit: Lit::default() });
    }

    pub fn close(&mut self) {
        if let Some(open) = &mut self.open {
            open.leave();
        }
    }

    pub fn is_open(&self) -> bool {
        self.open.as_ref().is_some_and(|open| !open.leaving)
    }

    pub fn is_animating(&self) -> bool {
        self.open.as_ref().is_some_and(|open| !open.shown.is_settled() || open.lit.is_moving())
    }

    /// How far down the open menu reaches, so the page under it can grow to fit.
    pub fn reach(&self) -> Option<f32> {
        let open = self.open.as_ref().filter(|open| !open.leaving)?;
        Some(open.at.y + height_of(open.items.len()) + EDGE)
    }

    #[cfg(test)]
    pub fn stands(&self) -> Option<Rect> {
        self.open.as_ref()?.stood
    }

    pub fn show(&mut self, ui: &egui::Ui, canvas: &mut Canvas, bounds: Rect, dt: f32, live: bool) -> Option<T> {
        let open = self.open.as_mut()?;
        let chose = open.show(ui, canvas, bounds, dt, live);
        if open.leaving && open.shown.value <= GONE {
            self.open = None;
        }
        chose
    }
}

impl<T: Clone> Open<T> {
    fn leave(&mut self) {
        self.leaving = true;
        self.shown.animate_to(0.0, motion::HOVER);
    }

    fn size(&self, canvas: &Canvas) -> Vec2 {
        let widest = self.items.iter().map(|item| canvas.text_width(item.words, font::body())).fold(0.0, f32::max);
        vec2((2.0 * PAD + TEXT_X + widest + TRAILING).max(WIDTH_LEAST), height_of(self.items.len()))
    }

    fn stand(&mut self, size: Vec2, bounds: Rect) -> Rect {
        match self.stood {
            Some(rect) if self.leaving => rect,
            _ => *self.stood.insert(placed(self.at, size, bounds)),
        }
    }

    fn show(&mut self, ui: &egui::Ui, canvas: &mut Canvas, bounds: Rect, dt: f32, live: bool) -> Option<T> {
        self.shown.step(dt);
        let live = live && !self.leaving;
        let rect = self.stand(self.size(canvas), bounds);
        let mark = canvas.mark();
        self.surface(ui, canvas, rect);
        let mut chose = None;
        if live {
            ui.interact(rect, Id::new(MENU_ID), Sense::click());
            let mut over = None;
            for (index, item) in self.items.iter().enumerate() {
                let offered = item.choice.is_some();
                let sense = if offered { Sense::click() } else { Sense::hover() };
                let touched = ui.interact(row_of(rect, index), item_id(index), sense);
                if offered && touched.hovered() {
                    over = Some(index);
                }
                if offered && touched.clicked() {
                    chose = item.choice.clone();
                }
            }
            self.lit.follow(over);
        }
        self.lit.step(dt);
        self.plate(canvas, rect);
        for (index, item) in self.items.iter().enumerate() {
            let row = row_of(rect, index);
            let ink = if item.choice.is_some() { label() } else { tertiary_label() };
            glyph::paint(canvas, item.glyph, pos2(row.left() + GLYPH_X, row.center().y), GLYPH, ink);
            canvas.text(pos2(row.left() + TEXT_X, row.center().y), Align2::LEFT_CENTER, item.words, font::body(), ink);
        }
        self.arrive(canvas, mark, rect);
        let away = ui.input(|input| {
            input.pointer.any_pressed() && input.pointer.interact_pos().is_some_and(|at| !rect.contains(at))
        });
        if chose.is_some() || (live && away) {
            self.leave();
        }
        chose
    }

    fn surface(&self, ui: &egui::Ui, canvas: &mut Canvas, rect: Rect) {
        let shade = egui::Shadow { offset: [0, 8], blur: 24, spread: 0, color: Color32::from_black_alpha(SHADE) };
        canvas.add(shade.as_shape(rect, RADIUS));
        glass::paint(canvas, rect, RADIUS, BODY);
        let Some(pointer) = ui.input(|input| input.pointer.latest_pos()) else { return };
        let within = controls::animate(ui, Id::new(MENU_ID).with("light"), rect.contains(pointer));
        if within > 0.0 {
            canvas.add(Shape::mesh(light(rect.shrink(0.5), pointer, within)));
        }
        canvas.add(Shape::mesh(glint(rect, pointer, canvas.pixels_per_point())));
    }

    fn plate(&self, canvas: &mut Canvas, rect: Rect) {
        let glow = self.lit.glow.value.clamp(0.0, 1.0);
        if glow <= 0.0 {
            return;
        }
        let (top, bottom) = (rect.top() + self.lit.top.value, rect.top() + self.lit.bottom.value);
        let plate = Rect::from_min_max(pos2(rect.left() + PAD, top), pos2(rect.right() - PAD, bottom.max(top + 1.0)));
        let mark = canvas.mark();
        glass::paint(canvas, plate, ITEM_RADIUS, LIT);
        canvas.since(mark).iter_mut().for_each(|shape| fade(shape, glow));
    }

    fn arrive(&self, canvas: &mut Canvas, mark: Mark, rect: Rect) {
        let shown = self.shown.value;
        if !self.leaving && self.shown.is_settled() {
            return;
        }
        let small = if self.leaving { LEAVES_TO } else { ARRIVES_FROM };
        let grow = scale_about(rect.clamp(self.at), lerp(small, 1.0, shown));
        let seen = (shown * 1.6).clamp(0.0, 1.0);
        for shape in canvas.since(mark) {
            shape.transform(grow);
            fade(shape, seen);
        }
    }
}

fn placed(at: Pos2, size: Vec2, bounds: Rect) -> Rect {
    let left = at.x.min(bounds.right() - size.x - EDGE).max(bounds.left() + EDGE);
    let top = at.y.min(bounds.bottom() - size.y - EDGE).max(bounds.top() + EDGE);
    Rect::from_min_size(pos2(left, top), size)
}

fn row_of(rect: Rect, index: usize) -> Rect {
    let top = rect.top() + PAD + ITEM_H * index as f32;
    Rect::from_min_size(pos2(rect.left() + PAD, top), vec2(rect.width() - 2.0 * PAD, ITEM_H))
}

fn kept_inside(rect: Rect, at: Pos2) -> Pos2 {
    let nearest = rect.shrink(RADIUS).clamp(at);
    let away = at - nearest;
    let far = away.length();
    if far > RADIUS { nearest + away * (RADIUS / far) } else { at }
}

fn light(rect: Rect, pointer: Pos2, strength: f32) -> Mesh {
    let mut mesh = Mesh::default();
    mesh.colored_vertex(kept_inside(rect, pointer), white(LIGHT * strength));
    for ring in 1..=RINGS {
        let share = ring as f32 / RINGS as f32;
        let lit = white(LIGHT * strength * (1.0 - share) * (1.0 - share));
        for spoke in 0..SPOKES {
            let (sin, cos) = (TAU * spoke as f32 / SPOKES as f32).sin_cos();
            mesh.colored_vertex(kept_inside(rect, pointer + LIGHT_REACH * share * vec2(cos, sin)), lit);
        }
    }
    for spoke in 0..SPOKES {
        let next = (spoke + 1) % SPOKES;
        mesh.add_triangle(0, 1 + spoke, 1 + next);
        for ring in 1..RINGS {
            let (inner, outer) = (1 + (ring - 1) * SPOKES, 1 + ring * SPOKES);
            mesh.add_triangle(inner + spoke, outer + spoke, outer + next);
            mesh.add_triangle(inner + spoke, outer + next, inner + next);
        }
    }
    mesh
}

fn glint(rect: Rect, pointer: Pos2, pixels_per_point: f32) -> Mesh {
    let blob = shape::Blob { rect, top_radius: RADIUS, bottom_radius: RADIUS, ear: 0.0, bleed_top: false, wave: None };
    let outline = shape::outline(&blob, pixels_per_point);
    let normals = shape::vertex_normals(&outline);
    glass::rim_mesh(&outline, &normals, GLINT_WIDTH, pixels_per_point, |at, _| {
        let away = at.distance(pointer) / GLINT_REACH;
        white(GLINT * (-away * away).exp())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const DT: f32 = 1.0 / 60.0;

    fn items() -> Vec<Item<u8>> {
        vec![
            Item { words: "Pin", glyph: Glyph::Pin, choice: Some(1) },
            Item { words: "Show in Finder", glyph: Glyph::Folder, choice: Some(2) },
            Item { words: "Four are pinned", glyph: Glyph::Pin, choice: None },
        ]
    }

    #[test]
    fn opens_at_pointer_inside_bounds() {
        let bounds = Rect::from_min_size(pos2(10.0, 20.0), vec2(340.0, 150.0));
        let size = vec2(WIDTH_LEAST, height_of(2));
        assert_eq!(size.y, 70.0);
        assert_eq!(placed(pos2(60.0, 40.0), size, bounds).min, pos2(60.0, 40.0));
        let cornered = placed(pos2(345.0, 168.0), size, bounds);
        assert!(bounds.contains_rect(cornered), "{cornered:?}");
        assert_eq!(cornered.size(), size);
        assert_eq!(row_of(cornered, 1).top() - row_of(cornered, 0).top(), ITEM_H);
    }

    #[test]
    fn open_menu_reports_its_reach() {
        let mut menu = Menu::default();
        assert!(!menu.is_open() && menu.reach().is_none());
        menu.open(pos2(60.0, 40.0), items());
        assert!(menu.is_open() && menu.is_animating());
        assert_eq!(menu.reach(), Some(40.0 + height_of(3) + EDGE));
        menu.close();
        assert!(!menu.is_open() && menu.reach().is_none(), "what is under it is free again while it fades");
    }

    #[test]
    fn leaving_menu_stays_where_it_stood() {
        let size = vec2(WIDTH_LEAST, height_of(3));
        let roomy = Rect::from_min_size(pos2(10.0, 20.0), vec2(340.0, 260.0));
        let tight = Rect::from_min_size(pos2(10.0, 20.0), vec2(340.0, 110.0));
        let mut menu = Menu::default();
        menu.open(pos2(60.0, 120.0), items());
        let open = menu.open.as_mut().expect("it was opened");
        assert_eq!(open.stand(size, roomy).min, pos2(60.0, 120.0));
        let squeezed = open.stand(size, tight);
        assert_eq!(squeezed.bottom(), tight.bottom() - EDGE, "an open menu is kept inside what holds it");
        assert_eq!(open.stand(size, roomy).min, pos2(60.0, 120.0));
        open.leave();
        assert_eq!(open.stand(size, tight).min, pos2(60.0, 120.0), "it fades where it was, not shoved up");
        assert_eq!(menu.stands().map(|rect| rect.min), Some(pos2(60.0, 120.0)));
        let mut unseen = Menu::default();
        unseen.open(pos2(60.0, 120.0), items());
        unseen.close();
        let open = unseen.open.as_mut().expect("it is still fading");
        assert_eq!(open.stand(size, tight), squeezed, "one closed before it was ever drawn is placed once");
        assert_eq!(open.stand(size, roomy), squeezed);
    }

    #[test]
    fn light_glides_and_stretches() {
        let mut lit = Lit::default();
        lit.follow(Some(0));
        assert_eq!((lit.top.value, lit.bottom.value), (PAD, PAD + ITEM_H), "it appears on the item, not sliding in");
        for _ in 0..30 {
            lit.step(DT);
        }
        lit.follow(Some(2));
        let mut tallest = 0.0f32;
        for _ in 0..120 {
            lit.step(DT);
            tallest = tallest.max(lit.bottom.value - lit.top.value);
        }
        assert!(tallest > ITEM_H + 6.0, "the leading edge runs ahead: {tallest}");
        assert!((lit.top.value - (PAD + 2.0 * ITEM_H)).abs() < 0.2 && !lit.is_moving());
        lit.follow(None);
        for _ in 0..60 {
            lit.step(DT);
        }
        assert!(lit.glow.value < GONE);
    }

    #[test]
    fn light_stays_inside_the_body() {
        let rect = Rect::from_min_size(pos2(0.0, 0.0), vec2(164.0, 70.0));
        let corner = kept_inside(rect, pos2(-20.0, -20.0));
        assert!((corner.distance(pos2(RADIUS, RADIUS)) - RADIUS).abs() < 0.01, "{corner:?}");
        assert_eq!(kept_inside(rect, pos2(80.0, 30.0)), pos2(80.0, 30.0));
        let mesh = light(rect, pos2(2.0, 2.0), 1.0);
        assert_eq!(mesh.vertices.len() as u32, 1 + RINGS * SPOKES);
        assert!(mesh.vertices.iter().all(|vertex| rect.expand(0.01).contains(vertex.pos)));
        assert_eq!(mesh.vertices.last().map(|vertex| vertex.color), Some(Color32::TRANSPARENT));
    }
}
