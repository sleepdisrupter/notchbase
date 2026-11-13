//! Turns audio into six loudness bands for the notch's music dance.

mod fft;
mod spectrum;

pub use spectrum::{Analyzer, BANDS, Bands};
