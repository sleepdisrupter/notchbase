//! Answering a question. What was typed is parsed into terms, the index's names are scanned for them across a few
//! threads, and the rows that fit are gathered and ranked. A newer question stops an older one between strides.

mod apps;
#[cfg(test)]
mod bounds;
mod collect;
mod fit;
mod fuzzy;
mod holders;
mod letter;
mod mark;
mod parse;
mod parts;
mod squeeze;
mod sweep;
#[cfg(test)]
mod tests;
mod words;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use super::apps::Book;
use super::context::Context;
use super::guess::suggest::{Suggested, suggest_below};
use super::key::Key;
use super::model::{EntryId, Live};
use super::rank::{self, Judge, Scored, launcher};
use super::shared::Ask;
use super::usage::Usage;
use super::view::{Answer, SHOWN};
use super::weights::USAGE_SHORTLIST;
use mark::START;
use words::Reading;

pub use apps::Launchers;
pub use fit::NOBODY;
pub use fit::{Away, Fit};
pub use fuzzy::{FUZZY_BELOW, trace};
pub use parse::{Query, Term, parse};
pub use squeeze::squeezed;

pub struct Stop<'a> {
    pub latest: &'a AtomicU64,
    pub mine: u64,
}

impl Stop<'_> {
    pub fn stopped(&self) -> bool {
        self.latest.load(Ordering::Acquire) > self.mine
    }
}

#[derive(Default)]
pub struct Scratch {
    pub launchers: Launchers,
    pub book: Arc<Book>,
    pub everything: bool,
    home: Option<PathBuf>,
    own: Vec<u8>,
}

impl Scratch {
    pub fn new(home: Option<PathBuf>) -> Self {
        Self { home, ..Self::default() }
    }
}

pub struct Ground<'a> {
    pub live: &'a Live,
    pub query: &'a Query,
    pub context: &'a Context,
    pub home: Option<EntryId>,
    pub stop: &'a Stop<'a>,
    pub book: &'a Book,
    pub reach: Away,
    all: u8,
    read: u8,
    late: u8,
}

impl<'a> Ground<'a> {
    pub fn new(
        live: &'a Live,
        query: &'a Query,
        context: &'a Context,
        home: Option<EntryId>,
        stop: &'a Stop<'a>,
    ) -> Self {
        let bits = |set: &dyn Fn(&Term) -> bool| bits(&query.terms, set);
        let (all, read) = (bits(&|_| true), bits(&|term| term.reading.is_some()));
        let late = if query.only_letters() { all } else { 0 };
        let reach = query.terms.iter().map(Term::reach).max().unwrap_or_default();
        Self { live, query, context, home, stop, book: Book::none(), reach, all, read, late }
    }

    pub fn reading(self, book: &'a Book) -> Self {
        Self { book, ..self }
    }

    pub fn showing(self, everything: bool) -> Self {
        Self { reach: if everything { Away::Foreign } else { self.reach }, ..self }
    }

    pub fn all(&self) -> u8 {
        self.all
    }

    pub fn read(&self, id: EntryId, flags: u8) -> u8 {
        if self.read == 0 {
            return 0;
        }
        let (live, name) = (self.live, self.live.folded(id));
        let holds = |reading: &Reading| {
            reading.holds(name, flags, live.modified(id), live.created(id))
                || (reading.launches() && launcher(live, id, name, flags))
        };
        let admitted = |term: &Term| term.admits(live, id, flags, self.home);
        bits(&self.query.terms, |term| term.reading.as_ref().is_some_and(holds) && admitted(term))
    }

    pub fn reads_apps(&self) -> bool {
        self.query.terms.iter().any(|term| term.reading.as_ref().is_some_and(Reading::launches))
    }

    pub fn late(&self, id: EntryId, wanted: u8) -> u8 {
        let wanted = wanted & self.late;
        if wanted == 0 || self.live.is_dead(id) {
            return 0;
        }
        let (live, name, flags) = (self.live, self.live.folded(id), self.live.flags(id));
        let asked = self.query.terms.iter().enumerate().filter(|(index, _)| wanted & (1 << index) != 0);
        let held = asked.filter(|(_, term)| term.within(name) && term.admits(live, id, flags, self.home));
        held.fold(0, |bits, (index, term)| bits | 1 << index | if name.starts_with(&term.needle) { START } else { 0 })
    }

    pub fn owned(&self, id: EntryId) -> u8 {
        START | self.within(id, self.live.flags(id), self.live.folded(id))
    }

    pub fn within(&self, id: EntryId, flags: u8, name: &[u8]) -> u8 {
        bits(&self.query.terms, |term| term.within(name) && term.admits(self.live, id, flags, self.home))
    }
}

