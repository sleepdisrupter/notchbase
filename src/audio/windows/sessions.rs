//! App rows from the sessions on the default output, skipping our own. System Sounds gets its own row, sorted last.

use std::cell::OnceCell;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use ::windows::Win32::Foundation::S_OK;
use ::windows::Win32::Media::Audio::{
    AudioSessionStateExpired, IAudioSessionControl, IAudioSessionControl2, IAudioSessionManager2, IMMDevice,
};
use ::windows::Win32::System::Com::CLSCTX_ALL;
use ::windows::core::Interface;
use egui::ColorImage;

use super::app_labels::load_label;
use super::processes::app_key_for_process;
use super::session::{Session, session_icon};
use crate::audio::activity::Activity;
use crate::audio::grouping::{Group, Member, group_members};
use crate::audio::labels::AppLabels;
use crate::audio::sorting::sort_key;
use crate::audio::{AppId, AudioApp};

const SYSTEM_KEY: &str = "#system-sounds";
const SYSTEM_NAME: &str = "System Sounds";

#[derive(Default)]
pub struct Sessions {
    apps: Vec<AudioApp>,
    sessions: Vec<Vec<Session>>,
    labels: AppLabels,
    system_icon: OnceCell<Option<Arc<ColorImage>>>,
    activity: Activity,
}

impl Sessions {
    pub fn apps(&self) -> &[AudioApp] {
        &self.apps
    }

    pub fn clear(&mut self) {
        self.apps.clear();
        self.sessions.clear();
    }

    pub fn refresh(&mut self, device: &IMMDevice) -> bool {
        let Some(controls) = session_controls(device) else { return false };
        let members: Vec<Member<Session>> = controls.iter().filter_map(|control| self.read(control)).collect();
        let now = Instant::now();
        let old_peaks: HashMap<AppId, f32> = self.apps.iter().map(|a| (a.id, a.peak)).collect();
        let mut rows: Vec<(AudioApp, bool, Vec<Session>)> = group_members(members)
            .into_iter()
            .map(|group| {
                self.activity.note(group.id, group.peak, now);
                let peak = old_peaks.get(&group.id).copied().unwrap_or(group.peak);
                let app = self.app_row(&group, peak);
                (app, group.system, group.members)
            })
            .collect();
        rows.sort_by_cached_key(|(app, system, _)| sort_key(app, *system, self.activity.is_recent(app.id, now)));
        self.activity.retain(|id| rows.iter().any(|(app, ..)| app.id == id));
        (self.apps, self.sessions) = rows.into_iter().map(|(app, _, sessions)| (app, sessions)).unzip();
        true
    }

    pub fn update_levels(&mut self) {
        let now = Instant::now();
        for (app, sessions) in self.apps.iter_mut().zip(&self.sessions) {
            let peak = sessions.iter().filter_map(Session::peak).fold(0.0f32, f32::max);
            app.peak = peak.clamp(0.0, 1.0);
            let levels: Vec<(f32, bool)> = sessions.iter().filter_map(Session::levels).collect();
            if !levels.is_empty() {
                app.volume = levels.iter().map(|&(volume, _)| volume).fold(0.0, f32::max);
                app.muted = levels.iter().all(|&(_, muted)| muted);
            }
            self.activity.note(app.id, app.peak, now);
        }
    }

    pub fn set_volume(&mut self, id: AppId, volume: f32) {
        if let Some(app) = self.apply(id, |session| session.set_volume(volume)) {
            app.volume = volume;
        }
    }

    pub fn set_muted(&mut self, id: AppId, muted: bool) {
        if let Some(app) = self.apply(id, |session| session.set_muted(muted)) {
            app.muted = muted;
        }
    }

    fn apply(&mut self, id: AppId, change: impl Fn(&Session) -> bool) -> Option<&mut AudioApp> {
        let i = self.apps.iter().position(|a| a.id == id)?;
        let mut accepted = false;
        for session in &self.sessions[i] {
            accepted |= change(session);
        }
        accepted.then(|| &mut self.apps[i])
    }

    fn read(&mut self, control: &IAudioSessionControl) -> Option<Member<Session>> {
        let control2: IAudioSessionControl2 = control.cast().ok()?;
        if unsafe { control2.GetState() }.ok()? == AudioSessionStateExpired {
            return None;
        }
        let system = unsafe { control2.IsSystemSoundsSession() } == S_OK;
        let process = unsafe { control2.GetProcessId() }.unwrap_or(0);
        if !system && process == std::process::id() {
            return None;
        }
        let key = if system {
            self.system_icon.get_or_init(|| session_icon(control));
            SYSTEM_KEY.to_owned()
        } else {
            app_key_for_process(process)?
        };
        let (handle, levels) = Session::open(control)?;
        Some(Member { key, system, volume: levels.volume, muted: levels.muted, peak: levels.peak, handle })
    }

    fn app_row(&mut self, group: &Group<Session>, peak: f32) -> AudioApp {
        let (name, icon) = if group.system {
            (SYSTEM_NAME.to_owned(), self.system_icon.get().cloned().flatten())
        } else {
            let label = self.labels.get_or_load(&group.key, || load_label(&group.key));
            (label.name.clone(), label.icon.clone())
        };
        AudioApp { id: group.id, name, volume: group.volume.clamp(0.0, 1.0), muted: group.muted, peak, icon }
    }
}

fn session_controls(device: &IMMDevice) -> Option<Vec<IAudioSessionControl>> {
    let list = unsafe {
        let manager = device.Activate::<IAudioSessionManager2>(CLSCTX_ALL, None).ok()?;
        manager.GetSessionEnumerator().ok()?
    };
    let count = unsafe { list.GetCount() }.unwrap_or(0);
    Some((0..count).filter_map(|i| unsafe { list.GetSession(i) }.ok()).collect())
}
