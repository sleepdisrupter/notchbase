//! The Windows side of the dock: the same dock, drawn the same way, over Win32 instead of AppKit.

pub mod app;
pub mod app_menu;
pub mod files;
mod hooks;
pub mod lifecycle;
pub mod main;
pub mod os;
pub mod pulse;
mod shell;
mod sys;
pub mod taskbar;
pub mod wallpaper;
pub mod windows;
