//! Transcoding with an export session, trying the presets from best to most compatible.

use std::path::Path;

use block2::RcBlock;
use objc2::AllocAnyThread as _;
use objc2::rc::{Retained, autoreleasepool};
use objc2::runtime::Bool;
use objc2_av_foundation::{AVAsset, AVAssetExportSession, AVFileType};
use objc2_foundation::NSURL;

use super::super::{Container, MediaError, Quality};
use super::asset::{self, Media};
use super::diagnose;
use super::presets::{self, Preset};
use super::session;
use super::wait;
use crate::shelf::Task;

const ACTION: &str = "convert";

pub(super) fn transcode(
    source: &Path,
    target: &Path,
    container: Container,
    quality: Quality,
    task: &mut Task,
) -> Result<(), MediaError> {
    autoreleasepool(|_| {
        let asset = asset::open(source).ok_or(MediaError::Unsupported)?;
        let output = NSURL::from_file_path(target).ok_or(MediaError::Failed)?;
        let file_type = presets::file_type(container).ok_or(MediaError::Unsupported)?;
        asset::readable(&asset).map_err(|error| diagnose::failure(ACTION, source, error.as_deref()))?;
        if container.is_audio_only() {
            ensure_audio(&asset, source)?;
        }
        let session = export_session(&asset, source, presets::candidates(container, quality), file_type)?;
        unsafe {
            session.setOutputURL(Some(&output));
            session.setOutputFileType(Some(file_type));
            session.setShouldOptimizeForNetworkUse(true);
        }
        session::run(&session, source, task)
    })
}

fn ensure_audio(asset: &AVAsset, source: &Path) -> Result<(), MediaError> {
    match asset::tracks(asset, Media::Audio) {
        Ok(tracks) if tracks.count() > 0 => Ok(()),
        Ok(_) => Err(MediaError::NoAudio),
        Err(error) => Err(diagnose::failure(ACTION, source, error.as_deref())),
    }
}

fn export_session(
    asset: &AVAsset,
    source: &Path,
    candidates: &[Preset],
    file_type: &AVFileType,
) -> Result<Retained<AVAssetExportSession>, MediaError> {
    let unsupported = || diagnose::refusal(ACTION, source, "no export preset fits it", MediaError::Unsupported);
    let preset =
        candidates.iter().copied().find(|&preset| compatible(preset, asset, file_type)).ok_or_else(unsupported)?;
    unsafe { AVAssetExportSession::initWithAsset_presetName(AVAssetExportSession::alloc(), asset, preset.name()) }
        .ok_or_else(unsupported)
}

fn compatible(preset: Preset, asset: &AVAsset, file_type: &AVFileType) -> bool {
    wait::reply(|sender| {
        let on_answer = RcBlock::new(move |compatible: Bool| {
            let _ = sender.send(compatible.as_bool());
        });
        unsafe {
            AVAssetExportSession::determineCompatibilityOfExportPreset_withAsset_outputFileType_completionHandler(
                preset.name(),
                asset,
                Some(file_type),
                &on_answer,
            );
        }
    })
    .unwrap_or(false)
}
