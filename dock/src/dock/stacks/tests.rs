use std::time::{Duration, SystemTime};

use super::*;
use crate::config::{Config, StackView};

const DT: f32 = 1.0 / 120.0;

fn item(n: usize) -> Item {
    let at = SystemTime::UNIX_EPOCH + Duration::from_secs(1000 + n as u64);
    Item {
        path: PathBuf::from(format!("/Downloads/file {n}.zip")),
        name: format!("file {n}.zip"),
        added: at,
        modified: at,
        size: 1000 * n as u64,
        folder: false,
    }
}

fn listing(n: usize) -> Listing {
    Listing { items: (0..n).map(item).collect(), partial: 0 }
}

/// A dock with an app and the Downloads folder.
fn dock(view: StackView) -> (Dock, PathBuf) {
    let config = Config { tile_size: 48.0, large_size: 56.0, stack_view: view, ..Config::default() };
    let mut d = Dock::new(
        config,
        std::env::temp_dir().join("notchbase-dock-stack-tests"),
        vec2(1000.0, super::super::WINDOW_HEIGHT),
    );
    let folder = PathBuf::from("/Downloads");
    d.tiles.push(crate::tiles::Tile::new("app".into(), Kind::App, "/Applications/A.app".into(), "A".into(), false));
    d.tiles.push(crate::tiles::Tile::new("dl".into(), Kind::Folder, folder.clone(), "Downloads".into(), false));
    (d, folder)
}

fn fonts() -> egui::Context {
    let ctx = egui::Context::default();
    notchbase_paint::font::install(&ctx);
    ctx
}

fn frames(d: &mut Dock, ctx: &egui::Context, seconds: f32) {
    for _ in 0..(seconds / DT) as usize {
        d.step(DT);
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            let _ = d.compose(ui.painter());
        });
        output.textures_delta.clear();
    }
}

fn open(d: &mut Dock, ctx: &egui::Context) {
    frames(d, ctx, 0.1);
    let at = d.icon_rect(1).center();
    d.set_pointer(Some(at));
    d.press(false);
    d.release();
    frames(d, ctx, 0.8);
}

fn stack(d: &Dock) -> &StackPopup {
    match &d.popup.as_ref().expect("a stack is open").kind {
        PopupKind::Stack(s) => s,
        _ => panic!("not a stack"),
    }
}

#[test]
fn a_new_download_drops_onto_the_pile_and_swells_the_tile() {
    let (mut d, folder) = dock(StackView::Automatic);
    d.set_listing(folder.clone(), listing(2));
    assert!(d.stacks[&folder].arriving().is_none(), "the first reading is how the folder was, not an arrival");
    assert_eq!(d.tiles[1].gulp.velocity, 0.0);
    d.set_listing(folder.clone(), listing(3));
    let (path, v) = d.stacks[&folder].arriving().expect("the new file is landing");
    assert_eq!(path, Path::new("/Downloads/file 2.zip"), "the newest one");
    assert!(v < 0.05);
    assert!(d.tiles[1].gulp.velocity > 0.0, "the tile's liquid swells as it lands");
    let mut last = 0.0_f32;
    let mut peak = 0.0_f32;
    for _ in 0..240 {
        d.step(DT);
        let v = d.stacks[&folder].arrive.value;
        peak = peak.max(v);
        assert!(v >= last - 0.08 || v > 0.95, "it falls in one move, settling without bouncing back up high");
        last = v;
    }
    assert!(peak < 1.2, "a small bounce, not a wobble: {peak}");
    assert!(d.stacks[&folder].arriving().is_none(), "and it settles");
}

#[test]
fn the_wanted_pictures_are_the_top_of_the_pile_and_a_change_redraws_one() {
    let (mut d, folder) = dock(StackView::Automatic);
    d.set_listing(folder.clone(), listing(5));
    let wants = d.take_picture_wants();
    assert_eq!(wants.len(), PILE.len(), "only the pile, until the stack opens");
    assert_eq!(wants[0], PathBuf::from("/Downloads/file 4.zip"), "the newest first");
    assert!(d.take_picture_wants().is_empty(), "never asked twice");
    let mut changed = listing(5);
    changed.items[4].modified += Duration::from_secs(9);
    d.set_listing(folder, changed);
    assert_eq!(d.take_picture_wants(), [PathBuf::from("/Downloads/file 4.zip")]);
}

