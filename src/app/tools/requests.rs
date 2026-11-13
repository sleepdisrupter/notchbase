//! Carries out what tools ask the host for: captures, images, the clipboard, opening things and more.

use std::path::PathBuf;
use std::sync::Arc;

use notchbase_sdk::protocol::{Event, ImageFormat, ImageId, MediaControl, Reply, Request};
use winit::event_loop::ActiveEventLoop;

use crate::app::App;
use crate::app::capture::CaptureReply;
use crate::app::clipboard::Clipboard;
use crate::app::events::{self, UserEvent};
use crate::bridge::{Page, ToolSlot};
use crate::capture::{CaptureMode, Screenshot};
use crate::media::Control;
use crate::notch::Notch;
use crate::platform::{self, KnownDir};
use crate::record::unique_file;
use crate::system::calendar::Change;
use crate::tools::outside;

const SCREENSHOTS: &str = "Screenshots";
pub(in crate::app) const NO_IMAGE: &str = "That image is no longer available";
const NO_FOLDER: &str = "There's no folder to save images in";
const TEXT_NOT_COPIED: &str = "Couldn't copy the text";
const COULDNT_OPEN: &str = "Couldn't open it";

impl App {
    pub(super) fn perform_all(&mut self, event_loop: &ActiveEventLoop, requests: Vec<(ToolSlot, Request)>) {
        for (slot, request) in requests {
            self.perform(event_loop, slot, request);
        }
    }

    fn perform(&mut self, event_loop: &ActiveEventLoop, slot: ToolSlot, request: Request) {
        match request {
            Request::Notify { text } => self.notify(&text),
            Request::Notice { notice } => self.update_notch(|notch| notch.show_notice(slot, notice.clone())),
            Request::DismissNotice => self.update_notch(|notch| notch.dismiss_notice(slot)),
            Request::CopyText { text } => {
                if let Err(err) = self.clipboard.copy_text(text) {
                    self.report(TEXT_NOT_COPIED, err);
                }
            }
            Request::CopyImage { image } => self.copy_image(image),
            Request::Annotate { image } => self.open_annotator(event_loop, image),
            Request::SaveImage { image, folder, prefix, format, reply } => {
                self.save_image(slot, image, folder, &prefix, format, reply);
            }
            Request::PickColor { reply } => self.capture_for(event_loop, slot, reply, CaptureMode::Color),
            Request::CaptureScreen { reply } => self.capture_for(event_loop, slot, reply, CaptureMode::Region),
            Request::StartRecording { options } => self.record_for(event_loop, options),
            Request::StopRecording => {
                self.recorder.stop();
                self.sync_screen_recording();
            }
            Request::CancelCapture => self.cancel_capture(),
            Request::SaveSetting { key, value } => self.set_tool_setting(slot, key, value),
            Request::Shelve { paths } => {
                self.panel.shelf.add(paths.into_iter().map(PathBuf::from).collect());
            }
            Request::Reveal { path } => crate::filedrag::reveal(PathBuf::from(path).as_path()),
            Request::Open { target } => {
                if let Err(err) = platform::open(&target) {
                    self.report(COULDNT_OPEN, err);
                }
            }
            Request::OpenPage => self.update_notch(|notch| notch.open_page(Page::Tool(slot))),
            Request::OpenSettings => {
                let id = self.tools.host(slot).map(|host| host.id().to_owned()).unwrap_or_default();
                self.update_notch(|notch| notch.open_tool_settings(&id));
            }
            Request::SaveEvent { event } => self.change_calendar(Change::Save(event)),
            Request::DeleteEvent { id } => self.change_calendar(Change::Delete(id)),
            Request::Collapse => self.update_notch(Notch::collapse),
            Request::Hold { held } => self.hold_for(slot, held),
            Request::Media { control } => self.control_media(match control {
                MediaControl::PlayPause => Control::PlayPause,
                MediaControl::Next => Control::Next,
                MediaControl::Previous => Control::Previous,
            }),
            Request::Chime => crate::platform::chime(),
            Request::KeepAwake { awake } => self.keep_awake(slot, awake),
            Request::MuteMicrophone { muted } => self.mute_microphone(muted),
            Request::Fetch { request, reply } => {
                self.answer_later(slot, reply, move || Reply::Fetched { result: outside::fetch(&request) });
            }
            Request::ReadFile { path, reply } => {
                let file = self.tool_file(slot, &path);
                self.answer_later(slot, reply, move || Reply::Read { result: outside::read(&file) });
            }
            Request::WriteFile { path, text, reply } => {
                let file = self.tool_file(slot, &path);
                let write = move || Reply::Saved { result: outside::write(&file, &text) };
                match reply {
                    Some(reply) => self.answer_later(slot, reply, write),
                    None => self.in_background(move || drop(write())),
                }
            }
            Request::Log { text } => {
                let id = self.tools.host(slot).map_or("tool", |host| host.id());
                eprintln!("notchbase: {id}: {text}");
            }
        }
    }

