use std::time::Instant;

use egui::{Rect, vec2};
use notchbase_sdk::Notice;
use notchbase_sdk::element::lower_static;
use notchbase_sdk::prelude::{Icon, Tone, badge, button, row, text};
use notchbase_sdk::protocol::{NoticeNode, NoticeSize};

use crate::notch::Notch;
use crate::notch::activity::{ActivityKind, LANE_INSET};
use crate::notch::geometry::CANVAS;
use crate::notch::moves::transition::Motion;
use crate::notch::notice::controls::INLINE_GAP;
use crate::notch::notice::{ISLAND_H, ToolNotice};
use crate::notch::tests::fixtures::{Rig, click, island, settle, shortcuts};
use crate::platform::Style;

use super::{CAMERA, SLOT, controls, copied, macbook, pressed, show, showing, with_controls};

#[test]
fn island_tucks_controls_in_capsule() {
    let rig = Rig::new();
    let wordy = || Notice::new(text("Copied"), text("2560×1600 and on the clipboard"));
    let (mut plain, mut n) = (island(), island());
    show(&rig, &mut plain, ToolNotice::new(SLOT, wordy().into_node()), Instant::now());
    show(&rig, &mut n, with_controls(wordy()), Instant::now());
    let (body, at) = (n.silhouette().rect(), controls(&n));
    assert_eq!(body.height(), ISLAND_H);
    let room = showing(&n).controls.x + INLINE_GAP;
    assert!(body.width() > plain.silhouette().rect().width() + room - LANE_INSET, "it grows to fit them");
    assert!(body.contains_rect(at));
    let (side, above) = (body.right() - at.right(), at.top() - body.top());
    assert!((side - above).abs() < 0.01, "the pill sits evenly in the rounded end");
    assert!((at.center().y - body.center().y).abs() < 0.01);
}

#[test]
fn notch_hangs_controls_under_camera() {
    for camera in [vec2(185.0, 32.0), vec2(185.0, 38.0), vec2(180.0, 32.0), vec2(200.0, 37.0), vec2(160.0, 30.0)] {
        let rig = Rig::new();
        let mut n = Notch::with_preview(Style::Notch, Some(camera), None);
        let row = row![button("Annotate").on_press(()), button("Share").on_press(())];
        let shown = NoticeNode { controls: Some(lower_static(row)), ..copied().into_node() };
        show(&rig, &mut n, ToolNotice::new(SLOT, shown), Instant::now());
        let (body, at) = (n.silhouette().rect(), controls(&n));
        assert!(at.top() >= camera.y + 6.0, "{camera:?}: clear of the housing");
        assert!(body.contains_rect(at) && body.bottom() - at.bottom() >= 8.0, "{camera:?}: inside the shape");
        assert_eq!(at.center().x, body.center().x);
    }
}

#[test]
fn lone_button_ends_right_wing() {
    for camera in [vec2(185.0, 32.0), vec2(185.0, 38.0), vec2(180.0, 32.0), vec2(200.0, 37.0), vec2(160.0, 30.0)] {
        let rig = Rig::new();
        let mut n = Notch::with_preview(Style::Notch, Some(camera), None);
        let saved = Notice::new(badge(Icon::Checkmark, Tone::Green, 16.0), text("Saved"));
        show(&rig, &mut n, with_controls(saved), Instant::now());
        let (body, at) = (n.silhouette().rect(), controls(&n));
        let housing = Rect::from_center_size(egui::pos2(CANVAS.x * 0.5, camera.y * 0.5), camera);
        assert!(at.left() >= housing.right() + 10.0, "{camera:?}: the button sits in the right wing");
        assert!(body.contains_rect(at), "{camera:?}: inside the shape");
        assert!((at.center().y - camera.y * 0.5).abs() < 0.01, "{camera:?}: on the camera's line");
        assert!(body.height() <= camera.y + 2.0, "{camera:?}: one slim strip, no row underneath");
        let [leading, trailing] = showing(&n).widths.expect("measured");
        let content_right = body.left() + LANE_INSET + leading + 8.0 + trailing;
        assert!(content_right <= housing.left() - 10.0 + 0.01, "{camera:?}: the words stay left of the camera");
        assert!(body.width() <= n.panel_width(), "{camera:?}: never wider than the panel it opens");
    }
}

fn shot(icon: Option<Icon>) -> ToolNotice {
    let words = Notice::new(badge(Icon::Checkmark, Tone::Green, 30.0), badge(Icon::Checkmark, Tone::Green, 12.0));
    let annotate = button("Annotate").on_press(());
    let annotate = lower_static(match icon {
        Some(icon) => annotate.icon(icon),
        None => annotate,
    });
    ToolNotice::new(SLOT, NoticeNode { controls: Some(annotate), ..words.size(NoticeSize::Large).into_node() })
}

