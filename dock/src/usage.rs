//! The usage widget at the foot of Claude's and ChatGPT's menus: how much of the plan's 5-hour and weekly limits is
//! used and when each resets, in the app's own colour. The numbers come from the same readers as notchbase's Stats
//! page (`crates/usage`): Claude's from the Claude app's history file or Anthropic's usage endpoint, ChatGPT's from the
//! logs the Codex CLI writes.

use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use egui::{Align2, Color32, Rect, pos2, vec2};
use notchbase_paint::color::{self, CHATGPT, CLAUDE, GRAY, PEARL, RED, white};
use notchbase_paint::font;
use notchbase_paint::spring::{POINT_PRECISION, Spring, SpringConfig, UNIT_PRECISION, smoothstep};
use notchbase_usage::{Claude, Codex, PlanUsage, USAGE_STALE_AFTER, UsageWindow};

/// The narrowest a menu with the widget gets, so the reset times fit beside the labels.
pub const WIDTH: f32 = 236.0;
/// The space the hairline above it takes, like a menu divider.
pub const DIVIDER: f32 = 9.0;
/// Left and right of the rows, lined up with the menu's dividers.
const INSET: f32 = 18.0;
const TOP: f32 = 3.0;
const BOTTOM: f32 = 2.0;
const LINE: f32 = 16.0;
const BAR_GAP: f32 = 5.0;
const BAR: f32 = 4.0;
const ROW: f32 = LINE + BAR_GAP + BAR;
const ROW_GAP: f32 = 10.0;
const MESSAGE: f32 = 18.0;
const CAPTION: f32 = 16.0;
/// Near the limit the bar warms from the app's colour to red.
const WARM_FROM: f32 = 0.85;
const WARM_FULL: f32 = 0.95;
const FILL: SpringConfig = SpringConfig::new(0.5, 1.0);
const GROW: SpringConfig = SpringConfig::new(0.3, 1.0);
const PULSE_PERIOD: f32 = 1.4;

const DAY_MINUTES: u64 = 24 * 60;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Provider {
    Claude,
    ChatGpt,
}

impl Provider {
    /// Which plan an app's usage belongs to, by its bundle's name: Claude, or ChatGPT and Codex, which share a plan.
    pub fn of(path: &Path) -> Option<Self> {
        match path.file_stem()?.to_str()? {
            "Claude" => Some(Provider::Claude),
            "ChatGPT" | "Codex" => Some(Provider::ChatGpt),
            _ => None,
        }
    }

    fn accent(self) -> Color32 {
        match self {
            Provider::Claude => CLAUDE,
            Provider::ChatGpt => CHATGPT,
        }
    }

    fn signed_out(self) -> &'static str {
        match self {
            Provider::Claude => "Sign in to Claude to see usage",
            Provider::ChatGpt if cfg!(windows) => "Use Codex on this PC to see usage",
            Provider::ChatGpt => "Use Codex on this Mac to see usage",
        }
    }

    fn index(self) -> usize {
        self as usize
    }
}

/// The readers, started the first time a menu needs one, and what each last said.
pub struct Usage {
    claude: Option<Claude>,
    codex: Option<Codex>,
    known: [PlanUsage; 2],
    /// Fixed usage for snapshots: never read.
    pinned: [bool; 2],
    /// The widget's height and its two bars, gliding as numbers arrive.
    height: Spring,
    bars: [Spring; 2],
    clock: f32,
}

impl Default for Usage {
    fn default() -> Self {
        Self {
            claude: None,
            codex: None,
            known: [PlanUsage::Loading, PlanUsage::Loading],
            pinned: [false; 2],
            height: Spring::new(0.0, GROW, POINT_PRECISION),
            bars: std::array::from_fn(|_| Spring::new(0.0, FILL, UNIT_PRECISION)),
            clock: 0.0,
        }
    }
}

impl Usage {
    /// Shows fixed usage instead of reading it, for snapshots.
    pub fn pin(&mut self, provider: Provider, usage: PlanUsage) {
        self.known[provider.index()] = usage;
        self.pinned[provider.index()] = true;
    }

