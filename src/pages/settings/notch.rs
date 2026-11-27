//! The notch's own switches: the popups it replaces, how it wakes and moves aside, and window layouts.

use egui::{Id, Rect};

use super::rows::{Row, Toggle, toggled};
use super::search::Said;
use crate::bridge::{Action, PanelData};
use crate::pages::PageCtx;
use crate::system::brightness;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum NotchRow {
    Volume,
    Brightness,
    Displays,
    Edge,
    Keys,
    #[cfg_attr(not(any(target_os = "macos", test)), expect(dead_code))]
    Menus,
    #[cfg_attr(not(any(target_os = "macos", test)), expect(dead_code))]
    LockScreen,
    #[cfg_attr(not(any(target_os = "macos", test)), expect(dead_code))]
    Haptics,
    Layouts,
}

use NotchRow::*;

pub(super) const POPUPS: &[NotchRow] = if brightness::SUPPORTED { &[Volume, Brightness] } else { &[Volume] };
#[cfg(target_os = "macos")]
pub(super) const NOTCH: &[NotchRow] = &[Displays, Edge, Keys, Menus, LockScreen, Haptics];
#[cfg(not(target_os = "macos"))]
pub(super) const NOTCH: &[NotchRow] = &[Displays, Edge, Keys];
pub(super) const WINDOWS: &[NotchRow] = &[Layouts];

#[cfg(target_os = "macos")]
const VOLUME_DETAIL: &str = "Replaces the macOS volume and media popups";
#[cfg(not(target_os = "macos"))]
const VOLUME_DETAIL: &str = "Replaces the Windows volume and media popups";

impl NotchRow {
    fn title(self) -> &'static str {
        match self {
            Volume => "Volume indicator",
            Brightness => "Brightness indicator",
            Displays => "All displays",
            Edge => "Wake at the top edge",
            Keys => "Move away for keys and clicks",
            Menus => "Make room for menus",
            LockScreen => "Show on the lock screen",
            Haptics => "Haptics",
            Layouts => "Window layouts",
        }
    }

    fn detail(self) -> &'static str {
        match self {
            Volume => VOLUME_DETAIL,
            Brightness => "Replaces the macOS brightness popup",
            Displays => "Show an island on every screen",
            Edge => "Wakes only when the pointer hits the top",
            Keys => "Slides aside for a held key or a click beside it",
            Menus => "Slides right when an app's menus reach it",
            LockScreen => "Stays visible, untouchable, while the Mac is locked",
            Haptics => "A light tap on the trackpad as things open",
            Layouts => "Drag a window to the notch to lay it out",
        }
    }

    fn words(self) -> &'static str {
        match self {
            Volume => "sound audio media music popup hud",
            Brightness => "screen display light popup hud",
            Displays => "screens monitors external island",
            Edge => "hover open pointer mouse",
            Keys => "keyboard dodge hide",
            Menus => "menu bar apps",
            LockScreen => "locked login",
            Haptics => "trackpad vibration feedback",
            Layouts => "snap tile halves thirds windows",
        }
    }

    fn key(self) -> &'static str {
        match self {
            Volume => "volume",
            Brightness => "brightness",
            Displays => "displays",
            Edge => "edge",
            Keys => "keys",
            Menus => "menus",
            LockScreen => "lock",
            Haptics => "haptics",
            Layouts => "snap",
        }
    }

    pub(super) fn on(self, data: &PanelData) -> bool {
        match self {
            Volume => data.volume_indicator,
            Brightness => data.brightness_indicator,
            Displays => data.all_displays,
            Edge => data.wake_at_edge,
            Keys => data.flee_from_keys,
            Menus => data.dodge_menus,
            LockScreen => data.lock_screen,
            Haptics => data.haptics,
            Layouts => data.snap_windows,
        }
    }

    fn set(self, on: bool) -> Action {
        match self {
            Volume => Action::SetVolumeIndicator(on),
            Brightness => Action::SetBrightnessIndicator(on),
            Displays => Action::SetAllDisplays(on),
            Edge => Action::SetWakeAtEdge(on),
            Keys => Action::SetFleeFromKeys(on),
            Menus => Action::SetDodgeMenus(on),
            LockScreen => Action::SetLockScreen(on),
            Haptics => Action::SetHaptics(on),
            Layouts => Action::SetSnapWindows(on),
        }
    }

    pub(super) fn said(self) -> Said {
        Said { title: self.title().to_owned(), detail: self.detail().to_owned(), words: self.words() }
    }

    pub(super) fn paint(self, ctx: &mut PageCtx<'_>, rect: Rect, data: &PanelData, live: bool) -> Option<Action> {
        let on = self.on(data);
        let row = Row { rect, title: self.title(), detail: self.detail() };
        let toggle = Toggle { id: Id::new(format!("notchbase.settings.{}", self.key())), on, enabled: live };
        toggled(ctx, &row, &toggle).then(|| self.set(!on))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_row_sets_its_own_switch() {
        let rows = [Volume, Brightness, Displays, Edge, Keys, Menus, LockScreen, Haptics, Layouts];
        let mut keys: Vec<&str> = rows.iter().map(|row| row.key()).collect();
        keys.dedup();
        assert_eq!(keys.len(), rows.len());
        assert_eq!(Layouts.set(true), Action::SetSnapWindows(true));
        assert_eq!(Volume.set(false), Action::SetVolumeIndicator(false));
        assert!(rows.iter().all(|row| !row.title().is_empty() && !row.detail().is_empty()));
    }

    #[test]
    fn platform_keeps_its_own_rows() {
        assert_eq!(POPUPS.contains(&Brightness), brightness::SUPPORTED);
        assert_eq!(NOTCH.contains(&Haptics), cfg!(target_os = "macos"));
        assert_eq!(WINDOWS, [Layouts]);
    }
}
