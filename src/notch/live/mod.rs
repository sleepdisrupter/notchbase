//! Live activities in the closed notch: what's going on (tools, agents, downloads, the lock) and how
//! they share the space.

pub mod agents;
pub(super) mod downloading;
pub(super) mod lock;
pub(super) mod sides;
pub(super) mod video;

use std::time::{Duration, Instant};

use egui::{Pos2, Rect, Vec2, pos2, vec2};
use notchbase_sdk::now_ms;
use notchbase_sdk::protocol::{LiveNode, Node, Surface};

use crate::bridge::{Action, Activity, AgentLive, ToolSlot};
use crate::platform::Style;
use crate::ui::canvas::Canvas;
use crate::ui::element::{Env, Scene};
use crate::ui::spring::{Spring, SpringConfig, UNIT_PRECISION, smoothstep};

use super::geometry::Geometry;
use super::moves::transition::Motion;
use super::{Mode, Notch};

const NOTCH_WING: f32 = 56.0;
const CAMERA_CLEARANCE: f32 = 10.0;
const ISLAND_W: f32 = 116.0;
const ISLAND_H: f32 = 34.0;
const ISLAND_GAP: f32 = 16.0;
const LANE_INSET: f32 = 14.0;
const REMEASURE: f32 = 0.5;
const SECOND_MS: u64 = 1000;
const SWAP_IN: SpringConfig = SpringConfig::new(0.3, 1.0);
const LEAVE: SpringConfig = SpringConfig::new(0.16, 1.0);
const SHOWN: f32 = 0.001;
pub(super) const HOVER: SpringConfig = SpringConfig::new(0.34, 0.72);
const DRAWN_OUT: f32 = 0.95;
const CONTROLS_GAP: f32 = 8.0;
const CONTROLS_REACH: f32 = 6.0;
const PRESSABLE_FROM: f32 = 0.6;

#[derive(Clone, Debug, PartialEq)]
pub(super) struct Live {
    pub(super) activity: Activity,
    pub(super) widths: [f32; 2],
}

#[derive(Clone, Debug)]
pub(super) struct Leaving {
    live: Live,
    fade: Spring,
}

struct Parts {
    sizes: [Vec2; 2],
    controls: Option<Vec2>,
    look: video::Look,
}

impl Notch {
    pub fn set_activities(&mut self, activities: Vec<Activity>) -> bool {
        if self.preview.is_some() || self.app_activities == activities {
            return false;
        }
        self.app_activities = activities;
        self.combine_activities();
        true
    }

    pub(super) fn combine_activities(&mut self) {
        let lock = self.lock_activity();
        let (urgent, others): (Vec<&Activity>, Vec<&Activity>) =
            self.app_activities.iter().partition(|activity| activity.urgent());
        let (downloads, rest): (Vec<&Activity>, Vec<&Activity>) =
            others.into_iter().partition(|activity| matches!(activity, Activity::Video(_)));
        let combined: Vec<Activity> = lock.iter().chain(urgent).chain(downloads).chain(rest).cloned().collect();
        if combined != self.activities {
            self.video.follow(&combined);
            self.downloading.follow(&combined, self.preview.is_none());
            self.activities = combined;
            self.arrange();
        }
    }

    pub(super) fn music_leads(&self) -> bool {
        self.music_playing()
            && self.music_until.is_some()
            && !self.activities.is_empty()
            && !self.download_leads()
            && !self.activities.first().is_some_and(Activity::urgent)
    }

    fn download_leads(&self) -> bool {
        matches!(self.activities.first(), Some(Activity::Video(_)))
    }

    pub(super) fn arrange(&mut self) {
        let leads = self.music_leads();
        let main = if leads { None } else { self.activities.first().cloned() };
        let room = if self.crowded_by_menus() { 1 } else { sides::MOST };
        let sides = self.activities.iter().skip(usize::from(!leads)).take(room).cloned().collect();
        self.place_sides(sides);
        self.set_main(main);
    }

