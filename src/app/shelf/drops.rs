//! Files dragged onto or out of the notch: hovering, dropping and putting them on the Shelf.

use std::path::PathBuf;
use std::time::Instant;

use egui::Pos2;
use winit::event::{ElementState, WindowEvent};
use winit::window::WindowId;

use crate::app::App;
use crate::bridge::Page;
use crate::filedrag::{self, Arrival, DragSensor, Dropped, Sensed};
use crate::notch::Notch;
use crate::shelf::{ItemId, Notice};

const ALL_ALREADY_SHELVED: &str = "Already on the Shelf";
const SOME_ALREADY_SHELVED: &str = "Some were already on the Shelf";
const DRAG_OUT_TARGETS_OWN_WINDOW: bool = cfg!(windows);
const NO_DOWNLOADS: &str = "Couldn't find your Downloads folder";
const NOT_SAVED: &str = "Couldn't save what was dropped";

#[derive(Default)]
pub(in crate::app) struct Drops {
    sensor: DragSensor,
    own_press: bool,
    began_in_reach: Option<bool>,
    awaiting_release: bool,
    hovered_files: usize,
    dropped: Vec<PathBuf>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Hover {
    Show(usize),
    Hide,
    Keep,
}

impl Hover {
    fn apply(self, notch: &mut Notch) {
        match self {
            Hover::Show(files) => notch.set_drop_hover(Some(files)),
            Hover::Hide => notch.set_drop_hover(None),
            Hover::Keep => {}
        }
    }
}

struct Incoming {
    catching: bool,
    hover: Hover,
}

impl App {
    pub(in crate::app) fn track_drop(&mut self, id: WindowId, event: &WindowEvent) {
        let hover = self.drops.track(event);
        self.show_hover(id, hover);
    }

    fn show_hover(&mut self, id: WindowId, hover: Hover) {
        if takes_the_active_notch(hover, self.keys.is_some() || self.typing) {
            self.notches.focus(id);
        }
        if let Some(nw) = self.notches.window_mut(id) {
            hover.apply(&mut nw.notch);
        }
    }

    fn take_arrivals(&mut self) {
        for (id, arrival) in self.drop_inbox.take() {
            let hover = self.arrive(id, arrival);
            self.show_hover(id, hover);
        }
    }

    fn arrive(&mut self, id: WindowId, arrival: Arrival) -> Hover {
        match arrival {
            Arrival::Hover(_) | Arrival::Drop(_) if self.drops.dragging_out() => Hover::Keep,
            Arrival::Hover(count) => self.drops.hover(count),
            Arrival::Offer(link) => {
                self.video_offered(&link);
                Hover::Keep
            }
            Arrival::Leave => self.drops.hide(),
            Arrival::Drop(items) => {
                let items = self.take_videos(items);
                let unsaved = self.drops.receive(items);
                self.save_in_background(id, unsaved);
                self.drops.hide()
            }
            Arrival::Saved(saved) => {
                if let Some(problem) = saved.problems.first() {
                    self.notify(problem);
                }
                self.drops.dropped.extend(saved.paths);
                Hover::Keep
            }
        }
    }

    fn save_in_background(&mut self, id: WindowId, items: Vec<Dropped>) {
        if items.is_empty() {
            return;
        }
        let Some(folder) = self.drop_inbox.folder().map(PathBuf::from) else {
            self.notify(NO_DOWNLOADS);
            return;
        };
        let inbox = self.drop_inbox.clone();
        let saving = std::thread::Builder::new()
            .name("notchbase-drop".into())
            .spawn(move || inbox.post(id, Arrival::Saved(filedrag::save(items, &folder))));
        if saving.is_err() {
            self.notify(NOT_SAVED);
        }
    }

