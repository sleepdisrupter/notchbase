//! The blurred desktop picture the macOS glass shows in Mission Control. Windows has no Mission Control, so only the
//! picture's shape is here, for the liquid's drawing code.

/// The blurred desktop behind the window, premultiplied RGBA, its first row the window's top.
pub struct Picture {
    pub rgba: Vec<u8>,
    pub size: [usize; 2],
}