#[test]
fn a_few_fan_out_and_many_open_as_a_scrolling_grid() {
    let ctx = fonts();
    let (mut d, folder) = dock(StackView::Automatic);
    d.set_listing(folder.clone(), listing(4));
    open(&mut d, &ctx);
    assert!(stack(&d).layout == Layout::Fan);
    assert!(stack(&d).rows[..4].iter().all(|r| (r.value - 1.0).abs() < 0.02), "every row has risen");
    d.close_popup();
    frames(&mut d, &ctx, 0.6);
    assert!(d.popup.is_none(), "the fan folds back down and goes");

    d.set_listing(folder, listing(60));
    open(&mut d, &ctx);
    assert!(stack(&d).layout == Layout::Grid);
    let most = stack(&d).scroll_most;
    assert!(most > 0.0, "sixty files don't fit: it scrolls");
    d.scroll(-100_000.0, false);
    frames(&mut d, &ctx, 0.6);
    assert!((stack(&d).scroll.value - most).abs() < 0.5, "and stops at the end");
    d.scroll(100_000.0, false);
    frames(&mut d, &ctx, 0.6);
    assert!(stack(&d).scroll.value.abs() < 0.5, "and at the top");
}

#[test]
fn the_view_setting_decides_the_layout() {
    let ctx = fonts();
    for (view, count, expected) in
        [(StackView::Grid, 3, Layout::Grid), (StackView::Fan, 30, Layout::Fan), (StackView::Automatic, 0, Layout::Fan)]
    {
        let (mut d, folder) = dock(view);
        d.set_listing(folder, listing(count));
        open(&mut d, &ctx);
        assert!(stack(&d).layout == expected, "{view:?} with {count}");
    }
}

#[test]
fn right_click_offers_actions_and_trash_takes_it_away_at_once() {
    let ctx = fonts();
    let (mut d, folder) = dock(StackView::Grid);
    d.set_listing(folder.clone(), listing(3));
    open(&mut d, &ctx);
    let first = d.stack_cell(0).expect("drawn").center();
    d.set_pointer(Some(first));
    frames(&mut d, &ctx, 0.1);
    assert_eq!(d.popup.as_ref().unwrap().hover, Some(0));
    d.press(true);
    d.release();
    frames(&mut d, &ctx, 0.4);
    let card = stack(&d).menu.as_ref().expect("a card for that file").rect;
    assert_eq!(stack(&d).menu.as_ref().unwrap().path, PathBuf::from("/Downloads/file 2.zip"));
    let trash_row = pos2(card.center().x, card.bottom() - 6.0 - CARD_ROW * 0.5);
    d.set_pointer(Some(trash_row));
    frames(&mut d, &ctx, 0.05);
    d.press(false);
    d.release();
    assert_eq!(d.stacks[&folder].items.len(), 2, "gone from the stack straight away");
    assert!(d.popup.as_ref().is_none_or(|p| p.shut));
}

#[test]
fn dragging_an_item_out_starts_a_real_file_drag() {
    let ctx = fonts();
    let (mut d, folder) = dock(StackView::Fan);
    d.set_listing(folder, listing(2));
    open(&mut d, &ctx);
    let row = d.stack_cell(1).expect("drawn").center();
    d.set_pointer(Some(row));
    frames(&mut d, &ctx, 0.05);
    assert!(d.popup.as_ref().unwrap().hover.is_some_and(|h| h != FINDER));
    d.press(false);
    d.set_pointer(Some(row + vec2(-30.0, -20.0)));
    d.step(DT);
    assert!(matches!(d.effects.as_slice(), [Effect::DragFiles(p)] if p.len() == 1), "a drag of that file");
    assert!(d.popup.as_ref().is_some_and(|p| !p.shut), "the stack stays up while the file is over it");
    assert!(d.interactive(), "and the window keeps the mouse");
    // Out past the stack, it folds away; the drag ending lets the press go.
    d.set_pointer(Some(pos2(5.0, 5.0)));
    d.step(DT);
    assert!(d.popup.as_ref().is_none_or(|p| p.shut), "the stack makes way once the drag leaves it");
    d.file_drag_ended();
    assert!(d.press.is_none());
    d.effects.clear();
    d.step(DT);
    assert!(d.effects.is_empty(), "one drag, not another");
}

#[test]
fn downloads_in_progress_keep_the_tile_moving_and_an_empty_folder_is_just_the_folder() {
    let (mut d, folder) = dock(StackView::Automatic);
    d.set_listing(folder.clone(), Listing { items: Vec::new(), partial: 1 });
    assert!(d.stacks_animating(), "the progress bar glides");
    d.set_listing(folder.clone(), Listing::default());
    assert!(!d.stacks_animating());
    assert!(d.take_picture_wants().is_empty(), "nothing to picture: the folder's own icon shows");
}

