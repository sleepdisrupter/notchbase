//! Which layout each app usually gets, learned from what's picked and kept in `snap-habits.json`. Standard and
//! ultrawide screens are learned apart.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::{Choice, Zone};

/// Each pick for an app fades the earlier ones by this much, so a new habit takes over.
const FORGET: f32 = 0.9;
/// Layouts remembered for each app and kind of screen.
const MOST_KEPT: usize = 6;
// A layout is the usual one once it has the weight of two picks and 60% of the app's.
const ENOUGH_USES: f32 = 2.0;
const ENOUGH_SHARE: f32 = 0.6;
/// Apps remembered at most. The least used one is forgotten to make room.
const MOST_APPS: usize = 200;
const FILE: &str = "snap-habits.json";

/// The kind of screen, standard or ultrawide. Each kind keeps its own habits.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Class {
    Standard,
    Wide,
}

impl Class {
    fn id(self) -> &'static str {
        match self {
            Class::Standard => "standard",
            Class::Wide => "wide",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct Tally {
    choice: String,
    weight: f32,
}

#[derive(Default, Serialize, Deserialize)]
pub struct Habits {
    apps: BTreeMap<String, Vec<Tally>>,
    #[serde(skip)]
    path: Option<PathBuf>,
}

impl Habits {
    pub fn load() -> Self {
        let path = crate::platform::data_dir().map(|dir| dir.join(FILE));
        let mut habits: Self = path
            .as_ref()
            .and_then(|path| std::fs::read(path).ok())
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        habits.path = path;
        habits
    }

    pub fn learn(&mut self, app: &str, class: Class, choice: Choice) {
        let key = key(app, class);
        let tallies = self.apps.entry(key.clone()).or_default();
        for tally in tallies.iter_mut() {
            tally.weight *= FORGET;
        }
        match tallies.iter_mut().find(|tally| tally.choice == choice.id()) {
            Some(tally) => tally.weight += 1.0,
            None => tallies.push(Tally { choice: choice.id(), weight: 1.0 }),
        }
        tallies.sort_by(|a, b| b.weight.total_cmp(&a.weight));
        tallies.truncate(MOST_KEPT);
        if self.apps.len() > MOST_APPS {
            self.forget_least_used(&key);
        }
    }

    /// The app's usual layout on this kind of screen, once one stands out.
    pub fn usual(&self, app: &str, class: Class) -> Option<Choice> {
        let tallies = self.apps.get(&key(app, class))?;
        let total: f32 = tallies.iter().map(|tally| tally.weight).sum();
        let best = tallies.first()?;
        let sure = best.weight >= ENOUGH_USES && best.weight / total >= ENOUGH_SHARE;
        sure.then(|| Choice::from_id(&best.choice)).flatten()
    }

    pub fn save(&self) {
        let Some(path) = &self.path else { return };
        let Ok(bytes) = serde_json::to_vec(self) else { return };
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(path, bytes);
    }

    fn forget_least_used(&mut self, keep: &str) {
        let weight = |tallies: &Vec<Tally>| tallies.iter().map(|tally| tally.weight).sum::<f32>();
        let weakest = self
            .apps
            .iter()
            .filter(|(key, _)| key.as_str() != keep)
            .min_by(|a, b| weight(a.1).total_cmp(&weight(b.1)))
            .map(|(key, _)| key.clone());
        if let Some(weakest) = weakest {
            self.apps.remove(&weakest);
        }
    }
}

fn key(app: &str, class: Class) -> String {
    format!("{}|{}", app.to_lowercase(), class.id())
}

impl Choice {
    fn id(self) -> String {
        match self {
            Choice::Auto => "auto".to_owned(),
            Choice::Fill => "fill".to_owned(),
            Choice::Zone(zone) => format!("{zone:?}"),
        }
    }

    fn from_id(id: &str) -> Option<Self> {
        match id {
            "auto" => return Some(Choice::Auto),
            "fill" => return Some(Choice::Fill),
            _ => {}
        }
        Zone::ALL.into_iter().find(|zone| format!("{zone:?}") == id).map(Choice::Zone)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LEFT: Choice = Choice::Zone(Zone::Left);
    const RIGHT: Choice = Choice::Zone(Zone::Right);

    #[test]
    fn nothing_is_assumed_from_a_single_use() {
        let mut habits = Habits::default();
        habits.learn("Safari", Class::Standard, LEFT);
        assert_eq!(habits.usual("Safari", Class::Standard), None);
    }

    #[test]
    fn repeated_choice_becomes_usual() {
        let mut habits = Habits::default();
        for _ in 0..3 {
            habits.learn("Safari", Class::Standard, LEFT);
        }
        assert_eq!(habits.usual("safari", Class::Standard), Some(LEFT));
        assert_eq!(habits.usual("Safari", Class::Wide), None, "another kind of screen has its own habits");
        assert_eq!(habits.usual("Mail", Class::Standard), None);
    }

    #[test]
    fn split_between_two_isnt_a_habit() {
        let mut habits = Habits::default();
        for _ in 0..3 {
            habits.learn("Mail", Class::Standard, LEFT);
            habits.learn("Mail", Class::Standard, RIGHT);
        }
        assert_eq!(habits.usual("Mail", Class::Standard), None);
    }

    #[test]
    fn changed_mind_wins_over_time() {
        let mut habits = Habits::default();
        for _ in 0..4 {
            habits.learn("Notes", Class::Wide, LEFT);
        }
        for _ in 0..8 {
            habits.learn("Notes", Class::Wide, Choice::Auto);
        }
        assert_eq!(habits.usual("Notes", Class::Wide), Some(Choice::Auto));
    }

    #[test]
    fn habits_round_trip_through_json() {
        let mut habits = Habits::default();
        for _ in 0..3 {
            habits.learn("Figma", Class::Wide, Choice::Zone(Zone::CenterThird));
        }
        let again: Habits = serde_json::from_slice(&serde_json::to_vec(&habits).unwrap()).unwrap();
        assert_eq!(again.usual("Figma", Class::Wide), Some(Choice::Zone(Zone::CenterThird)));
    }

    #[test]
    fn least_used_app_is_forgotten() {
        let mut habits = Habits::default();
        habits.learn("Rare", Class::Standard, LEFT);
        for n in 0..MOST_APPS {
            for _ in 0..2 {
                habits.learn(&format!("app{n}"), Class::Standard, LEFT);
            }
        }
        assert!(habits.apps.len() <= MOST_APPS);
        assert!(!habits.apps.contains_key(&key("Rare", Class::Standard)));
    }
}
