use notchbase_sdk::testing::Harness;

use super::*;

fn on_battery(level: u8) -> Power {
    Power { level: Some(level), plugged: false, charging: false }
}

fn charging(level: u8) -> Power {
    Power { level: Some(level), plugged: true, charging: true }
}

fn said(harness: &Harness<Battery>, words: &str) -> bool {
    harness.noticed().iter().any(|text| text == words)
}

#[test]
fn first_reading_is_quiet() {
    let mut harness = Harness::<Battery>::start();
    assert!(harness.power(charging(100)).is_empty());
    let mut harness = Harness::<Battery>::start();
    assert!(harness.power(on_battery(8)).is_empty(), "a battery that was already low at launch is not news");
    assert!(harness.power(on_battery(7)).is_empty());
}

#[test]
fn charger_shows_the_level() {
    let mut harness = Harness::<Battery>::start();
    harness.power(on_battery(64));
    harness.power(charging(64));
    assert!(said(&harness, "Charging") && said(&harness, "64%"));
    harness.power(charging(65));
    assert!(harness.requests().is_empty(), "charging on quietly says nothing more");
    harness.power(on_battery(65));
    assert!(said(&harness, "On Battery") && said(&harness, "65%"));
    harness.power(Power { level: Some(65), plugged: true, charging: false });
    assert!(said(&harness, "Plugged In"));
}

#[test]
fn warns_once_per_threshold() {
    let mut harness = Harness::<Battery>::start();
    harness.power(on_battery(22));
    assert!(harness.power(on_battery(21)).is_empty());
    harness.power(on_battery(20));
    assert!(said(&harness, "Low Battery") && said(&harness, "20%"));
    assert!(harness.power(on_battery(19)).is_empty());
    assert!(harness.power(on_battery(11)).is_empty());
    harness.power(on_battery(10));
    assert!(said(&harness, "Low Battery") && said(&harness, "10%"));
    assert!(harness.power(on_battery(9)).is_empty());
    harness.power(charging(9));
    harness.power(on_battery(9));
    assert!(said(&harness, "On Battery"));
    assert!(harness.power(on_battery(8)).is_empty(), "unplugging while low already showed the level");
}

#[test]
fn full_is_announced_once() {
    let mut harness = Harness::<Battery>::start();
    harness.power(charging(98));
    assert!(harness.power(charging(99)).is_empty());
    harness.power(charging(100));
    assert!(said(&harness, "Fully Charged"));
    assert!(harness.power(Power { level: Some(100), plugged: true, charging: false }).is_empty());
    assert!(harness.power(charging(99)).is_empty());
    assert!(harness.power(charging(100)).is_empty());
}

#[test]
fn no_battery_shows_nothing() {
    let mut harness = Harness::<Battery>::start();
    let desktop = Power { level: None, plugged: true, charging: false };
    assert!(harness.power(desktop).is_empty());
    assert!(harness.power(Power { plugged: false, ..desktop }).is_empty());
}

#[test]
fn each_alert_can_be_switched_off() {
    let mut harness = Harness::<Battery>::start_with([(CHARGER, false), (LOW, false), (FULL, false)]);
    harness.power(on_battery(21));
    assert!(harness.power(on_battery(20)).is_empty());
    assert!(harness.power(charging(20)).is_empty());
    assert!(harness.power(charging(100)).is_empty());
    harness.change_settings([(FULL, true)]);
    harness.power(on_battery(100));
    harness.power(charging(99));
    harness.power(charging(100));
    assert!(said(&harness, "Fully Charged"));
}

#[test]
fn listens_for_power_and_settings() {
    let mut harness = Harness::<Battery>::start();
    assert!(harness.tile().is_none() && harness.live().is_none());
    let manifest = Battery::manifest();
    assert_eq!(manifest.settings.len(), 3);
    assert!(manifest.page.is_none());
}

#[test]
fn focus_holds_charger_news() {
    let mut harness = Harness::<Battery>::start();
    harness.power(on_battery(22));
    harness.focus(Focus { active: true, name: Some("Work".into()) });
    assert!(harness.power(charging(22)).is_empty());
    assert!(harness.power(on_battery(21)).is_empty());
    harness.power(on_battery(20));
    assert!(said(&harness, "Low Battery"), "a low battery still warns during a Focus");
    harness.focus(Focus::default());
    harness.power(charging(20));
    assert!(said(&harness, "Charging"));
}
