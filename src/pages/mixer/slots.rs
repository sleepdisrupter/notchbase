//! Keeps app rows still under the pointer while apps come and go.

use std::collections::HashSet;

use crate::audio::AppId;

use super::layout::APP_ROWS;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Slot {
    pub(super) id: AppId,
    pub(super) leaving: bool,
}

impl Slot {
    fn staying(id: AppId) -> Self {
        Self { id, leaving: false }
    }
}

pub(super) fn sync(
    slots: &mut Vec<Slot>,
    wanted: &[AppId],
    present: &HashSet<AppId>,
    frozen: bool,
    faded: impl Fn(AppId) -> bool,
) {
    if frozen {
        hold_positions(slots, wanted, present);
    } else {
        reflow(slots, wanted, faded);
    }
}

fn hold_positions(slots: &mut Vec<Slot>, wanted: &[AppId], present: &HashSet<AppId>) {
    for slot in slots.iter_mut() {
        slot.leaving = !present.contains(&slot.id);
    }
    for &id in wanted {
        if slots.len() >= APP_ROWS {
            break;
        }
        if !slots.iter().any(|s| s.id == id) {
            slots.push(Slot::staying(id));
        }
    }
}

fn reflow(slots: &mut Vec<Slot>, wanted: &[AppId], faded: impl Fn(AppId) -> bool) {
    slots.retain(|s| !(s.leaving && faded(s.id)));
    let mut next: Vec<Slot> = wanted.iter().copied().map(Slot::staying).collect();
    for (i, slot) in slots.iter().enumerate() {
        if !wanted.contains(&slot.id) {
            next.insert(i.min(next.len()), Slot { id: slot.id, leaving: true });
        }
    }
    *slots = next;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(slots: &[Slot]) -> Vec<(AppId, bool)> {
        slots.iter().map(|s| (s.id, s.leaving)).collect()
    }

    #[test]
    fn rows_hold_still_under_pointer() {
        let mut slots = vec![Slot::staying(1), Slot::staying(2), Slot::staying(3)];
        let present: HashSet<AppId> = [2, 3, 4].into();
        sync(&mut slots, &[3, 4, 2], &present, true, |_| true);
        assert_eq!(ids(&slots), vec![(1, true), (2, false), (3, false), (4, false)], "no row moved");

        sync(&mut slots, &[3, 4, 2], &present, false, |_| true);
        assert_eq!(ids(&slots), vec![(3, false), (4, false), (2, false)]);
    }

    #[test]
    fn leaving_rows_hold_place() {
        let mut slots = vec![Slot::staying(1), Slot::staying(2)];
        let present: HashSet<AppId> = [2].into();
        sync(&mut slots, &[2], &present, false, |_| false);
        assert_eq!(ids(&slots), vec![(1, true), (2, false)], "fades in place first");
        sync(&mut slots, &[2], &present, false, |_| true);
        assert_eq!(ids(&slots), vec![(2, false)], "then closes");
        let mut slots = vec![Slot { id: 1, leaving: true }];
        sync(&mut slots, &[1], &[1].into(), false, |_| false);
        assert_eq!(ids(&slots), vec![(1, false)]);
    }

    #[test]
    fn frozen_rows_stay_under_cap() {
        let mut slots = Vec::new();
        let present: HashSet<AppId> = (1..=6).collect();
        sync(&mut slots, &[1, 2, 3, 4], &present, true, |_| true);
        sync(&mut slots, &[5, 6, 1, 2], &present, true, |_| true);
        assert_eq!(slots.len(), APP_ROWS);
    }
}
