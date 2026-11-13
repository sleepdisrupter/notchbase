//! Mixes the audio sources into one track. Missing time becomes silence, the sum is clipped, and output only goes
//! as far as the slowest source, so audio and video stay aligned.

use std::collections::VecDeque;

use notchbase_wasapi::{Audio, CHANNELS, FRAME_BYTES, RATE};

use super::timeline::Samples;
use super::writer::Writer;

/// Audio is written in chunks of up to 4800 frames, 100 ms at 48 kHz.
const CHUNK: u64 = 4_800;

struct Lane {
    source: Option<Audio>,
    clock: Samples,
    pending: VecDeque<i16>,
}

impl Lane {
    fn collect(&mut self, elapsed: i64, note: &mut Option<String>) {
        if let Some(source) = &mut self.source {
            loop {
                match source.next_packet() {
                    Ok(Some(packet)) => {
                        let gap = self.clock.gap_before(packet.starts_at);
                        pad(&mut self.pending, &mut self.clock, gap);
                        let frames = (packet.pcm.len() / FRAME_BYTES) as u64;
                        let samples = packet.pcm.chunks_exact(2).take(frames as usize * CHANNELS);
                        self.pending.extend(samples.map(|pair| i16::from_le_bytes([pair[0], pair[1]])));
                        self.clock.advance(frames);
                    }
                    Ok(None) => break,
                    Err(err) => {
                        *note = Some(format!("Audio stopped partway: {err}"));
                        self.source = None;
                        break;
                    }
                }
            }
        }
        // Loopback sends nothing while the system is silent, so a lane that falls behind gets silence.
        let behind = self.clock.behind(elapsed);
        pad(&mut self.pending, &mut self.clock, behind);
    }

    fn close(&mut self, end: i64) {
        let missing = self.clock.short_of(end);
        pad(&mut self.pending, &mut self.clock, missing);
    }
}

pub(super) struct Mixer {
    lanes: Vec<Lane>,
    out: Samples,
    bytes: Vec<u8>,
}

impl Mixer {
    pub(super) fn new(sources: Vec<Audio>) -> Self {
        let lanes = sources
            .into_iter()
            .map(|source| Lane { source: Some(source), clock: Samples::new(RATE), pending: VecDeque::new() })
            .collect();
        Self { lanes, out: Samples::new(RATE), bytes: Vec::new() }
    }

    pub(super) fn start(&mut self, origin: i64) -> Result<(), String> {
        for source in self.lanes.iter_mut().filter_map(|lane| lane.source.as_mut()) {
            source.start(origin)?;
        }
        Ok(())
    }

    pub(super) fn pump(&mut self, writer: &Writer, elapsed: i64, note: &mut Option<String>) -> Result<(), String> {
        for lane in &mut self.lanes {
            lane.collect(elapsed, note);
        }
        self.emit(writer)
    }

    pub(super) fn finish(&mut self, writer: &Writer, end: i64) -> Result<(), String> {
        for lane in &mut self.lanes {
            lane.close(end);
        }
        self.emit(writer)
    }

    /// How far every lane has got, which is as far as the mix can go.
    fn ready(&self) -> u64 {
        self.lanes.iter().map(|lane| lane.clock.written()).min().unwrap_or_default()
    }

    fn emit(&mut self, writer: &Writer) -> Result<(), String> {
        let ready = self.ready();
        while self.out.written() < ready {
            let frames = (ready - self.out.written()).min(CHUNK);
            self.mix(frames as usize * CHANNELS);
            let time = self.out.time();
            let duration = self.out.time_of(self.out.written() + frames) - time;
            writer.audio(&self.bytes, time, duration)?;
            self.out.advance(frames);
        }
        Ok(())
    }

    fn mix(&mut self, samples: usize) {
        self.bytes.clear();
        for _ in 0..samples {
            let sum: i32 = self.lanes.iter_mut().map(|lane| i32::from(lane.pending.pop_front().unwrap_or(0))).sum();
            let clamped = sum.clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16;
            self.bytes.extend_from_slice(&clamped.to_le_bytes());
        }
    }
}

fn pad(pending: &mut VecDeque<i16>, clock: &mut Samples, frames: u64) {
    pending.extend(std::iter::repeat_n(0, frames as usize * CHANNELS));
    clock.advance(frames);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lane(samples: &[i16], frames: u64) -> Lane {
        let mut clock = Samples::new(RATE);
        clock.advance(frames);
        Lane { source: None, clock, pending: samples.iter().copied().collect() }
    }

    #[test]
    fn two_sources_are_summed_and_clipped() {
        let mut mixer = Mixer::new(Vec::new());
        mixer.lanes = vec![lane(&[1000, -1000, 30_000, -30_000], 2), lane(&[24, 24, 30_000, -30_000], 2)];
        mixer.mix(4);
        let mixed: Vec<i16> = mixer.bytes.chunks_exact(2).map(|pair| i16::from_le_bytes([pair[0], pair[1]])).collect();
        assert_eq!(mixed, [1024, -976, i16::MAX, i16::MIN]);
    }

    #[test]
    fn output_waits_for_the_slowest_source() {
        let mut mixer = Mixer::new(Vec::new());
        mixer.lanes = vec![lane(&[0; 20], 10), lane(&[0; 8], 4)];
        assert_eq!(mixer.ready(), 4);
        let mut note = None;
        for lane in &mut mixer.lanes {
            lane.collect(10_000_000, &mut note);
        }
        assert_eq!(mixer.ready(), 36_000);
        mixer.lanes.iter_mut().for_each(|lane| lane.close(20_000_000));
        assert_eq!(mixer.ready(), 96_000);
        assert!(mixer.lanes.iter().all(|lane| lane.pending.len() == 96_000 * CHANNELS));
        assert_eq!(note, None);
    }
}