    fn tool_file(&self, slot: ToolSlot, path: &str) -> PathBuf {
        let id = self.tools.host(slot).map(|host| host.id().to_owned()).unwrap_or_default();
        let data = crate::tools::data_dir(&id).unwrap_or_default();
        outside::resolve(&data, platform::known_dir(KnownDir::Home).as_deref(), path)
    }

    fn answer_later(&self, slot: ToolSlot, id: u64, work: impl FnOnce() -> Reply + Send + 'static) {
        let replies = self.replies.clone();
        self.in_background(move || replies.post((slot, Event::Reply { id, reply: work() })));
    }

    fn in_background(&self, work: impl FnOnce() + Send + 'static) {
        let _ = std::thread::Builder::new().name("notchbase-tool-request".into()).spawn(work);
    }

    fn capture_for(&mut self, event_loop: &ActiveEventLoop, slot: ToolSlot, reply: u64, mode: CaptureMode) {
        if self.capture_requested() {
            self.tools.send(slot, Event::Reply { id: reply, reply: empty_reply(mode) });
            return;
        }
        self.capture_reply = Some(CaptureReply { slot, id: reply, mode });
        self.request_capture(event_loop, mode);
    }

    fn copy_image(&mut self, image: ImageId) {
        let Some(shot) = self.tools.images.full(image) else {
            self.notify(NO_IMAGE);
            return;
        };
        let send = events::sender(&self.proxy);
        let failed = move |err: arboard::Error| send(UserEvent::ScreenshotNotCopied(err.to_string()));
        if let Err(err) = Clipboard::copy_image_in_background(shot, failed) {
            self.screenshot_not_copied(&err.to_string());
        }
    }

    fn save_image(
        &mut self,
        slot: ToolSlot,
        image: ImageId,
        folder: Option<String>,
        prefix: &str,
        format: ImageFormat,
        reply: Option<u64>,
    ) {
        let folder = folder
            .map(PathBuf::from)
            .or_else(|| platform::known_dir(KnownDir::Pictures).map(|pictures| pictures.join(SCREENSHOTS)));
        let job = self
            .tools
            .images
            .full(image)
            .ok_or(NO_IMAGE)
            .and_then(|shot| folder.map(|folder| (shot, folder)).ok_or(NO_FOLDER));
        let replies = self.replies.clone();
        let prefix = prefix.to_owned();
        let task = move || {
            let result =
                job.map_err(str::to_owned).and_then(|(shot, folder)| write_image(&shot, &folder, &prefix, format));
            if let Some(id) = reply {
                replies.post((slot, Event::Reply { id, reply: Reply::Saved { result } }));
            }
        };
        let _ = std::thread::Builder::new().name("notchbase-save-image".into()).spawn(task);
    }
}

pub(in crate::app) fn empty_reply(mode: CaptureMode) -> Reply {
    match mode {
        CaptureMode::Color => Reply::Color { color: None },
        CaptureMode::Region | CaptureMode::Area => Reply::Capture { capture: None },
    }
}

fn write_image(
    shot: &Arc<Screenshot>,
    folder: &std::path::Path,
    prefix: &str,
    format: ImageFormat,
) -> Result<String, String> {
    std::fs::create_dir_all(folder).map_err(|err| format!("Couldn't create {}: {err}", folder.display()))?;
    let (extension, format) = match format {
        ImageFormat::Png => ("png", image::ImageFormat::Png),
        ImageFormat::Jpeg => ("jpg", image::ImageFormat::Jpeg),
    };
    let path = unique_file(folder, prefix, platform::local_time(), extension);
    let rgba = image::RgbaImage::from_raw(shot.width as u32, shot.height as u32, shot.rgba.clone())
        .ok_or("The screenshot was the wrong size")?;
    let saved = match format {
        image::ImageFormat::Jpeg => image::DynamicImage::ImageRgba8(rgba).to_rgb8().save_with_format(&path, format),
        _ => rgba.save_with_format(&path, format),
    };
    saved.map_err(|err| format!("Couldn't save {}: {err}", path.display()))?;
    Ok(path.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capture_saved_without_clobbering() {
        let folder = std::env::temp_dir().join(format!("notchbase-save-{}", std::process::id()));
        let shot = Arc::new(Screenshot { width: 4, height: 2, rgba: vec![128; 32] });
        let first = write_image(&shot, &folder, "Screenshot", ImageFormat::Png).unwrap();
        let second = write_image(&shot, &folder, "Screenshot", ImageFormat::Png).unwrap();
        let jpeg = write_image(&shot, &folder, "Screenshot", ImageFormat::Jpeg).unwrap();
        assert_ne!(first, second);
        assert!(jpeg.ends_with(".jpg"));
        assert_eq!(image::open(&first).unwrap().width(), 4);
        let _ = std::fs::remove_dir_all(&folder);
    }
}
