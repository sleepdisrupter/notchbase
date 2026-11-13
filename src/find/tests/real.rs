use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;
use std::time::Duration;

use crate::find::apps::{self, Book};
use crate::find::context::{self, Context};
use crate::find::query::{Scratch, Stop, run};
use crate::find::shared::Ask;
use crate::find::tests::corpus;
use crate::find::usage::Usage;
use crate::find::view::Answer;
use crate::find::{Key, key_of, store};
use crate::platform;

const FOLDER: &str = "NOTCHBASE_FIND_EVAL";
const QUESTIONS: &str = "NOTCHBASE_FIND_ASK";
const SHOWN: &str = "NOTCHBASE_FIND_ROWS";
const ROSTER: &str = "NOTCHBASE_FIND_ROSTER";
const RESOLVE: &str = "NOTCHBASE_FIND_RESOLVE";
const BETWEEN: char = '|';
const ROWS: usize = 8;
const EVERYDAY: &str = "|s|c|sp|zen|chrome|term|code|set|blue|wifi|display|pho|doc|down|desk|read|car|notch|inv|resume|pdf|\
                        screenshot|notes|music|config|index|main|git|.zshrc|package|find|key|logic|pre|act|mail|cal";

pub(crate) struct Told<'a> {
    pub answer: &'a Answer,
    pub entries: usize,
    pub took: Duration,
}