#[test]
fn wide_button_leans_the_shape() {
    for camera in [vec2(185.0, 32.0), vec2(185.0, 38.0), vec2(180.0, 32.0), vec2(200.0, 37.0), vec2(160.0, 30.0)] {
        let rig = Rig::new();
        let mut n = Notch::with_preview(Style::Notch, Some(camera), None);
        show(&rig, &mut n, shot(Some(Icon::Lock)), Instant::now());
        let (body, at) = (n.silhouette().rect(), controls(&n));
        let housing = Rect::from_center_size(egui::pos2(CANVAS.x * 0.5, camera.y * 0.5), camera);
        assert!(body.height() <= camera.y + 2.0, "{camera:?}: one slim strip, no row underneath");
        assert!(at.left() >= housing.right() + 10.0 && body.contains_rect(at), "{camera:?}: in the right wing");
        assert!(body.width() <= n.panel_width() + 0.01, "{camera:?}: never wider than the panel it opens");
        assert!(body.center().x >= housing.center().x - 0.01, "{camera:?}: it only ever leans toward the button");
        let [leading, trailing] = showing(&n).widths.expect("measured");
        let content_right = body.left() + LANE_INSET + leading + 8.0 + trailing;
        assert!(content_right <= housing.left() - 10.0 + 0.01, "{camera:?}: the words stay left of the camera");
    }
}

#[test]
fn leaning_shape_glides_home() {
    let rig = Rig::new();
    let mut n = macbook();
    show(&rig, &mut n, shot(Some(Icon::Lock)), Instant::now());
    assert!(n.silhouette().center_x > CANVAS.x * 0.5 + 1.0);
    let beside = egui::pos2(n.silhouette().rect().left() + 20.0, n.silhouette().rect().top() + 16.0);
    assert_eq!(click(&rig, &mut n, beside), []);
    assert_eq!(n.mode, crate::notch::Mode::Expanded);
    settle(&mut n);
    assert!((n.silhouette().center_x - CANVAS.x * 0.5).abs() < 0.01, "the open panel sits on the camera");
}

#[test]
fn even_wings_need_no_lean() {
    let rig = Rig::new();
    let mut n = macbook();
    show(&rig, &mut n, shot(None), Instant::now());
    assert!((n.silhouette().center_x - CANVAS.x * 0.5).abs() < 0.01);
    assert!(controls(&n).left() >= CANVAS.x * 0.5 + CAMERA.x * 0.5 + 10.0);
}

#[test]
fn menus_send_button_underneath() {
    let rig = Rig::new();
    let mut n = macbook();
    show(&rig, &mut n, shot(Some(Icon::Lock)), Instant::now());
    let left = n.silhouette().rect().left();
    n.set_menu_edge(Some(left + 4.0));
    n.step(1.0 / 60.0);
    settle(&mut n);
    let (body, at) = (n.silhouette().rect(), controls(&n));
    assert!(at.top() >= CAMERA.y + 6.0, "the button hangs under the camera again");
    assert!(body.contains_rect(at));
}

#[test]
fn wordy_button_hangs_underneath() {
    let rig = Rig::new();
    let mut n = macbook();
    show(&rig, &mut n, with_controls(copied()), Instant::now());
    let (body, at) = (n.silhouette().rect(), controls(&n));
    assert!(body.width() <= n.panel_width());
    assert!(at.top() >= CAMERA.y + 6.0 && (at.center().x - body.center().x).abs() < 0.01);
}

#[test]
fn controls_go_under_text_and_banner() {
    let detail = || text("notchbase 0.2 is live on all regions");
    for style in [Style::Island, Style::Notch] {
        for shown in [copied().below(detail()), copied().below(detail()).banner()] {
            let rig = Rig::new();
            let mut bare = Notch::with_preview(style, Some(CAMERA), None);
            let mut n = Notch::with_preview(style, Some(CAMERA), None);
            show(&rig, &mut n, with_controls(shown), Instant::now());
            let mut without = showing(&n).clone();
            without.node.controls = None;
            show(&rig, &mut bare, ToolNotice::new(SLOT, *without.node), Instant::now());
            let (body, at) = (n.silhouette().rect(), controls(&n));
            let content = bare.silhouette().rect();
            assert!(body.contains_rect(at), "{style:?}");
            assert!(at.top() >= content.bottom() - 16.0 && body.bottom() > content.bottom(), "{style:?}");
            assert_eq!(click(&rig, &mut n, at.center()), [pressed()]);
        }
    }
}

#[test]
fn plain_notice_takes_no_presses() {
    let rig = Rig::new();
    let mut n = macbook();
    show(&rig, &mut n, ToolNotice::new(SLOT, copied().into_node()), Instant::now());
    assert_eq!(n.silhouette().rect().height(), CAMERA.y);
    assert!(!n.hovers_controls(Some(n.silhouette().rect().center())));
}

#[test]
fn shaking_alert_stays_springy() {
    for mut n in [island(), macbook()] {
        let rig = Rig::new();
        let alert = ToolNotice::new(SLOT, Notice::new(text("Timer"), text("done")).shake().into_node());
        n.start_activity(ActivityKind::Tool(alert), Instant::now());
        rig.frame(&mut n, &shortcuts(None), Vec::new());
        assert!(showing(&n).widths.is_some(), "the first frame measures it");
        assert_eq!(n.morph.w.config, n.spring_configs(Motion::Alert).0);
        assert_eq!(n.morph.w.target, n.activity_geometry().w, "and it heads for its measured size");
    }
}

#[test]
fn right_wing_button_takes_press() {
    let rig = Rig::new();
    let mut n = macbook();
    let saved = Notice::new(badge(Icon::Checkmark, Tone::Green, 16.0), text("Saved"));
    show(&rig, &mut n, with_controls(saved), Instant::now());
    let at = controls(&n).center();
    assert!(at.x > n.silhouette().rect().center().x + CAMERA.x * 0.5, "it sits right of the camera");
    assert_eq!(click(&rig, &mut n, at), [pressed()]);
    assert_eq!(n.mode, crate::notch::Mode::Activity);
}
