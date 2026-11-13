//! How well an app's name answers a question: by its shown name, its words, its initials and its words run together.

use std::ops::Range;

use crate::find::apps::{Card, Used, mistyped};
use crate::find::model::{EntryId, Live};
use crate::find::query::{Fit, Ground, Term, squeezed, trace};
use crate::find::weights::{
    APP, APP_ALIAS, APP_BACKSTAGE, APP_RUNNING, APP_WORD, BOUNDARY, RUN, TIER_FUZZY, TIER_KIND, TIER_WORD,
};

use super::launch::{extra, homed, pane, shown, shown_as, stands_alone};
use super::place::newness;
use super::short::Scored;
use super::text::{self, Graded, boundary, cover, heads, initials};
use super::words::SEVERAL;

const INITIALS_FROM: usize = 2;
const LOOSE_FROM: usize = 3;

struct Voiced {
    text: f32,
    fit: Fit,
    crisp: bool,
}

pub fn named(
    ground: &Ground<'_>,
    (id, running): (EntryId, bool),
    card: Option<&Card>,
    original: &mut String,
) -> Option<Scored> {
    let live = ground.live;
    let flags = live.flags(id);
    if unlisted(live, id, flags, card) {
        return None;
    }
    live.name_into(id, original);
    let (file, cased) = (shown(live.folded(id)), shown_as(original));
    let said = match card.and_then(Card::titled).filter(|(title, _)| *title != file) {
        Some((title, shown)) => {
            let by_file = || voiced(ground, id, flags, file, cased).filter(|filed| filed.crisp).map(aliased);
            let filed = if pane(live.folded(id)) { None } else { by_file() };
            better(voiced(ground, id, flags, title, shown), filed)
        }
        None => voiced(ground, id, flags, file, cased),
    }?;
    let backstage = card.is_some_and(|card| card.backstage && card.used == Used::default());
    let familiar = card.map_or(0.0, |card| card.familiar(ground.context.now));
    let known = familiar > 0.0 || homed(live, id, flags);
    let asked_for = said.fit.own != 0 || said.fit.squeezed || ground.reads_apps();
    let ahead = said.crisp && asked_for && known && !backstage && !extra(live.folded(id));
    let modified = live.modified(id);
    let open = if running { APP_RUNNING } else { 0.0 };
    let standing =
        APP + open + newness(ground.context.now, live.created(id)) + if backstage { APP_BACKSTAGE } else { 0.0 };
    Some(Scored { score: said.text + standing + familiar, modified, id, fit: Fit { ahead, ..said.fit } })
}

pub fn lit(ground: &Ground<'_>, fit: &Fit, name: &[u8], cased: &str) -> Vec<Range<usize>> {
    if fit.squeezed {
        let found = ground.query.phrase.as_deref().and_then(|phrase| squeezed(name, phrase));
        return found.map_or_else(Vec::new, |found| found.runs(name));
    }
    let mut ranges = Vec::new();
    for (index, term) in ground.query.terms.iter().enumerate() {
        if let Some(graded) = text::grade(name, term).filter(|_| fit.owns(index)) {
            ranges.push(text::refine(graded, name, cased, term).range(term));
        } else if fit.fuzzy {
            ranges.extend(loose(term, name).into_iter().flat_map(|(_, at)| at).map(|at| at..at + 1));
        } else if !fit.reads(index) && cased.len() == name.len() {
            ranges.extend(heads(cased).take(term.needle.len()).map(|at| at..at + 1));
        }
    }
    ranges
}

fn voiced(ground: &Ground<'_>, id: EntryId, flags: u8, name: &[u8], cased: &str) -> Option<Voiced> {
    let (all, own, read) = (ground.all(), ground.within(id, flags, name), ground.read(id, flags));
    let missing = all & !(own | read);
    let admitted = |term: &Term| term.admits(ground.live, id, flags, ground.home);
    if !ground.query.terms.iter().all(admitted) {
        return None;
    }
    let said = Fit { own, read, app: true, ..Fit::default() };
    let whole = || ground.query.phrase.as_deref().and_then(|phrase| phrased(name, phrase));
    if missing == 0 {
        let (mean, crisp) = spoken(ground, name, cased, &said)?;
        let several = ground.query.terms.len() >= SEVERAL && own == all;
        return Some(match several.then(whole).flatten() {
            Some((phrased, edge)) if phrased > mean => Voiced { text: phrased, fit: said, crisp: edge },
            _ => Voiced { text: mean, fit: said, crisp },
        });
    }
    if let Some((phrased, true)) = whole() {
        return Some(Voiced { text: phrased, fit: Fit { squeezed: true, app: true, ..Fit::default() }, crisp: true });
    }
    let missed = || ground.query.terms.iter().enumerate().filter(|(index, _)| missing & (1 << index) != 0);
    let short = missed().all(|(_, term)| abbreviates(term, name)).then(|| spoken(ground, name, cased, &said));
    if let Some((text, crisp)) = short.flatten() {
        return Some(Voiced { text, fit: said, crisp });
    }
    let text = loosely(ground, name, cased, &said)?;
    Some(Voiced { text, fit: Fit { fuzzy: true, ..said }, crisp: false })
}

