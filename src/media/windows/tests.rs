use std::time::{Duration, Instant};

use notchbase_spectrum::Analyzer;

use super::{Capture, Source};
use crate::media::artwork;

#[test]
#[ignore = "talks to the real media session and output device"]
fn session_and_output_can_be_read() {
    let mut source = Source::new();
    let track = source.poll();
    let art = track.as_ref().and_then(|_| source.artwork()).and_then(|bytes| artwork::decode(&bytes));
    let timed = track.as_ref().and_then(|t| t.timeline).is_some();
    eprintln!("session: {}, artwork: {}, timeline: {timed}", track.is_some(), art.is_some());

    let mut capture = Capture::open().expect("loopback opens");
    let mut analyzer = Analyzer::new(capture.rate());
    let (mut samples, mut frames) = (0, 0);
    let deadline = Instant::now() + Duration::from_millis(600);
    let mut mono = Vec::new();
    while Instant::now() < deadline {
        mono.clear();
        capture.read(&mut mono).expect("reads");
        samples += mono.len();
        frames += usize::from(analyzer.push(&mono).is_some());
        std::thread::sleep(Duration::from_millis(12));
    }
    eprintln!("samples: {samples}, spectra: {frames}");
    assert_eq!(capture.rate(), 48_000);
}
