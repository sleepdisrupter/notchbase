//! Builds one PDF from images and PDFs, moving each document's pages over with everything they refer to.

use std::path::Path;

use lopdf::{Dictionary, Document, Object, ObjectId, dictionary};

use super::graph::shift_ids;
use super::image_page::image_page;
use super::load;
use super::page_tree::{distinct_pages, flattened_page};
use super::save::save;
use crate::shelf::Kind;
use crate::shelf::names::{couldnt_read, file_name};
use crate::shelf::ops::Source;

const PROGRESS_BEFORE_SAVING: f32 = 0.9;

pub(in crate::shelf) fn combine(sources: &[Source], progress: &mut dyn FnMut(f32)) -> Result<(Vec<u8>, u32), String> {
    let mut out = Document::with_version("1.7");
    let page_tree = out.new_object_id();
    let mut pages = Vec::new();
    for (done, source) in sources.iter().enumerate() {
        match source.kind {
            Kind::Pdf => move_pages(&mut out, &source.path, page_tree, &mut pages)?,
            Kind::Image => pages.push(image_page(&mut out, &source.path, page_tree)?.into()),
            Kind::Video | Kind::Audio | Kind::Folder | Kind::Other => {
                return Err(format!("Can't combine {}", file_name(&source.path)));
            }
        }
        progress(PROGRESS_BEFORE_SAVING * (done + 1) as f32 / sources.len() as f32);
    }
    let page_count = pages.len() as u32;
    let tree = dictionary! { "Type" => "Pages", "Kids" => pages, "Count" => page_count };
    out.objects.insert(page_tree, tree.into());
    let catalog = out.add_object(dictionary! { "Type" => "Catalog", "Pages" => page_tree });
    out.trailer.set("Root", catalog);
    let bytes = save(&mut out).map_err(|_| "Couldn't write the combined PDF".to_owned())?;
    Ok((bytes, page_count))
}

fn move_pages(out: &mut Document, path: &Path, page_tree: ObjectId, pages: &mut Vec<Object>) -> Result<(), String> {
    let mut doc = load(path)?;
    shift_ids(&mut doc, out.max_id + 1).ok_or_else(|| couldnt_read(path))?;
    let standalone_pages: Vec<(ObjectId, Dictionary)> =
        distinct_pages(&doc).into_iter().filter_map(|id| Some((id, flattened_page(&doc, id)?))).collect();
    if standalone_pages.is_empty() {
        return Err(couldnt_read(path));
    }
    let highest_id = doc.objects.keys().map(|id| id.0).max().unwrap_or(0).max(doc.max_id);
    out.objects.extend(doc.objects);
    for (id, mut page) in standalone_pages {
        page.set("Parent", page_tree);
        out.objects.insert(id, page.into());
        pages.push(id.into());
    }
    out.max_id = out.max_id.max(highest_id);
    Ok(())
}