    fn set_main(&mut self, live: Option<Activity>) {
        if self.live.as_ref().map(|live| &live.activity) == live.as_ref() {
            return;
        }
        let appearing = self.live.is_none() != live.is_none();
        let swapped = match (&self.live, &live) {
            (Some(current), Some(next)) => sides::key(&current.activity) != sides::key(next),
            _ => false,
        };
        if swapped || live.is_none() {
            self.let_live_go();
        }
        if swapped && self.shows_live() {
            self.live_fade.snap_to(0.0);
            self.live_fade.animate_to(1.0, SWAP_IN);
        }
        let widths = self.live.as_ref().map_or([0.0; 2], |current| current.widths);
        self.live = live.map(|activity| Live { activity, widths });
        if let (Some(leaving), Some(live)) = (&self.live_leaving, &self.live)
            && sides::key(&leaving.live.activity) == sides::key(&live.activity)
        {
            self.live_leaving = None;
        }
        self.live_second = None;
        if appearing && !self.activity.is_live() {
            let motion = match (self.mode, self.live.is_some()) {
                (Mode::Peek, _) => Motion::Peek,
                (_, true) => Motion::Pop,
                (_, false) => Motion::Close,
            };
            self.retarget(motion);
        }
    }

    fn let_live_go(&mut self) {
        let shown = self.live_alpha(self.morph.current()) * smoothstep(0.8, 1.0, self.attract.value);
        let lock = |live: &Live| matches!(live.activity, Activity::Lock(_));
        self.live_leaving = self.live.clone().filter(|live| shown > SHOWN && !lock(live)).map(|live| {
            let mut fade = Spring::new(shown, LEAVE, UNIT_PRECISION);
            fade.animate_to(0.0, LEAVE);
            Leaving { live, fade }
        });
    }

    pub(super) fn step_spin(&mut self, dt: f32) {
        let leaving = self.live_leaving.as_ref().map(|leaving| &leaving.live.activity);
        let mood = self.activities.iter().chain(leaving).find_map(|activity| match activity {
            Activity::Agent(agent) => Some(agent.mood),
            _ => None,
        });
        if let Some(mood) = mood {
            self.spin.follow(mood, dt);
            self.said.follow(mood, dt);
        }
    }

    pub(super) fn step_live_leaving(&mut self, dt: f32) {
        let Some(leaving) = &mut self.live_leaving else { return };
        leaving.fade.step(dt);
        if leaving.fade.is_settled() {
            self.live_leaving = None;
        }
    }

    fn leaving_alpha(&self, leaving: &Leaving) -> f32 {
        self.fading_out(&leaving.fade)
    }

    pub(super) fn leaving_live(&self) -> Option<(&Activity, f32)> {
        self.live_leaving.as_ref().map(|leaving| (&leaving.live.activity, self.leaving_alpha(leaving)))
    }

    pub(super) fn fading_out(&self, fade: &Spring) -> f32 {
        let covered = [&self.panel_fade, &self.drop_fade, &self.ask_fade].map(|fade| fade.value.clamp(0.0, 1.0));
        let covered = covered.into_iter().fold(0.0, f32::max);
        fade.value.clamp(0.0, 1.0) * smoothstep(0.8, 1.0, self.attract.value) * (1.0 - covered)
    }

    pub(super) fn live_geometry(&self) -> Geometry {
        let [leading, trailing] = self.live.as_ref().map_or([0.0; 2], |live| live.widths);
        match self.style {
            Style::Notch => {
                let wing = NOTCH_WING.max(LANE_INSET + leading.max(trailing) + CAMERA_CLEARANCE);
                Geometry { w: self.notch.x + 2.0 * wing, h: self.notch.y, r: 10.0, ear: 5.0 }
            }
            Style::Island => {
                let w = ISLAND_W.max(2.0 * LANE_INSET + leading + ISLAND_GAP + trailing);
                Geometry { w, h: ISLAND_H, r: ISLAND_H * 0.5, ear: 0.0 }
            }
        }
    }

    pub(super) fn shows_live(&self) -> bool {
        self.live.is_some() && !self.activity.is_live() && matches!(self.mode, Mode::Idle | Mode::Peek)
    }

    pub(super) fn live_moves(&self) -> bool {
        let agent = |activity: &Activity| match activity {
            Activity::Agent(agent) => agents::moves(agent) || !self.spin.is_settled() || !self.said.is_settled(),
            Activity::Video(live) => video::continuous(live),
            Activity::Lock(opening) => *opening && self.lock_opening(),
            Activity::Tool(..) => false,
        };
        (self.shows_live() && self.live.as_ref().is_some_and(|live| agent(&live.activity)))
            || (self.sides_shown() && self.sides.iter().any(|side| agent(&side.activity)))
            || self.live_leaving.is_some()
    }

