//! Where the calendar stands: ready, waiting for access, loading and so on.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Ready,
    Setup,
    Loading,
    Waiting,
    Denied,
    Unavailable,
}
