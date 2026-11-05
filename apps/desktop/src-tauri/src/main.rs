// A release build on Windows is a GUI application, so it opens no console window.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    app_lib::run();
}