    pub(super) fn live_tick(&mut self) -> bool {
        let Some(second) = self.timer_second() else { return false };
        if self.live_second == Some(second) {
            return false;
        }
        self.live_second = Some(second);
        true
    }

    pub(super) fn live_wakeup(&self, now: Instant) -> Option<Instant> {
        let at = self.live.as_ref().and_then(|live| ticks_from(&live.activity))?;
        let into = now_ms().abs_diff(at) % SECOND_MS;
        Some(now + Duration::from_millis(SECOND_MS - into))
    }

    pub(super) fn live_ui(&mut self, ui: &egui::Ui, env: &Env, g: Geometry, body: Rect) -> Vec<Action> {
        self.live_controls = None;
        if let Some(leaving) = self.live_leaving.clone() {
            let parts = self.live_parts(ui, env, &leaving.live.activity, body);
            self.paint_live(ui, env, &leaving.live.activity, &parts, body, (self.leaving_alpha(&leaving), false));
        }
        let Some(live) = self.live.clone() else { return Vec::new() };
        let parts = self.live_parts(ui, env, &live.activity, body);
        let room = match parts.controls {
            Some(size) if self.live_hover.target > 0.0 => CONTROLS_GAP + size.x,
            _ => 0.0,
        };
        self.fit_live([parts.sizes[0].x, parts.sizes[1].x + room]);
        let alpha = self.live_alpha(g) * smoothstep(0.8, 1.0, self.attract.value);
        let actions = self.paint_live(ui, env, &live.activity, &parts, body, (alpha, true));
        if !actions.is_empty() {
            self.wait_for_the_pointer_to_come_back();
        }
        actions
    }

    /// Opens the live activity's controls (a meeting's join and hide buttons, say) while the pointer is on it.
    pub(super) fn hover_live(&mut self, cursor: Option<Pos2>) -> bool {
        let controls =
            matches!(&self.live, Some(Live { activity: Activity::Tool(_, node), .. }) if node.controls.is_some());
        let over = controls
            && self.shows_live()
            && self.preview.is_none()
            && self.attract.value > DRAWN_OUT
            && !self.hovering_side()
            && cursor.is_some_and(|c| self.hover_zone(self.live_geometry()).contains(c));
        let target = if over { 1.0 } else { 0.0 };
        if self.live_hover.target == target {
            return false;
        }
        self.live_hover.animate_to(target, HOVER);
        true
    }

    pub(in crate::notch) fn hovers_live_controls(&self, cursor: Option<Pos2>) -> bool {
        cursor.is_some_and(|at| self.live_controls.is_some_and(|zone| zone.contains(at)))
    }

    fn live_parts(&self, ui: &egui::Ui, env: &Env, activity: &Activity, body: Rect) -> Parts {
        let canvas = Canvas::new(ui.painter());
        let look = match activity {
            Activity::Video(video) => self.video_look(video),
            _ => video::Look::default(),
        };
        let sizes = measure(&canvas, env, activity, body.width());
        let controls = match activity {
            Activity::Tool(slot, LiveNode { controls: Some(node), .. }) => {
                Some(crate::ui::element::Measure { canvas: &canvas, env, slot: *slot }.size(node, body.width()))
            }
            _ => None,
        };
        Parts { sizes, controls, look }
    }

