use std::sync::Arc;

use crate::find::asking::Asker;
use crate::find::context::Context;
use crate::find::key::key_of;
use crate::find::kind::Kind;
use crate::find::model::tier;
use crate::find::os::raw;
use crate::find::scan::Listing;
use crate::find::shared::{Ask, Order};

use super::engine::{NOW, bench, listing, quiet};

const MENU: [&str; 6] = ["ProgramData", "Microsoft", "Windows", "Start Menu", "Programs", "TreeSize Free"];
const SHORTCUT: &str = "TreeSize Free.lnk";
const SCREENSHOTS: usize = 25;
const SYSTEM: u8 = 1;

fn taken_just_now(mut listing: Listing) -> Listing {
    listing.rows.iter_mut().for_each(|row| (row.modified, row.created) = (NOW, NOW));
    listing
}

#[test]
fn start_menu_shortcut_leads() {
    let mut bench = bench("start-menu", quiet(), false);
    let mut asker = Asker::new(bench.find.shared.clone(), Some(bench.disk.dir().to_path_buf()));
    bench.writer.launch();
    let ticket = bench.scanner.take().first().expect("the launch scans the root").ticket;
    let disk = bench.disk.dir().to_path_buf();
    let plan = bench.find.shared.gate.read().expect("nobody panicked holding the plan").clone();
    let shots: Vec<String> = (0..SCREENSHOTS).map(|shot| format!("shot-{shot:04}TreeSize_Free.png")).collect();
    let shots: Vec<(&str, u8)> = shots.iter().map(|name| (name.as_str(), 0)).collect();
    let mut listings = vec![
        listing(&disk, &[(MENU[0], raw::DIR | raw::HIDDEN), ("Screenshots", raw::DIR)], ticket),
        taken_just_now(listing(&disk.join("Screenshots"), &shots, ticket)),
    ];
    let mut dir = disk.clone();
    for (step, name) in MENU.iter().enumerate() {
        dir.push(name);
        let below = MENU.get(step + 1).map_or((SHORTCUT, 0), |folder| (*folder, raw::DIR));
        listings.push(Listing { tier: plan.inside(&dir), ..listing(&dir, &[below], ticket) });
    }
    bench.writer.obey(Order::Listed(listings));
    bench.writer.obey(Order::Finished(ticket));
    let shortcut = dir.join(SHORTCUT);
    let listed = bench.find.shared.reading().resolve(&shortcut).expect("the shortcut has a row");
    assert_eq!(tier(bench.find.shared.reading().flags(listed)), SYSTEM, "it lies below ProgramData");

    let context = Context::at(NOW, 0, None);
    bench.find.shown(context.clone());
    for text in ["treesize free", "TreeSize  Free ", "treesizefree", "treesize"] {
        let generation = bench.find.ask(text);
        asker.answer(&Ask { generation, text: Arc::from(text), context: context.clone() });
        let answer = bench.find.take().expect("the question was answered");
        assert_eq!(&*answer.query, text, "the answer names the typed text byte for byte");
        let first = &answer.rows[0];
        assert_eq!((&*first.name, first.app, first.kind), ("TreeSize Free", true, Kind::App), "{text}");
        assert_eq!((&*first.path, first.key), (shortcut.as_path(), key_of(&shortcut).0), "{text}");
        assert!(answer.rows[1..].iter().all(|row| !row.app), "{text}: its folder and the pictures are files");
        assert!(answer.rows.iter().any(|row| row.name.ends_with(".png")), "{text}");
    }
}
