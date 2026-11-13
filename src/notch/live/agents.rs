//! The agents' live activity: a mark, a count of runs and a few words on what they're doing.

use std::time::Duration;

use egui::{Align2, Color32, FontId, Pos2, pos2, vec2};
use notchbase_sdk::prelude::*;
use notchbase_sdk::protocol::NoticeNode;

use crate::assistant::Finished;
use crate::bridge::{AgentLive, Mood};
use crate::ui::agent_mark::{self, Spin, WAITING, clock};
use crate::ui::canvas::Canvas;
use crate::ui::color::{label, secondary_label};
use crate::ui::font;
use crate::ui::spring::{Spring, SpringConfig, UNIT_PRECISION, smoothstep};

pub(in crate::notch) const MARK: f32 = 15.0;
const COUNT_GAP: f32 = 4.0;
const BADGE: f32 = 18.0;
const DONE_FOR: Duration = Duration::from_millis(3200);
const NEEDS_YOU: &str = "Needs you";
const SAY: SpringConfig = SpringConfig::new(0.28, 1.0);
const RISE: f32 = 7.0;
const OLD_GONE_BY: f32 = 0.55;
const NEW_FROM: f32 = 0.25;

type Words = (String, FontId, Color32);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Say {
    Clock,
    Verb(&'static str),
    NeedsYou,
}

impl Say {
    fn of(mood: Mood) -> Self {
        match mood {
            Mood::Thinking => Say::Clock,
            Mood::Doing(verb) => Say::Verb(verb),
            Mood::Waiting => Say::NeedsYou,
        }
    }
}

pub(in crate::notch) struct Said {
    say: Option<Say>,
    shown: Option<Words>,
    before: Option<Words>,
    change: Spring,
}

impl Default for Said {
    fn default() -> Self {
        Self { say: None, shown: None, before: None, change: Spring::new(1.0, SAY, UNIT_PRECISION) }
    }
}

impl Said {
    pub(in crate::notch) fn follow(&mut self, mood: Mood, dt: f32) {
        let say = Say::of(mood);
        if self.say.is_some_and(|said| said != say) {
            self.before = self.shown.clone();
            self.change.snap_to(0.0);
            self.change.animate_to(1.0, SAY);
        }
        self.say = Some(say);
        self.change.step(dt);
        if self.change.is_settled() {
            self.before = None;
        }
    }

    pub(in crate::notch) fn is_settled(&self) -> bool {
        self.change.is_settled()
    }
}

pub(super) fn leading_width(canvas: &Canvas, agent: &AgentLive) -> f32 {
    match count(agent) {
        Some(count) => MARK + COUNT_GAP + canvas.text_width(count, font::section()),
        None => MARK,
    }
}

pub(in crate::notch) fn trailing_width(canvas: &Canvas, agent: &AgentLive, now_ms: u64) -> f32 {
    let (text, font, _) = trailing(agent, now_ms);
    canvas.text_width(text, font)
}

pub(in crate::notch) fn paint_leading(canvas: &mut Canvas, agent: &AgentLive, left_center: Pos2, spin: &Spin) {
    let center = pos2(left_center.x + MARK * 0.5, left_center.y);
    mark(canvas, agent, center, MARK, spin);
    if let Some(count) = count(agent) {
        let at = pos2(left_center.x + MARK + COUNT_GAP, left_center.y);
        canvas.text(at, Align2::LEFT_CENTER, count, font::section(), secondary_label());
    }
}

pub(in crate::notch) fn paint_trailing(canvas: &mut Canvas, agent: &AgentLive, right_center: Pos2, now_ms: u64) {
    let (text, font, tint) = trailing(agent, now_ms);
    canvas.text(right_center, Align2::RIGHT_CENTER, text, font, tint);
}

pub(super) fn paint_trailing_said(
    canvas: &mut Canvas,
    agent: &AgentLive,
    right_center: Pos2,
    now_ms: u64,
    said: &mut Said,
) {
    let words = trailing(agent, now_ms);
    let changed = said.change.value.clamp(0.0, 1.0);
    if let Some((text, font, tint)) = said.before.clone().filter(|_| changed < 1.0) {
        let at = right_center - vec2(0.0, RISE * changed);
        let fading = 1.0 - smoothstep(0.0, OLD_GONE_BY, changed);
        canvas.text(at, Align2::RIGHT_CENTER, text, font, tint.gamma_multiply(fading));
    }
    let (text, font, tint) = words.clone();
    let at = right_center + vec2(0.0, RISE * (1.0 - changed));
    let shown = if said.before.is_some() { smoothstep(NEW_FROM, 1.0, changed) } else { 1.0 };
    canvas.text(at, Align2::RIGHT_CENTER, text, font, tint.gamma_multiply(shown));
    said.shown = Some(words);
}

pub(in crate::notch) fn mark(canvas: &mut Canvas, agent: &AgentLive, center: Pos2, size: f32, spin: &Spin) {
    agent_mark::mark(canvas, agent.provider, agent.mood, center, size, spin);
}

pub(in crate::notch) fn moves(agent: &AgentLive) -> bool {
    agent_mark::moves(agent.mood)
}

fn trailing(agent: &AgentLive, now_ms: u64) -> (String, FontId, Color32) {
    match agent.mood {
        Mood::Thinking => (clock(now_ms.saturating_sub(agent.since_ms)), font::value(), secondary_label()),
        Mood::Doing(verb) => (verb.to_owned(), font::headline(), label()),
        Mood::Waiting => (NEEDS_YOU.to_owned(), font::headline(), WAITING),
    }
}

fn count(agent: &AgentLive) -> Option<String> {
    (agent.count > 1).then(|| agent.count.to_string())
}

pub fn done(finished: &Finished) -> NoticeNode {
    let words = format!("{} done", finished.project);
    Notice::new(badge(Icon::Checkmark, Tone::Green, BADGE), text(words).callout()).pop().duration(DONE_FOR).into_node()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assistant::Provider;

    fn agent(mood: Mood, count: usize) -> AgentLive {
        AgentLive { provider: Provider::Claude, mood, since_ms: 1_000, count }
    }

    #[test]
    fn thinking_times_doing_says() {
        assert_eq!(trailing(&agent(Mood::Thinking, 1), 193_000).0, "3:12");
        assert_eq!(trailing(&agent(Mood::Thinking, 1), 3_700_000).0, "1:01:39");
        assert_eq!(trailing(&agent(Mood::Doing("Editing"), 1), 5_000).0, "Editing");
        let (words, _, tint) = trailing(&agent(Mood::Waiting, 1), 5_000);
        assert_eq!((words.as_str(), tint), (NEEDS_YOU, WAITING));
    }

    #[test]
    fn count_only_for_several_runs() {
        assert_eq!(count(&agent(Mood::Thinking, 1)), None);
        assert_eq!(count(&agent(Mood::Thinking, 3)).as_deref(), Some("3"));
        assert!(moves(&agent(Mood::Doing("Running"), 1)));
        assert!(!moves(&agent(Mood::Waiting, 1)));
    }
}