fn at(minutes_ago: u64, name: &str) -> Item {
    let t = SystemTime::now() - Duration::from_secs(60 * minutes_ago);
    Item {
        path: PathBuf::from(format!("/Downloads/{name}")),
        name: name.into(),
        added: t,
        modified: t,
        size: 1,
        folder: false,
    }
}

#[test]
fn a_few_files_have_no_headings_and_many_spanning_days_do() {
    let midnight = local_midnight(SystemTime::now());
    let few: Vec<Item> = (0..8).map(|n| at(n * 60 * 24 * 3, &format!("f{n}"))).collect();
    let (_, lines, _) = model(&few, "", 4, StackSort::Added, midnight);
    assert!(lines.iter().all(|(_, l)| matches!(l, Line::Cells(_))), "eight files: just the files");

    let same_day: Vec<Item> = (0..30).map(|n| at(0, &format!("f{n}"))).collect();
    let (_, lines, _) = model(&same_day, "", 5, StackSort::Added, midnight);
    assert!(lines.iter().all(|(_, l)| matches!(l, Line::Cells(_))), "one day's worth needs no heading");

    let many: Vec<Item> = (0..30).map(|n| at(n as u64 * 60 * 48, &format!("f{n}"))).collect();
    let (shown, lines, _) = model(&many, "", 5, StackSort::Added, midnight);
    assert_eq!(shown.len(), 30);
    let headings: Vec<&str> = lines
        .iter()
        .filter_map(|(_, l)| match l {
            Line::Heading(h) => Some(*h),
            Line::Cells(_) => None,
        })
        .collect();
    assert_eq!(headings.first(), Some(&"Today"));
    assert!(headings.contains(&"Earlier"));
    let mut seen = headings.clone();
    seen.dedup();
    assert_eq!(seen, headings, "each heading once");
    let (_, lines, _) = model(&many, "", 5, StackSort::Name, midnight);
    assert!(lines.iter().all(|(_, l)| matches!(l, Line::Cells(_))), "sorted by name: no dates");
}

#[test]
fn the_filter_keeps_names_holding_every_word_and_drops_headings() {
    let midnight = local_midnight(SystemTime::now());
    let mut items: Vec<Item> = (0..40).map(|n| at(n as u64 * 60 * 24, &format!("file {n}.zip"))).collect();
    items.push(at(3, "Quarterly Report.pdf"));
    items.push(at(4, "report draft.txt"));
    let (shown, lines, _) = model(&items, "REPORT", 5, StackSort::Added, midnight);
    assert_eq!(shown, [40, 41]);
    assert!(lines.iter().all(|(_, l)| matches!(l, Line::Cells(_))));
    let (shown, _, _) = model(&items, "report draft", 5, StackSort::Added, midnight);
    assert_eq!(shown, [41]);
    let (shown, _, _) = model(&items, "nothing like it", 5, StackSort::Added, midnight);
    assert!(shown.is_empty());
}

#[test]
fn arrows_move_along_rows_and_keep_their_column_across_headings() {
    // Two items under one heading, then a row of five and a row of two.
    let lines = vec![
        (0.0, Line::Heading("Today")),
        (30.0, Line::Cells(0..2)),
        (122.0, Line::Heading("Earlier")),
        (152.0, Line::Cells(2..7)),
        (244.0, Line::Cells(7..9)),
    ];
    assert_eq!(grid_step(&lines, 0, &Key::Right), Some(1));
    assert_eq!(grid_step(&lines, 1, &Key::Right), Some(2), "on past the end of a row");
    assert_eq!(grid_step(&lines, 0, &Key::Left), Some(0));
    assert_eq!(grid_step(&lines, 1, &Key::Down), Some(3), "the same column below, across the heading");
    assert_eq!(grid_step(&lines, 6, &Key::Down), Some(8), "the last of a short row");
    assert_eq!(grid_step(&lines, 8, &Key::Down), Some(8), "stays at the bottom");
    assert_eq!(grid_step(&lines, 5, &Key::Up), Some(1), "column three above is past the row: its last");
    assert_eq!(grid_step(&lines, 8, &Key::Right), Some(8));
}

