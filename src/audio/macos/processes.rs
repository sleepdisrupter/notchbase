//! Audio processes from Core Audio, each keyed by the app responsible for it, so helpers count as their app.

use std::collections::{HashMap, HashSet};
use std::ffi::c_void;
use std::sync::OnceLock;

use super::app_labels::running_app;
use super::property::{AudioObjectID, PropertyAddress, read, read_ids, read_string};
use super::selectors::{HW_PROCESS_OBJECTS, PROCESS_BUNDLE_ID, PROCESS_PID, PROCESS_RUNNING_OUTPUT, SYSTEM_OBJECT};
use super::symbols::global_symbol;
use crate::audio::core_audio::bundle_ids::parent_bundle_id;
use crate::audio::keys::pid_key;

type ResponsibleFor = unsafe extern "C" fn(pid: i32) -> i32;

pub struct Process {
    pub object: AudioObjectID,
    pub bundle: String,
    pub pid: i32,
}

pub struct AppProcesses {
    pub key: String,
    pub processes: Vec<Process>,
}

pub struct ScannedProcess {
    pub key: String,
    pub running: bool,
    pub process: Process,
}

#[derive(Default)]
pub struct ProcessScanner {
    keys: HashMap<i32, String>,
}

impl ProcessScanner {
    pub fn scan(&mut self) -> Vec<ScannedProcess> {
        let me = std::process::id() as i32;
        let mut alive = HashSet::new();
        let mut scanned = Vec::new();
        for object in read_ids(SYSTEM_OBJECT, PropertyAddress::global(HW_PROCESS_OBJECTS)) {
            let Some(pid) = read::<i32>(object, PropertyAddress::global(PROCESS_PID)) else { continue };
            if pid <= 0 || pid == me {
                continue;
            }
            alive.insert(pid);
            let running = read::<u32>(object, PropertyAddress::global(PROCESS_RUNNING_OUTPUT))
                .is_some_and(|running| running != 0);
            let bundle = read_string(object, PropertyAddress::global(PROCESS_BUNDLE_ID)).unwrap_or_default();
            let key = self.app_key(pid, &bundle);
            scanned.push(ScannedProcess { key, running, process: Process { object, bundle, pid } });
        }
        self.keys.retain(|pid, _| alive.contains(pid));
        scanned
    }

    fn app_key(&mut self, pid: i32, bundle: &str) -> String {
        if let Some(key) = self.keys.get(&pid) {
            return key.clone();
        }
        let owner = responsible_pid(pid).unwrap_or(pid);
        let owner_bundle = running_app(owner).and_then(|app| app.bundleIdentifier()).map(|id| id.to_string());
        let bundle = owner_bundle.as_deref().filter(|id| !id.is_empty()).or((!bundle.is_empty()).then_some(bundle));
        let key = match bundle {
            Some(bundle) => parent_bundle_id(bundle).to_owned(),
            None => pid_key(owner),
        };
        self.keys.insert(pid, key.clone());
        key
    }
}

pub(crate) fn responsible_pid(pid: i32) -> Option<i32> {
    static FUNCTION: OnceLock<Option<ResponsibleFor>> = OnceLock::new();
    let function = (*FUNCTION.get_or_init(|| {
        let symbol = global_symbol(c"responsibility_get_pid_responsible_for_pid")?;
        Some(unsafe { std::mem::transmute::<*mut c_void, ResponsibleFor>(symbol.as_ptr()) })
    }))?;
    let owner = unsafe { function(pid) };
    (owner > 0).then_some(owner)
}
