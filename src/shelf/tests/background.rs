use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

use super::fixtures::{PdfFixture, TempDir, half_transparent, save_image, wait_until};
use crate::poll::Wake;
use crate::shelf::{Format, Kind, Notice, Op, Shelf, THUMB_PX, store};

fn counting_wake() -> (Wake, Arc<AtomicUsize>) {
    let wakes = Arc::new(AtomicUsize::new(0));
    let counter = wakes.clone();
    let wake: Wake = Arc::new(move || {
        counter.fetch_add(1, Ordering::Relaxed);
    });
    (wake, wakes)
}

struct Gate {
    open: Mutex<bool>,
    opened: Condvar,
}

impl Gate {
    fn closed() -> Arc<Self> {
        Arc::new(Self { open: Mutex::new(false), opened: Condvar::new() })
    }

    fn wake_that_waits(self: &Arc<Self>) -> Wake {
        let gate = self.clone();
        Arc::new(move || {
            let open = gate.open.lock().unwrap();
            drop(gate.opened.wait_while(open, |open| !*open).unwrap());
        })
    }

    fn open(&self) {
        *self.open.lock().unwrap() = true;
        self.opened.notify_all();
    }
}

fn wait_for_notice(shelf: &mut Shelf) -> Notice {
    let mut notice = None;
    wait_until(shelf, |shelf| {
        notice = notice.take().or_else(|| shelf.take_notice());
        notice.is_some() && shelf.busy().is_none()
    });
    notice.unwrap()
}

#[test]
fn info_and_thumbs_arrive_by_sync() {
    let dir = TempDir::new();
    let photo = save_image(&half_transparent(), dir.path("photo.png"));
    let pdf = PdfFixture::pages(4).save(dir.path("doc.pdf"));
    let (wake, wakes) = counting_wake();
    let mut shelf = Shelf::with_locations(wake, None, None);
    shelf.add(vec![photo, pdf]);
    wait_until(&mut shelf, |shelf| {
        let items = shelf.items();
        items[0].dims.is_some() && items[0].thumb.is_some() && items[1].pages.is_some()
    });
    let items = shelf.items();
    assert_eq!(items[0].dims, Some([64, 48]));
    assert_eq!(items[1].pages, Some(4));
    assert_eq!(items[0].thumb_rev, 1);
    let thumb = items[0].thumb.as_ref().unwrap();
    assert!(thumb.size[0] <= THUMB_PX as usize && thumb.size[1] <= THUMB_PX as usize);
    assert!(wakes.load(Ordering::Relaxed) > 0);
}

#[test]
fn ops_run_one_at_a_time() {
    let dir = TempDir::new();
    let store_file = dir.path("data").join("shelf.json");
    let photo = save_image(&half_transparent(), dir.path("photo.png"));
    let pdf = PdfFixture::pages(4).save(dir.path("doc.pdf"));
    let gate = Gate::closed();
    let mut shelf = Shelf::with_locations(gate.wake_that_waits(), Some(store_file.clone()), None);
    shelf.add(vec![photo, pdf]);
    let (photo_id, pdf_id) = (shelf.items()[0].id, shelf.items()[1].id);

    shelf.run(Op::Convert(Format::Jpeg), &[photo_id]);
    shelf.run(Op::Split, &[pdf_id]);
    assert_eq!(shelf.busy().map(|busy| busy.op), Some(Op::Convert(Format::Jpeg)));
    gate.open();
    assert_eq!(wait_for_notice(&mut shelf), Notice::ok("Saved photo.jpg"));
    assert!(!dir.path("doc pages").exists());

    let items = shelf.items();
    assert_eq!(items.len(), 3);
    assert_eq!(items[2].path, dir.path("photo.jpg"));
    assert!(items[2].added.elapsed() < Duration::from_secs(60));
    assert_eq!(store::load(&store_file).len(), 3);

    shelf.run(Op::Split, &[pdf_id]);
    assert_eq!(wait_for_notice(&mut shelf), Notice::ok("Split into 4 pages"));
    let folder = shelf.items().last().unwrap();
    assert_eq!(folder.kind, Kind::Folder);
    assert_eq!(folder.path, dir.path("doc pages"));
}
