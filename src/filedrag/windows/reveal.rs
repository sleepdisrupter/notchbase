//! Shows a file selected in Explorer, falling back to `explorer.exe /select,` if the shell call fails.

use std::os::windows::process::CommandExt as _;
use std::path::Path;
use std::process::Command;

use ::windows::Win32::UI::Shell::SHOpenFolderAndSelectItems;

use super::com::{ItemIdList, ensure_com_on_this_thread};
use super::shell_path::shell_parsable;

pub(in crate::filedrag) fn reveal(path: &Path) {
    ensure_com_on_this_thread();
    let Ok(item) = ItemIdList::for_path(path) else {
        return;
    };
    let opened_and_selected = unsafe { SHOpenFolderAndSelectItems(item.as_ptr(), None, 0) }.is_ok();
    if !opened_and_selected {
        open_explorer_selecting(path);
    }
}

fn open_explorer_selecting(path: &Path) {
    let _ = Command::new("explorer.exe").raw_arg(explorer_select_argument(path)).spawn();
}

fn explorer_select_argument(path: &Path) -> String {
    format!("/select,\"{}\"", shell_parsable(path).display())
}
