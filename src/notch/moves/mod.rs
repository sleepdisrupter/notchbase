//! How the notch moves: leaning toward the pointer, fleeing it, dodging menus, hugging the top
//! edge, surging, the launch at start and the springs between one mode and the next.

pub(super) mod attract;
pub(super) mod dodge;
pub(super) mod flee;
mod flow;
pub(super) mod hug;
mod launch;
pub(super) mod surge;
pub(super) mod transition;
