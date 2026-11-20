//! From the duplicated display to H.264, by the quickest way the PC has: on the graphics card
//! from end to end when its video processor and encoder take textures, else through the
//! processor (the picture read off the card, converted to NV12 and handed to whichever encoder
//! there is). A card route that fails, or gives nothing back, gives way to the processor route
//! for good.

use std::time::{Duration, Instant};

use super::duplicate::Duplicator;
use super::{encoder::Encoder, gpu::Gpu};
use crate::display::nv12;

/// How long the card route may take to give back its first picture before it gives way.
const FIRST_PICTURE: Duration = Duration::from_secs(2);
/// Set (to anything) to keep pictures off the graphics card, to compare the two routes.
const OFF_CARD: &str = "NOTCHBASE_ROAM_OFF_CARD";

enum Route {
    Card(Gpu),
    Processor(nv12::Converter),
}

pub struct Pictures {
    route: Route,
    encoder: Encoder,
    size: [u32; 2],
    fps: u32,
    /// On the card route: when the first picture went in, and whether one came out.
    first_in: Option<Instant>,
    came_out: bool,
    /// Lines for the log, about the route taken.
    notes: Vec<String>,
}

/// One encoded step: the pictures that came out, and how long getting the picture ready and
/// encoding it took.
pub struct Step {
    pub pictures: Vec<Vec<u8>>,
    pub ready: Duration,
    pub encode: Duration,
}

impl Pictures {
    pub fn new(duplicator: &mut Duplicator, size: [u32; 2], fps: u32) -> Result<Self, String> {
        let mut notes = Vec::new();
        let on_card = if std::env::var_os(OFF_CARD).is_some() {
            Err(format!("{OFF_CARD} is set"))
        } else {
            let (device, context) = duplicator.device();
            Gpu::new(device, context, size, fps)
                .and_then(|gpu| Ok((gpu, Encoder::on_card(size, fps, device, duplicator.luid())?)))
        };
        let (route, encoder) = match on_card {
            Ok((gpu, encoder)) => {
                duplicator.keep_on_card(true);
                (Route::Card(gpu), encoder)
            }
            Err(why) => {
                notes.push(format!("extended display pictures come off the graphics card: {why}"));
                (Route::Processor(nv12::Converter::new(size, size)), Encoder::new(size, fps)?)
            }
        };
        notes.push(format!("extended display encoder: {}", encoder.about));
        Ok(Self { route, encoder, size, fps, first_in: None, came_out: false, notes })
    }

    pub fn take_notes(&mut self) -> Vec<String> {
        std::mem::take(&mut self.notes)
    }

    /// How long the encoder took over each picture that came out since the last call.
    pub fn take_delays(&mut self) -> Vec<Duration> {
        self.encoder.take_delays()
    }

    pub fn held_since(&self) -> Option<Instant> {
        self.encoder.held_since()
    }

    /// What the encoder has finished, without waiting.
    pub fn drain(&mut self, duplicator: &mut Duplicator) -> Result<Vec<Vec<u8>>, String> {
        match self.encoder.drain() {
            Ok(pictures) => Ok(self.out(pictures)),
            Err(why) => self.give_way(duplicator, why).map(|()| Vec::new()),
        }
    }

    /// Encodes the display as it is now, a key picture when `key`.
    pub fn encode(&mut self, duplicator: &mut Duplicator, key: bool) -> Result<Step, String> {
        let start = Instant::now();
        if let Route::Card(gpu) = &mut self.route {
            if self.first_in.is_some_and(|at| !self.came_out && at.elapsed() >= FIRST_PICTURE) {
                let why = "the encoder gave nothing back".to_owned();
                self.give_way(duplicator, why)?;
                return self.encode(duplicator, true);
            }
            let Some((desk, pointer, origin)) = duplicator.on_card() else {
                return Ok(Step { pictures: Vec::new(), ready: Duration::ZERO, encode: Duration::ZERO });
            };
            let nv12 = match gpu.convert(desk, pointer, origin) {
                Ok(nv12) => nv12,
                Err(why) => {
                    self.give_way(duplicator, why)?;
                    return self.encode(duplicator, true);
                }
            };
            let ready = start.elapsed();
            self.first_in.get_or_insert_with(Instant::now);
            return match self.encoder.encode_texture(&nv12, key) {
                Ok(pictures) => {
                    let pictures = self.out(pictures);
                    Ok(Step { pictures, ready, encode: start.elapsed() - ready })
                }
                Err(why) => {
                    self.give_way(duplicator, why)?;
                    self.encode(duplicator, true)
                }
            };
        }
        let Route::Processor(converter) = &mut self.route else { unreachable!() };
        let nv12 = converter.convert(duplicator.frame());
        let ready = start.elapsed();
        let pictures = self.encoder.encode(nv12, key)?;
        Ok(Step { pictures, ready, encode: start.elapsed() - ready })
    }

    fn out(&mut self, pictures: Vec<Vec<u8>>) -> Vec<Vec<u8>> {
        self.came_out |= !pictures.is_empty();
        pictures
    }

    /// Uses `percent` of the encoder's full bitrate.
    pub fn set_rate(&mut self, percent: u32) {
        if let Some(text) = self.encoder.set_rate(percent) {
            self.notes.push(text);
        }
    }

    /// The card route failed: the processor route from now on. A failure there is the end.
    fn give_way(&mut self, duplicator: &mut Duplicator, why: String) -> Result<(), String> {
        if matches!(self.route, Route::Processor(_)) {
            return Err(why);
        }
        self.notes.push(format!("extended display pictures come off the graphics card from now on: {why}"));
        duplicator.keep_on_card(false);
        let rate = self.encoder.rate();
        self.encoder = Encoder::new(self.size, self.fps)?;
        self.encoder.set_rate(rate);
        self.route = Route::Processor(nv12::Converter::new(self.size, self.size));
        self.notes.push(format!("extended display encoder: {}", self.encoder.about));
        Ok(())
    }
}
