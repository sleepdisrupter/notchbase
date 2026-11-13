//! A video download's live activity: the rim that fills with progress, its marks and its few words.

use std::f32::consts::{FRAC_PI_2, PI, TAU};

use egui::{Align2, Color32, Id, Painter, Pos2, Rect, Response, Sense, Shape, Stroke, Vec2, pos2, vec2};

use crate::bridge::{Action, Activity, VideoLive, VideoStage};
use crate::notch::Notch;
use crate::notch::geometry::Silhouette;
use crate::platform::Style;
use crate::ui::canvas::{Canvas, fade, scale_about};
use crate::ui::color::{self, label, secondary_label, white};
use crate::ui::glass::{self, Glass};
use crate::ui::glyph::{self, Glyph};
use crate::ui::spring::{Spring, SpringConfig, UNIT_PRECISION, lerp, smoothstep};
use crate::ui::{controls, font};
use crate::webvideo::Site;

pub(in crate::notch) const MARK: f32 = 15.0;
const RADIUS: f32 = 6.0;
const WIDTH: f32 = 1.8;
const TRACK: f32 = 0.16;
const ARC_STEPS: f32 = 48.0;
const TIP: f32 = 2.1;
const HALO: f32 = 2.8;
const PLAY: f32 = 5.0;
const NOTE: f32 = 7.0;
const CHECK: f32 = 8.0;
const FAILED: f32 = 13.0;
const COMET: f32 = 0.32;
const COMET_STEPS: usize = 14;
const SPIN_PERIOD: f32 = 0.95;
const GLOW_PERIOD: f32 = 1.3;
const FILL: SpringConfig = SpringConfig::new(0.55, 1.0);
const POP: SpringConfig = SpringConfig::new(0.42, 0.62);
const WORDS_MOST: f32 = 150.0;
const TITLE_MOST: usize = 26;
const WORD_GAP: f32 = 7.0;
const CANCEL: f32 = 20.0;
const CANCEL_GAP: f32 = 8.0;
const CROSS: f32 = 9.0;
const PRESS_INSET: f32 = 1.0;
const RIM_GAP: f32 = 1.6;
const RIM_CORE: f32 = 1.5;
const RIM_GLOWS: [(f32, f32); 3] = [(9.0, 0.07), (5.5, 0.13), (3.0, 0.24)];
const RIM_CORNER_STEPS: usize = 14;
const RIM_HEAD: f32 = 1.9;
const SHEEN_PERIOD: f32 = 2.1;
const SHEEN_LENGTH: f32 = 0.2;
const SHEEN_STEPS: usize = 10;
const STAGES: [VideoStage; 6] = [
    VideoStage::Preparing,
    VideoStage::Finding,
    VideoStage::Downloading,
    VideoStage::Joining,
    VideoStage::Saved,
    VideoStage::Failed,
];

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(in crate::notch) struct Look {
    pub(in crate::notch) share: f32,
    pub(in crate::notch) done: f32,
}

struct Shown {
    id: u64,
    share: Spring,
    done: Spring,
}

#[derive(Default)]
pub(in crate::notch) struct Reel {
    shown: Vec<Shown>,
}

impl Reel {
    pub(in crate::notch) fn follow(&mut self, activities: &[Activity]) {
        let lives: Vec<&VideoLive> = activities.iter().filter_map(video_of).collect();
        self.shown.retain(|shown| lives.iter().any(|live| live.id == shown.id));
        for live in lives {
            let done = if live.stage == VideoStage::Saved { 1.0 } else { 0.0 };
            match self.shown.iter_mut().find(|shown| shown.id == live.id) {
                Some(shown) => {
                    if shown.share.target != live.share {
                        shown.share.animate_to(live.share, FILL);
                    }
                    if shown.done.target != done {
                        shown.done.animate_to(done, POP);
                    }
                }
                None => self.shown.push(Shown {
                    id: live.id,
                    share: Spring::new(live.share, FILL, UNIT_PRECISION),
                    done: Spring::new(done, POP, UNIT_PRECISION),
                }),
            }
        }
    }

