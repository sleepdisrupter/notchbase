//! The usage color: the assistant's own color, warming to red near the limit.

use egui::Color32;

use crate::assistant::Provider;
use crate::ui::color;
use crate::ui::spring::smoothstep;

const WARM_FROM: f32 = 0.85;
const WARM_FULL: f32 = 0.95;

pub(super) fn usage(assistant: Provider, used: f32) -> Color32 {
    color::mix(accent(assistant), color::RED, smoothstep(WARM_FROM, WARM_FULL, used))
}

fn accent(assistant: Provider) -> Color32 {
    match assistant {
        Provider::Claude => color::CLAUDE,
        Provider::Codex => color::CHATGPT,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn usage_warms_to_red() {
        for assistant in Provider::ALL {
            assert_eq!(usage(assistant, 0.2), accent(assistant));
            assert_eq!(usage(assistant, WARM_FROM), accent(assistant));
            assert_eq!(usage(assistant, 1.0), color::RED);
        }
    }
}