pub(crate) fn each_answer(mut seen: impl FnMut(&Told<'_>)) {
    let dir = PathBuf::from(std::env::var(FOLDER).expect("the folder holding a copy of index.bin and usage.bin"));
    let (mut live, _) = store::load(&dir).expect("a readable index");
    for place in ["/System/Library/ExtensionKit/Extensions", "/System/Library/CoreServices/Applications"] {
        for bundle in std::fs::read_dir(place).into_iter().flatten().flatten().map(|entry| entry.path()) {
            let path = format!("{}/", bundle.display());
            corpus::plant(&mut live, &corpus::Planted { path: &path, modified: 1, created: 1, flags: 0 });
        }
    }
    for app in apps::every() {
        if live.resolve(&app).is_none() && app.is_dir() {
            let path = format!("{}/", app.display());
            let system = crate::find::model::with_tier(0, 1);
            corpus::plant(&mut live, &corpus::Planted { path: &path, modified: 1, created: 1, flags: system });
        }
    }
    let home = std::env::var("HOME").map(PathBuf::from).ok();
    let now = context::now();
    let usage = Usage::load(Some(&dir.join(store::USAGE_FILE)), now);
    let latest = AtomicU64::new(1);
    let stop = Stop { latest: &latest, mine: 1 };
    let mut scratch = Scratch::new(home);
    let running: Arc<[Key]> = platform::running_apps().iter().map(|path| key_of(path)).collect();
    let open = running.len();
    let ask = |text: &str| Ask {
        generation: 1,
        text: Arc::from(text),
        context: Context::at(now, 0, None).among(running.clone()),
    };
    run(&live, &usage, &ask(""), &mut scratch, &stop).expect("the empty list");
    let listed = scratch.launchers.roster(&live);
    let used = apps::used();
    scratch.book = Arc::new(Book::default().revised(&listed, used.as_ref(), apps::read));
    if let Ok(file) = std::env::var(RESOLVE) {
        for path in std::fs::read_to_string(file).expect("a list of paths").lines() {
            let found = live.resolve(std::path::Path::new(path)).map(|id| live.flags(id));
            println!("{}\t{path}", found.map_or("absent".to_owned(), |flags| format!("flags={flags:08b}")));
        }
    }
    if let Ok(file) = std::env::var(ROSTER) {
        let shown = |app: &&apps::Listed| scratch.book.card(app.key).is_some_and(|card| card.listed);
        let listed: Vec<String> = listed.iter().filter(shown).map(|app| app.path.display().to_string()).collect();
        std::fs::write(file, listed.join("\n")).expect("a written roster");
    }
    let known = used.map_or(0, |used| used.len());
    println!("{} entries, {} apps, use known for {known}, {open} open", live.len(), listed.len());
    let asked = std::env::var(QUESTIONS).unwrap_or_else(|_| EVERYDAY.to_owned());
    for text in asked.split(BETWEEN) {
        let started = std::time::Instant::now();
        let answer = run(&live, &usage, &ask(text), &mut scratch, &stop).expect("an answer");
        seen(&Told { answer: &answer, entries: live.len(), took: started.elapsed() });
    }
}

#[test]
#[ignore = "answers everyday questions from a copy of a real index; point NOTCHBASE_FIND_EVAL at the copy's folder"]
fn real_index_answers_questions() {
    each_answer(|told| {
        let answer = told.answer;
        println!("\n[{}] {} rows in {:?}", answer.query, answer.rows.len(), told.took);
        let rows = std::env::var(SHOWN).ok().and_then(|rows| rows.parse().ok()).unwrap_or(ROWS);
        for row in answer.rows.iter().take(rows) {
            let kind = if row.app {
                "app "
            } else if row.folder {
                "dir "
            } else {
                "file"
            };
            println!("  {kind} {:<44} {}", row.name, row.place);
        }
    });
}

#[test]
#[ignore = "audits a copy of a real index against the disk; point NOTCHBASE_FIND_EVAL at the copy's folder"]
fn real_index_matches_disk() {
    use std::collections::BTreeMap;
    use std::path::Path;
    use std::time::UNIX_EPOCH;

    let dir = PathBuf::from(std::env::var(FOLDER).expect("the folder holding a copy of index.bin"));
    let saved = std::fs::metadata(dir.join("index.bin")).and_then(|file| file.modified()).expect("an index file");
    let saved = saved.duration_since(UNIX_EPOCH).map_or(0, |since| since.as_secs());
    let (live, _) = store::load(&dir).expect("a readable index");
    let home = PathBuf::from(std::env::var("HOME").expect("a home"));
    let area = |path: &Path| {
        let below = path.strip_prefix(&home).ok().and_then(|below| below.components().next());
        below.map_or_else(|| "(outside home)".to_owned(), |part| part.as_os_str().to_string_lossy().into_owned())
    };
    let older = |path: &Path| {
        let changed = std::fs::symlink_metadata(path).ok().map(|seen| (seen.modified().ok(), seen.created().ok()));
        let stamp = |at: Option<std::time::SystemTime>| {
            at.and_then(|at| at.duration_since(UNIX_EPOCH).ok()).map_or(0, |since| since.as_secs())
        };
        changed.is_some_and(|(modified, created)| stamp(modified).max(stamp(created)) + 3_600 < saved)
    };
    let (mut walked, mut absent) = (0usize, 0usize);
    let mut missed: BTreeMap<String, (usize, Vec<PathBuf>)> = BTreeMap::new();
    let mut waiting = vec![home.clone()];
    while let Some(folder) = waiting.pop() {
        let Ok(listed) = std::fs::read_dir(&folder) else { continue };
        for entry in listed.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().into_owned();
            let Ok(kind) = entry.file_type() else { continue };
            walked += 1;
            let held = live.resolve(&path).is_some();
            if !held && older(&path) {
                absent += 1;
                let (count, samples) = missed.entry(area(&path)).or_default();
                *count += 1;
                if samples.len() < 6 {
                    samples.push(path.clone());
                }
            }
            let enter = kind.is_dir() && held && !name.starts_with('.') && folder != home.join("Library");
            if enter && path != home.join("Library") {
                waiting.push(path);
            }
        }
    }
    println!("walked {walked} items below home (entering only folders the index holds), {absent} absent from it");
    for (area, (count, samples)) in &missed {
        println!("  {count:>7}  {area}");
        samples.iter().for_each(|path| println!("             {}", path.display()));
    }
    let (mut checked, mut stale) = (0usize, 0usize);
    let mut gone: BTreeMap<String, (usize, Vec<PathBuf>)> = BTreeMap::new();
    let mut path = PathBuf::new();
    for id in 0..live.len() as u32 {
        if live.is_dead(id) {
            continue;
        }
        live.path_into(id, &mut path);
        if !path.starts_with(&home) || path.starts_with(home.join("Library")) {
            continue;
        }
        checked += 1;
        if std::fs::symlink_metadata(&path).is_err() {
            stale += 1;
            let (count, samples) = gone.entry(area(&path)).or_default();
            *count += 1;
            if samples.len() < 6 {
                samples.push(path.clone());
            }
        }
    }
    println!("checked {checked} entries below home, {stale} no longer on the disk");
    for (area, (count, samples)) in &gone {
        println!("  {count:>7}  {area}");
        samples.iter().for_each(|path| println!("             {}", path.display()));
    }
}