    pub(in crate::notch) fn step(&mut self, dt: f32, pinned: bool) {
        for shown in &mut self.shown {
            if pinned {
                shown.share.snap_to(shown.share.target);
                shown.done.snap_to(shown.done.target);
            }
            shown.share.step(dt);
            shown.done.step(dt);
        }
    }

    pub(in crate::notch) fn moving(&self) -> bool {
        self.shown.iter().any(|shown| !shown.share.is_settled() || !shown.done.is_settled())
    }

    pub(in crate::notch) fn look(&self, live: &VideoLive) -> Look {
        self.shown.iter().find(|shown| shown.id == live.id).map_or(
            Look { share: live.share, done: if live.stage == VideoStage::Saved { 1.0 } else { 0.0 } },
            |shown| Look { share: shown.share.value.clamp(0.0, 1.0), done: shown.done.value.max(0.0) },
        )
    }
}

fn video_of(activity: &Activity) -> Option<&VideoLive> {
    match activity {
        Activity::Video(live) => Some(live),
        _ => None,
    }
}

pub(in crate::notch) fn continuous(live: &VideoLive) -> bool {
    !matches!(live.stage, VideoStage::Saved | VideoStage::Failed)
}

pub(in crate::notch) fn cancellable(live: &VideoLive) -> bool {
    continuous(live)
}

pub(in crate::notch) fn tint(site: Site) -> Color32 {
    match site {
        Site::YouTube => color::RED,
        Site::Instagram => color::YELLOW,
        _ => color::PEARL,
    }
}

fn words(stage: VideoStage, share: f32) -> (String, Color32) {
    match stage {
        VideoStage::Preparing => ("Setup".to_owned(), secondary_label()),
        VideoStage::Finding => ("Finding".to_owned(), secondary_label()),
        VideoStage::Downloading => (format!("{}%", (share * 100.0).floor() as u32), label()),
        VideoStage::Joining => ("Saving".to_owned(), secondary_label()),
        VideoStage::Saved => ("Saved".to_owned(), color::GREEN),
        VideoStage::Failed => ("Failed".to_owned(), color::RED),
    }
}

fn words_font(stage: VideoStage) -> egui::FontId {
    if stage == VideoStage::Downloading { font::value() } else { font::headline() }
}

pub(in crate::notch) fn trailing_width(canvas: &Canvas) -> f32 {
    let width = |stage| canvas.text_width(words(stage, 1.0).0, words_font(stage));
    STAGES.into_iter().map(width).fold(0.0, f32::max).min(WORDS_MOST)
}

pub(in crate::notch) fn paint_trailing(canvas: &mut Canvas, live: &VideoLive, look: Look, right_center: Pos2) {
    let (text, tone) = words(live.stage, look.share);
    canvas.text(right_center, Align2::RIGHT_CENTER, text, words_font(live.stage), tone);
}

pub(in crate::notch) fn paint_leading(canvas: &mut Canvas, live: &VideoLive, look: Look, center: Pos2, seconds: f32) {
    let tint = tint(live.site);
    let glow = 0.5 + 0.5 * (seconds * TAU / GLOW_PERIOD).sin();
    match live.stage {
        VideoStage::Failed => glyph::paint(canvas, Glyph::Exclamation, center, FAILED, color::RED),
        VideoStage::Saved => {
            let landed = smoothstep(0.0, 0.6, look.done);
            canvas.ring(center, RADIUS, Stroke::new(WIDTH, color::mix(tint, color::GREEN, landed)));
            let mark = canvas.mark();
            glyph::paint(canvas, Glyph::Checkmark, center, CHECK, color::GREEN);
            let pop = scale_about(center, look.done.clamp(0.0, 1.3));
            for shape in canvas.since(mark) {
                fade(shape, landed);
                shape.transform(pop);
            }
        }
        VideoStage::Finding => {
            canvas.ring(center, RADIUS, Stroke::new(WIDTH, white(TRACK)));
            comet(canvas, center, seconds, tint);
        }
        VideoStage::Preparing => {
            canvas.ring(center, RADIUS, Stroke::new(WIDTH, white(TRACK)));
            arc(canvas, center, look.share, color::PEARL, glow);
        }
        VideoStage::Downloading => {
            canvas.ring(center, RADIUS, Stroke::new(WIDTH, white(TRACK)));
            arc(canvas, center, look.share, tint, glow);
            kind_mark(canvas, live, center, tint);
        }
        VideoStage::Joining => {
            canvas.ring(center, RADIUS, Stroke::new(WIDTH, tint.gamma_multiply(lerp(0.55, 1.0, glow))));
            kind_mark(canvas, live, center, tint);
        }
    }
}

