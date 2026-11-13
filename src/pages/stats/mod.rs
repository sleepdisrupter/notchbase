//! The Stats page: gauges for the CPU, GPU, memory and disk, with the assistant's usage card and its week below.
//! Picking a gauge lists the apps or the storage behind it.

mod apps;
mod arc;
mod card;
mod format;
mod gauges;
mod storage;
mod tone;
mod week;

use egui::{Rect, pos2, vec2};

use super::PageCtx;
use crate::assistant::Provider;
use crate::bridge::Action;
use crate::stats::Snapshot;
use crate::ui::metrics;
use apps::AppList;
use card::UsageCard;
use gauges::{GAUGES_H, Gauges};
use storage::StorageList;

#[derive(Default)]
pub struct StatsPage {
    gauges: Gauges,
    apps: AppList,
    storage: StorageList,
    card: UsageCard,
}

impl StatsPage {
    pub fn estimate() -> f32 {
        GAUGES_H
    }

    pub fn height(&self, stats: &Snapshot, assistant: Option<Provider>) -> f32 {
        let top = GAUGES_H + self.lists_height();
        match assistant {
            Some(assistant) => {
                let card = card::height(stats.usage(assistant).0, assistant);
                top + metrics::SECTION_GAP + card + week::height(stats.history.week(assistant))
            }
            None => top,
        }
    }

    pub fn ui(&mut self, ctx: &mut PageCtx<'_>, stats: &Snapshot, assistant: Option<Provider>) -> Vec<Action> {
        let rect = ctx.rect;
        let gauges = Rect::from_min_size(rect.min, vec2(rect.width(), GAUGES_H));
        let mut actions = Vec::new();
        if let Some(picked) = self.gauges.ui(ctx, stats, gauges) {
            actions.push(Action::ListApps(Some(picked).filter(|picked| stats.listing != Some(*picked))));
        }
        actions.extend(self.apps.ui(ctx, stats, gauges.left_bottom(), rect.width()));
        let under_apps = gauges.left_bottom() + vec2(0.0, self.apps.height());
        actions.extend(self.storage.ui(ctx, stats, under_apps, rect.width()));
        let Some(assistant) = assistant else { return actions };
        let card = Rect::from_min_size(
            pos2(rect.left(), gauges.bottom() + self.lists_height() + metrics::SECTION_GAP),
            vec2(rect.width(), card::height(stats.usage(assistant).0, assistant)),
        );
        actions.extend(self.card.ui(ctx, stats, card, assistant));
        week::paint(ctx, card.bottom(), stats.history.week(assistant), assistant);
        actions
    }

    pub fn is_animating(&self) -> bool {
        self.card.is_animating()
            || self.gauges.is_animating()
            || self.apps.is_animating()
            || self.storage.is_animating()
    }

    fn lists_height(&self) -> f32 {
        self.apps.height() + self.storage.height()
    }
}

#[cfg(test)]
mod tests {
    use std::time::SystemTime;

    use super::*;
    use crate::stats::{PlanUsage, UsageWindow, Week};

    fn weekly_only() -> PlanUsage {
        let weekly = UsageWindow { used: 0.4, resets_at: None, estimated: false, minutes: Some(10_080) };
        PlanUsage::Ready { five_hour: None, weekly: Some(weekly), fetched_at: SystemTime::UNIX_EPOCH }
    }

    #[test]
    fn page_stacks_gauges_over_the_card() {
        let page = StatsPage::default();
        let mut snapshot = Snapshot::default();
        for claude in [PlanUsage::Loading, PlanUsage::SignedOut, weekly_only()] {
            snapshot.claude = claude;
            let expected = GAUGES_H + metrics::SECTION_GAP + card::height(&snapshot.claude, Provider::Claude);
            assert_eq!(page.height(&snapshot, Some(Provider::Claude)), expected);
        }
        assert!(!page.is_animating());
    }

    #[test]
    fn no_five_hour_limit_is_shorter() {
        let page = StatsPage::default();
        let loading = page.height(&Snapshot::default(), Some(Provider::Claude));
        let weekly = page.height(&Snapshot { claude: weekly_only(), ..Snapshot::default() }, Some(Provider::Claude));
        assert_eq!(page.height(&Snapshot::default(), None), GAUGES_H, "no assistant, no usage card");
        assert!(weekly < loading);
    }

    #[test]
    fn week_only_for_chosen_assistant() {
        let page = StatsPage::default();
        let mut snapshot = Snapshot::default();
        let bare = page.height(&snapshot, Some(Provider::Claude));
        snapshot.history.chatgpt = Week::ending_today([None, None, None, None, None, None, Some(0.4)]);
        assert_eq!(page.height(&snapshot, Some(Provider::Claude)), bare, "no Claude samples, no strip");
        let codex = page.height(&snapshot, Some(Provider::Codex));
        let codex_card = card::height(&snapshot.chatgpt, Provider::Codex);
        assert_eq!(codex, GAUGES_H + metrics::SECTION_GAP + codex_card + week::height(&snapshot.history.chatgpt));
        assert!(week::height(&snapshot.history.chatgpt) > 0.0);
        assert_eq!(page.height(&snapshot, None), GAUGES_H);
    }
}
