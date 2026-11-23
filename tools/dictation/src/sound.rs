//! What the microphone heard: 16 kHz mono samples, how loud they are, where the speech is, and
//! where a long recording can be cut without splitting a word.

/// Whisper's own rate. Everything is recorded or converted to it.
pub const RATE: usize = 16_000;
/// 20 ms frames for loudness.
const FRAME: usize = RATE / 50;
/// A frame is speech when it's this many decibels over the room's noise.
const OVER_NOISE: f32 = 9.0;
/// Speech is never quieter than this, however quiet the room.
const SPEECH_FLOOR: f32 = -52.0;
/// Anything this loud is speech, even when nobody paused for the room to be heard.
const CLEARLY_SPEECH: f32 = -40.0;
/// Below this the microphone is off or muted.
const DEAD: f32 = -88.0;
/// At least this much speech, or it was a cough or a bump.
const SPEECH_MIN: usize = 8;
/// Kept before the first and after the last speech, so soft starts and endings survive.
const LEAD: usize = RATE / 4;
const TAIL: usize = RATE * 2 / 5;
/// Quiet recordings are lifted to this peak, by at most `GAIN_MAX`.
const PEAK: f32 = 0.7;
const GAIN_MAX: f32 = 8.0;
/// A pause this long is a place to cut.
const PAUSE: usize = 25;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Speech {
    /// Where speech starts and ends, padded, in samples.
    pub from: usize,
    pub to: usize,
    /// How many seconds of it were speech.
    pub voiced: f32,
}

/// The loudness of each 20 ms frame, in dBFS.
fn frames(samples: &[i16]) -> Vec<f32> {
    samples.chunks(FRAME).map(decibels).collect()
}

pub fn decibels(samples: &[i16]) -> f32 {
    if samples.is_empty() {
        return DEAD - 10.0;
    }
    let sum: f64 = samples.iter().map(|&s| f64::from(s) * f64::from(s)).sum();
    let rms = (sum / samples.len() as f64).sqrt() / 32768.0;
    (20.0 * rms.max(1e-6).log10()) as f32
}

/// The meter in the notch: 0 for silence, 1 for shouting.
pub fn level(samples: &[i16]) -> f32 {
    ((decibels(samples) + 58.0) / 46.0).clamp(0.0, 1.0)
}

fn threshold(loudness: &[f32]) -> f32 {
    let mut sorted: Vec<f32> = loudness.iter().copied().filter(|db| *db > DEAD).collect();
    if sorted.is_empty() {
        return f32::INFINITY;
    }
    sorted.sort_by(f32::total_cmp);
    let noise = sorted[sorted.len() / 10];
    (noise + OVER_NOISE).clamp(SPEECH_FLOOR, CLEARLY_SPEECH)
}

/// Where the speech is, or `None` when nothing was said.
pub fn speech(samples: &[i16]) -> Option<Speech> {
    let loudness = frames(samples);
    let threshold = threshold(&loudness);
    let voiced: Vec<usize> = (0..loudness.len()).filter(|&i| loudness[i] >= threshold).collect();
    if voiced.len() < SPEECH_MIN {
        return None;
    }
    let (first, last) = (voiced[0], voiced[voiced.len() - 1]);
    Some(Speech {
        from: (first * FRAME).saturating_sub(LEAD),
        to: ((last + 1) * FRAME + TAIL).min(samples.len()),
        voiced: (voiced.len() * FRAME) as f32 / RATE as f32,
    })
}

/// The speech, trimmed and evened out for Whisper.
pub fn prepared(samples: &[i16], speech: Speech) -> Vec<i16> {
    let part = &samples[speech.from..speech.to];
    let peak = part.iter().map(|s| s.unsigned_abs()).max().unwrap_or(0).max(1);
    let gain = (PEAK * 32767.0 / f32::from(peak)).clamp(1.0, GAIN_MAX);
    if gain <= 1.01 {
        return part.to_vec();
    }
    part.iter().map(|&s| (f32::from(s) * gain).round().clamp(-32768.0, 32767.0) as i16).collect()
}

