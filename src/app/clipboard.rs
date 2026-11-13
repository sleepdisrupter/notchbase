//! The clipboard, and copying screenshots to it off the main thread.

use std::borrow::Cow;
use std::sync::Arc;
use std::thread;

use crate::capture::Screenshot;

const COPY_THREAD: &str = "notchbase-clipboard";

#[derive(Default)]
pub(super) struct Clipboard(Option<arboard::Clipboard>);

impl Clipboard {
    pub(super) fn copy_image_in_background(
        shot: Arc<Screenshot>,
        failed: impl FnOnce(arboard::Error) + Send + 'static,
    ) -> std::io::Result<()> {
        let copy = move || {
            if let Err(err) = copy_image(shot) {
                failed(err);
            }
        };
        thread::Builder::new().name(COPY_THREAD.into()).spawn(copy).map(drop)
    }

    pub(super) fn copy_text(&mut self, text: String) -> Result<(), arboard::Error> {
        self.system()?.set_text(text)
    }

    fn system(&mut self) -> Result<&mut arboard::Clipboard, arboard::Error> {
        let clipboard = match self.0.take() {
            Some(clipboard) => clipboard,
            None => arboard::Clipboard::new()?,
        };
        Ok(self.0.insert(clipboard))
    }
}

fn copy_image(shot: Arc<Screenshot>) -> Result<(), arboard::Error> {
    let image = arboard::ImageData { width: shot.width, height: shot.height, bytes: Cow::Borrowed(&shot.rgba) };
    arboard::Clipboard::new()?.set_image(image)
}
