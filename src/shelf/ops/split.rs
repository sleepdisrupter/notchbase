//! Split: one PDF into a folder with a file for each page.

use std::fs;
use std::path::{Path, PathBuf};

use super::unique::write_new_file;
use super::{Outcome, Outputs, Source};
use crate::shelf::names::{counted, file_name, file_stem};
use crate::shelf::{Kind, pdf};

pub(super) fn run(sources: &[Source], outputs: &Outputs, progress: &mut dyn FnMut(f32)) -> Outcome {
    match split(sources, outputs, progress) {
        Ok((folder, pages)) => Outcome::one(folder, format!("Split into {}", counted(pages as u64, "page"))),
        Err(text) => Outcome::failed(text),
    }
}

fn split(sources: &[Source], outputs: &Outputs, progress: &mut dyn FnMut(f32)) -> Result<(PathBuf, usize), String> {
    let [source] = sources else {
        return Err("Split one PDF at a time".to_owned());
    };
    if source.kind != Kind::Pdf {
        return Err(format!("Can't split {}", file_name(&source.path)));
    }
    let pages = pdf::Pages::open(&source.path)?;
    let stem = file_stem(&source.path);
    let folder = outputs.create_dir(&source.path, &format!("{stem} pages"))?;
    if let Err(text) = write_pages(&pages, &folder, &stem, progress) {
        let _ = fs::remove_dir_all(&folder);
        return Err(text);
    }
    Ok((folder, pages.len()))
}

fn write_pages(pages: &pdf::Pages, folder: &Path, stem: &str, progress: &mut dyn FnMut(f32)) -> Result<(), String> {
    for index in 0..pages.len() {
        let page_stem = format!("{stem} {}", index + 1);
        let bytes = pages.extract(index)?;
        write_new_file(folder, &page_stem, "pdf", &bytes).map_err(|_| format!("Couldn't save {page_stem}.pdf"))?;
        progress((index + 1) as f32 / pages.len() as f32);
    }
    Ok(())
}
