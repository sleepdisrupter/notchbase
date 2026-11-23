//! Packs the speech as FLAC before it's sent: half the size of WAV, so it uploads in half the time.

use flacenc::bitsink::ByteSink;
use flacenc::component::{BitRepr, Stream};
use flacenc::config::Encoder;
use flacenc::error::Verify;
use flacenc::source::{Context, Fill, FrameBuf};

use crate::sound::RATE;

const BITS: usize = 16;

pub fn encode(samples: &[i16]) -> Result<Vec<u8>, String> {
    let failed = |what: &str| format!("couldn't pack the audio ({what})");
    let config = Encoder::default().into_verified().map_err(|_| failed("config"))?;
    let size = config.block_size;
    let mut stream = Stream::new(RATE, 1, BITS).map_err(|_| failed("stream"))?;
    let mut block = FrameBuf::with_size(1, size).map_err(|_| failed("block"))?;
    let mut context = Context::new(BITS, 1);
    let mut frames = ByteSink::new();
    let wide: Vec<i32> = samples.iter().map(|&s| i32::from(s)).collect();
    for chunk in wide.chunks(size) {
        (&mut block, &mut context).fill_interleaved(chunk).map_err(|_| failed("fill"))?;
        let number = context.current_frame_number().ok_or_else(|| failed("frame"))?;
        let frame = flacenc::encode_fixed_size_frame(&config, &block, number, stream.stream_info())
            .map_err(|_| failed("encode"))?;
        stream.stream_info_mut().update_frame_info(&frame);
        frame.write(&mut frames).map_err(|_| failed("write"))?;
    }
    let info = stream.stream_info_mut();
    info.set_block_sizes(size, size).map_err(|_| failed("sizes"))?;
    info.set_md5_digest(&context.md5_digest());
    info.set_total_samples(context.total_samples());
    let mut out = ByteSink::new();
    stream.write(&mut out).map_err(|_| failed("header"))?;
    let mut bytes = out.as_slice().to_vec();
    bytes.extend_from_slice(frames.as_slice());
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn speech_packs_smaller_than_wav() {
        let samples: Vec<i16> = (0..RATE * 2).map(|i| ((i as f32 * 0.05).sin() * 8000.0) as i16).collect();
        let packed = encode(&samples).unwrap();
        assert_eq!(&packed[..4], b"fLaC");
        assert!(packed.len() < samples.len() * 2 / 2, "{} bytes", packed.len());
    }

    #[test]
    fn odd_lengths_pack() {
        assert!(encode(&[1, 2, 3]).unwrap().starts_with(b"fLaC"));
        assert!(encode(&vec![0; 4097]).unwrap().starts_with(b"fLaC"));
    }
}
