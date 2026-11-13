//! One alert: its title, message, tone and how long it stays.

use std::time::Duration;

use notchbase_sdk::prelude::*;
use notchbase_sdk::protocol::Map;

use crate::http::Reply;

const BADGE: f32 = 30.0;
const LONE_BADGE: f32 = 22.0;
const TITLE_LIMIT: usize = 60;
const MESSAGE_LIMIT: usize = 140;
const ECHO_LIMIT: usize = 32;
const SHOWN_FOR: Duration = Duration::from_millis(4000);
const LONGEST: Duration = Duration::from_secs(24 * 60 * 60);
const TONES: [(&str, Tone); 9] = [
    ("blue", Tone::Blue),
    ("green", Tone::Green),
    ("orange", Tone::Orange),
    ("red", Tone::Red),
    ("purple", Tone::Purple),
    ("teal", Tone::Teal),
    ("yellow", Tone::Yellow),
    ("pink", Tone::Pink),
    ("gray", Tone::Gray),
];

pub type Fields = Map<String, Value>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stay {
    For(Duration),
    UntilClicked,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Alert {
    pub title: String,
    pub message: Option<String>,
    pub icon: Icon,
    pub tone: Tone,
    pub stay: Stay,
    pub shake: bool,
}

impl Alert {
    pub fn read(fields: &Fields) -> Result<Self, Reply> {
        let title = read_text(fields, "title")?.map(|title| tidy(&title, TITLE_LIMIT)).unwrap_or_default();
        if title.is_empty() {
            return Err(Reply::bad("a title is required"));
        }
        let message = match read_text(fields, "message")? {
            Some(message) => Some(message),
            None => read_text(fields, "text")?,
        };
        Ok(Self {
            title,
            message: message.map(|message| tidy(&message, MESSAGE_LIMIT)).filter(|message| !message.is_empty()),
            icon: read_text(fields, "icon")?.map_or(Ok(Icon::Bell), |name| icon(&name))?,
            tone: read_text(fields, "tone")?.map_or(Ok(Tone::Blue), |name| tone(&name))?,
            stay: stay(read_flag(fields, "sticky")?, read_number(fields, "duration_ms")?)?,
            shake: read_flag(fields, "shake")?.unwrap_or(true),
        })
    }

    pub fn notice(&self) -> Notice {
        let title = text(&self.title).headline();
        let notice = match &self.message {
            Some(message) => {
                Notice::new(badge(self.icon, self.tone, BADGE), title).below(text(message).callout().secondary().wrap())
            }
            None => Notice::new(badge(self.icon, self.tone, LONE_BADGE), title),
        };
        let notice = if self.shake { notice.banner().shake() } else { notice.banner().pop() };
        match self.stay {
            Stay::For(duration) => notice.duration(duration),
            Stay::UntilClicked => notice.sticky(),
        }
    }
}

fn stay(sticky: Option<bool>, duration_ms: Option<f64>) -> Result<Stay, Reply> {
    match duration_ms.map(f64::round) {
        _ if sticky == Some(true) => Ok(Stay::UntilClicked),
        None => Ok(Stay::For(SHOWN_FOR)),
        Some(ms) if ms < 0.0 => Err(Reply::bad("duration_ms must be 0 or more")),
        Some(ms) if ms < 1.0 => Ok(Stay::UntilClicked),
        Some(ms) => Ok(Stay::For(Duration::from_millis(ms.min(LONGEST.as_millis() as f64) as u64))),
    }
}

fn icon(name: &str) -> Result<Icon, Reply> {
    let key = name.trim().to_ascii_lowercase().replace(['-', ' '], "_");
    serde_json::from_value(Value::String(key)).map_err(|_| Reply::bad(format!("unknown icon \"{}\"", echo(name))))
}

fn tone(name: &str) -> Result<Tone, Reply> {
    let key = name.trim().to_ascii_lowercase();
    TONES
        .iter()
        .find_map(|(known, tone)| (*known == key).then_some(*tone))
        .ok_or_else(|| Reply::bad(format!("unknown tone \"{}\"", echo(name))))
}

fn read_text(fields: &Fields, key: &str) -> Result<Option<String>, Reply> {
    match fields.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(text)) => Ok(Some(text.clone())),
        Some(Value::Number(number)) => Ok(Some(number.to_string())),
        Some(Value::Bool(on)) => Ok(Some(on.to_string())),
        Some(_) => Err(Reply::bad(format!("{key} must be text"))),
    }
}

