//! Walks the references inside a PDF object.

use std::collections::HashSet;

use lopdf::{Document, Object, ObjectId};

pub(super) fn each_ref(object: &Object, visit: &mut impl FnMut(ObjectId)) {
    let mut pending = vec![object];
    while let Some(object) = pending.pop() {
        match object {
            Object::Reference(id) => visit(*id),
            Object::Array(items) => pending.extend(items.iter()),
            Object::Dictionary(dict) => pending.extend(dict.iter().map(|(_, value)| value)),
            Object::Stream(stream) => pending.extend(stream.dict.iter().map(|(_, value)| value)),
            _ => {}
        }
    }
}

pub(super) fn each_ref_mut(object: &mut Object, visit: &mut impl FnMut(&mut Object)) {
    let mut pending = vec![object];
    while let Some(object) = pending.pop() {
        match object {
            Object::Reference(_) => visit(object),
            Object::Array(items) => pending.extend(items.iter_mut()),
            Object::Dictionary(dict) => pending.extend(dict.iter_mut().map(|(_, value)| value)),
            Object::Stream(stream) => pending.extend(stream.dict.iter_mut().map(|(_, value)| value)),
            _ => {}
        }
    }
}

fn each_ref_in_document(doc: &mut Document, visit: &mut impl FnMut(&mut Object)) {
    for object in doc.objects.values_mut() {
        each_ref_mut(object, visit);
    }
    for (_, value) in doc.trailer.iter_mut() {
        each_ref_mut(value, visit);
    }
}

pub(super) fn null_dangling_refs(doc: &mut Document) {
    let existing: HashSet<ObjectId> = doc.objects.keys().copied().collect();
    each_ref_in_document(doc, &mut |reference| {
        if let Object::Reference(id) = reference
            && !existing.contains(id)
        {
            *reference = Object::Null;
        }
    });
}

pub(super) fn shift_ids(doc: &mut Document, by: u32) -> Option<()> {
    let highest = doc.objects.keys().map(|id| id.0).max().unwrap_or(0).max(doc.max_id);
    highest.checked_add(by)?;
    let objects = std::mem::take(&mut doc.objects);
    doc.objects =
        objects.into_iter().map(|((number, generation), object)| ((number + by, generation), object)).collect();
    each_ref_in_document(doc, &mut |reference| {
        if let Object::Reference((number, _)) = reference {
            *number = number.saturating_add(by);
        }
    });
    doc.max_id = highest + by;
    Some(())
}
