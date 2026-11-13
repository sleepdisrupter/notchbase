//! Splits audio into six bands, each scaled against its own slowly falling peak.

use std::f32::consts::PI;

use crate::fft;

pub const BANDS: usize = 6;
pub type Bands = [f32; BANDS];

const SIZE: usize = 1024;
const HOP: usize = 512;
// Band edges in Hz, bass to treble.
const EDGES: [f32; BANDS + 1] = [30.0, 110.0, 280.0, 650.0, 1500.0, 3500.0, 10_000.0];
const SILENCE: f32 = 1e-4;
const RANGE_DB: f32 = 30.0;
const QUIETEST_PEAK_DB: f32 = -30.0;
const PEAK_FALL_DB_PER_SECOND: f32 = 6.0;
const PUNCH: f32 = 1.7;

pub struct Analyzer {
    rate: f32,
    samples: Vec<f32>,
    fresh: usize,
    window: Vec<f32>,
    frame: Vec<f32>,
    bins: Vec<f32>,
    peaks: Bands,
}

impl Analyzer {
    pub fn new(rate: u32) -> Self {
        // Hann window.
        let window = (0..SIZE).map(|i| 0.5 - 0.5 * (2.0 * PI * i as f32 / SIZE as f32).cos()).collect();
        Self {
            rate: rate as f32,
            samples: Vec::with_capacity(SIZE * 2),
            fresh: 0,
            window,
            frame: vec![0.0; SIZE],
            bins: Vec::with_capacity(SIZE / 2),
            peaks: [QUIETEST_PEAK_DB; BANDS],
        }
    }

    pub fn push(&mut self, mono: &[f32]) -> Option<Bands> {
        self.samples.extend_from_slice(mono);
        self.fresh += mono.len();
        if self.samples.len() > SIZE {
            self.samples.drain(..self.samples.len() - SIZE);
        }
        if self.samples.len() < SIZE || self.fresh < HOP {
            return None;
        }
        let elapsed = self.fresh as f32 / self.rate;
        self.fresh = 0;
        Some(self.analyze(elapsed))
    }

    fn analyze(&mut self, elapsed: f32) -> Bands {
        let rms = (self.samples.iter().map(|s| s * s).sum::<f32>() / SIZE as f32).sqrt();
        if rms < SILENCE {
            return [0.0; BANDS];
        }
        for ((out, sample), weight) in self.frame.iter_mut().zip(&self.samples).zip(&self.window) {
            *out = sample * weight;
        }
        fft::magnitudes(&self.frame, &mut self.bins);
        let edges = bin_edges(self.rate, self.bins.len());
        // A full-scale sine reads 0 dB once the Hann window has halved it.
        let full_scale = 20.0 * (SIZE as f32 / 4.0).log10();
        let fall = PEAK_FALL_DB_PER_SECOND * elapsed;
        std::array::from_fn(|band| {
            let power: f32 = self.bins[edges[band]..edges[band + 1]].iter().map(|m| m * m).sum();
            let db = 10.0 * power.max(1e-12).log10() - full_scale;
            // Measured against the band's own recent peak, so quiet songs still move.
            let peak = &mut self.peaks[band];
            *peak = db.max(*peak - fall).max(QUIETEST_PEAK_DB);
            ((db - (*peak - RANGE_DB)) / RANGE_DB).clamp(0.0, 1.0).powf(PUNCH)
        })
    }
}

fn bin_edges(rate: f32, bins: usize) -> [usize; BANDS + 1] {
    let hz_per_bin = rate / SIZE as f32;
    let mut edges = [0; BANDS + 1];
    for (i, hz) in EDGES.iter().enumerate() {
        let at = ((hz / hz_per_bin).round() as usize).max(1);
        edges[i] = if i == 0 { at } else { at.max(edges[i - 1] + 1) }.min(bins);
    }
    edges
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(hz: f32, amplitude: f32, n: usize) -> Vec<f32> {
        (0..n).map(|i| amplitude * (2.0 * PI * hz * i as f32 / 48_000.0).sin()).collect()
    }

    fn settle(analyzer: &mut Analyzer, samples: &[f32]) -> Bands {
        samples.chunks(480).filter_map(|chunk| analyzer.push(chunk)).last().expect("produced bands")
    }

    #[test]
    fn bands_cover_distinct_bins() {
        let edges = bin_edges(48_000.0, SIZE / 2);
        assert!(edges.windows(2).all(|pair| pair[0] < pair[1]), "{edges:?}");
        assert_eq!(edges[0], 1);
        let low_rate = bin_edges(8_000.0, SIZE / 2);
        assert!(low_rate.iter().all(|&edge| edge <= SIZE / 2));
    }

    #[test]
    fn bass_lights_low_bands() {
        let mut analyzer = Analyzer::new(48_000);
        let bands = settle(&mut analyzer, &tone(60.0, 0.5, 48_000));
        assert!(bands[0] > 0.8, "{bands:?}");
        assert!(bands[3..].iter().all(|&b| b < 0.2), "{bands:?}");
    }

    #[test]
    fn high_note_lights_high_band() {
        let mut analyzer = Analyzer::new(48_000);
        let bands = settle(&mut analyzer, &tone(5000.0, 0.5, 48_000));
        assert!(bands[5] > 0.8, "{bands:?}");
        assert!(bands[0] < 0.2, "{bands:?}");
    }

    #[test]
    fn silence_is_flat_every_hop() {
        let mut analyzer = Analyzer::new(48_000);
        assert_eq!(analyzer.push(&[0.0; 400]), None);
        assert_eq!(analyzer.push(&[0.0; 700]), Some([0.0; BANDS]));
        assert_eq!(analyzer.push(&[0.0; 100]), None);
    }

    #[test]
    fn quiet_music_moves_after_peak_falls() {
        let mut analyzer = Analyzer::new(48_000);
        settle(&mut analyzer, &tone(60.0, 0.9, 24_000));
        let quiet = settle(&mut analyzer, &tone(60.0, 0.05, 48_000 * 5));
        assert!(quiet[0] > 0.6, "{quiet:?}");
    }

    #[test]
    fn waits_for_a_full_window() {
        let mut analyzer = Analyzer::new(48_000);
        assert_eq!(analyzer.push(&[]), None);
        assert_eq!(analyzer.push(&tone(440.0, 0.5, SIZE - 1)), None);
        assert!(analyzer.push(&tone(440.0, 0.5, 1)).is_some(), "the first full window");
        assert_eq!(analyzer.push(&tone(440.0, 0.5, HOP - 1)), None, "a hop has not passed yet");
        assert!(analyzer.push(&tone(440.0, 0.5, 1)).is_some());
    }

    #[test]
    fn bands_stay_in_unit_range() {
        let mut analyzer = Analyzer::new(44_100);
        let noise: Vec<f32> = (0..44_100).map(|i| ((i * 7919 % 2003) as f32 / 1001.5 - 1.0) * 0.9).collect();
        for bands in noise.chunks(441).filter_map(|chunk| analyzer.push(chunk)) {
            assert!(bands.iter().all(|band| (0.0..=1.0).contains(band)), "{bands:?}");
        }
    }

    #[test]
    fn band_edges_fit_any_rate() {
        for rate in [8_000.0, 22_050.0, 44_100.0, 48_000.0, 96_000.0, 192_000.0] {
            let edges = bin_edges(rate, SIZE / 2);
            assert!(edges.windows(2).all(|pair| pair[0] <= pair[1]), "{rate}: {edges:?}");
            assert!(edges[0] >= 1 && edges[BANDS] <= SIZE / 2, "{rate}: {edges:?}");
        }
    }
}