fn read_number(fields: &Fields, key: &str) -> Result<Option<f64>, Reply> {
    let number = match fields.get(key) {
        None | Some(Value::Null) => return Ok(None),
        Some(Value::Number(number)) => number.as_f64(),
        Some(Value::String(text)) => text.trim().parse().ok(),
        Some(_) => None,
    };
    number.filter(|number| number.is_finite()).map(Some).ok_or_else(|| Reply::bad(format!("{key} must be a number")))
}

fn read_flag(fields: &Fields, key: &str) -> Result<Option<bool>, Reply> {
    let flag = match fields.get(key) {
        None | Some(Value::Null) => return Ok(None),
        Some(Value::Bool(on)) => Some(*on),
        Some(Value::Number(number)) => number.as_u64().filter(|number| *number <= 1).map(|number| number == 1),
        Some(Value::String(text)) => match text.trim().to_ascii_lowercase().as_str() {
            "" | "true" | "1" | "yes" | "on" => Some(true),
            "false" | "0" | "no" | "off" => Some(false),
            _ => None,
        },
        Some(_) => None,
    };
    flag.map(Some).ok_or_else(|| Reply::bad(format!("{key} must be true or false")))
}

fn tidy(text: &str, limit: usize) -> String {
    let spaced: String = text.chars().map(|char| if char.is_control() { ' ' } else { char }).collect();
    let joined = spaced.split_whitespace().collect::<Vec<_>>().join(" ");
    joined.chars().take(limit).collect::<String>().trim_end().to_owned()
}

fn echo(name: &str) -> String {
    tidy(name, ECHO_LIMIT)
}

#[cfg(test)]
mod tests {
    use notchbase_sdk::protocol::{Node, TextStyle};

    use super::*;

    fn alert(message: Option<&str>, stay: Stay, shake: bool) -> Alert {
        let message = message.map(str::to_owned);
        Alert { title: "Deploy".into(), message, icon: Icon::Checkmark, tone: Tone::Green, stay, shake }
    }

    #[test]
    fn notice_is_a_shaking_banner() {
        let node = alert(Some("All green"), Stay::For(Duration::from_millis(6000)), true).notice().into_node();
        assert!(node.banner && node.shake && node.pop);
        assert_eq!(node.duration_ms, 6000);
        assert_eq!(node.leading, Node::Badge { icon: Icon::Checkmark, size: BADGE, tone: Tone::Green });
        assert!(
            matches!(&node.trailing, Node::Text(title) if title.text == "Deploy" && title.style == TextStyle::Headline)
        );
        let Some(Node::Text(below)) = &node.below else { panic!("{:?}", node.below) };
        assert_eq!(
            (below.text.as_str(), below.style, below.tone, below.wrap),
            ("All green", TextStyle::Callout, Tone::Secondary, true)
        );
    }

    #[test]
    fn calm_sticky_alert_only_pops() {
        let node = alert(None, Stay::UntilClicked, false).notice().into_node();
        assert!(node.banner && node.pop && !node.shake);
        assert!(node.below.is_none());
        assert_eq!(node.leading, Node::Badge { icon: Icon::Checkmark, size: LONE_BADGE, tone: Tone::Green });
        assert_eq!(node.duration_ms, 0);
    }

    #[test]
    fn text_is_tidied_and_cut() {
        assert_eq!(tidy("  Build\n\tdone\u{1b}[31m  ", 60), "Build done [31m");
        assert_eq!(tidy(&"é".repeat(80), TITLE_LIMIT).chars().count(), TITLE_LIMIT);
        assert_eq!(tidy("abc def", 4), "abc");
    }
}
