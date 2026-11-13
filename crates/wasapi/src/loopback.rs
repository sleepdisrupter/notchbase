//! Mono loopback of the default output, used for the music spectrum.

use crate::audio::Audio;
use crate::pcm::{self, FRAME_BYTES, RATE};

pub struct Loopback {
    audio: Audio,
}

impl Loopback {
    pub fn open() -> Result<Self, String> {
        let mut audio = Audio::desktop()?;
        audio.start(0)?;
        Ok(Self { audio })
    }

    pub fn rate(&self) -> u32 {
        RATE
    }

    pub fn read(&mut self, mono: &mut Vec<f32>) -> Result<(), String> {
        while let Some(packet) = self.audio.next_packet()? {
            mono.extend(packet.pcm.chunks_exact(FRAME_BYTES).map(pcm::downmix));
        }
        Ok(())
    }
}