fn kind_mark(canvas: &mut Canvas, live: &VideoLive, center: Pos2, tint: Color32) {
    let ink = color::mix(tint, Color32::WHITE, 0.25);
    if live.audio {
        glyph::paint(canvas, Glyph::Note, center, NOTE, ink);
    } else {
        glyph::paint(canvas, Glyph::Play, center + vec2(0.4, 0.0), PLAY, ink);
    }
}

fn at(center: Pos2, turn: f32) -> Pos2 {
    let angle = -FRAC_PI_2 + TAU * turn;
    center + vec2(angle.cos(), angle.sin()) * RADIUS
}

fn arc(canvas: &mut Canvas, center: Pos2, share: f32, tint: Color32, glow: f32) {
    let share = share.clamp(0.0, 1.0);
    if share > 0.0 {
        let steps = (share * ARC_STEPS).ceil().max(1.0) as usize;
        let points: Vec<Pos2> = (0..=steps).map(|i| at(center, share * i as f32 / steps as f32)).collect();
        canvas.circle(points[0], WIDTH * 0.5, tint);
        canvas.add(Shape::line(points, Stroke::new(WIDTH, tint)));
    }
    let tip = at(center, share);
    canvas.circle(tip, TIP + HALO * glow, tint.gamma_multiply(0.16 + 0.16 * glow));
    canvas.circle(tip, TIP, color::mix(tint, Color32::WHITE, 0.45));
}

fn comet(canvas: &mut Canvas, center: Pos2, seconds: f32, tint: Color32) {
    let head = (seconds / SPIN_PERIOD).fract();
    for step in 0..COMET_STEPS {
        let (from, to) = (step as f32 / COMET_STEPS as f32, (step + 1) as f32 / COMET_STEPS as f32);
        let piece = vec![at(center, head - COMET * (1.0 - from)), at(center, head - COMET * (1.0 - to))];
        canvas.add(Shape::line(piece, Stroke::new(WIDTH, tint.gamma_multiply(to * to))));
    }
    canvas.circle(at(center, head), WIDTH * 0.5, color::mix(tint, Color32::WHITE, 0.35));
}

pub(in crate::notch) struct Opened {
    pub(in crate::notch) end: f32,
    pub(in crate::notch) action: Option<Action>,
}

pub(in crate::notch) struct Ear<'a> {
    pub(in crate::notch) live: &'a VideoLive,
    pub(in crate::notch) look: Look,
    pub(in crate::notch) start: f32,
    pub(in crate::notch) sign: f32,
    pub(in crate::notch) y: f32,
}

pub(in crate::notch) fn open(ui: &egui::Ui, canvas: &mut Canvas, ear: &Ear<'_>, shown: bool, input: bool) -> Opened {
    let title = shortened(if ear.live.title.is_empty() { ear.live.site.name() } else { &ear.live.title });
    let detail = match ear.live.stage {
        VideoStage::Downloading if ear.live.detail.is_empty() => words(ear.live.stage, ear.look.share).0,
        VideoStage::Downloading => format!("{} \u{b7} {}", words(ear.live.stage, ear.look.share).0, ear.live.detail),
        _ => ear.live.detail.clone(),
    };
    let widths =
        [canvas.text_width(title.as_str(), font::headline()), canvas.text_width(detail.as_str(), font::callout())];
    let gap = if detail.is_empty() { 0.0 } else { WORD_GAP };
    let width = widths[0] + gap + widths[1];
    let left = if ear.sign > 0.0 { ear.start } else { ear.start - width };
    let mut end = ear.start + ear.sign * width;
    if shown {
        canvas.text(pos2(left, ear.y), Align2::LEFT_CENTER, title, font::headline(), label());
        let after = pos2(left + widths[0] + gap, ear.y);
        canvas.text(after, Align2::LEFT_CENTER, detail, font::callout(), secondary_label());
    }
    let mut action = None;
    if cancellable(ear.live) {
        let center = pos2(end + ear.sign * (CANCEL_GAP + CANCEL * 0.5), ear.y);
        end = center.x + ear.sign * CANCEL * 0.5;
        if shown && cancel(ui, canvas, center, Id::new(("notchbase.video.cancel", ear.live.id)), input) {
            action = Some(Action::CancelVideo(ear.live.id));
        }
    }
    Opened { end, action }
}