    /// A menu with the widget just opened: it starts from its resting size with empty bars, which then fill.
    pub fn opened(&mut self, provider: Provider) {
        self.height.snap_to(height(&self.known[provider.index()]));
        for bar in &mut self.bars {
            bar.snap_to(0.0);
        }
    }

    /// Asks for fresh numbers (each reader keeps its own pace, so asking every frame is fine) and takes any that came.
    pub fn poll(&mut self, provider: Provider) {
        if self.pinned[provider.index()] {
            return;
        }
        // The dock draws every frame while a menu is open, so new numbers are seen without a wake.
        let wake: notchbase_usage::Wake = Arc::new(|| {});
        let latest = match provider {
            Provider::Claude => {
                let reader = self.claude.get_or_insert_with(|| Claude::new(wake));
                reader.poll();
                reader.latest()
            }
            Provider::ChatGpt => {
                let reader = self.codex.get_or_insert_with(|| Codex::new(wake));
                reader.poll();
                reader.latest()
            }
        };
        if let Some(usage) = latest {
            self.known[provider.index()] = usage;
        }
    }

    pub fn step(&mut self, provider: Provider, dt: f32) {
        let usage = &self.known[provider.index()];
        self.height.animate_to(height(usage), GROW);
        self.height.step(dt);
        let windows = match usage {
            PlanUsage::Ready { five_hour, weekly, .. } => [five_hour.as_ref(), weekly.as_ref()],
            _ => [None, None],
        };
        let shown = windows.iter().flatten().map(|w| w.used.clamp(0.0, 1.0));
        let mut targets = [0.0; 2];
        for (target, used) in targets.iter_mut().zip(shown) {
            *target = used;
        }
        for (bar, target) in self.bars.iter_mut().zip(targets) {
            bar.animate_to(target, FILL);
            bar.step(dt);
        }
        self.clock = (self.clock + dt) % (PULSE_PERIOD * 64.0);
    }

    /// The height the widget takes in the menu right now, its divider included.
    pub fn height_now(&self) -> f32 {
        DIVIDER + self.height.value.max(0.0)
    }

    /// Paints the widget from `top` (its divider's top) across the menu's `panel`, at opacity `a`.
    pub fn paint(&self, painter: &egui::Painter, provider: Provider, panel: Rect, top: f32, a: f32, divider: Color32) {
        let ppp = painter.pixels_per_point();
        let (left, right) = (panel.left() + INSET, panel.right() - INSET);
        let ly = ((top + 4.5) * ppp).floor() / ppp;
        painter.rect_filled(Rect::from_min_max(pos2(left, ly), pos2(right, ly + 1.0 / ppp)), 0.0, divider);
        let top = top + DIVIDER + TOP;
        let now = SystemTime::now();
        match &self.known[provider.index()] {
            PlanUsage::Loading => {
                let pulse = 0.5 - 0.5 * (self.clock / PULSE_PERIOD * std::f32::consts::TAU).cos();
                for (slot, label) in ["5-hour", "Weekly"].into_iter().enumerate() {
                    let y = top + slot as f32 * (ROW + ROW_GAP);
                    text(painter, pos2(left, y + LINE * 0.5), Align2::LEFT_CENTER, label, 12.0, GRAY, a);
                    let bar = Rect::from_min_max(pos2(left, y + LINE + BAR_GAP), pos2(right, y + ROW));
                    painter.rect_filled(bar, BAR * 0.5, white((0.08 + 0.06 * pulse) * a));
                }
            }
            PlanUsage::Ready { five_hour: None, weekly: None, .. } => {
                message(painter, left, right, top, "No usage limits on this plan", a);
            }
            PlanUsage::Ready { five_hour, weekly, fetched_at } => {
                let windows = [(five_hour.as_ref(), "5-hour"), (weekly.as_ref(), "Weekly")];
                let shown = windows.into_iter().filter_map(|(w, fallback)| w.map(|w| (w, fallback)));
                let mut y = top;
                for ((window, fallback), bar) in shown.zip(&self.bars) {
                    row(painter, left, right, y, window, fallback, bar.value, provider.accent(), now, a);
                    y += ROW + ROW_GAP;
                }
                let age = now.duration_since(*fetched_at).unwrap_or_default();
                if age > USAGE_STALE_AFTER {
                    let at = pos2(left, y - ROW_GAP + CAPTION * 0.5 + 2.0);
                    let words = format!("Updated {} ago", span(age));
                    text(painter, at, Align2::LEFT_CENTER, words, 10.5, GRAY, a);
                }
            }
            PlanUsage::SignedOut => message(painter, left, right, top, provider.signed_out(), a),
            PlanUsage::Expired => message(painter, left, right, top, "Open the app to refresh usage", a),
            PlanUsage::Error(words) => message(painter, left, right, top, words, a),
        }
    }
}

