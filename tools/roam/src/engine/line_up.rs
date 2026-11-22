//! Lining the screens up (see `align`): the computer whose desk is being arranged works out
//! every screen's guide from the desk the cursor follows, shows its own and sends each computer
//! it controls theirs. As its cursor nears an edge, it marks where the cursor would leave and
//! where it would come in. A computer told to show guides only draws them.

use super::*;
use crate::align::{self, Guide, Mark, Marked, Screen, Shown};

/// Guides come down by themselves after this long, in case whatever put them up is gone.
const LINE_UP_FOR: u64 = 20 * 60 * SECOND;
/// How often the guides are worked out again while they're up.
const GUIDES_EVERY: u64 = 300 * MS;
/// The cursor's marks are worked out at most this often.
const MARKS_EVERY: u64 = 30 * MS;

/// A screen on the desk and its colour, for the editor to colour it the same.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Coloured {
    /// The computer it's shown as part of on the desk (a lent screen, the computer lending it).
    pub id: String,
    /// Its place in that computer's list.
    pub display: u8,
    pub colour: u8,
}

/// This computer leading the line-up.
pub(super) struct Leading {
    since: u64,
    next: u64,
    serial: u16,
    /// What's up on this computer's own screens.
    local: Vec<Shown>,
    /// What each computer it controls was last sent.
    sent: HashMap<Id, Vec<Guide>>,
    /// The marks each computer (none: this one) shows now.
    marks: HashMap<Option<Id>, Vec<Mark>>,
    marks_at: u64,
    /// Where the cursor was when its marks were last due but not yet worked out.
    pending: Option<(Option<Id>, [f64; 2])>,
    coloured: Vec<Coloured>,
}

/// This computer showing another's guides.
#[derive(Default)]
pub(super) struct Following {
    from: Option<Id>,
    /// Guides of the set on its way.
    pending: Vec<(u16, Guide)>,
    shown: Vec<Guide>,
    marks: Vec<Mark>,
    /// The screens they were shown on, to show them again when the screens change.
    screens: Vec<Rect>,
    /// Taken down here: nothing shows until the line-up is over and starts again.
    hidden: bool,
}

/// The desk the cursor follows, with who each computer on it is (none: this one) and how each
/// of its screens is named and coloured.
struct NamedDesk {
    desk: Vec<Computer>,
    ids: Vec<Option<Id>>,
    screens: Vec<Vec<Screen>>,
    coloured: Vec<Coloured>,
}

/// One screen's place in a line of several: "Main screen", "Screen 2".
fn which(displays: &[Rect], index: usize) -> String {
    match displays.get(index) {
        Some(d) if d.contains([0.0, 0.0]) || displays.len() == 1 => "Main screen".into(),
        _ => format!("Screen {}", index + 1),
    }
}

fn size_words(rect: &Rect) -> String {
    format!("{} × {}", rect.w.round() as i64, rect.h.round() as i64)
}

impl Core {
    pub(super) fn lining_up(&self) -> bool {
        self.leading.is_some()
    }

    pub(super) fn coloured(&self) -> Vec<Coloured> {
        self.leading.as_ref().map(|l| l.coloured.clone()).unwrap_or_default()
    }

    /// The name of the computer whose guides are up on this one's screens.
    pub(super) fn guided_by(&self) -> Option<String> {
        let following = self.following.as_ref()?;
        (!following.shown.is_empty() && !following.hidden).then(|| following.from.map(|id| self.who(id))).flatten()
    }

    /// Turns the guides on or off.
    pub(super) fn line_up(&mut self, on: bool) {
        let now = self.now();
        match (on, self.leading.is_some()) {
            (true, false) => {
                self.note("lining up the screens: guides on every screen");
                self.leading = Some(Leading {
                    since: now,
                    next: 0,
                    serial: 0,
                    local: Vec::new(),
                    sent: HashMap::new(),
                    marks: HashMap::new(),
                    marks_at: 0,
                    pending: None,
                    coloured: Vec::new(),
                });
                self.refresh_guides(now);
            }
            (true, true) => {
                // Asked again: keep them up for longer.
                if let Some(leading) = &mut self.leading {
                    leading.since = now;
                }
            }
            (false, true) => self.stop_line_up("lining up the screens is over"),
            (false, false) => {}
        }
    }

