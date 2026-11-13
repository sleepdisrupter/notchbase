//! Reads a log backwards in 64 KiB chunks, at most 2 MiB, until it finds the newest line with rate limits.

use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::Path;
use std::time::SystemTime;

use super::snapshot::{self, Snapshot};

pub(super) const MAX_BYTES: u64 = 2 * 1024 * 1024;
const CHUNK: u64 = 64 * 1024;
const MAX_LINE: usize = 64 * 1024;
const MARKER: &[u8] = b"\"token_count\"";

pub(super) struct Tail {
    pub(super) found: Option<Snapshot>,
    pub(super) read: u64,
}

pub(super) fn last_snapshot(path: &Path, len: u64, written: SystemTime) -> io::Result<Tail> {
    let parse = |line: &[u8]| {
        // A plain byte search skips most lines without parsing any JSON.
        if !line.windows(MARKER.len()).any(|w| w == MARKER) {
            return None;
        }
        snapshot::parse(std::str::from_utf8(line).ok()?, written)
    };
    backwards(File::open(path)?, len, MAX_BYTES, parse)
}

fn backwards(
    mut file: impl Read + Seek,
    len: u64,
    cap: u64,
    parse: impl Fn(&[u8]) -> Option<Snapshot>,
) -> io::Result<Tail> {
    let floor = len.saturating_sub(cap);
    let mut lines = Lines::default();
    let mut buf = vec![0; CHUNK as usize];
    let mut end = len;
    while end > floor {
        let start = end.saturating_sub(CHUNK).max(floor);
        let chunk = &mut buf[..(end - start) as usize];
        file.seek(SeekFrom::Start(start))?;
        file.read_exact(chunk)?;
        if let Some(found) = lines.feed(chunk, start == 0, &parse) {
            return Ok(Tail { found: Some(found), read: len - start });
        }
        end = start;
    }
    Ok(Tail { found: None, read: len - floor })
}

/// Splits chunks read backwards into lines, newest first. A line longer than 64 KiB is skipped whole.
#[derive(Default)]
struct Lines {
    carry: Vec<u8>,
    overflowed: bool,
}

impl Lines {
    /// Feeds the chunk just before the last one. Its end completes the carried line, and its start is carried on.
    fn feed<T>(&mut self, chunk: &[u8], file_start: bool, mut parse: impl FnMut(&[u8]) -> Option<T>) -> Option<T> {
        match chunk.iter().position(|&b| b == b'\n') {
            Some(first) => {
                let mut complete = chunk[first + 1..].rsplit(|&b| b == b'\n');
                self.prepend(complete.next().unwrap_or_default());
                if let Some(found) = self.take().and_then(|line| parse(&line)) {
                    return Some(found);
                }
                if let Some(found) = complete.find_map(&mut parse) {
                    return Some(found);
                }
                self.prepend(&chunk[..first]);
            }
            None => self.prepend(chunk),
        }
        if file_start { self.take().and_then(|line| parse(&line)) } else { None }
    }

    fn prepend(&mut self, bytes: &[u8]) {
        if self.overflowed {
            return;
        }
        if self.carry.len() + bytes.len() > MAX_LINE {
            self.carry.clear();
            self.overflowed = true;
            return;
        }
        self.carry.splice(..0, bytes.iter().copied());
    }