    fn paint_live(
        &mut self,
        ui: &egui::Ui,
        env: &Env,
        activity: &Activity,
        parts: &Parts,
        body: Rect,
        (alpha, main): (f32, bool),
    ) -> Vec<Action> {
        if alpha <= SHOWN {
            return Vec::new();
        }
        let (sizes, look) = (parts.sizes, parts.look);
        let mut canvas = Canvas::new(ui.painter());
        let center_y = match self.style {
            Style::Notch => body.top() + self.notch.y * 0.5,
            Style::Island => body.center().y,
        };
        let leading =
            Rect::from_min_size(pos2(self.lead(body.left() + LANE_INSET), center_y - sizes[0].y * 0.5), sizes[0]);
        let trailing =
            Rect::from_min_size(pos2(body.right() - LANE_INSET - sizes[1].x, center_y - sizes[1].y * 0.5), sizes[1]);
        let clip = ui.painter().with_clip_rect(body.expand(1.0));
        let origin = pos2(body.center().x, body.top());
        let mut actions = Vec::new();
        match activity {
            Activity::Tool(slot, node) => {
                let shown = if main { self.live_hover.value.clamp(0.0, 1.0) } else { 0.0 };
                let controls = node.controls.as_ref().zip(parts.controls);
                let room = controls.map_or(0.0, |(_, size)| (CONTROLS_GAP + size.x) * shown);
                let trailing = trailing.translate(vec2(-room, 0.0));
                let parts = [(&node.leading, leading), (&node.trailing, trailing)];
                paint_lane(ui, env, &mut canvas, &mut self.textures, *slot, &parts);
                if let Some((controls, size)) = controls.filter(|_| shown > SHOWN) {
                    let rect =
                        Rect::from_min_size(pos2(body.right() - LANE_INSET - size.x, center_y - size.y * 0.5), size);
                    let pressable = shown > 0.5
                        && alpha > PRESSABLE_FROM
                        && self.shows_live()
                        && self.preview.is_none()
                        && !self.locked;
                    let mut buttons = Canvas::new(ui.painter());
                    let takes = pressable.then_some(Surface::Live);
                    let parts = [(controls, rect)];
                    actions = paint_parts(ui, env, &mut buttons, &mut self.textures, *slot, &parts, takes);
                    buttons.paint(&clip, origin, 1.0, Vec2::ZERO, alpha * shown);
                    if pressable {
                        self.live_controls = Some(rect.expand(CONTROLS_REACH));
                    }
                }
            }
            Activity::Agent(agent) => {
                agents::paint_leading(&mut canvas, agent, pos2(leading.left(), center_y), &self.spin);
                let right = pos2(trailing.right(), center_y);
                match main {
                    true => agents::paint_trailing_said(&mut canvas, agent, right, env.now_ms, &mut self.said),
                    false => agents::paint_trailing(&mut canvas, agent, right, env.now_ms),
                }
            }
            Activity::Lock(_) => {
                let x = if self.style == Style::Island { body.center().x } else { leading.center().x };
                lock::paint_padlock(&mut canvas, pos2(x, center_y), lock::GLYPH, self.lock_share());
            }
            Activity::Video(live) => {
                let center = pos2(leading.center().x, center_y);
                video::paint_leading(&mut canvas, live, look, center, self.clock.seconds());
                video::paint_trailing(&mut canvas, live, look, pos2(trailing.right(), center_y));
            }
        }
        canvas.paint(&clip, origin, 1.0, Vec2::ZERO, alpha);
        actions
    }

    pub(super) fn fit_live(&mut self, widths: [f32; 2]) {
        let Some(live) = &mut self.live else { return };
        let moved = live.widths.iter().zip(widths).any(|(was, now)| (was - now).abs() > REMEASURE);
        if !moved {
            return;
        }
        live.widths = widths;
        if !matches!(self.mode, Mode::Idle | Mode::Peek) {
            return;
        }
        if self.morph.springs().iter().all(|spring| spring.is_settled()) {
            self.retarget(if self.mode == Mode::Peek { Motion::Peek } else { Motion::Resize });
        } else {
            self.follow_target();
        }
    }

    fn timer_second(&self) -> Option<u64> {
        let at = ticks_from(&self.live.as_ref()?.activity)?;
        Some(now_ms().abs_diff(at) / SECOND_MS)
    }

    pub(super) fn live_alpha(&self, g: Geometry) -> f32 {
        let (idle, target, open) = (self.idle_geometry(), self.live_geometry(), self.expanded_geometry());
        let progress = |v: f32, from: f32, to: f32| if to - from > 1.0 { (v - from) / (to - from) } else { 0.0 };
        let grown = match self.style {
            Style::Notch => progress(g.w, idle.w, target.w),
            Style::Island => 1.0,
        };
        let beyond = progress(g.h, self.peek_of(target).h, open.h);
        self.live_fade.value.clamp(0.0, 1.0) * smoothstep(0.35, 0.9, grown) * (1.0 - smoothstep(0.0, 0.3, beyond))
    }
}