fn aliased(filed: Voiced) -> Voiced {
    Voiced { text: filed.text + APP_ALIAS, fit: Fit { alias: true, ..filed.fit }, ..filed }
}

fn better(shown: Option<Voiced>, filed: Option<Voiced>) -> Option<Voiced> {
    match (shown, filed) {
        (Some(shown), Some(filed)) if !shown.crisp && filed.text > shown.text => Some(filed),
        (shown, filed) => shown.or(filed),
    }
}

pub fn veiled(live: &Live, id: EntryId, card: Option<&Card>) -> bool {
    let file = live.folded(id);
    let retitled = card.and_then(Card::titled).is_some_and(|(title, _)| title != shown(file));
    unlisted(live, id, live.flags(id), card) || retitled
}

fn unlisted(live: &Live, id: EntryId, flags: u8, card: Option<&Card>) -> bool {
    let taken_on_trust = || !pane(live.folded(id)) && (stands_alone(flags) || homed(live, id, flags));
    card.map_or_else(|| !taken_on_trust(), |card| !card.listed)
}

fn phrased(name: &[u8], phrase: &[u8]) -> Option<(f32, bool)> {
    let score = text::phrased(name, phrase)?;
    Some((score, squeezed(name, phrase).is_some_and(|found| boundary(name, found.at))))
}

fn spoken(ground: &Ground<'_>, name: &[u8], cased: &str, fit: &Fit) -> Option<(f32, bool)> {
    let terms = &ground.query.terms;
    let (mut total, mut crisp) = (0.0, true);
    for (index, term) in terms.iter().enumerate() {
        let kind = if fit.reads(index) { TIER_KIND } else { 0.0 };
        let graded = fit.owns(index).then(|| text::grade(name, term)).flatten();
        total += match graded.map(|graded| text::refine(graded, name, cased, term)) {
            Some(graded) => {
                crisp &= !graded.loose || fit.reads(index);
                (graded.score + if starts_a_word(&graded, term) { APP_WORD } else { 0.0 }).max(kind)
            }
            None if fit.reads(index) => kind,
            None => abbreviated(term, name, cased)?,
        };
    }
    Some((total / terms.len().max(1) as f32, crisp))
}

fn loosely(ground: &Ground<'_>, name: &[u8], cased: &str, fit: &Fit) -> Option<f32> {
    let terms = &ground.query.terms;
    let mut total = 0.0;
    for (index, term) in terms.iter().enumerate() {
        let kind = if fit.reads(index) { TIER_KIND } else { 0.0 };
        let graded = fit.owns(index).then(|| text::grade(name, term)).flatten();
        total += match graded.map(|graded| text::refine(graded, name, cased, term)) {
            Some(graded) => (graded.score + if starts_a_word(&graded, term) { APP_WORD } else { 0.0 }).max(kind),
            None if fit.reads(index) => kind,
            None => loose(term, name)?.0,
        };
    }
    Some(total / terms.len().max(1) as f32)
}

fn loose(term: &Term, name: &[u8]) -> Option<(f32, Vec<usize>)> {
    if term.needle.len() < LOOSE_FROM || !term.plain() {
        return None;
    }
    let spread = trace(name, &term.needle).filter(|traced| traced.at.first().is_some_and(|at| boundary(name, *at)));
    let slipped = || {
        let typed = term.needle.len();
        let run = RUN * typed.saturating_sub(2) as f32;
        mistyped(&term.needle, name).map(|_| (TIER_FUZZY + BOUNDARY + run + cover(typed, name.len()), Vec::new()))
    };
    spread.map(|traced| (traced.score, traced.at)).or_else(slipped)
}

fn starts_a_word(graded: &Graded, term: &Term) -> bool {
    graded.at > 0 && !graded.loose && !term.strict
}

