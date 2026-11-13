//! Copies a download to disk while hashing it, with an optional size cap and a stop flag.

use std::io::{self, Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};

use sha2::{Digest, Sha256};

use crate::hash::{CHUNK, hex};

#[derive(Clone, Copy, Debug, Default)]
pub struct Limits<'a> {
    pub largest: Option<u64>,
    pub stop: Option<&'a AtomicBool>,
}

impl Limits<'_> {
    fn stopped(&self) -> bool {
        self.stop.is_some_and(|stop| stop.load(Ordering::Relaxed))
    }

    fn exceeded(&self, done: u64) -> bool {
        self.largest.is_some_and(|largest| done > largest)
    }
}

#[derive(Debug)]
pub enum CopyError {
    Read(io::Error),
    Write(io::Error),
    TooLarge,
    Stopped,
}

pub fn copy_hashed(
    mut from: impl Read,
    mut to: impl Write,
    limits: Limits,
    mut progress: impl FnMut(u64),
) -> Result<String, CopyError> {
    let mut hasher = Sha256::new();
    let mut buffer = vec![0; CHUNK];
    let mut done = 0u64;
    loop {
        if limits.stopped() {
            return Err(CopyError::Stopped);
        }
        let read = from.read(&mut buffer).map_err(CopyError::Read)?;
        if read == 0 {
            return Ok(hex(&hasher.finalize()));
        }
        done += read as u64;
        // Checked before the write, so nothing past the cap reaches the file.
        if limits.exceeded(done) {
            return Err(CopyError::TooLarge);
        }
        hasher.update(&buffer[..read]);
        to.write_all(&buffer[..read]).map_err(CopyError::Write)?;
        progress(done);
    }
}

