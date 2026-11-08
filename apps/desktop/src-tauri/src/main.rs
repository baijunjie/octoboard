// A release build on Windows is a GUI application, so it opens no console window.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    if app_lib::answer_permission_probe() {
        return;
    }
    app_lib::run();
}
