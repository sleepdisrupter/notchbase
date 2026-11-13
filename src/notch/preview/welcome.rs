//! The welcome walkthrough previews.

use crate::bridge::{Page, StepView, WelcomeStage, WelcomeView};
use crate::notch::Notch;
use crate::system::permissions::{Access, Standing};

use super::Preview;

pub(super) fn parse(name: &str) -> Option<Preview> {
    Some(match name {
        "welcome" | "walkthrough" => Preview::Welcome,
        "welcome-ready" => Preview::WelcomeReady,
        "welcome-step" | "permission-step" => Preview::WelcomeStep,
        "welcome-asked" => Preview::WelcomeAsked,
        "welcome-allowed" => Preview::WelcomeAllowed,
        "welcome-done" => Preview::WelcomeDone,
        _ => return None,
    })
}

pub(super) fn page(preview: Preview) -> Option<Page> {
    sample(preview).map(|_| Page::Welcome)
}

pub(super) fn pin(n: &mut Notch, preview: Preview) {
    if let Some(view) = sample(preview) {
        n.pin_page(Page::Welcome);
        n.pages.pin_welcome(view);
    }
}

fn sample(preview: Preview) -> Option<WelcomeView> {
    let (stage, first) = match preview {
        Preview::Welcome => (WelcomeStage::Hello, Standing::Undecided),
        Preview::WelcomeReady => {
            return Some(WelcomeView { first_run: true, ..WelcomeView::default() });
        }
        Preview::WelcomeStep | Preview::WelcomeAsked => (WelcomeStage::Step(0), Standing::Undecided),
        Preview::WelcomeAllowed => (WelcomeStage::Step(0), Standing::Granted),
        Preview::WelcomeDone => (WelcomeStage::AllSet, Standing::Granted),
        _ => return None,
    };
    let asked = preview == Preview::WelcomeAsked;
    let step = |access, standing| StepView { access, standing, asked: false };
    let mut steps: Vec<StepView> = Access::ALL.into_iter().map(|access| step(access, Standing::Undecided)).collect();
    steps[0] = StepView { standing: first, asked, ..steps[0] };
    Some(WelcomeView { due: false, first_run: true, stage, steps })
}

#[cfg(test)]
mod tests {
    use egui::vec2;

    use super::*;
    use crate::notch::Mode;
    use crate::platform::Style;

    const NAMES: [(&str, Preview); 6] = [
        ("welcome", Preview::Welcome),
        ("welcome-ready", Preview::WelcomeReady),
        ("welcome-step", Preview::WelcomeStep),
        ("welcome-asked", Preview::WelcomeAsked),
        ("welcome-allowed", Preview::WelcomeAllowed),
        ("welcome-done", Preview::WelcomeDone),
    ];

    #[test]
    fn walkthrough_previews_open_their_page() {
        for (name, preview) in NAMES {
            assert_eq!(Preview::parse(name), Some(preview), "{name}");
            assert_eq!(preview.page(), Some(Page::Welcome), "{name}");
            let n = Notch::with_preview(Style::Notch, Some(vec2(185.0, 32.0)), Some(preview));
            assert_eq!((n.mode, n.pager.page), (Mode::Expanded, Page::Welcome), "{name}");
        }
    }
}
