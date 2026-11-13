use super::*;

#[test]
fn idle_covers_hardware_notch() {
    let n = Notch::with_preview(Style::Notch, Some(vec2(185.0, 32.0)), None);
    let camera = Rect::from_min_size(pos2(340.0 - 92.5, 0.0), vec2(185.0, 32.0));
    assert_eq!(n.body_rect(n.morph.current()), camera);
    assert!(n.is_collapsed());
    let header = n.header();
    assert!(header.bar.top() >= camera.bottom(), "the icons sit under the camera");
    assert!(n.content_rect(Page::Tools).top() > header.bar.bottom());
    assert!(
        header.gear.bottom() <= camera.bottom() && header.gear.left() > camera.right(),
        "the gear sits beside the camera"
    );
}

#[test]
fn island_row_clears_gear() {
    let n = Notch::with_preview(Style::Island, None, None);
    let (header, panel) = (n.header(), n.body_rect(n.expanded_geometry()));
    assert!(header.bar.top() > panel.top() && header.bar.bottom() < header.body_top);
    assert!(header.bar.right() <= header.gear.left() - GEAR_GAP + 1e-3);
    assert_eq!(header.gear.right(), panel.right() - PAD);
    assert!((header.bar.center().x - panel.center().x).abs() < 1e-3, "a short row stays centered");
}

#[test]
fn crowded_row_shrinks_evenly() {
    let lane = Lane { left: 16.0, right: 300.0, anchor: 186.0, center_y: 20.0 };
    let few = row(lane, 6.0);
    let slots = layout(few, 6);
    assert!(slots.iter().all(|slot| (slot.width() - SLOT).abs() < 1e-3));
    let crowded = row(lane, 12.0);
    assert!(crowded.left() >= 16.0 && crowded.right() <= 300.0 + 1e-3);
    assert!(layout(crowded, 12).iter().all(|slot| slot.width() >= SLOT_MIN && slot.width() < SLOT));
}

#[test]
fn release_lands_on_nearest_tab() {
    let bar = Rect::from_center_size(pos2(200.0, 20.0), vec2(90.0, 20.0));
    let slots = layout(bar, 3);
    assert_eq!(nearest(&slots, slots[2].center().x + 3.0), 2);
    assert_eq!(nearest(&slots, -500.0), 0);
    assert_eq!(nearest(&slots, 900.0), 2);
}

#[test]
fn pill_sits_under_chosen_tab() {
    let mut n = Notch::with_preview(Style::Island, None, None);
    n.open_page(Page::Stats);
    let bar = n.header().bar;
    let slots = layout(bar, n.tabs.len());
    let at = n.tabs.iter().position(|tab| tab.page == Page::Stats).unwrap();
    n.pill_toward(&slots, None);
    let pill = n.tab_pill.rect(bar.center().y, bar.height() - 2.0 * PILL_INSET);
    assert!((pill.center().x - slots[at].center().x).abs() < 0.5, "no slide in from nowhere on open");
}

fn dots(n: &mut Notch, badges: Badges, tint: egui::Color32) -> Vec<egui::Pos2> {
    let ctx = egui::Context::default();
    crate::ui::font::install(&ctx);
    let mut found = Vec::new();
    let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
        let canvas = n.header_ui(ui, ui.painter(), false, badges);
        found = canvas
            .shapes
            .iter()
            .filter_map(|shape| match shape {
                egui::Shape::Circle(circle) if circle.fill == tint => Some(circle.center),
                _ => None,
            })
            .collect();
    });
    output.textures_delta.clear();
    found
}

#[test]
fn yellow_dot_while_run_waits() {
    let waiting = Badges { waiting: true, ..Badges::default() };
    for page in [Page::Stats, Page::Agents] {
        let mut n = Notch::with_preview(Style::Island, None, None);
        n.open_page(page);
        let found = dots(&mut n, waiting, color::YELLOW);
        assert_eq!(found.len(), 1, "one dot while on {page:?}");
        assert!(n.header().bar.expand(4.0).contains(found[0]), "the dot sits on a tab, not the gear");
        assert!(dots(&mut n, Badges::default(), color::YELLOW).is_empty());
        n.tabs.retain(|tab| tab.page != Page::Agents);
        assert!(dots(&mut n, waiting, color::YELLOW).is_empty(), "no Agents tab, no dot");
    }
}

#[test]
fn gear_and_tab_share_dot() {
    let mut n = Notch::with_preview(Style::Island, None, None);
    let both = Badges { unread: true, waiting: true };
    let gear = dots(&mut n, both, color::ACCENT);
    assert_eq!(gear, [badge::at(n.header().gear.center(), GEAR_GLYPH)]);
    assert_eq!(dots(&mut n, both, color::YELLOW).len(), 1);
}

#[test]
fn gear_returns_to_last_tab() {
    let mut n = Notch::with_preview(Style::Island, None, None);
    n.open_page(Page::Stats);
    n.toggle_settings();
    assert_eq!(n.pager.page, Page::Settings);
    n.switch_to(Page::Notifications);
    n.toggle_settings();
    assert_eq!(n.pager.page, Page::Stats);
}

#[test]
fn only_switch_ends_light_up() {
    let sweep = Sweep { from: 0.0, to: 4.0, value: 2.0, emphasis: 1.0 };
    assert_eq!([0, 1, 2, 3, 4].map(|i| sweep.open(i)), [0.5, 0.0, 0.0, 0.0, 0.5]);
    let overshoot = Sweep { value: 4.3, ..sweep };
    assert_eq!(overshoot.open(4), 1.0);
}

#[test]
fn added_tab_makes_room() {
    let mut n = Notch::with_preview(Style::Island, None, None);
    n.open_page(Page::Tools);
    crate::notch::tests::fixtures::settle(&mut n);
    let lane = n.header().lane;
    let centers = |tabs: &[Gliding]| tabs.iter().map(|tab| tab.seat.center().x).collect::<Vec<f32>>();
    let before = centers(&n.gliding_tabs(lane));
    assert!(before.windows(2).all(|pair| pair[1] - pair[0] > 10.0), "tabs already there are simply there");
    n.panel_alpha = 1.0;
    let mut tabs: Vec<Tab> = n.tabs.clone();
    tabs.retain(|tab| tab.page != Page::Sound);
    tabs.insert(1, Tab::core(Page::Sound));
    n.sync_tabs(&tabs);
    let first = n.gliding_tabs(lane);
    let grown = first.iter().find(|tab| tab.tab.page == Page::Sound).expect("the new tab is laid out");
    assert_eq!(grown.shown, 0.0, "it starts with no room");
    let others: Vec<f32> =
        first.iter().filter(|tab| tab.tab.page != Page::Sound).map(|tab| tab.seat.center().x).collect();
    let jumped = others.iter().zip(&before).map(|(a, b)| (a - b).abs()).fold(0.0, f32::max);
    assert!(jumped < 0.5, "nothing moves on the first frame: {jumped}");
    for _ in 0..3 {
        n.step(1.0 / 60.0);
    }
    let partway = centers(&n.gliding_tabs(lane));
    crate::notch::tests::fixtures::settle(&mut n);
    let settled = n.gliding_tabs(lane);
    let target = layout(n.header().bar, n.tabs.len());
    for (tab, slot) in settled.iter().zip(&target) {
        assert!((tab.seat.center().x - slot.center().x).abs() < 0.01, "they end on their slots");
    }
    let last = centers(&settled);
    assert!(partway.iter().zip(&last).any(|(a, b)| (a - b).abs() > 1.0), "and glide there");
}
