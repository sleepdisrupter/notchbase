//! Splits a PDF into one-page documents, each keeping only what its page uses.

use std::collections::HashSet;
use std::path::Path;

use lopdf::{Document, Object, ObjectId, dictionary};

use super::graph::{each_ref, each_ref_mut};
use super::load;
use super::page_tree::{distinct_pages, flattened_page};
use super::save::save;

pub(in crate::shelf) struct Pages {
    doc: Document,
    ids: Vec<ObjectId>,
}

impl Pages {
    pub(in crate::shelf) fn open(path: &Path) -> Result<Self, String> {
        let doc = load(path)?;
        let ids = distinct_pages(&doc);
        Ok(Self { doc, ids })
    }

    pub(in crate::shelf) fn len(&self) -> usize {
        self.ids.len()
    }

    pub(in crate::shelf) fn extract(&self, index: usize) -> Result<Vec<u8>, String> {
        let couldnt_extract = || format!("Couldn't extract page {}", index + 1);
        let page_id = *self.ids.get(index).ok_or_else(couldnt_extract)?;
        let mut page = flattened_page(&self.doc, page_id).ok_or_else(couldnt_extract)?;
        page.remove(b"Parent");
        let other_pages: HashSet<ObjectId> = self.ids.iter().copied().filter(|&id| id != page_id).collect();
        let mut page = Object::Dictionary(page);
        unlink_pages(&mut page, &other_pages);

        let mut out = Document::with_version(self.doc.version.clone());
        self.copy_reachable(&page, page_id, &other_pages, &mut out);
        out.max_id = self.doc.objects.keys().map(|id| id.0).max().unwrap_or(0).max(self.doc.max_id);
        let page_tree = out.new_object_id();
        if let Object::Dictionary(dict) = &mut page {
            dict.set("Parent", page_tree);
        }
        out.objects.insert(page_id, page);
        let tree = dictionary! { "Type" => "Pages", "Kids" => vec![page_id.into()], "Count" => 1 };
        out.objects.insert(page_tree, tree.into());
        let catalog = out.add_object(dictionary! { "Type" => "Catalog", "Pages" => page_tree });
        out.trailer.set("Root", catalog);
        save(&mut out).map_err(|_| couldnt_extract())
    }

    fn copy_reachable(&self, from: &Object, page_id: ObjectId, other_pages: &HashSet<ObjectId>, out: &mut Document) {
        let mut pending = Vec::new();
        each_ref(from, &mut |id| pending.push(id));
        let mut copied = HashSet::from([page_id]);
        while let Some(id) = pending.pop() {
            if !copied.insert(id) {
                continue;
            }
            let Ok(object) = self.doc.get_object(id) else { continue };
            let mut object = object.clone();
            unlink_pages(&mut object, other_pages);
            each_ref(&object, &mut |reference| {
                if !copied.contains(&reference) {
                    pending.push(reference);
                }
            });
            out.objects.insert(id, object);
        }
    }
}

fn unlink_pages(object: &mut Object, pages: &HashSet<ObjectId>) {
    each_ref_mut(object, &mut |reference| {
        if let Object::Reference(id) = reference
            && pages.contains(id)
        {
            *reference = Object::Null;
        }
    });
}
