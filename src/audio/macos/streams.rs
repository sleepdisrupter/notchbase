//! Stream counts and formats. Taps only work on an output that takes 32-bit float samples.

use super::property::{AudioObjectID, PropertyAddress, data_size, read, read_ids, read_leading};
use super::selectors::{DEVICE_STREAM_CONFIGURATION, DEVICE_STREAMS, SCOPE_OUTPUT, STREAM_VIRTUAL_FORMAT};
use crate::audio::core_audio::fourcc;

const FORMAT_LINEAR_PCM: u32 = fourcc(b"lpcm");
const FORMAT_FLAG_IS_FLOAT: u32 = 1;

#[repr(C)]
#[derive(Clone, Copy)]
struct StreamFormat {
    sample_rate: f64,
    format_id: u32,
    flags: u32,
    bytes_per_packet: u32,
    frames_per_packet: u32,
    bytes_per_frame: u32,
    channels: u32,
    bits: u32,
    reserved: u32,
}

impl StreamFormat {
    fn is_float32(&self) -> bool {
        self.format_id == FORMAT_LINEAR_PCM && self.flags & FORMAT_FLAG_IS_FLOAT != 0 && self.bits == 32
    }
}

pub fn stream_count(device: AudioObjectID, scope: u32) -> usize {
    data_size(device, PropertyAddress::scoped(DEVICE_STREAMS, scope)) / size_of::<AudioObjectID>()
}

pub fn buffer_count(device: AudioObjectID, scope: u32) -> usize {
    read_leading::<u32>(device, PropertyAddress::scoped(DEVICE_STREAM_CONFIGURATION, scope)).map_or(0, |n| n as usize)
}

pub fn has_float32_format(object: AudioObjectID, selector: u32) -> bool {
    read::<StreamFormat>(object, PropertyAddress::global(selector)).is_some_and(|format| format.is_float32())
}

pub fn outputs_float32(device: AudioObjectID) -> bool {
    let streams = read_ids(device, PropertyAddress::scoped(DEVICE_STREAMS, SCOPE_OUTPUT));
    !streams.is_empty() && streams.iter().all(|&stream| has_float32_format(stream, STREAM_VIRTUAL_FORMAT))
}
