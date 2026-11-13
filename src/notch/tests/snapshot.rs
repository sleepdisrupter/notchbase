use egui::vec2;

use crate::notch::Notch;
use crate::notch::geometry::CANVAS;
use crate::notch::preview::{Preview, sample_thumbnail};
use crate::platform::Style;
use crate::tools::shortcuts::ShortcutView;
use crate::ui::snapshot::{Atlas, folder, rasterize};

use super::fixtures::{Rig, shortcuts};

pub(in crate::notch) const SCALE: f32 = 2.0;
const FRAMES: usize = 40;
const REBINDING: &str = "shortcut";
const SHELF_FILES: &str = "NOTCHBASE_SHELF_FILES";
const POINTER: &str = "NOTCHBASE_SNAPSHOT_POINTER";
const TOOLS: &str = "NOTCHBASE_SNAPSHOT_TOOLS";

impl Rig {
    pub(in crate::notch) fn shoot(&self, n: &mut Notch, keys: &[ShortcutView], frames: usize) -> image::RgbImage {
        self.ctx.set_pixels_per_point(SCALE);
        let mut atlas = Atlas::default();
        let mut primitives = Vec::new();
        for frame in 0..frames {
            let events = pointer().map(egui::Event::PointerMoved).into_iter().collect();
            let (_, mut output) = self.run(n, keys, events, 1.0 / 60.0);
            atlas.keep(&mut output);
            if frame + 1 == frames {
                primitives = self.ctx.tessellate(std::mem::take(&mut output.shapes), output.pixels_per_point);
            }
        }
        rasterize(&primitives, &atlas, CANVAS, SCALE).over_desk()
    }
}

fn pointer() -> Option<egui::Pos2> {
    let text = std::env::var(POINTER).ok()?;
    let (x, y) = text.split_once(',')?;
    Some(egui::pos2(x.trim().parse().ok()?, y.trim().parse().ok()?))
}

fn tools() -> Option<usize> {
    std::env::var(TOOLS).ok()?.trim().parse().ok()
}

fn chosen() -> Vec<String> {
    let names = std::env::var("NOTCHBASE_SNAPSHOT").unwrap_or_else(|_| "expanded,media,playing,copied".into());
    names.split(',').map(|name| name.trim().to_lowercase()).filter(|name| !name.is_empty()).collect()
}

#[test]
#[ignore = "writes PNGs of the notch to target/snapshots; run it by name to look at a change"]
fn snapshots() {
    let dir = folder();
    for name in chosen() {
        let keys = shortcuts((name == REBINDING).then_some(1));
        let preview = Preview::parse(&name).unwrap_or_else(|| panic!("no preview called {name}"));
        for (style, tag) in [(Style::Notch, "notch"), (Style::Island, "island")] {
            let mut rig = Rig::with_every_tile();
            if let Some(count) = tools() {
                rig.tools.truncate(count);
            }
            if let Some(files) = std::env::var_os(SHELF_FILES) {
                rig.shelf.add(std::env::split_paths(&files).collect());
            }
            if preview == Preview::Shot {
                rig.images.show_sample(sample_thumbnail());
            }
            let mut n = Notch::with_preview(style, Some(vec2(185.0, 32.0)), Some(preview));
            let shot = rig.shoot(&mut n, &keys, FRAMES);
            shot.save(dir.join(format!("{name}-{tag}.png"))).expect("a written snapshot");
        }
    }
}

#[test]
#[ignore = "writes PNGs of Find answering from a copy of a real index; see find::tests::real for what it needs"]
fn find_answers_from_a_real_index() {
    use std::sync::Arc;

    use crate::bridge::{FindRow, FindState, FindView};

    const ICON_PX: u32 = 64;
    let dir = folder();
    crate::find::tests::real::each_answer(|told| {
        let answer = told.answer;
        let pictured =
            |row: &FindRow| FindRow { icon: crate::filedrag::icon(&row.path, ICON_PX).map(Arc::new), ..row.clone() };
        let view = FindView {
            state: FindState::Ready,
            answered: 1,
            query: answer.query.clone(),
            rows: answer.rows.iter().map(pictured).collect(),
            reading: answer.reading.clone(),
            more: answer.more,
            entries: u32::try_from(told.entries).unwrap_or(u32::MAX),
            ..FindView::default()
        };
        let rig = Rig::with_every_tile();
        let mut n = Notch::with_preview(Style::Island, Some(vec2(185.0, 32.0)), Some(Preview::FindResults));
        n.pages.pin_find(view, None);
        let shot = rig.shoot(&mut n, &shortcuts(None), FRAMES);
        let slug: String =
            answer.query.chars().map(|typed| if typed.is_alphanumeric() { typed } else { '-' }).collect();
        shot.save(dir.join(format!("find-real-{slug}.png"))).expect("a written snapshot");
    });
}
