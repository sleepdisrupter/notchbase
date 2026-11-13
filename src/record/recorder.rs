//! The recorder the app holds. It picks the file name, starts a platform session and reports when the file is
//! saved. Dropping it waits for the recording, so quitting mid-recording still leaves a playable file.

use std::path::PathBuf;
use std::time::Instant;

use super::imp::{self, Session};
use super::job::{Job, Take};
use super::{Options, Region, naming};
use crate::platform;
use crate::poll::Wake;

const NO_FOLDER: &str = "There's no folder to save recordings in";
const PREFIX: &str = "Screen Recording";

pub struct Saved {
    pub path: PathBuf,
    pub seconds: f64,
    pub note: Option<String>,
}

struct Live {
    session: Session,
    path: PathBuf,
    stopping: bool,
}

pub struct Recorder {
    live: Option<Live>,
    wake: Wake,
}

impl Recorder {
    pub fn new(wake: Wake) -> Self {
        Self { live: None, wake }
    }

    pub fn is_busy(&self) -> bool {
        self.live.is_some()
    }

    pub fn is_recording(&self) -> bool {
        self.live.as_ref().is_some_and(|live| !live.stopping)
    }

    pub fn since(&self) -> Option<Instant> {
        self.live.as_ref().filter(|live| !live.stopping).and_then(|live| live.session.started())
    }

    pub fn start(&mut self, region: Region, options: &Options) -> Result<(), String> {
        if self.live.is_some() {
            return Ok(());
        }
        let folder = options.folder.clone().or_else(platform::videos_dir).ok_or(NO_FOLDER)?;
        std::fs::create_dir_all(&folder).map_err(|err| format!("Couldn't create {}: {err}", folder.display()))?;
        let path = naming::target(&folder, PREFIX, platform::local_time(), imp::EXTENSION);
        let job = Job {
            path: path.clone(),
            region,
            microphone: options.microphone,
            desktop: options.desktop,
            microphone_id: options.microphone_id.clone().filter(|id| !id.is_empty()),
            max_fps: options.max_fps,
        };
        let session = Session::start(job, self.wake.clone())?;
        self.live = Some(Live { session, path, stopping: false });
        Ok(())
    }

    pub fn stop(&mut self) {
        if let Some(live) = &mut self.live
            && !live.stopping
        {
            live.stopping = true;
            live.session.stop();
        }
    }

    pub fn poll(&mut self) -> Option<Result<Saved, String>> {
        let finished = self.live.as_mut()?.session.finished()?;
        let live = self.live.take()?;
        Some(finished.map(|Take { seconds, note }| Saved { path: live.path, seconds, note }))
    }
}

impl Drop for Recorder {
    fn drop(&mut self) {
        self.stop();
        if let Some(live) = &mut self.live {
            live.session.wait();
        }
    }
}