fn abbreviates(term: &Term, name: &[u8]) -> bool {
    term.needle.len() >= INITIALS_FROM && !term.strict && term.needle.first() == name.first()
}

fn abbreviated(term: &Term, name: &[u8], cased: &str) -> Option<f32> {
    let initials = abbreviates(term, name) && initials(cased).starts_with(&term.needle);
    initials.then(|| TIER_WORD + BOUNDARY + cover(term.needle.len(), name.len()))
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::sync::atomic::AtomicU64;

    use super::*;
    use crate::find::context::{Context, DAY};
    use crate::find::model::{Live, flag, with_tier};
    use crate::find::query::{Stop, parse};
    use crate::find::tests::corpus::{EPOCH, Planted, from_paths, plant};
    use crate::find::weights::{FRESH_NEW, GAP, RUN, SMART_CASE, TIER_EXACT, TIER_INSIDE, TIER_PREFIX};

    const TREESIZE: &str = "/ProgramData/Microsoft/Windows/Start Menu/Programs/TreeSize Free/TreeSize Free.lnk";
    const CODE: &str = "/ProgramData/Microsoft/Windows/Start Menu/Programs/Visual Studio Code.lnk";
    const LONG_AFTER: u32 = EPOCH + 400 * DAY;

    fn disk() -> Live {
        let mut live = from_paths(&["/Applications/TreeSize Free.app/", "/Applications/Google Chrome.app/"]);
        for path in [TREESIZE, CODE] {
            plant(&mut live, &Planted { path, modified: EPOCH, created: EPOCH, flags: with_tier(0, 1) });
        }
        plant(
            &mut live,
            &Planted { path: "/Applications/Safari.app", modified: EPOCH, created: EPOCH, flags: flag::LINK },
        );
        live
    }

    fn weighed(live: &Live, text: &str, path: &str, now: u32) -> Option<Scored> {
        let context = Context::at(now, 0, None);
        let (query, latest) = (parse(text, &context), AtomicU64::new(0));
        let stop = Stop { latest: &latest, mine: 0 };
        let ground = Ground::new(live, &query, &context, None, &stop);
        named(&ground, (live.resolve(Path::new(path)).expect("planted"), false), None, &mut String::new())
    }

    fn scored(typed: &str, path: &str) -> Option<f32> {
        weighed(&disk(), typed, path, LONG_AFTER).map(|found| found.score)
    }

    fn app(text: f32) -> Option<f32> {
        Some(text + APP)
    }

    fn quality(typed: usize, name: usize) -> f32 {
        RUN * (typed - 1) as f32 + cover(typed, name)
    }

    #[test]
    fn app_weighed_by_shown_name() {
        let start = TIER_PREFIX + BOUNDARY + quality(8, 13);
        assert_eq!(scored("treesize", TREESIZE), app(start), "tier one and six folders deep cost nothing");
        assert_eq!(scored("treesize", "/Applications/TreeSize Free.app"), app(start), "the same on both systems");
        let whole = TIER_EXACT + BOUNDARY + quality(6, 6);
        assert_eq!(scored("safari", "/Applications/Safari.app"), app(whole));
        assert_eq!(scored("Safari", "/Applications/Safari.app"), app(whole + SMART_CASE));
        let today = weighed(&disk(), "safari", "/Applications/Safari.app", EPOCH).unwrap();
        assert_eq!(today.score, whole + APP + FRESH_NEW, "an app is new, an update does not make it fresh");
        assert!(today.fit.app && today.fit.own == 1 && today.modified == EPOCH);
        assert_eq!(scored("zebra", TREESIZE), None);
    }

    #[test]
    fn app_word_counts_inside_not() {
        let chrome = "/Applications/Google Chrome.app";
        assert_eq!(scored("chrome", chrome), app(TIER_WORD + BOUNDARY + quality(6, 13) + APP_WORD));
        assert_eq!(scored("c", chrome), app(TIER_WORD + BOUNDARY + quality(1, 13) + APP_WORD));
        assert_eq!(scored("size", TREESIZE), app(TIER_PREFIX + BOUNDARY + quality(4, 13)), "a capital starts a word");
        assert_eq!(scored("oogle", chrome), app(TIER_INSIDE + quality(5, 13)));
        assert_eq!(scored("e", chrome), app(TIER_INSIDE + quality(1, 13)), "no word of it starts with that letter");
        assert_eq!(TIER_WORD + APP_WORD, TIER_PREFIX);
    }

    #[test]
    fn initials_name_app_from_two() {
        let initials = |typed: usize| app(TIER_WORD + BOUNDARY + cover(typed, 18));
        assert_eq!(
            (scored("vsc", CODE), scored("vs", CODE), scored("VSC", CODE)),
            (initials(3), initials(2), initials(3))
        );
        assert_eq!((scored("sc", CODE), scored("vsx", CODE), scored("vc", CODE)), (None, None, None));
        assert_eq!(
            scored("tsf", TREESIZE),
            app(TIER_WORD + BOUNDARY + cover(3, 13)),
            "a capital starts a word here too"
        );
        let (short, word) = (TIER_WORD + BOUNDARY + cover(2, 18), TIER_WORD + BOUNDARY + quality(4, 18) + APP_WORD);
        assert_eq!(scored("vs code", CODE), app((short + word) / 2.0));
        assert_eq!(scored("vs/", CODE), None, "a shortcut is no folder");
    }

    #[test]
    fn joined_words_span_spaces() {
        let whole = TIER_EXACT + BOUNDARY + 11.0 * RUN + GAP + cover(12, 13);
        let squeezed = weighed(&disk(), "treesizefree", TREESIZE, LONG_AFTER).unwrap();
        assert_eq!((Some(squeezed.score), squeezed.fit.squeezed, squeezed.fit.app), (app(whole), true, true));
        for typed in ["treesize free", "TreeSize  Free ", " treesize free", "tree size free", "tree sizefree"] {
            assert_eq!(scored(typed, TREESIZE), app(whole), "{typed}");
        }
        let apart = weighed(&disk(), "treesize free", TREESIZE, LONG_AFTER).unwrap();
        assert!(!apart.fit.squeezed && apart.fit.own == 0b11, "words that are there letter for letter stay words");
        assert_eq!(scored("treesizefre", TREESIZE), app(TIER_PREFIX + BOUNDARY + 10.0 * RUN + GAP + cover(11, 13)));
        assert_eq!(scored("treesizefreeware", TREESIZE), None);
    }

    #[test]
    fn apps_kind_takes_any_app() {
        assert_eq!((scored("apps", TREESIZE), scored("applications", CODE)), (app(TIER_KIND), app(TIER_KIND)));
        let narrowed = (TIER_KIND + (TIER_PREFIX + BOUNDARY + quality(4, 13))) / 2.0;
        assert_eq!(scored("apps tree", TREESIZE), app(narrowed));
        assert_eq!((scored("programs treesize", TREESIZE), scored("*.lnk", TREESIZE)), (None, None));
        assert_eq!(scored("lnk", CODE), app(TIER_KIND), "the ending is read, it is not in the shown name");
        assert_eq!(scored("treesize/", TREESIZE), None, "only folders were asked for");
        assert!(
            scored("treesize/", "/Applications/TreeSize Free.app").is_some() && scored("prog/visual", CODE).is_some()
        );
        assert_eq!(scored("apps/visual", CODE), None, "it does not lie below a folder of that name");
    }

    #[test]
    fn shown_name_matches_light() {
        let live = disk();
        let lit = |typed: &str, path: &str| {
            let context = Context::at(LONG_AFTER, 0, None);
            let (query, latest) = (parse(typed, &context), AtomicU64::new(0));
            let stop = Stop { latest: &latest, mine: 0 };
            let ground = Ground::new(&live, &query, &context, None, &stop);
            let id = live.resolve(Path::new(path)).expect("planted");
            let found = named(&ground, (id, false), None, &mut String::new()).expect("it matches");
            let mut original = String::new();
            live.name_into(id, &mut original);
            let ranges = super::lit(&ground, &found.fit, shown(live.folded(id)), shown_as(&original));
            ranges.iter().map(|lit| (lit.start, lit.end)).collect::<Vec<_>>()
        };
        assert_eq!(lit("treesizefree", TREESIZE), [(0, 8), (9, 13)]);
        assert_eq!(lit("free tree", TREESIZE), [(9, 13), (0, 4)]);
        assert_eq!(lit("vsc", CODE), [(0, 1), (7, 8), (14, 15)]);
        assert_eq!(lit("vs code", CODE), [(0, 1), (7, 8), (14, 18)]);
        assert_eq!(lit("apps visual", CODE), [(0, 6)], "a word that was read lights nothing");
        assert_eq!(lit("size", TREESIZE), [(4, 8)]);
    }
}
