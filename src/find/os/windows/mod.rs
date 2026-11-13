//! Windows: the fixed drives, change notifications through a completion port, and background priority.

mod bus;
mod gentle;
mod notify;
mod roots;

pub use gentle::be_gentle;
pub use notify::{KEEPS_CURSOR, cursor_now, watch};
pub use roots::{full_access, guarded, launchers, roots, sealed, unsealed};

pub use super::portable::{list, stat};