/// Somewhere in the middle of the last long pause after `after`, to cut a long recording at,
/// so the first part can be written down while the rest is still being said.
pub fn pause_after(samples: &[i16], after: usize) -> Option<usize> {
    let start = after.div_ceil(FRAME);
    let loudness = frames(samples.get(start * FRAME..)?);
    let threshold = threshold(&frames(samples));
    let mut best = None;
    let mut run = 0;
    let mut heard = false;
    // Only a pause that speech came back after: one still going may be a breath mid-word.
    for (i, db) in loudness.iter().enumerate() {
        if *db < threshold {
            run += 1;
            continue;
        }
        if run >= PAUSE && heard {
            best = Some((start + i - run / 2) * FRAME);
        }
        run = 0;
        heard = true;
    }
    best
}

/// How many frames of loudness last `length`.
pub fn frames_in(length: std::time::Duration) -> usize {
    (length.as_millis() as usize * RATE / 1000).div_ceil(FRAME)
}

/// Where to cut once speech has been followed by a pause at least `pause` frames long that is
/// still going: the end of a phrase, so it can be typed while the next is said. The cut leaves a
/// little of the quiet with the phrase.
pub fn ended_pause(samples: &[i16], pause: usize) -> Option<usize> {
    let loudness = frames(samples);
    let threshold = threshold(&loudness);
    let quiet = loudness.iter().rev().take_while(|db| **db < threshold).count();
    let spoken = loudness.len() - quiet;
    let voiced = loudness[..spoken].iter().filter(|db| **db >= threshold).count();
    (quiet >= pause && voiced >= SPEECH_MIN).then(|| (spoken + quiet.min(TAIL / FRAME)) * FRAME)
}

/// Converts interleaved 16-bit audio at any rate to 16 kHz mono.
#[derive(Debug)]
#[cfg_attr(not(any(windows, target_os = "macos", test)), allow(dead_code))]
pub struct Converter {
    channels: usize,
    step: f64,
    at: f64,
    /// A one-pole filter, so what's above 8 kHz doesn't fold back down.
    smooth: f32,
    alpha: f32,
}

#[cfg_attr(not(any(windows, target_os = "macos", test)), allow(dead_code))]
impl Converter {
    pub fn new(rate: u32, channels: usize) -> Self {
        let step = f64::from(rate) / RATE as f64;
        let cutoff = 7_000.0 / rate as f32;
        let alpha = 1.0 - (-2.0 * std::f32::consts::PI * cutoff).exp();
        Self { channels: channels.max(1), step, at: 0.0, smooth: 0.0, alpha: alpha.clamp(0.05, 1.0) }
    }