pub(in crate::notch) fn cancel(ui: &egui::Ui, canvas: &mut Canvas, center: Pos2, id: Id, input: bool) -> bool {
    let rect = Rect::from_center_size(center, Vec2::splat(CANCEL));
    let response = input.then(|| ui.interact(rect, id, Sense::click()));
    let hover = controls::animate(ui, id.with("hover"), response.as_ref().is_some_and(Response::hovered));
    let down = response.as_ref().is_some_and(Response::is_pointer_button_down_on);
    let drawn = if down { rect.shrink(PRESS_INSET) } else { rect };
    glass::paint(canvas, drawn, drawn.height() * 0.5, Glass::lerp(Glass::CONTROL, Glass::TILE_HOVER, hover));
    glyph::paint(canvas, Glyph::Xmark, drawn.center(), CROSS, color::mix(label(), Color32::WHITE, hover));
    response.is_some_and(|r| r.clicked())
}

fn shortened(title: &str) -> String {
    match title.char_indices().nth(TITLE_MOST) {
        Some((at, _)) => format!("{}\u{2026}", title[..at].trim_end()),
        None => title.to_owned(),
    }
}

fn rim(body: Rect, radius: f32) -> Vec<(Pos2, f32)> {
    let radius = radius.min(body.height()).min(body.width() * 0.5).max(0.0);
    let reach = radius + RIM_GAP;
    let corner = |center: Pos2, from: f32, to: f32| {
        (0..=RIM_CORNER_STEPS).map(move |step| {
            let angle = lerp(from, to, step as f32 / RIM_CORNER_STEPS as f32);
            center + vec2(angle.cos(), angle.sin()) * reach
        })
    };
    let left = corner(pos2(body.left() + radius, body.bottom() - radius), PI, FRAC_PI_2);
    let right = corner(pos2(body.right() - radius, body.bottom() - radius), FRAC_PI_2, 0.0);
    let mut run = 0.0;
    let mut last: Option<Pos2> = None;
    left.chain(right)
        .map(|point| {
            run += last.map_or(0.0, |last| last.distance(point));
            last = Some(point);
            (point, run)
        })
        .collect()
}

fn stretch(path: &[(Pos2, f32)], from: f32, to: f32) -> Vec<Pos2> {
    let Some(&(_, whole)) = path.last() else { return Vec::new() };
    let (from, to) = (from.clamp(0.0, 1.0) * whole, to.clamp(0.0, 1.0) * whole);
    if to - from <= 0.0 {
        return Vec::new();
    }
    let point_at = |run: f32| {
        let next = path.iter().position(|(_, reached)| *reached >= run).unwrap_or(path.len() - 1).max(1);
        let ((a, start), (b, end)) = (path[next - 1], path[next]);
        a.lerp(b, if end > start { ((run - start) / (end - start)).clamp(0.0, 1.0) } else { 0.0 })
    };
    let inside = path.iter().filter(|(_, run)| *run > from && *run < to).map(|(point, _)| *point);
    std::iter::once(point_at(from)).chain(inside).chain(std::iter::once(point_at(to))).collect()
}

