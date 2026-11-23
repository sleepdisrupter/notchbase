//! The badges apps put on their dock icons (unread counts and the like): which tile wears which, and drawing one the
//! way the Dock does, a red pill on the icon's top right corner. On a Mac they're read off the hidden system Dock.

use std::path::{Path, PathBuf};

use egui::{Color32, FontId, Rect, pos2, vec2};

/// The Dock's badge red.
const RED: Color32 = Color32::from_rgb(255, 59, 48);
/// How tall a badge is, as a part of the icon's art.
const HEIGHT: f32 = 0.36;
/// How far it reaches past the art's top and right edges, as a part of its own height.
const OVERHANG: f32 = 0.18;
/// Longer labels are cut to this many characters and an ellipsis.
const LONGEST: usize = 5;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Badge {
    /// The app's bundle, when the system says.
    pub path: Option<PathBuf>,
    pub title: String,
    pub label: String,
}

/// The badge on the app at `path` called `name`: matched by its bundle, or by its name when there's no bundle.
pub fn for_app<'a>(badges: &'a [Badge], path: &Path, name: &str) -> Option<&'a str> {
    badges
        .iter()
        .find(|b| b.path.as_deref() == Some(path))
        .or_else(|| badges.iter().find(|b| b.path.is_none() && !name.is_empty() && b.title == name))
        .map(|b| b.label.as_str())
}

/// What the badge says, cut short like the Dock does when it's long.
fn shown(label: &str) -> String {
    let label = label.trim();
    if label.chars().count() <= LONGEST {
        return label.to_owned();
    }
    let mut cut: String = label.chars().take(LONGEST - 1).collect();
    cut.push('…');
    cut
}

/// Draws `label` on the icon whose art fills `art`.
pub fn paint(painter: &egui::Painter, art: Rect, label: &str, alpha: f32) {
    if alpha <= 0.0 || art.width() < 4.0 {
        return;
    }
    let height = (art.width() * HEIGHT).max(8.0);
    let text = painter.layout_no_wrap(shown(label), FontId::proportional(height * 0.64), Color32::WHITE);
    let width = height.max(text.size().x + height * 0.55);
    let pill = Rect::from_min_size(
        pos2(art.right() + height * OVERHANG - width, art.top() - height * OVERHANG),
        vec2(width, height),
    );
    let shadow = egui::epaint::Shadow {
        offset: [0, 1],
        blur: 4,
        spread: 0,
        color: Color32::from_black_alpha((70.0 * alpha) as u8),
    };
    painter.add(shadow.as_shape(pill, (height * 0.5) as u8));
    painter.rect_filled(pill, height * 0.5, RED.gamma_multiply(alpha));
    let at = pill.center() - text.size() * 0.5;
    painter.galley(at, text, Color32::WHITE.gamma_multiply(alpha));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn badge(path: Option<&str>, title: &str, label: &str) -> Badge {
        Badge { path: path.map(PathBuf::from), title: title.into(), label: label.into() }
    }

    #[test]
    fn found_by_bundle_then_by_name() {
        let badges = [
            badge(Some("/Applications/Mail.app"), "Mail", "3"),
            badge(None, "Messages", "12"),
            badge(Some("/Applications/Slack.app"), "Slack", "•"),
        ];
        assert_eq!(for_app(&badges, Path::new("/Applications/Mail.app/"), "Mail"), Some("3"));
        assert_eq!(for_app(&badges, Path::new("/System/Applications/Messages.app"), "Messages"), Some("12"));
        assert_eq!(for_app(&badges, Path::new("/Applications/Other/Slack.app"), "Slack"), None, "another copy");
        assert_eq!(for_app(&badges, Path::new("/Applications/Notes.app"), "Notes"), None);
    }

    #[test]
    fn long_labels_are_cut() {
        assert_eq!(shown(" 7 "), "7");
        assert_eq!(shown("12345"), "12345");
        assert_eq!(shown("123456"), "1234…");
    }
}