    pub(in crate::app) fn update_drags(&mut self, now: Instant, cursor: Option<Pos2>, capturing: bool) -> bool {
        let sensed = self.drops.sensor.poll();
        let drag_out_ended = sensed == Sensed::Idle && self.drops.button_up();
        if drag_out_ended {
            self.finish_drag_out();
        }
        self.take_arrivals();
        self.shelve_dropped(now);
        let (catching, within_reach) = self.catch_incoming(sensed, cursor, capturing);
        catching | self.offer_videos(sensed, within_reach, catching)
    }

    pub(in crate::app) fn drag_out(&mut self, ids: &[ItemId]) {
        let paths = self.shelf_paths(ids);
        self.drag_paths(&paths);
    }

    pub(in crate::app) fn drag_paths(&mut self, paths: &[PathBuf]) {
        if paths.is_empty() {
            return;
        }
        let Some(nw) = self.notches.active_mut() else { return };
        self.drops.awaiting_release = true;
        if DRAG_OUT_TARGETS_OWN_WINDOW {
            nw.suspend_hit_testing();
        }
        if let Err(err) = filedrag::start_drag(nw.window(), paths) {
            self.notify(&err);
        }
    }

    fn finish_drag_out(&mut self) {
        self.panel.shelf.prune_missing();
        if let Some(nw) = self.notches.active_mut() {
            nw.release_primary_button();
            nw.resume_hit_testing();
            nw.redraw();
        }
    }

    fn shelve_dropped(&mut self, now: Instant) {
        let dropped = self.drops.take_dropped();
        if dropped.is_empty() {
            return;
        }
        let count = dropped.len();
        let added = self.panel.shelf.add(dropped);
        if let Some(text) = already_shelved(added, count) {
            self.panel.show_notice(Notice { text: text.to_owned(), ok: true }, now);
        }
        if let Some(nw) = self.notches.active_mut() {
            Hover::Hide.apply(&mut nw.notch);
            nw.notch.open_page(Page::Shelf);
            nw.redraw();
        }
    }

    fn catch_incoming(&mut self, sensed: Sensed, cursor: Option<Pos2>, capturing: bool) -> (bool, bool) {
        let Some(nw) = self.notches.active_mut() else { return (false, false) };
        let within_reach = cursor.is_some_and(|c| nw.notch.drop_catch_zone().contains(c)) && !capturing;
        let incoming = self.drops.incoming(sensed, within_reach, nw.notch.is_drop_target());
        if !nw.notch.is_picking_layout() {
            incoming.hover.apply(&mut nw.notch);
        }
        (incoming.catching, within_reach)
    }
}

impl Drops {
    pub(in crate::app) fn dragging_out(&self) -> bool {
        self.awaiting_release || filedrag::dragging_out()
    }

    pub(in crate::app) fn busy(&self) -> bool {
        self.own_press || self.dragging_out()
    }

    pub(in crate::app) fn carrying(&self) -> bool {
        self.hovered_files > 0
    }

    fn track(&mut self, event: &WindowEvent) -> Hover {
        match event {
            WindowEvent::MouseInput { state: ElementState::Pressed, .. } => {
                self.own_press = true;
                Hover::Keep
            }
            WindowEvent::HoveredFile(_) if !self.dragging_out() => {
                self.hovered_files += 1;
                Hover::Show(self.hovered_files)
            }
            WindowEvent::HoveredFileCancelled => self.hide(),
            WindowEvent::DroppedFile(path) if !self.dragging_out() => {
                self.dropped.push(path.clone());
                Hover::Keep
            }
            _ => Hover::Keep,
        }
    }

    fn hover(&mut self, count: usize) -> Hover {
        self.hovered_files = count;
        Hover::Show(count)
    }

    fn receive(&mut self, items: Vec<Dropped>) -> Vec<Dropped> {
        let (ready, unsaved): (Vec<Dropped>, Vec<Dropped>) =
            items.into_iter().partition(|item| matches!(item, Dropped::File(path) if !filedrag::is_temporary(path)));
        self.dropped.extend(ready.into_iter().filter_map(|item| match item {
            Dropped::File(path) => Some(path),
            Dropped::Data { .. } | Dropped::Link(_) | Dropped::Page(_) => None,
        }));
        unsaved
    }

