//! Media on Windows: the System Media Transport Controls for what's playing, and loopback capture for the levels.

mod capture;
mod source;
#[cfg(test)]
mod tests;

pub use capture::Capture;
pub use source::Source;