fn paint_rim(painter: &Painter, body: Rect, radius: f32, share: f32, tint: Color32, seconds: f32, alpha: f32) {
    let path = rim(body, radius);
    let lit = stretch(&path, 0.0, share);
    let (Some(&first), Some(&head)) = (lit.first(), lit.last()) else { return };
    for (width, strength) in RIM_GLOWS {
        painter.add(Shape::line(lit.clone(), Stroke::new(width, tint.gamma_multiply(strength * alpha))));
    }
    let core = color::mix(tint, Color32::WHITE, 0.3).gamma_multiply(alpha);
    painter.add(Shape::line(lit, Stroke::new(RIM_CORE, core)));
    painter.circle_filled(first, RIM_CORE * 0.5, core);
    let sweep = (seconds / SHEEN_PERIOD).fract() * (1.0 + SHEEN_LENGTH) - SHEEN_LENGTH;
    for step in 0..SHEEN_STEPS {
        let (a, b) = (step as f32 / SHEEN_STEPS as f32, (step + 1) as f32 / SHEEN_STEPS as f32);
        let piece = stretch(&path, (sweep + SHEEN_LENGTH * a) * share, (sweep + SHEEN_LENGTH * b) * share);
        let bright = (PI * (a + b) * 0.5).sin();
        painter.add(Shape::line(piece, Stroke::new(RIM_CORE, Color32::WHITE.gamma_multiply(0.55 * bright * alpha))));
    }
    let pulse = 0.5 + 0.5 * (seconds * TAU / GLOW_PERIOD).sin();
    painter.circle_filled(head, RIM_HEAD + 2.6 * pulse, tint.gamma_multiply((0.18 + 0.14 * pulse) * alpha));
    painter.circle_filled(head, RIM_HEAD, color::mix(tint, Color32::WHITE, 0.6).gamma_multiply(alpha));
}

impl Notch {
    pub(in crate::notch) fn step_video(&mut self, dt: f32) {
        let pinned = self.preview.is_some();
        self.video.step(dt, pinned);
    }

    pub(in crate::notch) fn video_moving(&self) -> bool {
        self.video.moving()
    }

    pub(super) fn video_look(&self, live: &VideoLive) -> Look {
        self.video.look(live)
    }

    pub(in crate::notch) fn shows_download(&self) -> bool {
        self.shows_live() && self.live.as_ref().is_some_and(|live| matches!(live.activity, Activity::Video(_)))
    }

    pub(in crate::notch) fn paint_video_rim(&self, painter: &Painter, silhouette: &Silhouette) {
        let attract = smoothstep(0.8, 1.0, self.attract.value);
        let main = self.live.as_ref().map(|live| (&live.activity, self.live_alpha(self.morph.current()) * attract));
        for (activity, shown) in self.leaving_live().into_iter().chain(main) {
            if let Activity::Video(live) = activity {
                self.paint_one_rim(painter, silhouette, live, shown);
            }
        }
    }