/// The widget's resting height for what's known, its divider left out.
fn height(usage: &PlanUsage) -> f32 {
    let rows = |n: usize| n as f32 * ROW + n.saturating_sub(1) as f32 * ROW_GAP;
    let body = match usage {
        PlanUsage::Loading => rows(2),
        PlanUsage::Ready { five_hour, weekly, fetched_at } => {
            let n = usize::from(five_hour.is_some()) + usize::from(weekly.is_some());
            let stale = SystemTime::now().duration_since(*fetched_at).unwrap_or_default() > USAGE_STALE_AFTER;
            if n == 0 { MESSAGE } else { rows(n) + if stale { CAPTION } else { 0.0 } }
        }
        _ => MESSAGE,
    };
    TOP + body + BOTTOM
}

/// One limit: its name and how much is used over a capsule bar, with when it resets.
#[allow(clippy::too_many_arguments)]
fn row(
    painter: &egui::Painter,
    left: f32,
    right: f32,
    y: f32,
    window: &UsageWindow,
    fallback: &str,
    fill: f32,
    accent: Color32,
    now: SystemTime,
    a: f32,
) {
    let mid = y + LINE * 0.5;
    let label = label(window.minutes, fallback);
    let label = text(painter, pos2(left, mid), Align2::LEFT_CENTER, label, 12.0, PEARL, a);
    let percent = (window.used.clamp(0.0, 1.0) * 100.0).round() as i32;
    let resets = window.resets_at.map(|at| match at.duration_since(now) {
        Ok(left) => format!("resets in {}", span(left)),
        Err(_) => "resets now".to_owned(),
    });
    // The percentages line up at the right; when each limit resets sits beside its name.
    let shown = format!("{percent}%");
    painter.text(pos2(right, mid + 1.0), Align2::RIGHT_CENTER, &shown, font::semibold(12.0), shade(a));
    let percent =
        painter.text(pos2(right, mid), Align2::RIGHT_CENTER, shown, font::semibold(12.0), PEARL.gamma_multiply(a));
    if let Some(resets) = resets {
        let room = (percent.left() - label.right() - 6.0 - 10.0).max(0.0);
        let resets = crate::dock::ellipsize(painter, &resets, room, font::regular(11.0));
        text(painter, pos2(label.right() + 6.0, mid), Align2::LEFT_CENTER, resets, 11.0, GRAY, a);
    }

    let bar = Rect::from_min_max(pos2(left, y + LINE + BAR_GAP), pos2(right, y + ROW));
    painter.rect_filled(bar, BAR * 0.5, white(0.12 * a));
    let fill = fill.clamp(0.0, 1.0);
    if fill > 0.002 {
        let lit = Rect::from_min_size(bar.min, vec2((bar.width() * fill).max(BAR), BAR));
        let ink = color::mix(accent, RED, smoothstep(WARM_FROM, WARM_FULL, fill));
        painter.rect_filled(lit, BAR * 0.5, ink.gamma_multiply(a));
    }
}

fn message(painter: &egui::Painter, left: f32, right: f32, top: f32, words: &str, a: f32) {
    let words = crate::dock::ellipsize(painter, words, right - left, font::regular(12.0));
    text(painter, pos2(left, top + MESSAGE * 0.5), Align2::LEFT_CENTER, words, 12.0, GRAY, a);
}

/// Text on the glass with the menu's soft shadow.
fn text(
    painter: &egui::Painter,
    at: egui::Pos2,
    align: Align2,
    words: impl ToString,
    size: f32,
    ink: Color32,
    a: f32,
) -> Rect {
    let words = words.to_string();
    painter.text(at + vec2(0.0, 1.0), align, &words, font::regular(size), shade(a));
    painter.text(at, align, words, font::regular(size), ink.gamma_multiply(a))
}

