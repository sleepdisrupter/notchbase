//! macOS: bulk listings, FSEvents, the volumes to index, and which folders need Full Disk Access.

mod access;
mod bulk;
mod events;
mod ffi;
mod gentle;
mod list;
mod roots;

pub use access::full_access;
pub use events::{KEEPS_CURSOR, cursor_now, watch};
pub use gentle::be_gentle;
pub use list::{list, stat};
pub use roots::{guarded, launchers, roots, sealed, unsealed};