    fn button_up(&mut self) -> bool {
        self.own_press = false;
        std::mem::take(&mut self.awaiting_release)
    }

    fn take_dropped(&mut self) -> Vec<PathBuf> {
        if !self.dropped.is_empty() {
            self.hovered_files = 0;
        }
        std::mem::take(&mut self.dropped)
    }

    fn incoming(&mut self, sensed: Sensed, within_reach: bool, showing: bool) -> Incoming {
        let began_in_reach = match sensed {
            Sensed::Idle => self.began_in_reach.take().unwrap_or(false),
            _ => *self.began_in_reach.get_or_insert(within_reach),
        };
        let came_in = !began_in_reach || matches!(sensed, Sensed::Files(_));
        let catching = within_reach && sensed != Sensed::Idle && came_in && !self.own_press && !self.dragging_out();
        let hover = match sensed {
            Sensed::Files(count) if catching => Hover::Show(count.max(self.hovered_files)),
            _ if showing && (sensed == Sensed::Idle || (!catching && self.hovered_files == 0)) => self.hide(),
            _ => Hover::Keep,
        };
        Incoming { catching, hover }
    }

    fn hide(&mut self) -> Hover {
        self.hovered_files = 0;
        Hover::Hide
    }
}

fn takes_the_active_notch(hover: Hover, holding_keys: bool) -> bool {
    hover != Hover::Keep && !holding_keys
}

fn already_shelved(added: usize, dropped: usize) -> Option<&'static str> {
    (added < dropped).then_some(if added == 0 { ALL_ALREADY_SHELVED } else { SOME_ALREADY_SHELVED })
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    fn hovered(path: &str) -> WindowEvent {
        WindowEvent::HoveredFile(PathBuf::from(path))
    }

    fn dropped(path: &str) -> WindowEvent {
        WindowEvent::DroppedFile(PathBuf::from(path))
    }

    #[test]
    fn hovered_files_counted() {
        let mut drops = Drops::default();
        assert_eq!(drops.track(&hovered("a.png")), Hover::Show(1));
        assert_eq!(drops.track(&hovered("b.png")), Hover::Show(2));
        assert_eq!(drops.track(&WindowEvent::HoveredFileCancelled), Hover::Hide);
        assert_eq!(drops.track(&hovered("c.png")), Hover::Show(1));
    }

    #[test]
    fn drops_taken_once() {
        let mut drops = Drops::default();
        drops.track(&hovered("a.png"));
        drops.track(&dropped("a.png"));
        drops.track(&dropped("b.png"));
        assert_eq!(drops.take_dropped(), [Path::new("a.png"), Path::new("b.png")]);
        assert_eq!(drops.hovered_files, 0);
        assert!(drops.take_dropped().is_empty());
    }

    #[test]
    fn own_drag_out_ignored() {
        let mut drops = Drops { awaiting_release: true, ..Drops::default() };
        assert_eq!(drops.track(&hovered("a.png")), Hover::Keep);
        drops.track(&dropped("a.png"));
        assert!(drops.take_dropped().is_empty());
        assert!(!drops.incoming(Sensed::Files(1), true, false).catching);
    }

    #[test]
    fn button_up_ends_drag_out() {
        let mut drops = Drops { own_press: true, awaiting_release: true, ..Drops::default() };
        assert!(drops.button_up());
        assert!(!drops.own_press);
        assert!(!drops.button_up());
    }

    #[test]
    fn files_over_notch_caught() {
        let mut drops = Drops::default();
        let incoming = drops.incoming(Sensed::Files(3), true, false);
        assert!(incoming.catching);
        assert_eq!(incoming.hover, Hover::Show(3));
    }

    #[test]
    fn hover_count_floor() {
        let mut drops = Drops::default();
        drops.track(&hovered("a.png"));
        drops.track(&hovered("b.png"));
        assert_eq!(drops.incoming(Sensed::Files(1), true, true).hover, Hover::Show(2));
    }