#[test]
fn the_search_shows_only_past_twenty_files_or_once_typing_starts() {
    let ctx = fonts();
    for (count, keys, typed, expected) in
        [(8, true, false, false), (30, true, false, true), (30, false, false, false), (8, true, true, true)]
    {
        let (mut d, folder) = dock(StackView::Grid);
        d.stack_keys = keys;
        d.set_listing(folder, listing(count));
        open(&mut d, &ctx);
        if typed {
            d.stack_key(Key::Text("f".into()));
            frames(&mut d, &ctx, 0.1);
        }
        assert_eq!(stack(&d).searching, expected, "{count} files, keys {keys}, typed {typed}");
    }
}

#[test]
fn typing_filters_the_grid_picks_the_first_and_esc_clears_then_closes() {
    let ctx = fonts();
    let (mut d, folder) = dock(StackView::Grid);
    d.stack_keys = true;
    d.set_listing(folder, listing(30));
    open(&mut d, &ctx);
    for c in ["1", "7"] {
        d.stack_key(Key::Text(c.into()));
    }
    frames(&mut d, &ctx, 0.1);
    assert_eq!(stack(&d).query, "17");
    assert_eq!(stack(&d).model.as_ref().unwrap().shown.len(), 1, "only file 17");
    assert_eq!(stack(&d).cursor, Some(PathBuf::from("/Downloads/file 17.zip")), "and it's picked");
    d.stack_key(Key::Back);
    frames(&mut d, &ctx, 0.1);
    assert_eq!(stack(&d).query, "1");
    assert!(stack(&d).model.as_ref().unwrap().shown.len() > 1);
    d.stack_key(Key::Esc);
    frames(&mut d, &ctx, 0.05);
    assert!(stack(&d).query.is_empty(), "Esc clears the filter first");
    assert!(d.popup.as_ref().is_some_and(|p| !p.shut));
    d.stack_key(Key::Esc);
    assert!(d.popup.as_ref().is_none_or(|p| p.shut), "then closes");
}

#[test]
fn arrows_space_and_cmd_delete_act_on_what_the_keys_picked() {
    let ctx = fonts();
    let (mut d, folder) = dock(StackView::Grid);
    d.set_listing(folder.clone(), listing(12));
    open(&mut d, &ctx);
    d.stack_key(Key::Right);
    assert_eq!(stack(&d).cursor, Some(PathBuf::from("/Downloads/file 11.zip")), "the first press picks the first");
    d.stack_key(Key::Right);
    d.stack_key(Key::Down);
    frames(&mut d, &ctx, 0.05);
    let cols = stack::grid_columns(12);
    let want = format!("/Downloads/file {}.zip", 11 - 1 - cols);
    assert_eq!(stack(&d).cursor, Some(PathBuf::from(&want)));
    d.stack_key(Key::Look);
    assert!(matches!(d.effects.as_slice(), [Effect::QuickLook(p)] if p == &[PathBuf::from(&want)]));
    d.effects.clear();
    d.stack_key(Key::Trash);
    assert_eq!(d.stacks[&folder].items.len(), 11);
    assert!(d.stacks[&folder].items.iter().all(|i| i.path != Path::new(&want)));
    d.stack_key(Key::All);
    assert_eq!(stack(&d).selected.len(), 11);
    d.stack_key(Key::Away);
    assert!(d.popup.as_ref().is_none_or(|p| p.shut), "another app's shortcut closes it");
}

#[test]
fn cmd_and_shift_clicks_pick_several_and_a_drag_takes_them_all() {
    let ctx = fonts();
    let (mut d, folder) = dock(StackView::Grid);
    d.set_listing(folder, listing(10));
    open(&mut d, &ctx);
    let click = |d: &mut Dock, n: usize, mods: crate::dock::Mods| {
        let c = d.stack_cell(n).expect("drawn").center();
        d.set_pointer(Some(c));
        frames(d, &ctx, 0.05);
        d.mods = mods;
        d.press(false);
        d.release();
        frames(d, &ctx, 0.05);
    };
    let cmd = crate::dock::Mods { command: true, shift: false };
    let shift = crate::dock::Mods { command: false, shift: true };
    click(&mut d, 0, cmd);
    click(&mut d, 2, cmd);
    assert_eq!(stack(&d).selected.len(), 2);
    assert!(d.popup.as_ref().is_some_and(|p| !p.shut), "picking opens nothing");
    click(&mut d, 2, cmd);
    assert_eq!(stack(&d).selected.len(), 1, "Cmd-click again lets it go");
    click(&mut d, 4, shift);
    assert_eq!(stack(&d).selected.len(), 3, "Shift picks the run from the last one clicked");

    d.mods = crate::dock::Mods::default();
    let c = d.stack_cell(3).unwrap().center();
    d.set_pointer(Some(c));
    frames(&mut d, &ctx, 0.05);
    d.press(false);
    d.set_pointer(Some(c + vec2(-30.0, -20.0)));
    d.step(DT);
    match d.effects.as_slice() {
        [Effect::DragFiles(paths)] => {
            assert_eq!(paths.len(), 3, "all of them");
            assert_eq!(paths[0], PathBuf::from("/Downloads/file 7.zip"), "in the order shown");
        }
        _ => panic!("no drag"),
    }
}

