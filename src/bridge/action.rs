//! Everything the notch can ask the app to do.

use std::path::PathBuf;

use global_hotkey::hotkey::HotKey;
use notchbase_sdk::protocol::{HandlerId, Surface, UiValue, Value};

use crate::assistant::{Provider, Reply};
use crate::audio::{AppId, DeviceKind};
use crate::media::Control;
use crate::shelf::{ItemId, Op};
use crate::stats::Resource;
use crate::system::permissions;
use crate::tools::shortcuts::ShortcutRef;

use super::{Pick, Pinned, ToolSlot};

#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    RecordShortcut(ShortcutRef),
    CancelRecording,
    SetShortcut(ShortcutRef, HotKey),
    ClearShortcut(ShortcutRef),
    Tool(ToolSlot, Surface, HandlerId, UiValue),
    SetToolSetting(ToolSlot, String, Value),
    PickToolFolder(ToolSlot, String),
    SetToolEnabled(String, bool),
    MoveTool(ToolSlot, Option<ToolSlot>),
    ReloadTools,
    OpenToolPage(ToolSlot),
    OpenPath(String),
    Typing(bool),
    SetVolume(AppId, f32),
    SetMuted(AppId, bool),
    SetDefaultDevice(DeviceKind, String),
    RefreshStats,
    ListApps(Option<Resource>),
    QuitApp {
        key: String,
        name: String,
        force: bool,
    },
    RevealPath(std::path::PathBuf),
    SetAssistant(Option<Provider>),
    SetQuestionsInNotch(bool),
    SetProgressInNotch(bool),
    SetVolumeIndicator(bool),
    SetBrightnessIndicator(bool),
    SetAllDisplays(bool),
    SetWakeAtEdge(bool),
    SetFleeFromKeys(bool),
    SetAutoUpdate(bool),
    SetOpenAtLogin(bool),
    InstallApp,
    UpdateNow,
    Quit,
    Answer(u64, Reply),
    ShowPrompt(u64),
    FocusWindow(u32),
    ShelfRun(Op, Vec<ItemId>),
    ShelfCancel,
    ShelfRemove(ItemId),
    ShelfClear,
    ShelfReveal(ItemId),
    DragOut(Vec<ItemId>),
    Media(Control),
    AirDrop(Vec<ItemId>),
    /// The dock introduction's answer: Turn On (true) or Not Now.
    AnswerDockIntro(bool),
    /// The dock introduction's chevron: open it into its card, or fold the card back.
    OpenDockIntro,
    CancelVideo(u64),
    AllowFullDiskAccess,
    SetLockScreen(bool),
    SetHaptics(bool),
    Dock(super::DockChange),
    SetDodgeMenus(bool),
    SetSnapWindows(bool),
    FindQuery(String),
    FindShown(bool),
    FindPick(Pick),
    FindPin(Pinned),
    FindUnpin(PathBuf),
    FindPinsOrder(Vec<PathBuf>),
    FindClose,
    SetFindEnabled(bool),
    SetFindSystemKey(bool),
    FindForget,
    FindHideAccess,
    ViewChanges,
    ChangesSeen,
    WelcomeStart,
    WelcomeAsk(permissions::Access),
    WelcomeNext,
    WelcomeSkip,
    WelcomeDone,
    ShowPermissions,
}
