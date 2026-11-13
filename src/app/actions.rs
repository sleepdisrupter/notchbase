//! Carries out each `Action` the notch sends back.

use winit::event_loop::ActiveEventLoop;

use crate::bridge::{Action, Page};

use super::App;

impl App {
    pub(super) fn handle(&mut self, event_loop: &ActiveEventLoop, action: Action) {
        match action {
            Action::RecordShortcut(which) => self.start_recording(which),
            Action::CancelRecording => self.stop_recording(),
            Action::SetShortcut(which, hotkey) => self.set_shortcut(which, hotkey),
            Action::ClearShortcut(which) => self.clear_shortcut(which),
            Action::Tool(slot, surface, handler, value) => self.tool_event(event_loop, slot, surface, handler, value),
            Action::SetToolSetting(slot, key, value) => self.set_tool_setting(slot, key, value),
            Action::PickToolFolder(slot, key) => self.pick_tool_folder(slot, key),
            Action::SetToolEnabled(id, on) => self.set_tool_enabled(event_loop, id, on),
            Action::MoveTool(slot, before) => self.move_tool(slot, before),
            Action::ReloadTools => self.reload_tools(event_loop),
            Action::OpenToolPage(slot) => self.update_notch(|notch| notch.open_page(Page::Tool(slot))),
            Action::OpenPath(path) => self.open_path(&path),
            Action::Typing(on) => self.set_typing(on),
            Action::SetVolume(id, volume) => {
                self.quiet_volume_indicator(id);
                self.panel.mixer.set_volume(id, volume);
            }
            Action::SetMuted(id, muted) => {
                self.quiet_volume_indicator(id);
                self.panel.mixer.set_muted(id, muted);
            }
            Action::SetDefaultDevice(kind, id) => self.panel.mixer.set_default_device(kind, &id),
            Action::RefreshStats => self.panel.stats.refresh_usage(),
            Action::ListApps(by) => self.panel.stats.list_apps(by),
            Action::QuitApp { key, name, force } => {
                if !self.panel.stats.quit_app(&key, force) {
                    self.notify(&format!("Couldn't quit {name}"));
                }
            }
            Action::RevealPath(path) => crate::filedrag::reveal(&path),
            Action::SetAssistant(assistant) => self.set_assistant(assistant),
            Action::SetQuestionsInNotch(on) => self.set_questions_in_notch(on),
            Action::SetProgressInNotch(on) => self.set_progress_in_notch(on),
            Action::UpdateNow => self.update_now(),
            Action::SetVolumeIndicator(on) => self.set_volume_indicator(on),
            Action::SetBrightnessIndicator(on) => self.set_brightness_indicator(on),
            Action::SetAllDisplays(on) => self.set_all_displays(event_loop, on),
            Action::SetWakeAtEdge(on) => self.set_wake_at_edge(on),
            Action::SetFleeFromKeys(on) => self.set_flee_from_keys(on),
            Action::SetHaptics(on) => self.set_haptics(on),
            Action::Dock(change) => self.change_dock(&change),
            Action::SetDodgeMenus(on) => self.set_dodge_menus(on),
            Action::SetSnapWindows(on) => self.set_snap_windows(on),
            Action::SetLockScreen(on) => self.set_lock_screen(on),
            Action::AllowFullDiskAccess => self.allow_full_disk_access(),
            Action::SetAutoUpdate(on) => self.set_auto_update(on),
            Action::SetOpenAtLogin(on) => self.set_open_at_login(on),
            Action::InstallApp => self.install_app(event_loop),
            Action::Quit => event_loop.exit(),
            Action::Answer(id, reply) => {
                if let Some(next) = self.answer(id, reply) {
                    self.handle(event_loop, next);
                }
            }
            Action::ShowPrompt(id) => self.show_prompt(id),
            Action::FocusWindow(pid) => self.focus_window(pid),
            Action::ShelfRun(op, ids) => self.run_shelf_op(op, &ids),
            Action::ShelfCancel => self.panel.shelf.cancel(),
            Action::ShelfRemove(id) => self.panel.shelf.remove(id),
            Action::ShelfClear => self.panel.shelf.clear(),
            Action::ShelfReveal(id) => self.reveal_shelf_item(id),
            Action::DragOut(ids) => self.drag_out(&ids),
            Action::Media(control) => self.control_media(control),
            Action::AirDrop(items) => self.airdrop_items(&items),
            Action::AnswerDockIntro(on) => self.answer_dock_intro(on, None),
            Action::OpenDockIntro => self.update_notch(|notch| {
                notch.toggle_dock_intro();
            }),
            Action::CancelVideo(id) => self.cancel_video(id),
            Action::FindQuery(text) => self.find_query(&text),
            Action::FindShown(shown) => self.find_shown(shown),
            Action::FindPick(pick) => self.find_pick(&pick),
            Action::FindPin(pinned) => self.find_pin(&pinned),
            Action::FindUnpin(path) => self.find_unpin(&path),
            Action::FindPinsOrder(order) => self.find_order_pins(&order),
            Action::FindClose => self.find_close(),
            Action::SetFindEnabled(on) => self.set_find_enabled(on),
            Action::SetFindSystemKey(on) => self.set_find_system_key(on),
            Action::FindForget => self.find_forget(),
            Action::FindHideAccess => self.find_hide_access(),
            Action::ViewChanges => self.view_changes(None),
            Action::ChangesSeen => self.mark_changes_seen(),
            Action::WelcomeStart => self.welcome_start(),
            Action::WelcomeAsk(access) => self.welcome_ask(access),
            Action::WelcomeNext => self.welcome_next(),
            Action::WelcomeSkip => self.welcome_skip(),
            Action::WelcomeDone => self.welcome_done(),
            Action::ShowPermissions => self.show_permissions(None),
        }
    }
}