    fn paint_one_rim(&self, painter: &Painter, silhouette: &Silhouette, live: &VideoLive, shown: f32) {
        let look = self.video.look(live);
        let (share, tint) = match live.stage {
            VideoStage::Downloading => (look.share, tint(live.site)),
            VideoStage::Preparing => (look.share, color::PEARL),
            VideoStage::Joining => (1.0, tint(live.site)),
            VideoStage::Saved => (1.0, color::mix(tint(live.site), color::GREEN, smoothstep(0.0, 0.6, look.done))),
            VideoStage::Finding | VideoStage::Failed => return,
        };
        let fading = if live.stage == VideoStage::Saved { 1.0 - smoothstep(0.9, 1.0, look.done) * 0.35 } else { 1.0 };
        let alpha = shown * fading;
        if alpha <= 0.01 || share <= 0.0 {
            return;
        }
        let radius = match self.style {
            Style::Notch => silhouette.bottom_r,
            Style::Island => silhouette.h * 0.5,
        };
        paint_rim(painter, silhouette.rect(), radius, share, tint, self.clock.seconds(), alpha);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::notch::tests::fixtures::{island, settle};

    pub(in crate::notch) fn video(stage: VideoStage, share: f32) -> VideoLive {
        VideoLive {
            id: 7,
            site: Site::YouTube,
            audio: false,
            stage,
            share,
            title: "Me at the zoo".into(),
            detail: "3.1 MB/s \u{b7} 0:12 left".into(),
            cover: None,
        }
    }

    fn body() -> Rect {
        Rect::from_min_size(pos2(100.0, 0.0), vec2(200.0, 34.0))
    }

    #[test]
    fn rim_runs_under_body() {
        let path = rim(body(), 17.0);
        let (first, last) = (path.first().unwrap().0, path.last().unwrap().0);
        assert!(first.x < body().left() && last.x > body().right(), "{first:?} {last:?}");
        assert!((first.y - last.y).abs() < 0.01 && (first.y - 17.0).abs() < 0.01);
        assert!(path.windows(2).all(|pair| pair[1].1 >= pair[0].1), "the running length only grows");
        let spine = |x: f32| pos2(x.clamp(body().left() + 17.0, body().right() - 17.0), 17.0);
        assert!(path.iter().all(|(point, _)| point.distance(spine(point.x)) > 17.0 + RIM_GAP - 0.01));
        let lowest = path.iter().map(|(point, _)| point.y).fold(0.0, f32::max);
        assert!((lowest - (34.0 + RIM_GAP)).abs() < 0.01);
    }

    #[test]
    fn half_download_half_rim() {
        let path = rim(body(), 10.0);
        let whole = path.last().unwrap().1;
        let length = |points: &[Pos2]| points.windows(2).map(|pair| pair[0].distance(pair[1])).sum::<f32>();
        let half = stretch(&path, 0.0, 0.5);
        assert!((length(&half) - whole * 0.5).abs() < 0.5, "{} of {whole}", length(&half));
        assert!((half.last().unwrap().x - body().center().x).abs() < 0.5, "it ends in the middle");
        assert!(stretch(&path, 0.0, 0.0).is_empty() && stretch(&path, 0.7, 0.3).is_empty());
        assert!((length(&stretch(&path, 0.0, 1.0)) - whole).abs() < 0.01);
    }

    #[test]
    fn progress_glides_tick_pops() {
        let mut n = island();
        n.set_activities(vec![Activity::Video(video(VideoStage::Downloading, 0.2))]);
        assert_eq!(n.video_look(&video(VideoStage::Downloading, 0.2)).share, 0.2, "it starts where it is");
        n.set_activities(vec![Activity::Video(video(VideoStage::Downloading, 0.8))]);
        assert!(n.video_moving() && n.is_animating());
        n.step(1.0 / 60.0);
        let eased = n.video_look(&video(VideoStage::Downloading, 0.8)).share;
        assert!(eased > 0.2 && eased < 0.8, "{eased}");
        n.set_activities(vec![Activity::Video(video(VideoStage::Saved, 1.0))]);
        settle(&mut n);
        assert_eq!(n.video_look(&video(VideoStage::Saved, 1.0)), Look { share: 1.0, done: 1.0 });
        n.set_activities(Vec::new());
        assert!(!n.video_moving());
    }

    #[test]
    fn camera_words_stay_short() {
        assert_eq!(words(VideoStage::Downloading, 0.427).0, "42%");
        for stage in STAGES {
            assert!(words(stage, 1.0).0.chars().count() <= 7);
        }
        assert_eq!(shortened("A very long title that keeps going and going"), "A very long title that kee\u{2026}");
        assert!(continuous(&video(VideoStage::Joining, 1.0)) && !continuous(&video(VideoStage::Saved, 1.0)));
    }

    #[test]
    fn wings_keep_one_width() {
        let rig = crate::notch::tests::fixtures::Rig::new();
        let mut n = crate::notch::tests::fixtures::notch();
        let widths: Vec<f32> = STAGES
            .into_iter()
            .map(|stage| {
                n.set_activities(vec![Activity::Video(video(stage, 0.42))]);
                for _ in 0..3 {
                    rig.frame(&mut n, &[], Vec::new());
                }
                n.live_geometry().w
            })
            .collect();
        assert!(widths.iter().all(|w| *w == widths[0]), "the notch never breathes in and out: {widths:?}");
        assert!(widths[0] > n.idle_geometry().w);
    }

    #[test]
    fn finished_rim_fades() {
        let rig = crate::notch::tests::fixtures::Rig::new();
        let mut n = island();
        n.set_activities(vec![Activity::Video(video(VideoStage::Downloading, 0.5))]);
        for _ in 0..60 {
            rig.frame(&mut n, &[], Vec::new());
        }
        n.set_activities(Vec::new());
        let (activity, shown) = n.leaving_live().expect("the download is still drawn as it leaves");
        assert!(matches!(activity, Activity::Video(_)) && shown > 0.5, "so its rim fades from {shown}");
        for _ in 0..60 {
            rig.frame(&mut n, &[], Vec::new());
        }
        assert!(n.leaving_live().is_none());
    }
}
