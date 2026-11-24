//! Culla desktop entry point.
// Keeps a console window from opening next to the app in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![forbid(unsafe_code)]

fn main() {
    culla_app_lib::run();
}