// A zero total counts as one byte so this never divides by zero.
pub fn fraction(done: u64, total: u64) -> f32 {
    (done as f64 / total.max(1) as f64).min(1.0) as f32
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sha256_hex;

    const ABC: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";

    struct Trickle<'a> {
        bytes: &'a [u8],
        step: usize,
        reads: usize,
    }

    impl<'a> Trickle<'a> {
        fn new(bytes: &'a [u8], step: usize) -> Self {
            Self { bytes, step, reads: 0 }
        }
    }

    impl Read for Trickle<'_> {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            self.reads += 1;
            let len = self.step.min(self.bytes.len()).min(buf.len());
            buf[..len].copy_from_slice(&self.bytes[..len]);
            self.bytes = &self.bytes[len..];
            Ok(len)
        }
    }

    struct Broken(io::ErrorKind);

    impl Read for Broken {
        fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
            Err(self.0.into())
        }
    }

    impl Write for Broken {
        fn write(&mut self, _: &[u8]) -> io::Result<usize> {
            Err(self.0.into())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    fn copy(from: impl Read, limits: Limits) -> (Result<String, CopyError>, Vec<u8>, Vec<u64>) {
        let mut written = Vec::new();
        let mut seen = Vec::new();
        let result = copy_hashed(from, &mut written, limits, |done| seen.push(done));
        (result, written, seen)
    }

    #[test]
    fn copies_and_hashes() {
        let (result, written, seen) = copy(&b"abc"[..], Limits::default());
        assert_eq!(result.unwrap(), ABC);
        assert_eq!(written, b"abc");
        assert_eq!(seen, [3]);
    }

    #[test]
    fn progress_after_each_read() {
        let (result, written, seen) = copy(Trickle::new(b"hello", 2), Limits::default());
        assert_eq!(result.unwrap(), sha256_hex(b"hello"));
        assert_eq!(written, b"hello");
        assert_eq!(seen, [2, 4, 5]);
    }

    #[test]
    fn empty_source_copies_nothing() {
        let (result, written, seen) = copy(&b""[..], Limits::default());
        assert_eq!(result.unwrap(), sha256_hex(b""));
        assert!(written.is_empty() && seen.is_empty());
    }

    #[test]
    fn hashes_across_chunks() {
        let bytes: Vec<u8> = (0..CHUNK * 3 + 5).map(|index| (index % 253) as u8).collect();
        let (result, written, seen) = copy(&bytes[..], Limits::default());
        assert_eq!(result.unwrap(), sha256_hex(&bytes));
        assert_eq!(written, bytes);
        assert_eq!(seen.last().copied(), Some(bytes.len() as u64));
        assert!(seen.windows(2).all(|pair| pair[0] < pair[1]));
    }

    #[test]
    fn exact_cap_is_allowed() {
        let limits = Limits { largest: Some(3), ..Limits::default() };
        assert_eq!(copy(&b"abc"[..], limits).0.unwrap(), ABC);
    }

    #[test]
    fn stops_before_writing_past_cap() {
        let limits = Limits { largest: Some(4), ..Limits::default() };
        let (result, written, seen) = copy(Trickle::new(b"abcde", 3), limits);
        assert!(matches!(result, Err(CopyError::TooLarge)));
        assert_eq!(written, b"abc");
        assert_eq!(seen, [3]);
    }

    #[test]
    fn zero_cap_allows_only_empty() {
        let limits = Limits { largest: Some(0), ..Limits::default() };
        assert!(matches!(copy(&b"a"[..], limits).0, Err(CopyError::TooLarge)));
        assert!(copy(&b""[..], limits).0.is_ok());
    }

    #[test]
    fn early_stop_reads_nothing() {
        let stop = AtomicBool::new(true);
        let mut source = Trickle::new(b"abc", 1);
        let mut written = Vec::new();
        let result = copy_hashed(&mut source, &mut written, Limits { stop: Some(&stop), ..Limits::default() }, |_| {});
        assert!(matches!(result, Err(CopyError::Stopped)));
        assert_eq!(source.reads, 0);
        assert!(written.is_empty());
    }

    #[test]
    fn stop_midway_ends_copy() {
        let stop = AtomicBool::new(false);
        let mut written = Vec::new();
        let limits = Limits { stop: Some(&stop), ..Limits::default() };
        let result = copy_hashed(Trickle::new(b"abcdef", 2), &mut written, limits, |done| {
            if done >= 4 {
                stop.store(true, Ordering::Relaxed);
            }
        });
        assert!(matches!(result, Err(CopyError::Stopped)));
        assert_eq!(written, b"abcd");
    }

    #[test]
    fn unset_stop_lets_copy_finish() {
        let stop = AtomicBool::new(false);
        let limits = Limits { stop: Some(&stop), ..Limits::default() };
        assert_eq!(copy(&b"abc"[..], limits).0.unwrap(), ABC);
    }

    #[test]
    fn read_errors_are_not_retried() {
        for kind in [io::ErrorKind::ConnectionReset, io::ErrorKind::Interrupted] {
            let (result, written, _) = copy(Broken(kind), Limits::default());
            assert!(matches!(result, Err(CopyError::Read(err)) if err.kind() == kind));
            assert!(written.is_empty());
        }
    }

    #[test]
    fn write_error_reports_no_progress() {
        let mut seen = Vec::new();
        let result =
            copy_hashed(&b"abc"[..], Broken(io::ErrorKind::StorageFull), Limits::default(), |done| seen.push(done));
        assert!(matches!(result, Err(CopyError::Write(err)) if err.kind() == io::ErrorKind::StorageFull));
        assert!(seen.is_empty());
    }

    #[test]
    fn fraction_is_done_over_total() {
        assert_eq!(fraction(0, 10), 0.0);
        assert_eq!(fraction(5, 10), 0.5);
        assert_eq!(fraction(10, 10), 1.0);
    }

    #[test]
    fn fraction_caps_at_one() {
        assert_eq!(fraction(15, 10), 1.0);
        assert_eq!(fraction(u64::MAX, 1), 1.0);
    }

    #[test]
    fn zero_total_counts_as_one() {
        assert_eq!(fraction(0, 0), 0.0);
        assert_eq!(fraction(1, 0), 1.0);
    }

    #[test]
    fn huge_totals_keep_proportion() {
        assert_eq!(fraction(u64::MAX / 2, u64::MAX), 0.5);
        assert_eq!(fraction(u64::MAX, u64::MAX), 1.0);
    }
}