    #[test]
    fn presses_in_notch_ignored() {
        let mut drops = Drops { own_press: true, ..Drops::default() };
        assert!(!drops.incoming(Sensed::Pressed, true, false).catching);
        drops.button_up();
        drops.incoming(Sensed::Idle, false, false);
        assert!(!drops.incoming(Sensed::Pressed, false, false).catching);
        assert!(drops.incoming(Sensed::Pressed, true, false).catching);
    }

    #[test]
    fn click_beside_goes_through() {
        let mut drops = Drops::default();
        assert!(!drops.incoming(Sensed::Pressed, true, false).catching);
        assert!(!drops.incoming(Sensed::Pressed, true, false).catching, "a maximize button gets its release");
        drops.incoming(Sensed::Idle, true, false);
        assert!(!drops.incoming(Sensed::Pressed, false, false).catching);
        assert!(drops.incoming(Sensed::Pressed, true, false).catching, "a drag from further away still lands");
    }

    #[test]
    fn pickup_beside_can_drop() {
        let mut drops = Drops::default();
        drops.incoming(Sensed::Pressed, true, false);
        assert!(drops.incoming(Sensed::Files(1), true, false).catching);
    }

    #[test]
    fn nothing_caught_out_of_reach() {
        let mut drops = Drops::default();
        let incoming = drops.incoming(Sensed::Files(2), false, false);
        assert!(!incoming.catching);
        assert_eq!(incoming.hover, Hover::Keep);
    }

    #[test]
    fn release_hides_drop_target() {
        let mut drops = Drops::default();
        drops.track(&hovered("a.png"));
        assert_eq!(drops.incoming(Sensed::Idle, true, true).hover, Hover::Hide);
        assert_eq!(drops.hovered_files, 0);
    }

    #[test]
    fn leaving_hides_sensed_hovers() {
        let mut drops = Drops::default();
        assert_eq!(drops.incoming(Sensed::Files(1), false, true).hover, Hover::Hide);
        drops.track(&hovered("a.png"));
        assert_eq!(drops.incoming(Sensed::Pressed, false, true).hover, Hover::Keep);
    }

    #[test]
    fn files_shelved_others_saved() {
        let mut drops = Drops::default();
        let file = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml");
        let unsaved = vec![
            Dropped::Data { name: Some("images.jpeg".into()), bytes: vec![1, 2, 3] },
            Dropped::Link("https://example.com/cat.png".into()),
        ];
        let mut items = vec![Dropped::File(file.clone())];
        items.extend(unsaved.clone());
        assert_eq!(drops.receive(items), unsaved);
        assert_eq!(drops.take_dropped(), [file]);
    }

    #[test]
    fn drop_target_sets_count() {
        let mut drops = Drops::default();
        assert_eq!(drops.hover(3), Hover::Show(3));
        assert_eq!(drops.incoming(Sensed::Files(1), true, true).hover, Hover::Show(3));
        assert_eq!(drops.hide(), Hover::Hide);
        assert_eq!(drops.hovered_files, 0);
    }

    #[test]
    fn drag_moves_active_notch() {
        assert!(takes_the_active_notch(Hover::Show(1), false));
        assert!(takes_the_active_notch(Hover::Hide, false));
        assert!(!takes_the_active_notch(Hover::Keep, false));
        assert!(!takes_the_active_notch(Hover::Show(1), true), "the typing notch keeps the keyboard and its caret");
        assert!(!takes_the_active_notch(Hover::Hide, true));
    }

    #[test]
    fn duplicates_reported() {
        assert_eq!(already_shelved(2, 2), None);
        assert_eq!(already_shelved(1, 2), Some(SOME_ALREADY_SHELVED));
        assert_eq!(already_shelved(0, 2), Some(ALL_ALREADY_SHELVED));
    }
}
