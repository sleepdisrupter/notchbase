use std::time::{Duration, Instant};

use egui::{pos2, vec2};

use crate::bridge::Page;
use crate::media::NowPlaying;
use crate::notch::activity::ActivityKind;
use crate::notch::moves::transition::Motion;
use crate::notch::tests::fixtures::{AWAY, island, ms, sample_notice, settle, song};
use crate::notch::{Mode, Notch};
use crate::platform::Style;

#[test]
fn remaining_ticks_every_second() {
    let mut n = island();
    let t0 = Instant::now();
    n.set_now_playing(Some(&song(true, 10.4)));
    assert!(n.music_tick(t0));
    assert!(!n.music_tick(t0 + Duration::from_millis(100)));
    let wake = n.music_wakeup(t0).expect("a wakeup");
    assert!(wake > t0 && wake <= t0 + Duration::from_secs(1));
    assert!(n.music_tick(t0 + Duration::from_millis(1_100)));
    assert!(!n.set_now_playing(Some(&song(true, 10.4))), "a steady clock isn't a change");
    n.set_now_playing(Some(&song(false, 10.4)));
    assert_eq!(n.music_wakeup(t0), None);
    assert!(n.set_now_playing(None));
}

#[test]
fn song_shows_only_on_change() {
    let mut n = island();
    n.set_now_playing(Some(&song(true, 10.0)));
    assert!(n.shows_music());
    let hide_at = n.music_until.expect("revealed");
    assert!(!n.set_now_playing(Some(&song(true, 10.0))));
    assert_eq!(n.music_until, Some(hide_at), "a steady song doesn't extend the reveal");
    n.tick(hide_at + Duration::from_millis(10), Some(AWAY));
    assert!(!n.shows_music());
    settle(&mut n);
    assert!(n.is_collapsed());

    n.show_volume(0.5, false);
    assert!(n.music_until.is_some(), "a volume change brings the song back after the bar");
    n.set_now_playing(Some(&song(false, 10.0)));
    assert!(n.music_until.is_some());
    let next = NowPlaying { title: "Next".into(), ..song(true, 0.0) };
    let before = n.music_until;
    n.set_now_playing(Some(&next));
    assert!(n.music_until >= before);
    n.set_now_playing(None);
    assert!(n.music_until.is_none() && !n.shows_music());
}

#[test]
fn opening_on_song_lands_on_media() {
    let mut n: Notch = island();
    n.set_now_playing(Some(&song(true, 0.0)));
    n.opens_to_music();
    assert_eq!(n.pager.page, Page::Media);
}

#[test]
fn approach_brings_song_back() {
    let mut n = island();
    n.set_now_playing(Some(&song(true, 10.0)));
    let hidden_at = n.music_until.expect("revealed") + Duration::from_millis(10);
    n.tick(hidden_at, Some(AWAY));
    assert!(!n.shows_music());
    n.tick(hidden_at, Some(pos2(340.0, 0.0)));
    n.tick(hidden_at, Some(pos2(340.0, 30.0)));
    assert!(n.shows_music(), "a nearby cursor calls the song up");
    n.tick(hidden_at + Duration::from_secs(5), Some(AWAY));
    assert!(!n.shows_music(), "and it tucks away after the cursor leaves");
}

#[test]
fn summoned_song_stays_while_near() {
    let mut n = island();
    n.set_now_playing(Some(&song(true, 10.0)));
    let t0 = n.music_until.expect("revealed") + ms(10);
    n.tick(t0, Some(AWAY));
    let edge = pos2(340.0, 0.0);
    n.tick(t0, Some(edge));
    let left = t0 + ms(900);
    n.tick(left, Some(edge));
    assert!(n.shows_music(), "it stays up for as long as the cursor is near");
    n.tick(left + ms(100), Some(AWAY));
    assert!(n.shows_music(), "a cursor that only brushes the edge doesn't make it flicker");
    n.tick(left + ms(500), Some(AWAY));
    assert!(!n.shows_music(), "and it doesn't hang around once the cursor is gone");
}

#[test]
fn tucked_song_folds_in_place() {
    let mut n = Notch::with_preview(Style::Notch, Some(vec2(185.0, 32.0)), None);
    n.set_now_playing(Some(&song(true, 10.0)));
    n.tick(Instant::now(), Some(AWAY));
    settle(&mut n);
    let tucked = n.silhouette();
    assert!(tucked.h == 32.0 && tucked.w > 185.0, "the song sits beside the camera");
    n.tick(n.music_until.expect("revealed") + ms(10), Some(AWAY));
    assert!(!n.shows_music());
    let mut last = tucked.w;
    for _ in 0..240 {
        n.step(1.0 / 120.0);
        let s = n.silhouette();
        assert!(s.h <= 32.0 + 1e-3, "it folds in along the camera's edge instead of sagging below it ({})", s.h);
        assert!(s.w <= last + 0.05, "and only ever narrows ({} after {last})", s.w);
        last = s.w;
    }
    assert!(n.is_collapsed());
}

