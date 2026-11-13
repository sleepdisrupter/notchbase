//! Frames of the recorded area, from DXGI desktop duplication when it starts and from GDI when it can't.

use std::time::Instant;

use super::dxgi::Duplicator;
use super::gdi::Grabber;
use crate::record::Region;

pub(super) enum Screen {
    Gpu(Box<Duplicator>),
    Gdi(Box<Grabber>),
}

impl Screen {
    pub(super) fn open(region: &Region) -> Result<Self, String> {
        match Duplicator::open(region) {
            Ok(duplicator) => Ok(Self::Gpu(Box::new(duplicator))),
            Err(err) => {
                eprintln!("notchbase: recording through GDI ({err})");
                Grabber::open(region).map(|grabber| Self::Gdi(Box::new(grabber)))
            }
        }
    }

    pub(super) fn wait(&mut self, deadline: Instant) -> Result<(), String> {
        match self {
            Self::Gpu(duplicator) => duplicator.wait(deadline),
            Self::Gdi(grabber) => {
                grabber.wait(deadline);
                Ok(())
            }
        }
    }

    pub(super) fn frame(&mut self) -> &[u8] {
        match self {
            Self::Gpu(duplicator) => duplicator.frame(),
            Self::Gdi(grabber) => grabber.frame(),
        }
    }
}