#[test]
fn past_the_most_pictures_the_ones_seen_longest_ago_go_but_never_the_pile() {
    let ctx = fonts();
    let (mut d, folder) = dock(StackView::Grid);
    d.set_listing(folder, listing(500));
    let pictures = |range: std::ops::Range<usize>| {
        range
            .map(|n| stack::Picture {
                path: PathBuf::from(format!("/Downloads/file {n}.zip")),
                image: egui::ColorImage::new([4, 4], vec![egui::Color32::WHITE; 16]),
                thumb: false,
            })
            .collect::<Vec<_>>()
    };
    d.give_pictures(pictures(0..500));
    d.upload_pictures(&ctx);
    assert!(d.files.ready.len() <= KEEP_PICTURES, "{}", d.files.ready.len());
    for n in 497..500 {
        assert!(d.files.ready.contains_key(Path::new(&format!("/Downloads/file {n}.zip"))), "the pile stays");
    }
    // One used just now outlives the rest.
    let fresh = PathBuf::from("/Downloads/file 450.zip");
    d.time += 10.0;
    d.give_pictures(pictures(450..451));
    d.upload_pictures(&ctx);
    assert!(d.picture(&fresh).is_some());
    d.time += 1.0;
    d.give_pictures(pictures(100..300));
    d.upload_pictures(&ctx);
    assert!(d.files.ready.contains_key(&fresh));
    assert!(d.files.ready.len() <= MOST_PICTURES);
}

#[test]
fn a_huge_grid_only_draws_what_is_on_screen() {
    let ctx = fonts();
    let (mut d, folder) = dock(StackView::Grid);
    d.set_listing(folder, listing(2000));
    open(&mut d, &ctx);
    let drawn = stack(&d).cells.len();
    assert!(drawn > 10 && drawn < 120, "{drawn} cells drawn of 2000");
    assert!(d.take_picture_wants().len() < 150, "and only those are pictured");
    d.scroll(-50_000.0, false);
    frames(&mut d, &ctx, 0.8);
    let last = stack(&d).cells.iter().map(|c| c.0).max().unwrap();
    assert!(last > 1900, "the end is reachable: {last}");
}