#[test]
fn tucking_leaves_panel_motion() {
    for style in [Style::Island, Style::Notch] {
        let mut n = Notch::with_preview(style, Some(vec2(185.0, 32.0)), None);
        n.set_now_playing(Some(&song(true, 10.0)));
        n.set_mode(Mode::Expanded, Motion::Open);
        n.step(1.0 / 60.0);
        let opening = n.morph.springs().map(|spring| spring.config);
        n.tick(n.music_until.expect("revealed") + ms(10), Some(pos2(340.0, 120.0)));
        assert!(!n.music_revealed());
        assert_eq!(n.morph.springs().map(|spring| spring.config), opening, "{style:?} still opens the way it started");
    }
}

fn dance(n: &mut Notch) -> f32 {
    for _ in 0..90 {
        n.set_music_levels([1.0, 0.8, 0.6, 0.5, 0.4, 0.3]);
        n.step(1.0 / 120.0);
    }
    n.silhouette().wave.map_or(0.0, |wave| wave.deepest())
}

#[test]
fn dances_through_activities() {
    let mut n = Notch::with_preview(Style::Notch, Some(vec2(185.0, 32.0)), None);
    let t0 = Instant::now();
    n.set_now_playing(Some(&song(true, 10.0)));
    n.tick(t0 + Duration::from_secs(10), Some(AWAY));
    settle(&mut n);
    assert!(!n.shows_music());
    n.start_activity(ActivityKind::Tool(sample_notice(2_200)), t0 + Duration::from_secs(10));
    settle(&mut n);
    assert!(dance(&mut n) > 0.5, "a picked color still dances to the song");

    n.set_now_playing(Some(&song(false, 10.0)));
    settle(&mut n);
    assert!(dance(&mut n) < 0.05, "a paused song keeps the edge still");
}

#[test]
fn open_panel_dances_bottom_edge() {
    for style in [Style::Island, Style::Notch] {
        let mut n = Notch::with_preview(style, Some(vec2(185.0, 32.0)), None);
        n.set_now_playing(Some(&song(true, 10.0)));
        n.open_page(Page::Media);
        settle(&mut n);
        assert!(dance(&mut n) > 0.5, "{style:?} open panel follows the song");

        n.set_now_playing(Some(&song(false, 10.0)));
        settle(&mut n);
        assert!(dance(&mut n) < 0.05, "{style:?} paused keeps the edge still");
    }
}

#[test]
fn listens_only_while_dancing() {
    let mut n = Notch::with_preview(Style::Notch, Some(vec2(185.0, 32.0)), None);
    let t0 = Instant::now();
    n.set_now_playing(Some(&song(true, 10.0)));
    assert!(n.dances(), "a new song is revealed and dances");
    n.tick(t0 + Duration::from_secs(10), Some(AWAY));
    settle(&mut n);
    assert!(!n.dances(), "once it tucks away nothing listens, so the Mac's purple dot goes out");
    n.start_activity(ActivityKind::Tool(sample_notice(2_200)), t0 + Duration::from_secs(10));
    assert!(n.dances(), "a notice over the playing song dances to it again");
    n.set_now_playing(Some(&song(false, 10.0)));
    assert!(!n.dances(), "a paused song is never listened to");
}

#[test]
fn silent_song_fades_out() {
    let rig = crate::notch::tests::fixtures::Rig::new();
    let mut n = Notch::with_preview(Style::Notch, Some(vec2(185.0, 32.0)), None);
    n.set_now_playing(Some(&song(true, 10.0)));
    for _ in 0..60 {
        rig.frame(&mut n, &[], Vec::new());
    }
    n.set_now_playing(Some(&song(false, 10.5)));
    n.set_now_playing(None);
    assert!(n.music.is_none() && n.music_until.is_none());
    assert!(n.music_leaving.is_some(), "the song it showed stays to fade out");
    let (_, mut output) = rig.run(&mut n, &[], Vec::new(), 1.0 / 60.0);
    output.textures_delta.clear();
    let drawn = output.shapes.iter().filter(|clipped| matches!(clipped.shape, egui::Shape::Text(_))).count();
    assert!(drawn > 0, "its title is still drawn on the next frame");
    for _ in 0..60 {
        rig.frame(&mut n, &[], Vec::new());
    }
    assert!(n.music_leaving.is_none());
    n.set_now_playing(Some(&song(true, 0.0)));
    n.set_now_playing(None);
    n.set_now_playing(Some(&song(true, 0.0)));
    assert!(n.music_leaving.is_none(), "a song that comes straight back is drawn once");
}