    fn stop_line_up(&mut self, why: &str) {
        let Some(leading) = self.leading.take() else { return };
        self.note(why);
        self.displays.guide(Vec::new());
        self.displays.mark(Vec::new());
        let serial = leading.serial.wrapping_add(1);
        for id in leading.sent.keys() {
            if let Some(link) = self.sessions.get_mut(id).and_then(Session::live) {
                link.queue(Message::Guided { serial, displays: Vec::new() });
            }
        }
        self.flush(self.now(), true);
    }

    /// The timed part: guides worked out again now and then, marks that were held back, and
    /// the guides taken down when it's been long enough.
    pub(super) fn line_up_tick(&mut self, now: u64) {
        let Some(leading) = &self.leading else { return };
        if now.saturating_sub(leading.since) > LINE_UP_FOR {
            return self.stop_line_up("lining up the screens: guides down after 20 minutes");
        }
        if now >= leading.next {
            self.refresh_guides(now);
        }
        if let Some(leading) = &mut self.leading
            && now >= leading.marks_at
            && let Some((on, at)) = leading.pending.take()
        {
            self.line_up_marks(on, at, now);
        }
    }

    /// When the timed part needs to run next.
    pub(super) fn line_up_deadline(&self) -> Option<u64> {
        let leading = self.leading.as_ref()?;
        Some(if leading.pending.is_some() { leading.next.min(leading.marks_at) } else { leading.next })
    }

    /// The desk the cursor follows, and how each of its screens is named and coloured.
    fn named_desk(&self) -> NamedDesk {
        let (desk, ids) = self.desk();
        let mut screens = Vec::new();
        let mut coloured = Vec::new();
        let mut next: u8 = 0;
        let mine = hex(&self.identity.id);
        let my_name = self.name();
        let (real, _) = self.real_screens();
        for (computer, id) in desk.iter().zip(&ids) {
            let mut list = Vec::new();
            match id {
                None => {
                    for (index, rect) in computer.displays.iter().enumerate() {
                        let colour = next;
                        next = next.wrapping_add(1);
                        let display = u8::try_from(index).unwrap_or(u8::MAX);
                        let hosted = self.extended.iter().find(|(_, e)| e.rect.is_some_and(|r| extend::same(&r, rect)));
                        if let Some((&(peer, lent), _)) = hosted {
                            let screen = self.screen_name(peer, lent);
                            list.push(Screen {
                                display,
                                colour,
                                title: screen.clone(),
                                detail: format!("Extended display of {my_name} · {}", size_words(rect)),
                                label: screen,
                            });
                            coloured.push(Coloured { id: hex(&peer), display: lent, colour });
                            continue;
                        }
                        let place = real.iter().position(|r| r == rect).unwrap_or(index);
                        let words = which(&real, place);
                        list.push(Screen {
                            display,
                            colour,
                            title: my_name.clone(),
                            detail: format!("{words} · {}", size_words(rect)),
                            label: if real.len() > 1 {
                                format!("{my_name} · {}", words.to_lowercase())
                            } else {
                                my_name.clone()
                            },
                        });
                        coloured.push(Coloured {
                            id: mine.clone(),
                            display: u8::try_from(place).unwrap_or(u8::MAX),
                            colour,
                        });
                    }
                }
                Some(peer) => {
                    let name = self.who(*peer);
                    let theirs = self.peers.get(peer).map(|p| p.displays.clone()).unwrap_or_default();
                    let lent = |index: usize| {
                        self.extended
                            .get(&(*peer, u8::try_from(index).unwrap_or(u8::MAX)))
                            .is_some_and(|e| e.rect.is_some())
                    };
                    let indices: Vec<usize> = (0..theirs.len()).filter(|i| !lent(*i)).collect();
                    for (place, rect) in indices.iter().zip(&computer.displays) {
                        let colour = next;
                        next = next.wrapping_add(1);
                        let display = u8::try_from(*place).unwrap_or(u8::MAX);
                        let words = which(&theirs, *place);
                        list.push(Screen {
                            display,
                            colour,
                            title: name.clone(),
                            detail: format!("{words} · {}", size_words(rect)),
                            label: if theirs.len() > 1 {
                                format!("{name} · {}", words.to_lowercase())
                            } else {
                                name.clone()
                            },
                        });
                        coloured.push(Coloured { id: hex(peer), display, colour });
                    }
                }
            }
            screens.push(list);
        }
        NamedDesk { desk, ids, screens, coloured }
    }