    pub fn push(&mut self, interleaved: &[i16], out: &mut Vec<i16>) {
        for frame in interleaved.chunks_exact(self.channels) {
            let mono = frame.iter().map(|&s| f32::from(s)).sum::<f32>() / self.channels as f32;
            let previous = self.smooth;
            self.smooth += self.alpha * (mono - self.smooth);
            while self.at < 1.0 {
                let sample = previous + (self.smooth - previous) * self.at as f32;
                out.push(sample.round().clamp(-32768.0, 32767.0) as i16);
                self.at += self.step;
            }
            self.at -= 1.0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(seconds: f32, amplitude: f32) -> Vec<i16> {
        let count = (seconds * RATE as f32) as usize;
        (0..count).map(|i| ((i as f32 * 0.07).sin() * amplitude * 32767.0) as i16).collect()
    }

    fn hiss(seconds: f32) -> Vec<i16> {
        let count = (seconds * RATE as f32) as usize;
        (0..count).map(|i| ((i * 7919 % 61) as i16 - 30) * 3).collect()
    }

    #[test]
    fn a_finished_phrase_is_cut_after_its_pause() {
        let pause = frames_in(std::time::Duration::from_millis(600));
        let phrase = [hiss(0.3), tone(1.2, 0.3)].concat();
        assert_eq!(ended_pause(&[phrase.clone(), hiss(0.3)].concat(), pause), None, "still a short breath");
        let samples = [phrase.clone(), hiss(0.8)].concat();
        let at = ended_pause(&samples, pause).expect("the phrase is over");
        assert!(at > phrase.len() && at <= phrase.len() + TAIL + FRAME, "{at}");
        assert_eq!(ended_pause(&[hiss(0.3), tone(0.1, 0.3), hiss(1.0)].concat(), pause), None, "a bump");
        assert_eq!(ended_pause(&hiss(2.0), pause), None, "nothing said");
        assert_eq!(ended_pause(&tone(2.0, 0.3), pause), None, "still talking");
    }

    #[test]
    fn silence_is_nothing() {
        assert_eq!(speech(&vec![0; RATE * 3]), None);
        assert_eq!(speech(&hiss(3.0)), None);
        assert_eq!(speech(&[]), None);
    }

    #[test]
    fn speech_is_found_and_padded() {
        let samples = [hiss(1.0), tone(1.5, 0.3), hiss(2.0)].concat();
        let found = speech(&samples).expect("speech is heard");
        assert!(found.from <= RATE && found.from >= RATE - LEAD - FRAME, "{found:?}");
        assert!(found.to >= RATE * 5 / 2 && found.to <= RATE * 5 / 2 + TAIL + FRAME, "{found:?}");
        assert!((found.voiced - 1.5).abs() < 0.1, "{found:?}");
    }

    #[test]
    fn bump_is_not_speech() {
        let samples = [hiss(1.0), tone(0.08, 0.5), hiss(1.0)].concat();
        assert_eq!(speech(&samples), None);
    }

    #[test]
    fn quiet_speech_is_lifted() {
        let samples = [hiss(0.5), tone(1.0, 0.05)].concat();
        let found = speech(&samples).unwrap();
        let louder = prepared(&samples, found);
        let peak = louder.iter().map(|s| s.unsigned_abs()).max().unwrap();
        assert!(peak > 10_000 && peak < 24_000, "{peak}");
        let loud = tone(1.0, 0.9);
        assert_eq!(prepared(&loud, speech(&loud).unwrap()), loud[speech(&loud).unwrap().from..].to_vec());
    }

    #[test]
    fn cuts_land_in_pauses() {
        let samples = [tone(3.0, 0.3), hiss(1.0), tone(2.0, 0.3)].concat();
        let cut = pause_after(&samples, 0).expect("the pause is found");
        assert!(cut > RATE * 3 && cut < RATE * 4, "{cut}");
        assert_eq!(pause_after(&samples, RATE * 4), None, "no pause after the second part");
        let trailing = [tone(3.0, 0.3), hiss(0.8)].concat();
        assert_eq!(pause_after(&trailing, 0), None, "a pause still going may be a breath");
    }

    #[test]
    fn meter_spans_quiet_to_loud() {
        assert_eq!(level(&[0; 800]), 0.0);
        assert!(level(&tone(0.05, 0.9)) > 0.8);
        let middle = level(&tone(0.05, 0.03));
        assert!(middle > 0.2 && middle < 0.7, "{middle}");
    }

    #[test]
    fn converter_downmixes_and_resamples() {
        let mut converter = Converter::new(48_000, 2);
        let stereo: Vec<i16> = (0..48_000).flat_map(|i| [(i % 100) as i16 * 100, (i % 100) as i16 * 100]).collect();
        let mut out = Vec::new();
        converter.push(&stereo, &mut out);
        assert!((out.len() as i64 - RATE as i64).abs() <= 1, "{}", out.len());
        let mut same = Converter::new(16_000, 1);
        let mut kept = Vec::new();
        same.push(&[1000; 160], &mut kept);
        assert_eq!(kept.len(), 160);
        assert!(kept[150] > 900, "{}", kept[150]);
    }
}