fn bits(terms: &[Term], set: impl Fn(&Term) -> bool) -> u8 {
    terms.iter().enumerate().filter(|(_, term)| set(term)).fold(0, |bits, (index, _)| bits | 1 << index)
}

pub fn asks_nothing(ask: &Ask) -> bool {
    parse(&ask.text, &ask.context).terms.is_empty()
}

pub fn run(live: &Live, usage: &Usage, ask: &Ask, scratch: &mut Scratch, stop: &Stop<'_>) -> Option<Answer> {
    let query = parse(&ask.text, &ask.context);
    let (home, book) = (scratch.home.clone(), scratch.book.clone());
    let (rows, more) = if query.terms.is_empty() {
        let everything = scratch.everything;
        let suggested =
            usage.read(|model| suggest_below(live, model, &ask.context, SHOWN, home.as_deref(), everything));
        let row = |found: Suggested| {
            rank::row_in(&book, live, found.id, Vec::new(), Some(found.why), found.predicted, home.as_deref())
        };
        scratch.launchers.refresh(live, stop, cleared(&mut scratch.own, live.len()))?;
        (suggested.into_iter().map(row).collect(), false)
    } else {
        let literal = |term: &&Term| term.reading.is_none();
        let needles: Vec<&[u8]> = query.terms.iter().filter(literal).map(|term| &term.needle[..]).collect();
        let stored = if needles.is_empty() { Vec::new() } else { usage.matching(&needles) };
        let at_home = home.as_deref().and_then(|home| live.resolve(home));
        let ground = Ground::new(live, &query, &ask.context, at_home, stop).reading(&book).showing(scratch.everything);
        let (shortlist, more) = search(&ground, scratch, &stored, mark::allowance())?;
        (rank::finish(&ground, usage, shortlist, home.as_deref()), more)
    };
    let answer =
        Answer { generation: ask.generation, query: ask.text.clone(), rows, reading: words::echo(&query.terms), more };
    (!stop.stopped()).then_some(answer)
}

fn search(
    ground: &Ground<'_>,
    scratch: &mut Scratch,
    stored: &[(Key, Box<str>)],
    most: Option<usize>,
) -> Option<(Vec<Scored>, bool)> {
    let (live, own) = (ground.live, cleared(&mut scratch.own, ground.live.len()));
    let remembered = remembered(ground, stored);
    let apps = apps::launchers(ground, &mut scratch.launchers, own)?;
    let mut parts = parts::cut(live, own, parts::MOST_PARTS);
    let cut = mark::mark(ground, &mut parts, most)?;
    sweep::sweep(ground, &mut parts)?;
    let spans = parts::spans(&parts);
    drop(parts);
    apps::sweep(ground, &scratch.launchers, own);
    let mut found = collect::collect(ground, own, spans)?;
    let candidates = found.count + remembered.len();
    let (loose, phrase) = (fuzzy::wanted(ground.query, candidates), squeeze::wanted(ground.query, candidates));
    if loose.is_some() || phrase.is_some() {
        let mut judge = Judge::new(ground);
        let mut weigh = |id: EntryId, fit: Fit| judge.weigh(id, live.flags(id), fit, None);
        let alike = fuzzy::survivors(ground, &mut parts::cut(live, own, parts::MOST_PARTS), loose, phrase)?;
        let squeezed: Vec<Scored> = alike.squeezed.into_iter().filter_map(|id| weigh(id, Fit::squeezed())).collect();
        let spread: Vec<Scored> = alike.spread.into_iter().filter_map(|id| weigh(id, Fit::fuzzy())).collect();
        squeezed.into_iter().chain(spread).for_each(|loose| found.offer(loose, ground.reach));
    }
    let mut shortlist = found.best.into_vec();
    shortlist.extend(found.tucked.into_vec());
    shortlist.extend(remembered);
    Some((apps::replace(shortlist, apps), found.more || cut))
}

pub fn warm(live: &Live, scratch: &mut Scratch, stop: &Stop<'_>) -> Option<()> {
    scratch.launchers.refresh(live, stop, cleared(&mut scratch.own, live.len()))
}

fn cleared(own: &mut Vec<u8>, rows: usize) -> &mut [u8] {
    if own.len() == rows {
        own.fill(0);
    } else {
        *own = vec![0; rows];
    }
    own
}

fn remembered(ground: &Ground<'_>, stored: &[(Key, Box<str>)]) -> Vec<Scored> {
    let live = ground.live;
    let mut judge = Judge::new(ground);
    let known = |(_, path): &(Key, Box<str>)| {
        let id = live.resolve(Path::new(&**path))?;
        let flags = live.flags(id);
        judge.weigh(id, flags, collect::fit(ground, id, flags, &|id| ground.owned(id))?, None)
    };
    stored.iter().take(USAGE_SHORTLIST).filter_map(known).collect()
}