    /// Works the guides out again, and shows or sends whichever changed.
    fn refresh_guides(&mut self, now: u64) {
        let NamedDesk { desk, ids, screens, coloured } = self.named_desk();
        let guides = align::guides(&desk, &screens);
        let Some(leading) = &mut self.leading else { return };
        leading.next = now + GUIDES_EVERY;
        leading.coloured = coloured;
        let mut local = None;
        let mut outgoing: Vec<(Id, Vec<Guide>)> = Vec::new();
        for ((computer, id), guides) in desk.iter().zip(&ids).zip(guides) {
            match id {
                None => {
                    let shown: Vec<Shown> = guides
                        .into_iter()
                        .filter_map(|guide| {
                            let rect = *computer.displays.get(usize::from(guide.display))?;
                            Some(Shown { rect, guide })
                        })
                        .collect();
                    if shown != leading.local {
                        leading.local = shown.clone();
                        local = Some(shown);
                    }
                }
                Some(peer) => {
                    let aligns = self.peers.get(peer).is_some_and(|p| p.aligns);
                    if aligns && leading.sent.get(peer) != Some(&guides) {
                        outgoing.push((*peer, guides));
                    }
                }
            }
        }
        // Computers no longer on the desk take theirs down.
        let gone: Vec<Id> = leading.sent.keys().filter(|id| !ids.contains(&Some(**id))).copied().collect();
        for id in &gone {
            leading.sent.remove(id);
        }
        if outgoing.is_empty() && gone.is_empty() && local.is_none() {
            return;
        }
        leading.serial = leading.serial.wrapping_add(1);
        let serial = leading.serial;
        for (peer, guides) in &outgoing {
            leading.sent.insert(*peer, guides.clone());
        }
        if let Some(shown) = local {
            self.displays.guide(shown);
        }
        for id in gone {
            if let Some(link) = self.sessions.get_mut(&id).and_then(Session::live) {
                link.queue(Message::Guided { serial, displays: Vec::new() });
            }
        }
        for (peer, guides) in outgoing {
            let Some(link) = self.sessions.get_mut(&peer).and_then(Session::live) else { continue };
            let displays = guides.iter().map(|g| g.display).collect();
            for guide in guides {
                link.queue(Message::Guide { serial, guide });
            }
            link.queue(Message::Guided { serial, displays });
        }
        self.flush(now, true);
    }

    /// The cursor is at `at` on `on` (none: this computer), in its units: marks where it would
    /// cross from there, on whichever computers' screens they go.
    pub(super) fn line_up_marks(&mut self, on: Option<Id>, at: [f64; 2], now: u64) {
        let Some(leading) = &mut self.leading else { return };
        if now < leading.marks_at {
            leading.pending = Some((on, at));
            return;
        }
        leading.marks_at = now + MARKS_EVERY;
        leading.pending = None;
        let NamedDesk { desk, ids, screens, .. } = self.named_desk();
        let Some(index) = ids.iter().position(|id| *id == on) else { return };
        let found = align::marks(&desk, &screens, index, at, align::REACH);
        let mut wanted: HashMap<Option<Id>, Vec<Mark>> = HashMap::new();
        for (computer, mark) in found {
            if let Some(id) = ids.get(computer) {
                wanted.entry(*id).or_default().push(mark);
            }
        }
        let Some(leading) = &mut self.leading else { return };
        let mut changes: Vec<(Option<Id>, Vec<Mark>)> = Vec::new();
        let everyone: HashSet<Option<Id>> = wanted.keys().chain(leading.marks.keys()).copied().collect();
        for id in everyone {
            let now_marks = wanted.remove(&id).unwrap_or_default();
            let before = leading.marks.get(&id).cloned().unwrap_or_default();
            if !same_marks(&before, &now_marks) {
                changes.push((id, now_marks));
            }
        }
        if changes.is_empty() {
            return;
        }
        for (id, marks) in &changes {
            if marks.is_empty() {
                leading.marks.remove(id);
            } else {
                leading.marks.insert(*id, marks.clone());
            }
        }
        let rects: Vec<Rect> = desk.first().map(|me| me.displays.clone()).unwrap_or_default();
        for (id, marks) in changes {
            match id {
                None => {
                    let marked = marks
                        .into_iter()
                        .filter_map(|mark| Some(Marked { rect: *rects.get(usize::from(mark.display))?, mark }))
                        .collect();
                    self.displays.mark(marked);
                }
                Some(peer) => {
                    if self.peers.get(&peer).is_some_and(|p| p.aligns)
                        && let Some(link) = self.sessions.get_mut(&peer).and_then(Session::live)
                    {
                        link.queue(Message::Marks(marks));
                    }
                }
            }
        }
        self.flush(now, false);
    }

