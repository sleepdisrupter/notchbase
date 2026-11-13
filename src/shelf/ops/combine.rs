//! Combine: images and PDFs into one PDF, in the order they were picked.

use std::path::PathBuf;

use super::{Outcome, Outputs, Source};
use crate::shelf::names::{counted, file_name, file_stem};
use crate::shelf::{Kind, pdf};

const COMBINED_STEM: &str = "Combined";

pub(super) fn run(sources: &[Source], outputs: &Outputs, progress: &mut dyn FnMut(f32)) -> Outcome {
    match combine(sources, outputs, progress) {
        Ok((path, pages)) => {
            let text = format!("Saved {} · {}", file_name(&path), counted(pages.into(), "page"));
            Outcome::one(path, text)
        }
        Err(text) => Outcome::failed(text),
    }
}

fn combine(sources: &[Source], outputs: &Outputs, progress: &mut dyn FnMut(f32)) -> Result<(PathBuf, u32), String> {
    let first = sources.first().ok_or("Nothing to combine")?;
    let (bytes, pages) = pdf::combine(sources, progress)?;
    let path = outputs.write(&first.path, &output_stem(sources), "pdf", &bytes)?;
    progress(1.0);
    Ok((path, pages))
}

fn output_stem(sources: &[Source]) -> String {
    match sources {
        [only] if only.kind == Kind::Image => file_stem(&only.path),
        _ => COMBINED_STEM.to_owned(),
    }
}