fn shade(a: f32) -> Color32 {
    Color32::from_black_alpha((110.0 * a) as u8)
}

/// A limit's name from its length: "5-hour", "Weekly", or the fallback when the length isn't known.
fn label(minutes: Option<u64>, fallback: &str) -> String {
    match minutes {
        Some(DAY_MINUTES) => "Daily".to_owned(),
        Some(m) if m == 7 * DAY_MINUTES => "Weekly".to_owned(),
        Some(m) if (28 * DAY_MINUTES..=31 * DAY_MINUTES).contains(&m) => "Monthly".to_owned(),
        Some(m) if m > 0 && m % DAY_MINUTES == 0 => format!("{}-day", m / DAY_MINUTES),
        Some(m) if m > 0 && m % 60 == 0 => format!("{}-hour", m / 60),
        _ => fallback.to_owned(),
    }
}

/// A span of time as it reads in the menu: "3d 4h", "2h 14m", "42m".
fn span(d: Duration) -> String {
    let s = d.as_secs();
    let (days, hours, minutes) = (s / 86_400, s % 86_400 / 3600, s % 3600 / 60);
    match (days, hours) {
        (0, 0) if minutes == 0 => "<1m".to_owned(),
        (0, 0) => format!("{minutes}m"),
        (0, h) => format!("{h}h {minutes}m"),
        (d, 0) => format!("{d}d"),
        (d, h) => format!("{d}d {h}h"),
    }
}

/// Made-up usage for snapshots: a session partly used and a week further along.
pub fn sample(provider: Provider) -> PlanUsage {
    let now = SystemTime::now();
    let window = |used: f32, minutes: u64, resets_in: u64| UsageWindow {
        used,
        resets_at: Some(now + Duration::from_secs(resets_in)),
        estimated: false,
        minutes: Some(minutes),
    };
    let (session, week) = match provider {
        Provider::Claude => (0.34, 0.61),
        Provider::ChatGpt => (0.18, 0.47),
    };
    PlanUsage::Ready {
        five_hour: Some(window(session, 300, 2 * 3600 + 14 * 60)),
        weekly: Some(window(week, 7 * DAY_MINUTES, 3 * 86_400 + 4 * 3600)),
        fetched_at: now,
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    #[test]
    fn knows_claude_and_chatgpt_by_bundle() {
        let of = |p: &str| Provider::of(&PathBuf::from(p));
        assert_eq!(of("/Applications/Claude.app"), Some(Provider::Claude));
        assert_eq!(of("/Applications/ChatGPT.app"), Some(Provider::ChatGpt));
        assert_eq!(of("/Applications/Codex.app"), Some(Provider::ChatGpt));
        assert_eq!(of("/Applications/Safari.app"), None);
    }

    #[test]
    fn spans_read_short() {
        assert_eq!(span(Duration::from_secs(42 * 60)), "42m");
        assert_eq!(span(Duration::from_secs(2 * 3600 + 14 * 60)), "2h 14m");
        assert_eq!(span(Duration::from_secs(3 * 86_400 + 4 * 3600)), "3d 4h");
        assert_eq!(span(Duration::from_secs(2 * 86_400)), "2d");
        assert_eq!(span(Duration::from_secs(20)), "<1m");
    }

    #[test]
    fn weekly_only_is_one_row_shorter() {
        let PlanUsage::Ready { weekly, fetched_at, .. } = sample(Provider::Claude) else { unreachable!() };
        let both = height(&sample(Provider::Claude));
        let one = height(&PlanUsage::Ready { five_hour: None, weekly, fetched_at });
        assert_eq!(both - one, ROW + ROW_GAP);
    }

    #[test]
    fn labels_follow_the_window_length() {
        assert_eq!(label(Some(300), "x"), "5-hour");
        assert_eq!(label(Some(7 * DAY_MINUTES), "x"), "Weekly");
        assert_eq!(label(None, "Weekly"), "Weekly");
    }
}
