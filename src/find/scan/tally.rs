//! Counts the folders still to list under each ticket, so the writer hears once when an errand's last folder is done.

use std::collections::HashMap;
use std::sync::mpsc::Sender;

use crate::find::shared::Order;

// After this many folders a live errand has had its turn, and the rest of it waits with the background scan.
pub(super) const LIVE_DIRS: usize = 2_048;

pub(super) struct Tally {
    sink: Sender<Order>,
    open: HashMap<u32, Count>,
}

#[derive(Default)]
struct Count {
    outstanding: usize,
    listed_live: usize,
}

impl Tally {
    pub(super) fn new(sink: Sender<Order>) -> Self {
        Self { sink, open: HashMap::new() }
    }

    pub(super) fn opened(&mut self, ticket: u32, jobs: usize) {
        self.open.entry(ticket).or_default().outstanding += jobs;
    }

    pub(super) fn closed(&mut self, ticket: u32) {
        let Some(count) = self.open.get_mut(&ticket) else { return };
        count.outstanding = count.outstanding.saturating_sub(1);
        if count.outstanding == 0 {
            self.open.remove(&ticket);
            let _ = self.sink.send(Order::Finished(ticket));
        }
    }

    pub(super) fn listed_live(&mut self, ticket: u32) {
        if let Some(count) = self.open.get_mut(&ticket) {
            count.listed_live += 1;
        }
    }

    pub(super) fn spent(&self, ticket: u32) -> bool {
        self.open.get(&ticket).is_some_and(|count| count.listed_live >= LIVE_DIRS)
    }

    #[cfg(test)]
    fn outstanding(&self) -> usize {
        self.open.values().map(|count| count.outstanding).sum()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;

    use super::*;

    fn finished(inbox: &mpsc::Receiver<Order>) -> Vec<u32> {
        inbox
            .try_iter()
            .filter_map(|order| if let Order::Finished(ticket) = order { Some(ticket) } else { None })
            .collect()
    }

    #[test]
    fn ticket_finishes_once() {
        let (sink, inbox) = mpsc::channel();
        let mut tally = Tally::new(sink);
        tally.opened(7, 2);
        tally.opened(9, 1);
        tally.closed(7);
        tally.opened(7, 3);
        assert_eq!((finished(&inbox), tally.outstanding()), (vec![], 5));
        (0..4).for_each(|_| tally.closed(7));
        assert_eq!(finished(&inbox), [7]);
        tally.closed(7);
        tally.closed(9);
        assert_eq!((finished(&inbox), tally.outstanding()), (vec![9], 0), "a closed ticket never finishes twice");
    }

    #[test]
    fn live_share_runs_out() {
        let (sink, _inbox) = mpsc::channel();
        let mut tally = Tally::new(sink);
        tally.opened(3, 1);
        tally.opened(4, 1);
        (1..LIVE_DIRS).for_each(|_| tally.listed_live(3));
        assert!(!tally.spent(3));
        tally.listed_live(3);
        tally.listed_live(8);
        assert!(tally.spent(3) && !tally.spent(4) && !tally.spent(8), "each errand has its own share");
    }
}
