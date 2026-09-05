#![windows_subsystem = "windows"]

mod animation;
mod app;
mod bubble;
mod bubble_layout;
mod capslock;
mod caret;
mod hook;
mod language;
mod registry;
mod settings;
mod tray;
mod types;
mod update;

fn main() {
    if let Err(error) = app::run() {
        app::show_startup_error(&error);
    }
}