/// Timings for a big folder, by hand: `cargo test --release -p notchbase-dock -- --ignored --nocapture timings`.
#[test]
#[ignore]
fn timings() {
    let ctx = fonts();
    for count in [500usize, 5000] {
        let dir = std::env::temp_dir().join(format!("notchbase-dock-timings-{count}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for n in 0..count {
            std::fs::write(dir.join(format!("download {n}.zip")), b"x").unwrap();
        }
        let t = std::time::Instant::now();
        let listing = stack::list(&dir).unwrap();
        let listed = t.elapsed();
        let (mut d, folder) = dock(StackView::Grid);
        d.set_listing(folder, listing);
        frames(&mut d, &ctx, 0.1);
        let at = d.icon_rect(1).center();
        d.set_pointer(Some(at));
        d.press(false);
        d.release();
        let t = std::time::Instant::now();
        frames(&mut d, &ctx, DT);
        let first = t.elapsed();
        frames(&mut d, &ctx, 0.6);
        let mut worst = Duration::ZERO;
        let mut all = Duration::ZERO;
        let n = 600;
        for k in 0..n {
            d.scroll(if k % 200 < 100 { -40.0 } else { 40.0 }, true);
            let t = std::time::Instant::now();
            frames(&mut d, &ctx, DT);
            let e = t.elapsed();
            worst = worst.max(e);
            all += e;
        }
        println!(
            "{count} files: listing {listed:?}, first open frame {first:?}, scroll frames mean {:?} worst {worst:?}, cells drawn {}",
            all / n,
            stack(&d).cells.len()
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// Frame-by-frame close trace, by hand: `cargo test -p notchbase-dock -- --ignored --nocapture close_trace`.
#[test]
#[ignore]
fn close_trace() {
    let ctx = fonts();
    for (view, count) in [(StackView::Fan, 3), (StackView::Fan, 8), (StackView::Grid, 3), (StackView::Grid, 500)] {
        let (mut d, folder) = dock(view);
        d.set_listing(folder, listing(count));
        open(&mut d, &ctx);
        d.close_popup();
        println!("{view:?} {count}");
        for f in 0..60 {
            let t = std::time::Instant::now();
            frames(&mut d, &ctx, DT);
            let e = t.elapsed();
            let Some(p) = &d.popup else {
                println!("  gone at frame {f}");
                break;
            };
            let rows = match &p.kind {
                PopupKind::Stack(s) => s.rows.iter().map(|r| format!("{:.2}", r.value)).collect::<Vec<_>>().join(" "),
                _ => String::new(),
            };
            println!(
                "  {f:2} open {:.3} drawn {:.0}x{:.0} at {:.0},{:.0}  {e:?}  rows {rows}",
                p.open.value,
                p.drawn.width(),
                p.drawn.height(),
                p.drawn.center().x,
                p.drawn.center().y
            );
        }
    }
}

#[test]
fn closing_shrinks_steadily_into_the_tile_and_is_gone_with_nothing_left_hanging() {
    let ctx = fonts();
    for (view, count) in [(StackView::Fan, 3), (StackView::Fan, 8), (StackView::Grid, 3), (StackView::Grid, 500)] {
        let (mut d, folder) = dock(view);
        d.set_listing(folder, listing(count));
        open(&mut d, &ctx);
        d.close_popup();
        let mut last = f32::MAX;
        let mut first_rows = None;
        let mut frames_left = 0;
        for f in 0..60 {
            // A late frame now and then, as when an app is launching.
            let dt = if f == 3 { 0.05 } else { DT };
            d.step(dt);
            let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
                let _ = d.compose(ui.painter());
            });
            output.textures_delta.clear();
            let Some(p) = &d.popup else {
                frames_left = f;
                break;
            };
            let size = p.drawn.width() * p.drawn.height();
            if view == StackView::Grid {
                assert!(size <= last + 0.5, "{view:?} {count}: grew at frame {f}");
                last = size;
            }
            if f == 2 {
                first_rows = Some(stack(&d).rows[..count.min(8)].iter().map(|r| r.value).fold(1.0_f32, f32::min));
            }
            if f > 0 && p.open.value < 0.05 && view == StackView::Grid {
                assert!(size < 400.0, "{view:?} {count}: a {size} pt² panel still showing as it goes");
            }
        }
        assert!(frames_left > 0, "{view:?} {count}: never went");
        assert!((frames_left as f32) * DT < 0.26, "{view:?} {count}: took {frames_left} frames");
        if view == StackView::Fan {
            assert!(first_rows.unwrap() < 0.999, "{view:?} {count}: the files start folding at once");
        }
    }
}

#[test]
fn an_empty_folder_fans_out_one_quiet_row_under_open_in_finder() {
    let ctx = fonts();
    let (mut d, folder) = dock(StackView::Automatic);
    d.set_listing(folder, listing(0));
    open(&mut d, &ctx);
    assert!(stack(&d).layout == Layout::Fan);
    assert_eq!(stack(&d).fan_rows, 1);
    assert!(stack(&d).cells.is_empty(), "nothing in it to pick or drag");
    assert!(stack(&d).rows[0].value > 0.98 && stack(&d).finder.value > 0.98);
    d.close_popup();
    frames(&mut d, &ctx, 0.4);
    assert!(d.popup.is_none(), "and it folds away like any fan");
}

#[test]
fn a_dozen_files_fan_out_in_one_gently_curving_column() {
    let ctx = fonts();
    let (mut d, folder) = dock(StackView::Automatic);
    d.set_listing(folder, listing(12));
    open(&mut d, &ctx);
    assert!(stack(&d).layout == Layout::Fan);
    let cells: Vec<Rect> = (0..12).map(|n| d.stack_cell(n).expect("every file shows")).collect();
    for pair in cells.windows(2) {
        assert!(pair[1].center().y < pair[0].center().y, "newest at the bottom, rising");
    }
    let lean = cells[11].right() - cells[0].right();
    assert!(lean > 20.0 && lean < 90.0, "a gentle curve, not a slant: {lean}");
}
