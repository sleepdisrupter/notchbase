//! The notchbase installer. It finds the newest release, downloads it and has the app install itself.
#![windows_subsystem = "windows"]

mod app;
mod brand;
mod dance;
mod install;
mod pages;
mod shot;
mod theme;
mod widgets;
mod window;

fn main() -> eframe::Result {
    eframe::run_native(brand::TITLE, app::options(), Box::new(|cc| Ok(Box::new(app::Setup::new(cc)))))
}