#[test]
fn pause_lets_bars_settle() {
    let mut n = island();
    n.set_now_playing(Some(&song(true, 10.0)));
    n.liquid.set([0.8; crate::media::BANDS]);
    settle(&mut n);
    n.set_now_playing(Some(&song(false, 10.5)));
    n.step(1.0 / 60.0);
    let level = n.liquid.level(0);
    assert!(level > 0.1 && level < 0.8, "{level}");
    settle(&mut n);
    assert!(n.liquid.level(0) < 0.01);
}

#[test]
fn docked_song_tucks_away() {
    let mut n = island();
    let t0 = Instant::now();
    n.set_now_playing(Some(&song(true, 10.0)));
    for step in 0..120u64 {
        n.tick(t0 + Duration::from_millis(step * 16), Some(AWAY));
        n.step(1.0 / 60.0);
    }
    assert!(n.hug.value > 0.99, "the song docks against the edge");
    let gone = n.music_until.expect("revealed") + Duration::from_millis(10);
    for step in 0..120u64 {
        n.tick(gone + Duration::from_millis(step * 16), Some(AWAY));
        n.step(1.0 / 60.0);
        assert!(n.silhouette().top < 1.0, "frame {step}: it never floats free on its way up");
    }
    assert!(n.is_collapsed());
}

fn cover_art(red: u8) -> std::sync::Arc<egui::ColorImage> {
    std::sync::Arc::new(egui::ColorImage::new([2, 2], vec![egui::Color32::from_rgb(red, 40, 200); 4]))
}

fn shows_cover(n: &Notch, art: &std::sync::Arc<egui::ColorImage>) -> bool {
    let shown = n.music.as_ref().and_then(|music| music.cover.shown());
    shown.is_some_and(|shown| std::sync::Arc::ptr_eq(&shown.image, art))
}

#[test]
fn skip_keeps_cover_until_art() {
    let mut n = island();
    let (first, second) = (cover_art(240), cover_art(20));
    n.set_now_playing(Some(&NowPlaying { artwork: Some(first.clone()), ..song(true, 10.0) }));
    let next = NowPlaying { title: "Next".into(), ..song(true, 0.0) };
    n.set_now_playing(Some(&next));
    assert!(shows_cover(&n, &first), "no grey placeholder while the next artwork is on its way");
    let wake = n.music_wakeup(Instant::now()).expect("a wakeup to give up waiting");
    assert!(wake > Instant::now() + Duration::from_millis(800));
    n.set_now_playing(Some(&NowPlaying { artwork: Some(second.clone()), ..next }));
    assert!(shows_cover(&n, &second));
    let music = n.music.as_ref().expect("playing");
    let now = Instant::now();
    assert!(music.cover.leaving(now).is_some_and(|(art, _)| art.is_some()), "the old cover slides out");
    assert!(music.tint.is_moving(now), "and the bars ease to the new colour");
    assert!(n.pulse_is_moving());
}

#[test]
fn skip_hands_words_over() {
    let mut n = island();
    n.set_now_playing(Some(&song(true, 10.0)));
    n.anticipate(crate::media::Control::Previous);
    n.set_now_playing(Some(&NowPlaying { title: "Before".into(), ..song(true, 0.0) }));
    let music = n.music.as_ref().expect("playing");
    let now = Instant::now();
    let (leaving, part) = music.words.leaving(now).expect("the old title is still drawn");
    assert_eq!(leaving.title, "Song");
    assert!(part.alpha > 0.9);
    assert!(music.words.arriving(now).shift < -0.9, "going back slides in from the left");
    assert_eq!(music.skip_direction, 1.0);
}

#[test]
fn no_artwork_settles_on_note() {
    let mut n = island();
    let t0 = Instant::now();
    n.set_now_playing(Some(&NowPlaying { artwork: Some(cover_art(240)), ..song(true, 10.0) }));
    n.set_now_playing(Some(&NowPlaying { title: "Untitled".into(), ..song(true, 0.0) }));
    n.music_tick(t0);
    assert!(n.music.as_ref().is_some_and(|music| music.cover.awaits()), "it waits a moment first");
    let due = n.music.as_ref().and_then(|music| music.cover.wakeup()).expect("waiting");
    assert!(n.music_tick(due));
    assert!(n.music.as_ref().is_some_and(|music| music.cover.shown().is_none() && !music.cover.awaits()));
}