fn measure(canvas: &Canvas, env: &Env, activity: &Activity, room: f32) -> [Vec2; 2] {
    match activity {
        Activity::Video(_) => [Vec2::splat(video::MARK), vec2(video::trailing_width(canvas), video::MARK)],
        Activity::Tool(slot, node) => {
            let measure = crate::ui::element::Measure { canvas, env, slot: *slot };
            [measure.size(&node.leading, room), measure.size(&node.trailing, room)]
        }
        Activity::Agent(agent) => agent_sizes(canvas, agent, env.now_ms),
        Activity::Lock(_) => [Vec2::splat(lock::GLYPH), Vec2::ZERO],
    }
}

fn agent_sizes(canvas: &Canvas, agent: &AgentLive, now_ms: u64) -> [Vec2; 2] {
    let height = agents::MARK;
    [vec2(agents::leading_width(canvas, agent), height), vec2(agents::trailing_width(canvas, agent, now_ms), height)]
}

pub(super) fn paint_lane(
    ui: &egui::Ui,
    env: &Env,
    canvas: &mut Canvas,
    textures: &mut crate::ui::element::Textures,
    slot: ToolSlot,
    parts: &[(&Node, Rect)],
) {
    paint_parts(ui, env, canvas, textures, slot, parts, None);
}

pub(super) fn paint_parts(
    ui: &egui::Ui,
    env: &Env,
    canvas: &mut Canvas,
    textures: &mut crate::ui::element::Textures,
    slot: ToolSlot,
    parts: &[(&Node, Rect)],
    takes: Option<Surface>,
) -> Vec<crate::bridge::Action> {
    let mut actions = Vec::new();
    let mut keys = Vec::new();
    let mut state = crate::ui::element::SurfaceState::default();
    let mut scene = Scene {
        ui,
        canvas,
        env,
        state: &mut state,
        textures,
        actions: &mut actions,
        keys: &mut keys,
        slot,
        surface: takes.unwrap_or(Surface::Live),
        live: takes.is_some(),
    };
    for (index, (node, rect)) in parts.iter().enumerate() {
        scene.paint(node, *rect, egui::Id::new(("notchbase.lane", slot.0, index, takes.map(|surface| surface as u8))));
    }
    actions
}

fn ticks_from(activity: &Activity) -> Option<u64> {
    match activity {
        Activity::Tool(_, node) => timer_of(node),
        Activity::Agent(agent) => Some(agent.since_ms),
        Activity::Video(_) | Activity::Lock(_) => None,
    }
}

fn timer_of(node: &LiveNode) -> Option<u64> {
    let mut found = None;
    for part in [&node.leading, &node.trailing] {
        visit(part, &mut |node| {
            if let Node::Timer { at, .. } = node {
                found.get_or_insert(*at);
            }
        });
    }
    found
}

fn visit(node: &Node, each: &mut impl FnMut(&Node)) {
    each(node);
    for child in node.children() {
        visit(child, each);
    }
}

#[cfg(test)]
pub(super) mod tests {
    use notchbase_sdk::protocol::{TextStyle, TimerMode, Tone};

    use super::*;
    use crate::notch::activity::ActivityKind;
    use crate::notch::tests::fixtures::{
        AWAY, OVER, REST, Rig, click, draw_out, island, ms, peek, sample_notice, settle, shortcuts,
    };

    pub(in crate::notch) fn recording(since: u64) -> Activity {
        let leading = Node::Dot { color: Tone::Red.color(), size: 9.0 };
        let trailing =
            Node::Timer { at: since, mode: TimerMode::Elapsed, style: TextStyle::Value, tone: Tone::Primary };
        Activity::Tool(ToolSlot(3), LiveNode { leading, trailing, controls: None, urgent: false })
    }

    pub(in crate::notch) fn claude() -> Activity {
        let mood = crate::bridge::Mood::Thinking;
        Activity::Agent(AgentLive { provider: crate::assistant::Provider::Claude, mood, since_ms: now_ms(), count: 1 })
    }

    pub(in crate::notch) fn download() -> Activity {
        Activity::Video(crate::bridge::VideoLive {
            id: 4,
            site: crate::webvideo::Site::YouTube,
            audio: false,
            stage: crate::bridge::VideoStage::Downloading,
            share: 0.4,
            title: "Me at the zoo".into(),
            detail: String::new(),
            cover: None,
        })
    }

