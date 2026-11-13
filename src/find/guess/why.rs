//! The reason shown with a guess: what you opened before it, the folder you are busy in, the app you use it with.

use std::path::Path;
use std::sync::Arc;

use crate::find::context::{Context, Stamp};
use crate::find::key::Key;
use crate::find::model::{Live, flag};
use crate::find::rank::{Candidate, launcher, shown_as};
use crate::find::usage::Model;
use crate::find::view::Why;
use crate::find::weights::DAYPART_MIN_OPENS;

const WORTH_SAYING: f32 = 1.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Signal {
    Opened,
    Edited,
    Heat,
    Sequence,
    App,
    Daypart,
}

pub fn why(live: &Live, model: &Model, who: &Candidate, signal: Signal, context: &Context) -> Option<Why> {
    let key = who.keys[0];
    let opened = model.known(key).filter(|record| record.opened());
    match signal {
        Signal::Opened => opened.map(|record| Why::Opened(record.last_open)),
        Signal::Edited => opened.filter(|record| record.last_edit != 0).map(|record| Why::Edited(record.last_edit)),
        Signal::Heat => busy(model, if who.flags & flag::DIR != 0 { key } else { who.keys[1] }, context.now),
        Signal::Sequence => after(live, model, key, context.now),
        Signal::App => with(model, key, context),
        Signal::Daypart => {
            let around = opened.and_then(|record| record.around(context.daypart()));
            around.filter(|habit| habit.count >= DAYPART_MIN_OPENS).map(|_| Why::AroundNow)
        }
    }
}

pub(super) fn busy(model: &Model, folder: Key, now: Stamp) -> Option<Why> {
    let heat = model.heat(folder, now);
    (heat >= WORTH_SAYING).then(|| Why::Busy(heat.round().min(f32::from(u16::MAX)) as u16))
}

pub(super) fn after(live: &Live, model: &Model, key: Key, now: Stamp) -> Option<Why> {
    let before = Path::new(model.path_of(model.after_previous(key, now)?.before)?);
    let name = before.file_name()?.to_string_lossy();
    let app = live.resolve(before).is_some_and(|id| launcher(live, id, live.folded(id), live.flags(id)));
    Some(Why::After(Arc::from(if app { shown_as(&name) } else { &*name })))
}

pub(super) fn with(model: &Model, key: Key, context: &Context) -> Option<Why> {
    let app = context.app.as_ref()?;
    model.with_app(model.known(key)?, app).map(|_| Why::With(app.clone()))
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::find::context::{HOUR, MINUTE};
    use crate::find::key::key_of;
    use crate::find::kind::Kind;
    use crate::find::tests::corpus::from_paths;
    use crate::find::usage::Usage;
    use crate::find::view::{How, Pick};

    const NOON: Stamp = 1_704_110_400;
    const SHORTCUT: &str = "/ProgramData/Microsoft/Windows/Start Menu/Programs/TreeSize Free.lnk";
    const BUNDLE: &str = "/Applications/Xcode.app";
    const ON_THE_DESKTOP: &str = "/Users/john/Desktop/Old Maps.lnk";
    const NO_LONGER_LISTED: &str = "/Applications/Gone.app";
    const NOTES: &str = "/Users/john/notes.md";

    #[test]
    fn after_app_names_the_app() {
        let live = from_paths(&[SHORTCUT, "/Applications/Xcode.app/", ON_THE_DESKTOP, NOTES]);
        let usage = Usage::load(None, NOON);
        let open = |path: &str, at: Stamp| {
            let pick =
                Pick { path: PathBuf::from(path), typed: String::new(), rank: 0, how: How::Open, predicted: false };
            usage.picked(&pick, Kind::Other, &Context::at(at, 0, None));
        };
        let said_after = |before: &str, hour: Stamp| {
            let at = NOON + hour * HOUR;
            open(before, at);
            open(NOTES, at + MINUTE);
            open(before, at + 2 * MINUTE);
            usage.read(|model| after(&live, model, key_of(Path::new(NOTES)), at + 3 * MINUTE))
        };
        assert_eq!(said_after(SHORTCUT, 0), Some(Why::After("TreeSize Free".into())));
        assert_eq!(said_after(BUNDLE, 1), Some(Why::After("Xcode".into())));
        assert_eq!(said_after(ON_THE_DESKTOP, 2), Some(Why::After("Old Maps.lnk".into())), "a link is a file");
        assert_eq!(said_after(NO_LONGER_LISTED, 3), Some(Why::After("Gone.app".into())), "the index has no such app");
    }
}
