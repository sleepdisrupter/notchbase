//! Everything the mixer shows at one moment, and whether a new one differs enough to be worth a repaint.

use std::sync::Arc;

use crate::audio::per_kind::PerKind;
use crate::audio::{AppId, AudioApp, AudioDevice, INPUT, MASTER, imp};

const VOLUME_STEP: f32 = 1e-4;
const METER_STEP: f32 = 0.006;

#[derive(Clone, Default)]
pub struct Snapshot {
    pub supported: bool,
    pub reason: Option<String>,
    pub apps: Vec<AudioApp>,
    pub master: Option<AudioApp>,
    pub input: Option<AudioApp>,
    pub devices: PerKind<Vec<AudioDevice>>,
    pub seq: u64,
}

impl Snapshot {
    pub fn capture(engine: &imp::Backend, seq: u64) -> Self {
        Self {
            supported: engine.is_supported(),
            reason: engine.unsupported_reason().map(str::to_owned),
            apps: engine.apps().to_vec(),
            master: engine.master().cloned(),
            input: engine.input().cloned(),
            devices: PerKind::from_fn(|kind| engine.devices(kind).to_vec()),
            seq,
        }
    }

    pub fn row_mut(&mut self, id: AppId) -> Option<&mut AudioApp> {
        match id {
            MASTER => self.master.as_mut(),
            INPUT => self.input.as_mut(),
            _ => self.apps.iter_mut().find(|a| a.id == id),
        }
    }

    pub fn rows_mut(&mut self) -> impl Iterator<Item = &mut AudioApp> {
        self.master.iter_mut().chain(self.input.iter_mut()).chain(self.apps.iter_mut())
    }

    pub fn differs(&self, other: &Snapshot) -> bool {
        self.seq != other.seq
            || self.supported != other.supported
            || self.reason != other.reason
            || self.devices != other.devices
            || !same_optional(&self.master, &other.master)
            || !same_optional(&self.input, &other.input)
            || self.apps.len() != other.apps.len()
            || self.apps.iter().zip(&other.apps).any(|(a, b)| !same_row(a, b))
    }
}

fn same_row(a: &AudioApp, b: &AudioApp) -> bool {
    let same_icon = match (&a.icon, &b.icon) {
        (Some(x), Some(y)) => Arc::ptr_eq(x, y),
        (None, None) => true,
        _ => false,
    };
    a.id == b.id
        && a.name == b.name
        && a.muted == b.muted
        && (a.volume - b.volume).abs() <= VOLUME_STEP
        && (a.peak - b.peak).abs() <= METER_STEP
        && same_icon
}

fn same_optional(a: &Option<AudioApp>, b: &Option<AudioApp>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => same_row(a, b),
        (None, None) => true,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app(id: AppId, peak: f32) -> AudioApp {
        AudioApp { id, name: format!("app {id}"), volume: 0.5, muted: false, peak, icon: None }
    }

    #[test]
    fn only_visible_changes_count() {
        let a = Snapshot { apps: vec![app(1, 0.25)], ..Snapshot::default() };
        let mut b = a.clone();
        assert!(!b.differs(&a));
        b.apps[0].peak = 0.2501;
        assert!(!b.differs(&a));
        b.apps[0].peak = 0.26;
        assert!(b.differs(&a));
        let mut c = a.clone();
        c.apps[0].volume = 0.6;
        assert!(c.differs(&a));
        let mut d = a.clone();
        d.seq = 1;
        assert!(d.differs(&a), "a newer command always publishes");
        let mut e = a.clone();
        e.apps.push(app(2, 0.0));
        assert!(e.differs(&a));
    }
}