    #[test]
    fn live_keeps_island_out() {
        let mut n = island();
        let t0 = Instant::now();
        let live = recording(now_ms());
        assert!(n.set_activities(vec![live.clone()]));
        assert!(!n.set_activities(vec![live]));
        assert!(n.needs_capsule());
        assert!(n.shows_live());
        assert!(n.next_wakeup().is_some_and(|at| at <= t0 + Duration::from_secs(1) + ms(5)));
        n.tick(t0, Some(AWAY));
        settle(&mut n);
        assert!(n.live_alpha(n.morph.current()) > 0.99);

        n.set_activities(Vec::new());
        assert!(!n.shows_live());
        n.tick(t0 + ms(1_100), Some(AWAY));
        settle(&mut n);
        assert!(n.is_collapsed());
    }

    #[test]
    fn notice_hides_live_briefly() {
        let mut n = island();
        let t0 = Instant::now();
        n.set_activities(vec![recording(now_ms())]);
        n.start_activity(ActivityKind::Tool(sample_notice(2_200)), t0);
        assert!(!n.shows_live());
        n.tick(t0 + ms(2_300), Some(AWAY));
        assert_eq!(n.mode, Mode::Idle);
        assert!(n.shows_live());
    }

    fn meeting() -> Activity {
        let leading = Node::Dot { color: Tone::Blue.color(), size: 9.0 };
        let trailing = Node::Timer {
            at: now_ms() + 60_000,
            mode: TimerMode::Remaining,
            style: TextStyle::Value,
            tone: Tone::Primary,
        };
        let hide = Node::Button {
            label: String::new(),
            icon: Some(notchbase_sdk::protocol::Icon::Xmark),
            style: Default::default(),
            on_press: Some(notchbase_sdk::protocol::HandlerId(7)),
        };
        Activity::Tool(ToolSlot(3), LiveNode { leading, trailing, controls: Some(hide), urgent: false })
    }

    #[test]
    fn hovering_live_shows_its_controls() {
        let rig = Rig::new();
        let keys = shortcuts(None);
        let mut n = island();
        n.set_activities(vec![meeting()]);
        let t0 = Instant::now();
        n.tick(t0, Some(AWAY));
        rig.frame(&mut n, &keys, Vec::new());
        settle(&mut n);
        let narrow = n.live.as_ref().expect("live").widths[1];
        assert!(n.live_controls.is_none(), "no buttons until the pointer comes");

        draw_out(&mut n, t0, OVER);
        for _ in 0..3 {
            rig.frame(&mut n, &keys, Vec::new());
            settle(&mut n);
        }
        assert!(n.live_hover.value > 0.99);
        assert!(n.live.as_ref().expect("live").widths[1] > narrow + 10.0, "the shape makes room for the buttons");
        let at = n.live_controls.expect("the hide button takes presses").center();

        n.tick(t0 + REST, Some(at));
        assert!(n.wants_input(at));
        let actions = click(&rig, &mut n, at);
        let hid = |action: &Action| {
            matches!(action, Action::Tool(ToolSlot(3), Surface::Live, notchbase_sdk::protocol::HandlerId(7), _))
        };
        assert!(actions.iter().any(hid), "{actions:?}");
        assert_ne!(n.mode, Mode::Expanded, "the button takes the click instead of the shape behind it");
        n.tick(t0 + REST * 20, Some(at));
        assert_ne!(n.mode, Mode::Expanded, "resting on the button never opens the panel");

        n.tick(t0 + REST * 21, Some(AWAY));
        settle(&mut n);
        assert_eq!(n.live_hover.value, 0.0);
    }

    #[test]
    fn live_without_controls_stays_put() {
        let rig = Rig::new();
        let keys = shortcuts(None);
        let mut n = island();
        n.set_activities(vec![recording(now_ms())]);
        draw_out(&mut n, Instant::now(), OVER);
        rig.frame(&mut n, &keys, Vec::new());
        assert_eq!(n.live_hover.target, 0.0);
        assert!(n.live_controls.is_none());
    }

    #[test]
    fn hovering_live_opens_panel() {
        let mut n = island();
        n.set_activities(vec![recording(now_ms())]);
        let t0 = peek(&mut n, Instant::now(), OVER);
        assert_eq!(n.mode, Mode::Peek);
        assert!(n.shows_live());
        n.tick(t0 + ms(320) + crate::notch::moves::surge::SURGE, Some(OVER));
        assert_eq!(n.mode, Mode::Expanded);
        settle(&mut n);
        assert_eq!(n.live_alpha(n.morph.current()), 0.0);
    }

