//! The Core Audio pieces that need no Mac: buffer lists, gain, the tap's render path, bundle ids and transports. They
//! build on every system so their tests run everywhere.

pub mod buffers;
pub mod bundle_ids;
pub mod gain;
mod render;
pub mod tap_shared;
pub mod transport;

pub const fn fourcc(code: &[u8; 4]) -> u32 {
    u32::from_be_bytes(*code)
}

#[cfg(test)]
mod tests {
    use super::fourcc;

    #[test]
    fn fourccs_match_the_headers() {
        assert_eq!(fourcc(b"glob"), 0x676C_6F62);
        assert_eq!(fourcc(b"prs#"), 0x7072_7323);
        assert_eq!(fourcc(b"dIn "), 0x6449_6E20);
    }
}
