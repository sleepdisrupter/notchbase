//! Row order: apps heard lately first, then by name, with system sounds last. Devices sort by name.

use super::{AppId, AudioApp, AudioDevice};

pub fn sort_key(app: &AudioApp, system: bool, recent: bool) -> (bool, bool, String, AppId) {
    (system, !recent, app.name.to_lowercase(), app.id)
}

pub fn sort_devices(devices: &mut [AudioDevice]) {
    devices.sort_by_cached_key(|d| (d.name.to_lowercase(), d.id.clone()));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::{DeviceForm, DeviceKind};

    fn app(id: AppId, name: &str) -> AudioApp {
        AudioApp { id, name: name.to_owned(), volume: 1.0, muted: false, peak: 0.0, icon: None }
    }

    #[test]
    fn playing_first_system_sounds_last() {
        let mut apps = [
            (app(1, "System Sounds"), true, true),
            (app(2, "zoom"), false, false),
            (app(3, "Spotify"), false, true),
            (app(4, "Arc"), false, false),
            (app(5, "brave"), false, true),
        ];
        apps.sort_by_cached_key(|(a, system, recent)| sort_key(a, *system, *recent));
        let order: Vec<AppId> = apps.iter().map(|(a, ..)| a.id).collect();
        assert_eq!(order, vec![5, 3, 4, 2, 1]);
    }

    #[test]
    fn devices_sort_by_name() {
        let device = |id: &str, name: &str| AudioDevice {
            id: id.to_owned(),
            name: name.to_owned(),
            kind: DeviceKind::Output,
            is_default: false,
            form: DeviceForm::Other,
        };
        let mut list = vec![device("3", "Speakers"), device("1", "airpods"), device("2", "Speakers")];
        sort_devices(&mut list);
        let ids: Vec<&str> = list.iter().map(|d| d.id.as_str()).collect();
        assert_eq!(ids, vec!["1", "2", "3"]);
    }
}
