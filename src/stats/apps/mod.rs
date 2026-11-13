//! Per-app CPU and memory for the gauges that unfold into a list. Only macOS reads apps; elsewhere the list is empty.

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
use macos as imp;

#[cfg(not(target_os = "macos"))]
mod other;
#[cfg(not(target_os = "macos"))]
use other as imp;

use std::cmp::Ordering;
use std::sync::Arc;

pub(super) use imp::{Watch, quit};

pub const LISTED: usize = 5;
/// Only macOS can list apps, so elsewhere the gauges don't unfold.
pub const LISTS_APPS: bool = cfg!(target_os = "macos");

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Resource {
    Cpu,
    Memory,
    Disk,
}

impl Resource {
    pub fn lists_apps(self) -> bool {
        matches!(self, Self::Cpu | Self::Memory)
    }
}

#[derive(Clone, Debug)]
pub struct AppUsage {
    pub key: String,
    pub name: String,
    pub icon: Option<Arc<egui::ColorImage>>,
    pub cpu: f32,
    pub memory: u64,
    pub quittable: bool,
}

impl AppUsage {
    fn amount(&self, of: Resource) -> f64 {
        amount(of, self.cpu, self.memory)
    }
}

fn amount(of: Resource, cpu: f32, memory: u64) -> f64 {
    match of {
        Resource::Cpu if cpu.is_finite() => f64::from(cpu),
        Resource::Cpu | Resource::Disk => 0.0,
        Resource::Memory => memory as f64,
    }
}

/// The five heaviest apps for one resource, ties broken by name.
pub fn top(apps: &[AppUsage], by: Resource) -> Vec<&AppUsage> {
    let mut ranked: Vec<&AppUsage> = apps.iter().collect();
    ranked.sort_by(|a, b| heavier_first(a, b, by));
    ranked.truncate(LISTED);
    ranked
}

fn heavier_first(a: &AppUsage, b: &AppUsage, by: Resource) -> Ordering {
    b.amount(by).total_cmp(&a.amount(by)).then_with(|| a.name.cmp(&b.name))
}

/// The keys worth publishing: the five heaviest by CPU and the five by memory, each once.
#[cfg(any(target_os = "macos", test))]
fn leading_keys<'a>(usage: impl Iterator<Item = (&'a str, f32, u64)> + Clone) -> Vec<&'a str> {
    let mut keys = Vec::new();
    for by in [Resource::Cpu, Resource::Memory] {
        let mut ranked: Vec<(&str, f64)> =
            usage.clone().map(|(key, cpu, memory)| (key, amount(by, cpu, memory))).collect();
        ranked.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0.cmp(b.0)));
        for (key, _) in ranked.into_iter().take(LISTED) {
            if !keys.contains(&key) {
                keys.push(key);
            }
        }
    }
    keys
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app(name: &str, cpu: f32, memory: u64) -> AppUsage {
        AppUsage { key: name.to_lowercase(), name: name.to_owned(), icon: None, cpu, memory, quittable: true }
    }

    fn names(apps: &[AppUsage], by: Resource) -> Vec<&str> {
        top(apps, by).into_iter().map(|app| app.name.as_str()).collect()
    }

    #[test]
    fn heaviest_five_come_first() {
        let apps = [
            app("Notes", 0.01, 900),
            app("Chrome", 0.31, 400),
            app("Xcode", 0.18, 800),
            app("Figma", 0.06, 100),
            app("Slack", 0.02, 300),
            app("Mail", 0.0, 50),
        ];
        assert_eq!(names(&apps, Resource::Cpu), ["Chrome", "Xcode", "Figma", "Slack", "Notes"]);
        assert_eq!(names(&apps, Resource::Memory), ["Notes", "Xcode", "Chrome", "Slack", "Figma"]);
    }

    #[test]
    fn ties_keep_a_steady_order() {
        let apps = [app("Zed", 0.0, 10), app("Arc", 0.0, 10), app("Mail", f32::NAN, 10)];
        assert_eq!(names(&apps, Resource::Memory), ["Arc", "Mail", "Zed"]);
        assert_eq!(names(&apps, Resource::Cpu), ["Arc", "Mail", "Zed"]);
        assert!(top(&[], Resource::Cpu).is_empty());
    }

    #[test]
    fn leaders_are_heaviest_of_either() {
        let usage: Vec<(String, f32, u64)> =
            (0..12).map(|i| (format!("app{i:02}"), i as f32 / 100.0, 1000 - i as u64 * 10)).collect();
        let keys = leading_keys(usage.iter().map(|(key, cpu, memory)| (key.as_str(), *cpu, *memory)));
        assert_eq!(keys, ["app11", "app10", "app09", "app08", "app07", "app00", "app01", "app02", "app03", "app04"]);
        let few = leading_keys(usage.iter().take(3).map(|(key, cpu, memory)| (key.as_str(), *cpu, *memory)));
        assert_eq!(few, ["app02", "app01", "app00"]);
    }
}