    #[test]
    fn finished_live_fades_out() {
        let mut n = Notch::with_preview(Style::Notch, Some(egui::vec2(185.0, 32.0)), None);
        n.set_activities(vec![claude()]);
        settle(&mut n);
        n.set_activities(Vec::new());
        let leaving = n.live_leaving.as_ref().expect("the finished run is still drawn while it leaves");
        assert!(leaving.fade.value > 0.99 && leaving.fade.target == 0.0);
        assert!(n.is_animating());
        n.step(1.0 / 60.0);
        let fading = n.live_leaving.as_ref().map_or(0.0, |leaving| n.leaving_alpha(leaving));
        assert!(fading > 0.1 && fading < 0.99, "{fading}");
        settle(&mut n);
        assert!(n.live_leaving.is_none() && !n.is_animating());

        n.set_activities(vec![claude()]);
        settle(&mut n);
        n.set_activities(vec![recording(now_ms())]);
        let swapped = n.live_leaving.as_ref().map(|leaving| &leaving.live.activity);
        assert!(matches!(swapped, Some(Activity::Agent(_))), "a swap fades the old one out under the new one");
        for _ in 0..30 {
            n.step(1.0 / 60.0);
        }
        n.set_activities(Vec::new());
        assert!(n.live_leaving.as_ref().is_some_and(|leaving| matches!(leaving.live.activity, Activity::Tool(..))));
        n.set_activities(vec![recording(now_ms())]);
        assert!(n.live_leaving.is_none(), "an activity that comes straight back is drawn once, not twice");
    }

    #[test]
    fn status_word_rolls_in() {
        let rig = crate::notch::tests::fixtures::Rig::new();
        let mut n = Notch::with_preview(Style::Notch, Some(egui::vec2(185.0, 32.0)), None);
        n.set_activities(vec![claude()]);
        for _ in 0..40 {
            rig.frame(&mut n, &[], Vec::new());
        }
        let doing = |verb| match claude() {
            Activity::Agent(agent) => Activity::Agent(AgentLive { mood: crate::bridge::Mood::Doing(verb), ..agent }),
            other => other,
        };
        n.set_activities(vec![doing("Editing")]);
        rig.frame(&mut n, &[], Vec::new());
        assert!(!n.said.is_settled() && n.is_animating(), "the clock rolls up and out as Editing rolls in");
        for _ in 0..40 {
            rig.frame(&mut n, &[], Vec::new());
        }
        assert!(n.said.is_settled());
        n.set_activities(vec![doing("Reading")]);
        rig.frame(&mut n, &[], Vec::new());
        assert!(!n.said.is_settled(), "every new verb rolls in");
    }

    #[test]
    fn padlock_leaves_by_folding() {
        let mut n = Notch::with_preview(Style::Notch, Some(egui::vec2(185.0, 32.0)), None);
        n.set_locked(true);
        settle(&mut n);
        n.set_locked(false);
        n.lock_tick(Instant::now() + Duration::from_secs(2));
        assert!(n.live.is_none());
        assert!(n.live_leaving.is_none());
    }

    #[test]
    fn notch_grows_wings() {
        let mut n = Notch::with_preview(Style::Notch, Some(egui::vec2(185.0, 32.0)), None);
        n.set_activities(vec![recording(now_ms())]);
        settle(&mut n);
        let g = n.morph.current();
        assert_eq!((g.w, g.h), (185.0 + 2.0 * NOTCH_WING, 32.0));
        assert!(n.live_alpha(g) > 0.99);
    }

    #[test]
    fn first_leads_next_become_sides() {
        let mut n = island();
        n.set_activities(vec![claude(), recording(now_ms()), claude(), recording(0)]);
        assert!(matches!(n.live.as_ref().map(|live| &live.activity), Some(Activity::Agent(_))));
        assert_eq!(n.sides.len(), 2);
        assert!(n.live_moves());
    }

