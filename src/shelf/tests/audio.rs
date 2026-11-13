use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use hound::{SampleFormat, WavReader, WavSpec, WavWriter};

use super::fixtures::{TempDir, audio_source, run, single_output};
use crate::shelf::names::file_name;
use crate::shelf::ops::{self, Outputs};
use crate::shelf::{Clip, Kind, Op, Task, audio, video};

const RATE: u32 = 44_100;

fn tone(frames: usize, channels: usize, bits: u32) -> Vec<i32> {
    let peak = f64::from((1u32 << (bits - 2)) - 1);
    let mut seed = 0x2545_f491_u32;
    (0..frames * channels)
        .map(|i| {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let hiss = i32::from((seed >> 24) as u8) - 128;
            let t = (i / channels) as f64 / f64::from(RATE);
            (peak * (t * 440.0 * std::f64::consts::TAU).sin()) as i32 + hiss
        })
        .collect()
}

fn save_wav(path: PathBuf, channels: usize, bits: u32, samples: &[i32]) -> PathBuf {
    let spec = WavSpec {
        channels: channels as u16,
        sample_rate: RATE,
        bits_per_sample: bits as u16,
        sample_format: SampleFormat::Int,
    };
    let mut wav = WavWriter::create(&path, spec).unwrap();
    for &sample in samples {
        wav.write_sample(sample).unwrap();
    }
    wav.finalize().unwrap();
    path
}

fn save_aiff(path: PathBuf, channels: usize, samples: &[i32]) -> PathBuf {
    let frames = (samples.len() / channels) as u32;
    let data: Vec<u8> = samples.iter().flat_map(|&sample| (sample as i16).to_be_bytes()).collect();
    let mut comm = Vec::new();
    comm.extend_from_slice(&(channels as u16).to_be_bytes());
    comm.extend_from_slice(&frames.to_be_bytes());
    comm.extend_from_slice(&16u16.to_be_bytes());
    comm.extend_from_slice(&[0x40, 0x0E, 0xAC, 0x44, 0, 0, 0, 0, 0, 0]);
    let mut body = b"AIFF".to_vec();
    body.extend_from_slice(b"COMM");
    body.extend_from_slice(&(comm.len() as u32).to_be_bytes());
    body.extend_from_slice(&comm);
    body.extend_from_slice(b"SSND");
    body.extend_from_slice(&(data.len() as u32 + 8).to_be_bytes());
    body.extend_from_slice(&[0; 8]);
    body.extend_from_slice(&data);
    let mut file = b"FORM".to_vec();
    file.extend_from_slice(&(body.len() as u32).to_be_bytes());
    file.extend_from_slice(&body);
    fs::write(&path, file).unwrap();
    path
}

fn read_wav(path: &Path) -> (u16, u16, Vec<i32>) {
    let mut wav = WavReader::open(path).unwrap();
    let spec = wav.spec();
    (spec.channels, spec.bits_per_sample, wav.samples::<i32>().map(Result::unwrap).collect())
}

fn duration(path: &Path) -> f64 {
    audio::seconds(path).or_else(|| video::probe(path)?.seconds).unwrap()
}

fn names(dir: &TempDir) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(&dir.0).unwrap().map(|entry| file_name(&entry.unwrap().path())).collect();
    names.sort();
    names
}

#[test]
fn audio_files_are_their_own_kind() {
    for name in ["song.mp3", "memo.M4A", "take.aiff", "mix.flac", "loop.ogg", "raw.wav"] {
        assert_eq!(Kind::of_path(Path::new(name), false), Kind::Audio, "{name}");
    }
    assert_eq!(Kind::of_path(Path::new("clip.mp4"), false), Kind::Video);
}

#[test]
fn flac_keeps_every_sample() {
    for (channels, bits, frames) in [(2, 16, 57_331), (1, 24, 20_000), (1, 16, 1_000)] {
        let dir = TempDir::new();
        let samples = tone(frames, channels, bits);
        let wav = save_wav(dir.path("tone.wav"), channels, bits, &samples);
        let flac = single_output(&run(Op::Encode(Clip::Flac), &[audio_source(&wav)])).to_path_buf();
        assert_eq!(file_name(&flac), "tone.flac");
        assert!(fs::metadata(&flac).unwrap().len() < fs::metadata(&wav).unwrap().len());
        assert_eq!(audio::seconds(&flac), Some(frames as f64 / f64::from(RATE)));
        let back = single_output(&run(Op::Encode(Clip::Wav), &[audio_source(&flac)])).to_path_buf();
        assert_eq!(file_name(&back), "tone 2.wav");
        assert_eq!(read_wav(&back), (channels as u16, bits as u16, samples));
    }
}

#[test]
fn system_encodes_compressed_audio() {
    let dir = TempDir::new();
    let wav = save_wav(dir.path("voice.wav"), 2, 16, &tone(2 * RATE as usize, 2, 16));
    let system: Vec<Clip> =
        Clip::for_audio().into_iter().filter(|clip| matches!(clip, Clip::M4a | Clip::Mp3)).collect();
    assert_eq!(system.contains(&Clip::Mp3), cfg!(windows));
    for clip in system {
        let output = single_output(&run(Op::Encode(clip), &[audio_source(&wav)])).to_path_buf();
        assert_eq!(file_name(&output), format!("voice.{}", clip.extension()));
        let seconds = duration(&output);
        assert!((seconds - 2.0).abs() < 0.15, "{clip:?} lasts {seconds}");
        let decoded = single_output(&run(Op::Encode(Clip::Flac), &[audio_source(&output)])).to_path_buf();
        let seconds = duration(&decoded);
        assert!((seconds - 2.0).abs() < 0.15, "{clip:?} decodes to {seconds}");
    }
}

#[test]
fn unopenable_files_decode_first() {
    let dir = TempDir::new();
    let samples: Vec<i32> = tone(RATE as usize, 2, 16);
    let aiff = save_aiff(dir.path("take.aiff"), 2, &samples);
    let m4a = single_output(&run(Op::Encode(Clip::M4a), &[audio_source(&aiff)])).to_path_buf();
    assert!((duration(&m4a) - 1.0).abs() < 0.1);
    let wav = single_output(&run(Op::Encode(Clip::Wav), &[audio_source(&aiff)])).to_path_buf();
    assert_eq!(read_wav(&wav), (2, 16, samples));
    assert_eq!(names(&dir), ["take.aiff", "take.m4a", "take.wav"]);
}

#[test]
fn cancelling_leaves_nothing_behind() {
    let dir = TempDir::new();
    let wav = save_wav(dir.path("long.wav"), 2, 16, &tone(5 * RATE as usize, 2, 16));
    let cancel = AtomicBool::new(true);
    let mut report = |_: f32| {};
    let outcome = ops::run(
        Op::Encode(Clip::Flac),
        &[audio_source(&wav)],
        &Outputs::new(None),
        &mut Task::new(&mut report, &cancel),
    );
    assert_eq!((outcome.notice.ok, outcome.notice.text.as_str()), (false, "Cancelled"));
    assert_eq!(names(&dir), ["long.wav"]);
}

#[test]
fn unreadable_audio_fails_cleanly() {
    let dir = TempDir::new();
    let bogus = dir.path("broken.mp3");
    fs::write(&bogus, b"not really audio").unwrap();
    let outcome = run(Op::Encode(Clip::Wav), &[audio_source(&bogus)]);
    assert_eq!((outcome.notice.ok, outcome.notice.text.as_str()), (false, "Couldn't convert broken.mp3"));
    assert_eq!(names(&dir), ["broken.mp3"]);
}
