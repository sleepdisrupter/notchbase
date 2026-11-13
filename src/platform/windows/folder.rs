//! A folder picker, using the shell's file dialog on a thread of its own.

use std::path::{Path, PathBuf};

use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize,
};
use windows::Win32::UI::Shell::{
    FOS_FORCEFILESYSTEM, FOS_PICKFOLDERS, FileOpenDialog, IFileOpenDialog, IShellItem, SHCreateItemFromParsingName,
    SIGDN_FILESYSPATH,
};
use windows::core::{HSTRING, w};

pub fn pick_folder(start: Option<PathBuf>, done: impl FnOnce(Option<PathBuf>) + Send + 'static) {
    let task = move || {
        let joined = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) }.is_ok();
        let picked = pick(start.as_deref());
        if joined {
            unsafe { CoUninitialize() };
        }
        done(picked);
    };
    let _ = std::thread::Builder::new().name("notchbase-pick-folder".into()).spawn(task);
}

fn pick(start: Option<&Path>) -> Option<PathBuf> {
    unsafe {
        let dialog: IFileOpenDialog = CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER).ok()?;
        let options = dialog.GetOptions().ok()?;
        dialog.SetOptions(options | FOS_PICKFOLDERS | FOS_FORCEFILESYSTEM).ok()?;
        let _ = dialog.SetTitle(w!("Choose a folder"));
        if let Some(start) = start.filter(|start| start.is_dir()) {
            let start = HSTRING::from(start.as_os_str());
            if let Ok(item) = SHCreateItemFromParsingName::<_, _, IShellItem>(&start, None) {
                let _ = dialog.SetFolder(&item);
            }
        }
        dialog.Show(None).ok()?;
        let name = dialog.GetResult().ok()?.GetDisplayName(SIGDN_FILESYSPATH).ok()?;
        let path = name.to_string().ok().map(PathBuf::from);
        CoTaskMemFree(Some(name.0.cast_const().cast()));
        path
    }
}
