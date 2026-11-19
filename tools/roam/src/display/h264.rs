//! H.264 the two ways systems hand it over: Annex B (each unit after a `00 00 01` start code),
//! which Media Foundation speaks and the stream carries, and AVCC (each unit after its length),
//! which VideoToolbox and AVFoundation speak.

pub const SPS: u8 = 7;
pub const PPS: u8 = 8;
pub const IDR: u8 = 5;
const AUD: u8 = 9;

pub fn kind(unit: &[u8]) -> u8 {
    unit.first().map_or(0, |b| b & 0x1F)
}

/// The units of an Annex B stream, without their start codes.
pub fn units(data: &[u8]) -> Vec<&[u8]> {
    let mut starts = Vec::new();
    let mut i = 0;
    while i + 3 <= data.len() {
        if data[i] == 0 && data[i + 1] == 0 && data[i + 2] == 1 {
            starts.push(i + 3);
            i += 3;
        } else {
            i += 1;
        }
    }
    let mut out = Vec::with_capacity(starts.len());
    for (n, &start) in starts.iter().enumerate() {
        let mut end = starts.get(n + 1).map_or(data.len(), |next| next - 3);
        // A four byte start code's leading zero belongs to it, not to this unit.
        while end > start && data[end - 1] == 0 && starts.get(n + 1).is_some() {
            end -= 1;
        }
        if end > start {
            out.push(&data[start..end]);
        }
    }
    out
}

/// The sequence and picture parameter sets in an Annex B stream, if it has both.
pub fn parameter_sets(data: &[u8]) -> Option<(&[u8], &[u8])> {
    let units = units(data);
    let sps = units.iter().find(|u| kind(u) == SPS)?;
    let pps = units.iter().find(|u| kind(u) == PPS)?;
    Some((sps, pps))
}

/// Whether an Annex B access unit starts a new picture from scratch.
pub fn is_key(data: &[u8]) -> bool {
    units(data).iter().any(|u| kind(u) == IDR)
}

/// The picture's units, length prefixed for AVFoundation, without parameter sets or delimiters.
pub fn to_avcc(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len() + 16);
    for unit in units(data) {
        if matches!(kind(unit), SPS | PPS | AUD) {
            continue;
        }
        out.extend_from_slice(&(unit.len() as u32).to_be_bytes());
        out.extend_from_slice(unit);
    }
    out
}

/// AVCC units (each after a big-endian length `length_size` bytes long) as Annex B, after
/// `before` (the parameter sets, for a key picture).
pub fn from_avcc(data: &[u8], length_size: usize, before: &[&[u8]]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len() + 64);
    for unit in before {
        out.extend_from_slice(&[0, 0, 0, 1]);
        out.extend_from_slice(unit);
    }
    let mut at = 0;
    while at + length_size <= data.len() {
        let length = data[at..at + length_size].iter().fold(0usize, |n, &b| (n << 8) | usize::from(b));
        at += length_size;
        let Some(unit) = data.get(at..at + length) else { break };
        out.extend_from_slice(&[0, 0, 0, 1]);
        out.extend_from_slice(unit);
        at += length;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const STREAM: &[u8] = &[
        0, 0, 0, 1, 0x09, 0xF0, // delimiter
        0, 0, 0, 1, 0x67, 1, 2, 3, // SPS
        0, 0, 1, 0x68, 4, 5, // PPS, three byte start code
        0, 0, 0, 1, 0x65, 6, 7, 8, 0, // IDR, ending in a zero byte of its own
    ];

    #[test]
    fn units_split_on_both_start_codes() {
        let units = units(STREAM);
        assert_eq!(units.len(), 4);
        assert_eq!(units[1], &[0x67, 1, 2, 3]);
        assert_eq!(units[2], &[0x68, 4, 5]);
        assert_eq!(units[3], &[0x65, 6, 7, 8, 0]);
        assert!(is_key(STREAM));
        assert_eq!(parameter_sets(STREAM), Some((&[0x67, 1, 2, 3][..], &[0x68, 4, 5][..])));
    }

    #[test]
    fn avcc_keeps_only_the_picture() {
        assert_eq!(to_avcc(STREAM), vec![0, 0, 0, 5, 0x65, 6, 7, 8, 0]);
    }

    #[test]
    fn avcc_back_to_annex_b_with_parameter_sets_first() {
        let avcc = [0, 0, 0, 3, 0x65, 1, 2, 0, 0, 0, 2, 0x41, 9];
        let back = from_avcc(&avcc, 4, &[&[0x67, 1], &[0x68, 2]]);
        let units = units(&back);
        assert_eq!(units, vec![&[0x67, 1][..], &[0x68, 2], &[0x65, 1, 2], &[0x41, 9]]);
        assert!(!is_key(&from_avcc(&avcc[7..], 4, &[])));
    }
}
