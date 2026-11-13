//! Runs an operation on the selected items. Each writes new files beside its sources, never touching the originals.

mod audio;
mod combine;
mod compress;
mod convert;
mod outputs;
mod split;
mod tally;
#[cfg(test)]
mod tests;
mod unique;
mod video;

use std::path::{Path, PathBuf};

use super::names::couldnt_read;
use super::{Kind, Notice, Op, Task};
use tally::Summary;

pub(super) use outputs::Outputs;

pub(super) struct Source {
    pub(super) path: PathBuf,
    pub(super) kind: Kind,
}

pub(super) struct Outcome {
    pub(super) outputs: Vec<PathBuf>,
    pub(super) notice: Notice,
}

impl Outcome {
    pub(super) fn failed(text: impl Into<String>) -> Self {
        Self { outputs: Vec::new(), notice: Notice::failed(text) }
    }

    fn one(output: PathBuf, text: String) -> Self {
        Self { outputs: vec![output], notice: Notice::ok(text) }
    }
}

pub(super) fn run(op: Op, sources: &[Source], outputs: &Outputs, task: &mut Task) -> Outcome {
    match op {
        Op::Convert(format) => {
            tally::each_file(sources, task, Summary::Converted, |source, _| convert::one(source, format, outputs))
        }
        Op::Encode(clip) => tally::each_file(sources, task, Summary::Converted, |source, task| match source.kind {
            Kind::Audio => audio::encode(source, clip, outputs, task),
            _ => video::encode(source, clip, outputs, task),
        }),
        Op::Compress => {
            tally::each_file(sources, task, Summary::Compressed, |source, task| compress::one(source, outputs, task))
        }
        Op::Combine => combine::run(sources, outputs, &mut |fraction| task.report(fraction)),
        Op::Split => split::run(sources, outputs, &mut |fraction| task.report(fraction)),
    }
}

fn file_size(path: &Path) -> Result<u64, String> {
    std::fs::metadata(path).map(|m| m.len()).map_err(|_| couldnt_read(path))
}
