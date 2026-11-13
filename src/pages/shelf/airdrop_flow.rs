use std::path::Path;
use std::sync::Arc;

use egui::{Event, Id};

use super::ShelfPage;
use super::buttons::AIRDROP_ID;
use crate::bridge::Action;
use crate::pages::harness::{Harness, PAGE, press, release};
use crate::shelf::{ItemId, Shelf, airdrop};

const FILES: [&str; 3] = ["Cargo.toml", "Cargo.lock", "rustfmt.toml"];
const FOLDER: &str = "src";
const SETTLE_FRAMES: usize = 60;

fn shelf_of(names: &[&str]) -> Shelf {
    let mut shelf = Shelf::new(Arc::new(|| {}));
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    shelf.add(names.iter().map(|name| root.join(name)).collect());
    shelf
}

fn ids(shelf: &Shelf) -> Vec<ItemId> {
    shelf.items().iter().map(|item| item.id).collect()
}

fn airdrops(actions: &[Action]) -> Vec<&Action> {
    actions.iter().filter(|action| matches!(action, Action::AirDrop(_))).collect()
}

struct Run {
    harness: Harness,
    page: ShelfPage,
    shelf: Shelf,
}

impl Run {
    fn new(names: &[&str]) -> Self {
        Self { harness: Harness::new(), page: ShelfPage::default(), shelf: shelf_of(names) }
    }

    fn frame(&mut self, events: Vec<Event>) -> Vec<Action> {
        let Self { harness, page, shelf } = self;
        harness.frame(events, |ctx| page.ui(ctx, shelf, None))
    }

    fn settle(&mut self) -> Vec<Action> {
        (0..SETTLE_FRAMES).flat_map(|_| self.frame(Vec::new())).collect()
    }

    fn click(&mut self, id: Id) -> Vec<Action> {
        let at = self.harness.rect_of(id).center();
        let mut actions = self.frame(press(at));
        actions.extend(self.frame(release(at)));
        actions
    }

    /// Picks files once the Shelf has shown them, so the fresh drop's own selection doesn't replace the pick.
    fn select(&mut self, picked: Vec<ItemId>) {
        self.settle();
        self.page.selected = picked;
        self.settle();
    }
}

#[test]
fn airdrop_only_where_there_is_airdrop() {
    let mut run = Run::new(&FILES);
    run.select(ids(&run.shelf));
    assert_eq!(run.harness.drawn(Id::new(AIRDROP_ID)), airdrop::AVAILABLE);
    assert_eq!(airdrop::AVAILABLE, cfg!(target_os = "macos"));
}

#[test]
fn airdrop_sends_folders_too() {
    if !airdrop::AVAILABLE {
        return;
    }
    let mut run = Run::new(&[FILES[0], FOLDER]);
    run.select(ids(&run.shelf));
    let height = run.page.height(&run.shelf, PAGE.width());
    let actions = run.click(Id::new(AIRDROP_ID));
    assert_eq!(airdrops(&actions), [&Action::AirDrop(ids(&run.shelf))]);
    assert!(run.page.selected.is_empty());
    assert!(run.page.liftoff.is_animating(), "the tiles lift off");
    assert_eq!(run.page.height(&run.shelf, PAGE.width()), height);
    assert_eq!(run.shelf.items().len(), 2, "the files stay on the Shelf");
}

#[test]
fn airdrop_sends_the_selection() {
    if !airdrop::AVAILABLE {
        return;
    }
    let mut run = Run::new(&FILES);
    let picked = ids(&run.shelf)[1..].to_vec();
    run.select(picked.clone());
    let actions = run.click(Id::new(AIRDROP_ID));
    assert_eq!(airdrops(&actions), [&Action::AirDrop(picked)]);
}

#[test]
fn nothing_selected_airdrops_everything() {
    if !airdrop::AVAILABLE {
        return;
    }
    let mut run = Run::new(&FILES);
    run.select(Vec::new());
    assert!(run.harness.drawn(Id::new(AIRDROP_ID)), "like every other button, it works on all files");
    let actions = run.click(Id::new(AIRDROP_ID));
    assert_eq!(airdrops(&actions), [&Action::AirDrop(ids(&run.shelf))]);
}

#[test]
fn staged_preview_picks_files() {
    let mut run = Run::new(&[FILES[0], FOLDER, FILES[1], FILES[2]]);
    run.page.stage_airdrop();
    let actions = run.settle();
    assert_eq!(run.page.selected, ids(&run.shelf)[..3], "folders included");
    assert!(airdrops(&actions).is_empty(), "staging sends nothing");
    assert_eq!(run.harness.drawn(Id::new(AIRDROP_ID)), airdrop::AVAILABLE);
}