    // ------------------------------------------------------------------ showing another's

    /// A guide, a finished set of them, or marks, from the computer controlling this one.
    pub(super) fn guide_message(&mut self, from: Id, message: Message) {
        let following = self.following.get_or_insert_with(Following::default);
        if following.from != Some(from) {
            *following = Following { from: Some(from), ..Following::default() };
        }
        match message {
            Message::Guide { serial, guide } => {
                following.pending.retain(|(s, g)| *s == serial && g.display != guide.display);
                following.pending.push((serial, guide));
            }
            Message::Guided { serial, displays } => {
                let pending = std::mem::take(&mut following.pending);
                following.shown = pending
                    .into_iter()
                    .filter(|(s, g)| *s == serial && displays.contains(&g.display))
                    .map(|(_, g)| g)
                    .collect();
                if following.shown.is_empty() {
                    following.marks.clear();
                    following.hidden = false;
                }
                let words = if following.shown.is_empty() { "down" } else { "up" };
                let count = following.shown.len();
                self.note(&format!("{}'s line-up guides {words} on {count} screen(s)", self.who(from)));
                self.show_followed(true);
            }
            Message::Marks(marks) => {
                following.marks = marks;
                self.show_followed(false);
            }
            _ => {}
        }
    }

    /// Takes down the guides being followed, until the line-up is over and starts again.
    pub(super) fn hide_followed(&mut self) {
        let Some(following) = &mut self.following else { return };
        following.hidden = true;
        self.note("line-up guides hidden here");
        self.displays.guide(Vec::new());
        self.displays.mark(Vec::new());
    }

    /// Puts the guides (when `guides`) and marks being followed on this computer's screens.
    fn show_followed(&mut self, guides: bool) {
        let (screens, _) = self.real_screens();
        let Some(following) = &mut self.following else { return };
        if following.hidden {
            return;
        }
        let rect = |display: u8| screens.get(usize::from(display)).copied();
        if guides {
            following.screens = screens.clone();
            let shown = following
                .shown
                .iter()
                .filter_map(|g| Some(Shown { rect: rect(g.display)?, guide: g.clone() }))
                .collect();
            self.displays.guide(shown);
        }
        let marked = following.marks.iter().filter_map(|m| Some(Marked { rect: rect(m.display)?, mark: *m })).collect();
        self.displays.mark(marked);
    }

    /// Takes down guides put up by `id`, when its session is over (none: whoever's they are).
    pub(super) fn unfollow(&mut self, id: Option<Id>) {
        let Some(following) = &self.following else { return };
        if id.is_some() && following.from != id {
            return;
        }
        let was_up = !following.shown.is_empty();
        self.following = None;
        if was_up {
            self.displays.guide(Vec::new());
            self.displays.mark(Vec::new());
        }
    }

    /// This computer's screens changed: guides being followed move with them.
    pub(super) fn followed_screens_changed(&mut self) {
        let (screens, _) = self.real_screens();
        if self.following.as_ref().is_some_and(|f| !f.shown.is_empty() && f.screens != screens) {
            self.show_followed(true);
        }
    }

    /// A session ended: its guides come down, and the line-up stops sending it any.
    pub(super) fn line_up_session_ended(&mut self, id: Id) {
        self.unfollow(Some(id));
        if let Some(leading) = &mut self.leading {
            leading.sent.remove(&id);
            leading.marks.remove(&Some(id));
            leading.next = 0;
        }
    }
}

/// Whether two sets of marks look the same: the same screens and edges, and within a pixel or
/// so along them.
fn same_marks(a: &[Mark], b: &[Mark]) -> bool {
    a.len() == b.len()
        && a.iter().zip(b).all(|(a, b)| {
            a.display == b.display && a.edge == b.edge && a.arrives == b.arrives && (a.along - b.along).abs() < 0.0004
        })
}
