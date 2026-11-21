//! Shows line-up guides on every screen of this Mac for a few seconds, with a mark on each, for
//! seeing how they look without a second computer: `cargo run -p roam --example line_up_guides`.

#[cfg(target_os = "macos")]
fn main() {
    use objc2::MainThreadMarker;
    use objc2_app_kit::{NSApplication, NSScreen};
    use roam::align::{self, Band, Guide, Mark, Marked, Shown};
    use roam::layout::{Edge, Rect};

    let mtm = MainThreadMarker::new().expect("the main thread");
    let app = NSApplication::sharedApplication(mtm);
    let screens = NSScreen::screens(mtm);
    let first = screens.firstObject().map_or(0.0, |s| s.frame().size.height);
    let mut shown = Vec::new();
    let mut marks = Vec::new();
    for (index, screen) in screens.iter().enumerate() {
        let frame = screen.frame();
        // Top-down, from the top left of the first screen, as Roam has them.
        let rect =
            Rect::new(frame.origin.x, first - frame.origin.y - frame.size.height, frame.size.width, frame.size.height);
        let colour = index as u8;
        let guide = Guide {
            display: index as u8,
            colour,
            title: screen.localizedName().to_string(),
            detail: format!("Screen {} · {} × {}", index + 1, rect.w, rect.h),
            bands: vec![
                Band { edge: Edge::Left, from: 0.1, to: 0.8, colour: colour + 1, label: "Desk PC".into() },
                Band { edge: Edge::Top, from: 0.3, to: 1.0, colour: colour + 2, label: "Another screen".into() },
            ],
        };
        shown.push(Shown { rect, guide });
        let mark = Mark { display: index as u8, edge: Edge::Left, along: 0.4, arrives: true };
        marks.push(Marked { rect, mark });
    }
    align::show_here(shown);
    align::mark_here(marks);
    std::thread::spawn(|| {
        std::thread::sleep(std::time::Duration::from_secs(3));
        let _ = std::process::Command::new("screencapture").args(["-x", "/tmp/roam-line-up.png"]).status();
        std::thread::sleep(std::time::Duration::from_secs(5));
        std::process::exit(0);
    });
    app.run();
}

#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("This example shows the guides on a Mac.");
}
