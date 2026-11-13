//! A plain radix-2 FFT that only returns the magnitudes we need.

use std::f32::consts::PI;

pub fn magnitudes(input: &[f32], out: &mut Vec<f32>) {
    let n = input.len();
    debug_assert!(n.is_power_of_two());
    let mut re = input.to_vec();
    let mut im = vec![0.0; n];
    // Bit-reversal reorder first, then the butterflies.
    let bits = n.trailing_zeros();
    for i in 0..n {
        let j = i.reverse_bits() >> (usize::BITS - bits);
        if j > i {
            re.swap(i, j);
        }
    }
    let mut len = 2;
    while len <= n {
        let step = -2.0 * PI / len as f32;
        for start in (0..n).step_by(len) {
            for k in 0..len / 2 {
                let (s, c) = (step * k as f32).sin_cos();
                let (a, b) = (start + k, start + k + len / 2);
                let (tr, ti) = (re[b] * c - im[b] * s, re[b] * s + im[b] * c);
                re[b] = re[a] - tr;
                im[b] = im[a] - ti;
                re[a] += tr;
                im[a] += ti;
            }
        }
        len *= 2;
    }
    out.clear();
    out.extend((0..n / 2).map(|k| re[k].hypot(im[k])));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pure_tone_lands_in_its_bin() {
        let n = 256;
        let tone: Vec<f32> = (0..n).map(|i| (2.0 * PI * 16.0 * i as f32 / n as f32).sin()).collect();
        let mut out = Vec::new();
        magnitudes(&tone, &mut out);
        assert_eq!(out.len(), n / 2);
        let loudest = out.iter().enumerate().max_by(|a, b| a.1.total_cmp(b.1)).map(|(k, _)| k);
        assert_eq!(loudest, Some(16));
        assert!((out[16] - n as f32 / 2.0).abs() < 0.5);
        assert!(out.iter().enumerate().filter(|&(k, _)| k != 16).all(|(_, &m)| m < 0.01));
    }

    #[test]
    fn silence_has_no_energy_in_any_bin() {
        let mut out = vec![1.0];
        magnitudes(&[0.0; 64], &mut out);
        assert_eq!(out, [0.0; 32]);
    }

    #[test]
    fn constant_signal_is_all_dc() {
        let mut out = Vec::new();
        magnitudes(&[0.25; 16], &mut out);
        assert!((out[0] - 4.0).abs() < 1e-5, "{out:?}");
        assert!(out[1..].iter().all(|m| *m < 1e-5), "{out:?}");
    }

    #[test]
    fn two_samples_make_one_bin() {
        let mut out = vec![3.0, 3.0];
        magnitudes(&[1.0, 1.0], &mut out);
        assert_eq!(out, [2.0]);
        magnitudes(&[1.0, -1.0], &mut out);
        assert_eq!(out, [0.0]);
    }
}