    fn take(&mut self) -> Option<Vec<u8>> {
        let line = std::mem::take(&mut self.carry);
        (!std::mem::take(&mut self.overflowed)).then_some(line)
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;
    use crate::codex::fixture::{Scratch, T0, at, stamp, token_count};

    fn words(text: &str, chunk: usize, cap: u64) -> Vec<String> {
        let mut seen = Vec::new();
        let bytes = text.as_bytes();
        let floor = (bytes.len() as u64).saturating_sub(cap) as usize;
        let mut lines = Lines::default();
        let mut end = bytes.len();
        while end > floor {
            let start = end.saturating_sub(chunk).max(floor);
            let hit = lines.feed(&bytes[start..end], start == 0, |line| {
                seen.push(String::from_utf8(line.to_vec()).unwrap());
                None::<()>
            });
            assert!(hit.is_none());
            end = start;
        }
        seen
    }

    #[test]
    fn lines_come_newest_first_any_chunking() {
        let text = "alpha\nbravo\ncharlie\n\ndelta\necho";
        let expected = ["echo", "delta", "", "charlie", "bravo", "alpha"];
        for chunk in 1..=text.len() + 2 {
            assert_eq!(words(text, chunk, u64::MAX), expected, "chunk {chunk}");
        }
        assert_eq!(words("one\ntwo\n", 3, u64::MAX), ["", "two", "one"]);
        assert_eq!(words("", 4, u64::MAX), Vec::<String>::new());
    }

    #[test]
    fn cap_stops_before_older_lines() {
        assert_eq!(words("alpha\nbravo\ncharlie", 4, 9), ["charlie"]);
        assert_eq!(words("alpha\nbravo\ncharlie", 4, 13), ["charlie"], "a line cut by the cap is dropped");
        assert_eq!(words("alpha\nbravo\ncharlie", 4, 14), ["charlie", "bravo"]);
    }

    #[test]
    fn overlong_lines_are_skipped() {
        let long = "x".repeat(MAX_LINE + 1);
        let text = format!("first\n{long}\nlast");
        assert_eq!(words(&text, 1000, u64::MAX), ["last", "first"]);
        let fits = "y".repeat(MAX_LINE);
        assert_eq!(words(&format!("{fits}\nz"), 1000, u64::MAX), ["z".to_owned(), fits]);
    }

    fn line(percent: u32, secs: u64) -> String {
        let limits =
            format!(r#"{{"limit_id": "codex", "primary": {{"used_percent": {percent}, "window_minutes": 300}}}}"#);
        token_count(&stamp(secs), &limits)
    }

    fn found_in(text: &str, cap: u64) -> Option<(f64, SystemTime)> {
        let tail = backwards(Cursor::new(text.as_bytes()), text.len() as u64, cap, |line| {
            snapshot::parse(std::str::from_utf8(line).ok()?, at(1))
        })
        .unwrap();
        tail.found.map(|found| (found.primary.unwrap().percent, found.at))
    }

    #[test]
    fn newest_wins_past_noise_and_torn_line() {
        let noise = format!(r#"{{"type": "response_item", "payload": {{"text": "{}"}}}}"#, "n".repeat(90_000));
        let text = [line(10, T0), noise.clone(), line(20, T0 + 60), noise, "{\"type\": \"event_msg\", \"pay".into()]
            .join("\n");
        assert_eq!(found_in(&text, MAX_BYTES), Some((20.0, at(T0 + 60))));
        assert_eq!(found_in(&line(30, T0), MAX_BYTES), Some((30.0, at(T0))), "the first line counts");
        assert_eq!(found_in(&text, 1000), None, "nothing within the cap");
    }

    #[test]
    fn reads_real_files_from_the_end() {
        let dir = Scratch::new();
        let lines = [line(5, T0), "garbage".into(), line(6, T0 + 1), r#"{"type":"turn_context"}"#.into()];
        let path = dir.write("rollout-a.jsonl", &lines, at(T0 + 2));
        let len = std::fs::metadata(&path).unwrap().len();
        let tail = last_snapshot(&path, len, at(T0 + 2)).unwrap();
        assert_eq!(tail.found.map(|found| found.at), Some(at(T0 + 1)));
        assert!(tail.read <= len);
        let empty = dir.write("rollout-b.jsonl", &[], at(T0));
        let tail = last_snapshot(&empty, 0, at(T0)).unwrap();
        assert_eq!((tail.found, tail.read), (None, 0));
        assert!(last_snapshot(&dir.path().join("missing.jsonl"), 10, at(T0)).is_err());
    }
}
