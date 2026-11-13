//! A small window for trying file drags by hand. Press in it and drag to pull two temp files out,
//! and it prints every drag event it sees.

// The app's own drag code, so what works here works in notchbase.
#[allow(dead_code, unused_imports)]
#[path = "../../src/filedrag/mod.rs"]
mod filedrag;

mod app;
mod outcome;
mod sample_files;
#[cfg(windows)]
mod self_test;

use std::path::Path;

use winit::event_loop::EventLoop;

const USAGE: &str = "\
drag_out: press in the window and drag to pull two temp files out into Explorer / Finder / an editor.
Every file drag event the window receives is printed, and so is every change the drag sensor sees.
  --reveal      show the first temp file in Explorer / Finder and exit
  --self-test   (Windows) style the window like the notch, script a drag onto it and check the result";

const THUMBNAIL_PX: u32 = 64;

fn main() {
    println!("{USAGE}");
    let files = sample_files::create();
    files.iter().for_each(|file| describe_thumbnail(file));
    if has_flag("--reveal") {
        filedrag::reveal(&files[0]);
        return;
    }
    let event_loop = EventLoop::<app::Note>::with_user_event().build().expect("event loop");
    let mut app = app::App::new(event_loop.create_proxy(), files, has_flag("--self-test"));
    event_loop.run_app(&mut app).expect("event loop run");
}

fn describe_thumbnail(file: &Path) {
    match filedrag::thumbnail(file, THUMBNAIL_PX) {
        Some(image) => println!("thumbnail of {}: {}x{}", file.display(), image.size[0], image.size[1]),
        None => println!("no thumbnail for {}", file.display()),
    }
}

fn has_flag(flag: &str) -> bool {
    std::env::args().any(|arg| arg == flag)
}
