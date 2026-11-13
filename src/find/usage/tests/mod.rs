mod disk;
mod facade;
mod moves;
mod prefix;
mod spelling;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::find::context::{Context, DAY, HOUR, MINUTE, Stamp};
use crate::find::key::{Key, key_of};
use crate::find::kind::Kind;
use crate::find::view::{How, Pick};

use super::{Seen, Usage, Used};

pub const MONDAY_NOON: Stamp = 1_704_110_400;
pub const CARGO: &str = "/Users/john/GitHub/notchbase/Cargo.toml";
pub const MAIN: &str = "/Users/john/GitHub/notchbase/src/main.rs";
pub const README: &str = "/Users/john/GitHub/notchbase/README.md";
pub const DIARY: &str = "/Users/john/private/diary.md";

pub fn key(path: &str) -> Key {
    key_of(Path::new(path))
}

pub fn at(now: Stamp, app: Option<&str>) -> Context {
    Context::at(now, 0, app.map(Arc::from))
}

pub fn pick(path: &str, typed: &str) -> Pick {
    Pick { path: PathBuf::from(path), typed: typed.to_owned(), rank: 0, how: How::Open, predicted: false }
}

pub fn edit(path: &str, at: Stamp) -> Seen {
    Seen::Edited { path: PathBuf::from(path), at, plain: true }
}

pub fn lived_in(usage: &Usage) -> Stamp {
    usage.seed(&[Used { path: PathBuf::from(README), last: MONDAY_NOON - 3 * DAY }], MONDAY_NOON);
    usage.picked(&pick(CARGO, "car"), Kind::Text, &at(MONDAY_NOON, Some("Xcode")));
    usage.picked(&pick(MAIN, "main"), Kind::Code, &at(MONDAY_NOON + 5 * MINUTE, Some("Xcode")));
    usage.picked(&pick(CARGO, "ca"), Kind::Text, &at(MONDAY_NOON + HOUR, Some("Terminal")));
    usage.picked(&pick(DIARY, "diary"), Kind::Text, &at(MONDAY_NOON + HOUR + MINUTE, None));
    let seen = [
        edit(MAIN, MONDAY_NOON + 2 * HOUR),
        edit("/Users/john/GitHub/notchbase/src/lib.rs", MONDAY_NOON + 2 * HOUR),
        Seen::Added { path: PathBuf::from(README), file_id: 4_242, created: MONDAY_NOON, plain: true },
        Seen::Removed { path: PathBuf::from(README) },
    ];
    usage.observe(&seen, MONDAY_NOON + 2 * HOUR);
    usage.left(&[key(MAIN)], Some(key(CARGO)), MONDAY_NOON + 2 * HOUR);
    MONDAY_NOON + 2 * HOUR
}