    #[test]
    fn download_leads_first() {
        let mut n = island();
        n.set_activities(vec![claude(), recording(now_ms()), download()]);
        assert!(matches!(n.live.as_ref().map(|live| &live.activity), Some(Activity::Video(_))));
        assert!(matches!(n.activities.as_slice(), [Activity::Video(_), Activity::Agent(_), Activity::Tool(..)]));
        assert_eq!(n.sides.len(), 2, "Claude and the tool become the ears");

        n.set_locked(true);
        let order = n.activities.as_slice();
        assert!(matches!(order, [Activity::Lock(false), Activity::Video(_), Activity::Agent(_), Activity::Tool(..)]));
        n.set_locked(false);
        n.lock_tick(Instant::now() + Duration::from_secs(2));
        assert!(matches!(n.live.as_ref().map(|live| &live.activity), Some(Activity::Video(_))));
    }

    #[test]
    fn dictation_leads_over_everything() {
        let mut n = island();
        let Activity::Tool(slot, mut live) = recording(now_ms()) else { unreachable!() };
        live.urgent = true;
        let dictating = Activity::Tool(ToolSlot(slot.0 + 1), live);
        n.set_now_playing(Some(&crate::notch::tests::fixtures::song(true, 10.0)));
        n.set_activities(vec![claude(), download(), recording(0), dictating.clone()]);
        assert_eq!(n.live.as_ref().map(|live| &live.activity), Some(&dictating), "in the middle, ahead of a download");
        assert!(!n.music_leads(), "and ahead of a new song");
        assert!(matches!(n.activities.as_slice(), [a, Activity::Video(_), ..] if *a == dictating));

        n.set_locked(true);
        assert!(matches!(n.activities.as_slice(), [Activity::Lock(false), a, ..] if *a == dictating));
    }

    #[test]
    fn download_keeps_middle_for_song() {
        let mut n = island();
        n.set_activities(vec![claude(), download()]);
        n.set_now_playing(Some(&crate::notch::tests::fixtures::song(true, 10.0)));
        assert!(n.music_revealed() && !n.music_leads() && !n.shows_music());
        assert!(matches!(n.live.as_ref().map(|live| &live.activity), Some(Activity::Video(_))));
        assert!(matches!(n.sides.first().map(|side| &side.activity), Some(Activity::Agent(_))));
        assert!(!n.dances(), "the song's dance would poke out under the download's line of light");

        n.set_activities(vec![claude()]);
        assert!(n.music_leads() && n.dances(), "once the download is done a new song takes the middle again");
    }

    #[test]
    fn song_leads_while_revealed() {
        let mut n = island();
        let t0 = Instant::now();
        n.set_activities(vec![claude()]);
        n.set_now_playing(Some(&crate::notch::tests::fixtures::song(true, 10.0)));
        assert!(n.music_leads() && n.live.is_none(), "a new song takes the middle");
        assert!(matches!(n.sides.first().map(|side| &side.activity), Some(Activity::Agent(_))));

        let later = t0 + crate::config::current().music.reveal() + ms(100);
        assert!(n.expire_music(later));
        assert!(!n.music_leads() && !n.music_revealed(), "the song tucks away");
        assert!(matches!(n.live.as_ref().map(|live| &live.activity), Some(Activity::Agent(_))));
        settle(&mut n);
        assert!(n.sides.is_empty(), "no ears left once the song has gone");
    }

    #[test]
    fn clears_every_camera() {
        let notches = [(185.0, 32.0), (185.0, 38.0), (180.0, 32.0), (200.0, 37.0), (160.0, 30.0)];
        for (width, height) in notches {
            for timer in [29.0, 36.0, 51.0, 65.0] {
                let mut n = Notch::with_preview(Style::Notch, Some(egui::vec2(width, height)), None);
                n.set_activities(vec![recording(now_ms())]);
                n.fit_live([9.0, timer]);
                settle(&mut n);
                let body = n.body_rect(n.morph.current());
                let camera = Rect::from_center_size(pos2(body.center().x, height * 0.5), egui::vec2(width, height));
                assert_eq!(body.height(), height);
                let dot_right = body.left() + LANE_INSET + 9.0;
                let timer_left = body.right() - LANE_INSET - timer;
                assert!(dot_right + CAMERA_CLEARANCE <= camera.left(), "{width}x{height} dot");
                assert!(timer_left >= camera.right() + CAMERA_CLEARANCE - 0.01, "{width}x{height} timer {timer}");
            }
        }
    }
}
