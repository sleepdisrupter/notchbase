//! Names of the apps recording audio right now, for the microphone indicator.

use super::app_labels::running_app;
use super::processes::responsible_pid;
use super::property::{PropertyAddress, read, read_ids};
use super::selectors::{HW_PROCESS_OBJECTS, PROCESS_PID, PROCESS_RUNNING_INPUT, SYSTEM_OBJECT};

pub fn recording_apps() -> Vec<String> {
    let me = std::process::id() as i32;
    let mut names: Vec<String> = Vec::new();
    for object in read_ids(SYSTEM_OBJECT, PropertyAddress::global(HW_PROCESS_OBJECTS)) {
        let recording =
            read::<u32>(object, PropertyAddress::global(PROCESS_RUNNING_INPUT)).is_some_and(|running| running != 0);
        let Some(pid) = read::<i32>(object, PropertyAddress::global(PROCESS_PID)).filter(|_| recording) else {
            continue;
        };
        let owner = responsible_pid(pid).unwrap_or(pid);
        if pid <= 0 || pid == me || owner == me {
            continue;
        }
        let name = running_app(owner).and_then(|app| app.localizedName()).map(|name| name.to_string());
        if let Some(name) = name.filter(|name| !name.trim().is_empty() && !names.contains(name)) {
            names.push(name);
        }
    }
    names
}
