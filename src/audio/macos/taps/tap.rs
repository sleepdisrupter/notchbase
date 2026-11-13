//! One app's running tap: the process tap, its aggregate device and the callback, plus what it covers so a change in
//! the app's processes or the output can be spotted.

use std::collections::HashSet;

use super::TapError;
use super::aggregate::Aggregate;
use super::api::TapApi;
use super::description::TapDescription;
use super::io_proc::RunningIo;
use super::process_tap::ProcessTap;
use crate::audio::core_audio::bundle_ids::owned_bundle_ids;
use crate::audio::core_audio::tap_shared::TapShared;
use crate::audio::macos::devices::Output;
use crate::audio::macos::processes::AppProcesses;
use crate::audio::macos::property::AudioObjectID;
use crate::audio::macos::selectors::SCOPE_INPUT;
use crate::audio::macos::streams::{buffer_count, outputs_float32};

pub struct Tap {
    io: RunningIo,
    processes: HashSet<AudioObjectID>,
    follows: Vec<String>,
    output_uid: String,
}

impl Tap {
    pub fn create(api: TapApi, owner: &AppProcesses, output: Output<'_>, gain: f32) -> Result<Self, TapError> {
        if !outputs_float32(output.device) || output.uid.is_empty() {
            return Err(TapError::Failed);
        }
        let skip = buffer_count(output.device, SCOPE_INPUT);
        let objects: Vec<AudioObjectID> = owner.processes.iter().map(|p| p.object).collect();
        let bundle_ids = owned_bundle_ids(&owner.key, owner.processes.iter().map(|p| p.bundle.as_str()));
        let description =
            TapDescription::new(api.description_class(), &objects, &bundle_ids).ok_or(TapError::Failed)?;
        let process_tap = ProcessTap::create(api, &description)?;
        let aggregate = Aggregate::create(output.uid, process_tap)?;
        let io = RunningIo::start(aggregate, TapShared::new(gain, skip))?;
        let follows = if description.follows_bundles() { bundle_ids } else { Vec::new() };
        Ok(Self { io, processes: objects.into_iter().collect(), follows, output_uid: output.uid.to_owned() })
    }

    pub fn serves(&self, owner: &AppProcesses, output_uid: &str) -> bool {
        self.output_uid == output_uid && owner.processes.iter().all(|p| self.covers(p.object, &p.bundle))
    }

    pub fn set_gain(&self, gain: f32) {
        self.io.shared().set_gain(gain);
    }

    pub fn take_peak(&self) -> f32 {
        self.io.shared().take_peak()
    }

    fn covers(&self, object: AudioObjectID, bundle: &str) -> bool {
        self.processes.contains(&object) || self.follows.iter().any(|b| b.eq_ignore_ascii_case(bundle))
    }
}
