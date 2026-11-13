//! Errands: work the writer sent the scanner, with one ticket per job. An errand lands once its last ticket finishes,
//! and then its duties run: bury what moved away, mark a root scanned, or note that the guarded folders are done.

use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Duty {
    Bury(PathBuf),
    Scanned { root: PathBuf, reach: u8 },
    Guarded,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Purpose {
    Live,
    Launch,
    Reconcile,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Errand {
    pub duties: Vec<Duty>,
    pub purpose: Purpose,
    tickets: usize,
}

#[derive(Default)]
pub(super) struct Errands {
    next_errand: u32,
    next_ticket: u32,
    open: HashMap<u32, Errand>,
    tickets: HashMap<u32, u32>,
}

impl Errands {
    pub(super) fn open(&mut self, duties: Vec<Duty>, purpose: Purpose) -> u32 {
        self.next_errand = self.next_errand.wrapping_add(1);
        self.open.insert(self.next_errand, Errand { duties, purpose, tickets: 0 });
        self.next_errand
    }

    pub(super) fn ticket(&mut self, errand: u32) -> Option<u32> {
        let open = self.open.get_mut(&errand)?;
        open.tickets += 1;
        self.next_ticket = self.next_ticket.wrapping_add(1);
        self.tickets.insert(self.next_ticket, errand);
        Some(self.next_ticket)
    }

    pub(super) fn duties(&mut self, errand: u32) -> Option<&mut Vec<Duty>> {
        self.open.get_mut(&errand).map(|open| &mut open.duties)
    }

    pub(super) fn errand_of(&self, ticket: u32) -> Option<u32> {
        self.tickets.get(&ticket).copied()
    }

    pub(super) fn finished(&mut self, ticket: u32) -> Option<(u32, Errand)> {
        let errand = self.tickets.remove(&ticket)?;
        let open = self.open.get_mut(&errand)?;
        open.tickets = open.tickets.saturating_sub(1);
        if open.tickets > 0 {
            return None;
        }
        self.open.remove(&errand).map(|landed| (errand, landed))
    }

    pub(super) fn idle(&mut self, errand: u32) -> Option<Errand> {
        let waiting = self.open.get(&errand).is_some_and(|open| open.tickets == 0);
        if waiting { self.open.remove(&errand) } else { None }
    }

    pub(super) fn any(&self, purpose: Purpose) -> bool {
        self.open.values().any(|open| open.purpose == purpose)
    }

    pub(super) fn covers(&self, duty: &Duty) -> bool {
        self.open.values().any(|open| open.duties.contains(duty))
    }

    pub(super) fn abandon(&mut self) {
        self.open.clear();
        self.tickets.clear();
    }

    pub(super) fn is_empty(&self) -> bool {
        self.open.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn errand_lands_once() {
        let mut errands = Errands::default();
        let errand = errands.open(vec![Duty::Guarded], Purpose::Launch);
        let (first, second) = (errands.ticket(errand).unwrap(), errands.ticket(errand).unwrap());
        assert_ne!(first, second);
        assert_eq!((errands.errand_of(first), errands.errand_of(second)), (Some(errand), Some(errand)));
        assert_eq!(errands.finished(second), None);
        let landed = errands.finished(first).expect("the last ticket");
        assert_eq!((landed.0, landed.1.duties, landed.1.purpose), (errand, vec![Duty::Guarded], Purpose::Launch));
        assert_eq!((errands.finished(first), errands.finished(second)), (None, None));
        assert!(errands.is_empty());
    }

    #[test]
    fn new_ticket_keeps_errand_open() {
        let mut errands = Errands::default();
        let errand = errands.open(Vec::new(), Purpose::Live);
        let shallow = errands.ticket(errand).unwrap();
        let deeper = errands.ticket(errand).unwrap();
        assert_eq!(errands.finished(shallow), None, "the folder it found is still being listed");
        assert!(errands.finished(deeper).is_some());
    }

    #[test]
    fn only_idle_errands_close() {
        let mut errands = Errands::default();
        let (empty, busy) = (errands.open(Vec::new(), Purpose::Live), errands.open(Vec::new(), Purpose::Reconcile));
        errands.ticket(busy);
        assert!(errands.any(Purpose::Reconcile) && !errands.any(Purpose::Launch));
        assert!(errands.idle(busy).is_none());
        assert!(errands.idle(empty).is_some() && errands.idle(empty).is_none());
        assert_eq!(errands.ticket(empty), None, "a closed errand takes no more work");
    }
}
